use crate::{
    DomainError, EvidenceRef,
    id::{CognitiveMutationId, ConflictId, PrincipalId, WorkspaceId},
    time::Timestamp,
};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationClass {
    Deterministic,
    PolicyGuided,
    Semantic,
    HumanRequired,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Create,
    Revise,
    Correct,
    Supersede,
    Expire,
    Delete,
    Reject,
    ResolveConflict,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MutationTargetKind {
    Memory,
    MemoryRevision,
    Claim,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CognitiveMutation {
    pub mutation_id: CognitiveMutationId,
    pub actor: PrincipalId,
    pub workspace_id: WorkspaceId,
    pub target_kind: MutationTargetKind,
    pub target_id: String,
    pub expected_state_revision: u32,
    pub mutation_kind: MutationKind,
    pub reason: String,
    pub provenance_refs: Vec<EvidenceRef>,
    pub resulting_revision_id: Option<String>,
    pub created_at: Timestamp,
}

impl CognitiveMutation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mutation_id: CognitiveMutationId,
        actor: PrincipalId,
        workspace_id: WorkspaceId,
        target_kind: MutationTargetKind,
        target_id: String,
        expected_state_revision: u32,
        mutation_kind: MutationKind,
        reason: String,
        at: Timestamp,
    ) -> Self {
        Self {
            mutation_id,
            actor,
            workspace_id,
            target_kind,
            target_id,
            expected_state_revision,
            mutation_kind,
            reason,
            provenance_refs: Vec::new(),
            resulting_revision_id: None,
            created_at: at,
        }
    }

    pub fn with_provenance(mut self, evidence: EvidenceRef) -> Self {
        self.provenance_refs.push(evidence);
        self
    }

    pub fn with_resulting_revision(mut self, revision_id: String) -> Self {
        self.resulting_revision_id = Some(revision_id);
        self
    }

    pub fn validate_expected_state(&self, current_state_revision: u32) -> Result<(), DomainError> {
        if self.expected_state_revision != current_state_revision {
            return Err(DomainError::RevisionConflict {
                expected: self.expected_state_revision as u64,
                current: current_state_revision as u64,
            });
        }
        Ok(())
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationOutcome {
    Resolved,
    AcceptedAmbiguity,
    HumanDeferred,
    Obsolete,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ReconciliationRecord {
    pub reconciliation_id: String,
    pub workspace_id: WorkspaceId,
    pub conflict_id: ConflictId,
    pub reconciliation_class: ReconciliationClass,
    pub outcome: ReconciliationOutcome,
    pub input_evidence_refs: Vec<EvidenceRef>,
    pub basis_refs: Vec<EvidenceRef>,
    pub policy_version: Option<String>,
    pub human_decision: Option<String>,
    pub resolved_by: PrincipalId,
    pub created_at: Timestamp,
}

impl ReconciliationRecord {
    pub fn deterministic(
        reconciliation_id: String,
        workspace_id: WorkspaceId,
        conflict_id: ConflictId,
        input_evidence: Vec<EvidenceRef>,
        basis: Vec<EvidenceRef>,
        resolved_by: PrincipalId,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if basis.is_empty() {
            return Err(DomainError::InvalidArgument(
                "deterministic reconciliation requires authoritative basis evidence".into(),
            ));
        }
        Ok(Self {
            reconciliation_id,
            workspace_id,
            conflict_id,
            reconciliation_class: ReconciliationClass::Deterministic,
            outcome: ReconciliationOutcome::Resolved,
            input_evidence_refs: input_evidence,
            basis_refs: basis,
            policy_version: None,
            human_decision: None,
            resolved_by,
            created_at: at,
        })
    }

    pub fn policy_guided(
        reconciliation_id: String,
        workspace_id: WorkspaceId,
        conflict_id: ConflictId,
        input_evidence: Vec<EvidenceRef>,
        policy_version: String,
        resolved_by: PrincipalId,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if policy_version.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "policy_version must not be blank".into(),
            ));
        }
        Ok(Self {
            reconciliation_id,
            workspace_id,
            conflict_id,
            reconciliation_class: ReconciliationClass::PolicyGuided,
            outcome: ReconciliationOutcome::Resolved,
            input_evidence_refs: input_evidence,
            basis_refs: Vec::new(),
            policy_version: Some(policy_version),
            human_decision: None,
            resolved_by,
            created_at: at,
        })
    }

    pub fn human_required(
        reconciliation_id: String,
        workspace_id: WorkspaceId,
        conflict_id: ConflictId,
        input_evidence: Vec<EvidenceRef>,
        human_decision: String,
        resolved_by: PrincipalId,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if human_decision.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "human_decision must not be blank".into(),
            ));
        }
        Ok(Self {
            reconciliation_id,
            workspace_id,
            conflict_id,
            reconciliation_class: ReconciliationClass::HumanRequired,
            outcome: ReconciliationOutcome::Resolved,
            input_evidence_refs: input_evidence,
            basis_refs: Vec::new(),
            policy_version: None,
            human_decision: Some(human_decision),
            resolved_by,
            created_at: at,
        })
    }

    pub fn semantic(
        reconciliation_id: String,
        workspace_id: WorkspaceId,
        conflict_id: ConflictId,
        input_evidence: Vec<EvidenceRef>,
        basis: Vec<EvidenceRef>,
        resolved_by: PrincipalId,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            reconciliation_id,
            workspace_id,
            conflict_id,
            reconciliation_class: ReconciliationClass::Semantic,
            outcome: ReconciliationOutcome::Resolved,
            input_evidence_refs: input_evidence,
            basis_refs: basis,
            policy_version: None,
            human_decision: None,
            resolved_by,
            created_at: at,
        })
    }

    pub fn defer_to_human(mut self) -> Result<Self, DomainError> {
        match self.reconciliation_class {
            ReconciliationClass::Semantic | ReconciliationClass::PolicyGuided => {
                self.outcome = ReconciliationOutcome::HumanDeferred;
                Ok(self)
            }
            ReconciliationClass::HumanRequired => Err(DomainError::PolicyViolation(
                "human-required reconciliation cannot be auto-deferred; it must be resolved by a human".into(),
            )),
            ReconciliationClass::Deterministic => Err(DomainError::PolicyViolation(
                "deterministic reconciliation cannot be deferred to human".into(),
            )),
        }
    }

    pub fn accept_ambiguity(mut self) -> Result<Self, DomainError> {
        match self.reconciliation_class {
            ReconciliationClass::Semantic => {
                self.outcome = ReconciliationOutcome::AcceptedAmbiguity;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot accept ambiguity for {:?} reconciliation",
                self.reconciliation_class
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CognitiveMutation, MutationKind, MutationTargetKind, ReconciliationRecord};
    use crate::{
        ClaimId, CognitiveMutationId, ConflictId, EventId, EvidenceRef, PrincipalId, WorkspaceId,
        now,
    };

    #[test]
    fn deterministic_reconciliation_requires_authoritative_basis_evidence() {
        let result = ReconciliationRecord::deterministic(
            "reconciliation-1".into(),
            WorkspaceId::new(),
            ConflictId::new(),
            vec![EvidenceRef::event(EventId::new())],
            Vec::new(),
            PrincipalId::new(),
            now(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn reconciliation_record_preserves_inputs_and_authoritative_basis() {
        let input = EvidenceRef::event(EventId::new());
        let basis = EvidenceRef::event(EventId::new());
        let record = ReconciliationRecord::deterministic(
            "reconciliation-1".into(),
            WorkspaceId::new(),
            ConflictId::new(),
            vec![input.clone()],
            vec![basis.clone()],
            PrincipalId::new(),
            now(),
        )
        .unwrap();

        assert_eq!(record.input_evidence_refs, vec![input]);
        assert_eq!(record.basis_refs, vec![basis]);
    }

    #[test]
    fn cognitive_mutation_rejects_stale_expected_state() {
        let mutation = CognitiveMutation::new(
            CognitiveMutationId::new(),
            PrincipalId::new(),
            WorkspaceId::new(),
            MutationTargetKind::Claim,
            ClaimId::new().to_string(),
            3,
            MutationKind::Correct,
            "correct stale claim".into(),
            now(),
        );

        assert_ne!(
            mutation.mutation_id,
            CognitiveMutationId::from_uuid(uuid::Uuid::nil())
        );
        assert!(mutation.validate_expected_state(4).is_err());
        assert!(mutation.validate_expected_state(3).is_ok());
    }
}
