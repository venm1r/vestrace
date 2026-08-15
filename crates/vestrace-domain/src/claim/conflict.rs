use crate::{
    DomainError, EvidenceRef,
    id::{ClaimId, ConflictId, PrincipalId, WorkspaceId},
    time::Timestamp,
};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    SemanticContradiction,
    TemporalConflict,
    SourceConflict,
    PolicyConflict,
    ScopeConflict,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStatus {
    Open,
    ReconciliationProposed,
    Resolved,
    AcceptedAmbiguity,
    Obsolete,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Conflict {
    pub conflict_id: ConflictId,
    pub workspace_id: WorkspaceId,
    pub conflict_kind: ConflictKind,
    pub participant_refs: Vec<ClaimId>,
    pub status: ConflictStatus,
    pub state_revision: u32,
    pub detected_by: PrincipalId,
    pub evidence_refs: Vec<EvidenceRef>,
    pub reconciliation_ref: Option<String>,
    pub created_at: Timestamp,
}

impl Conflict {
    pub fn new(
        conflict_id: ConflictId,
        workspace_id: WorkspaceId,
        conflict_kind: ConflictKind,
        participant_refs: Vec<ClaimId>,
        detected_by: PrincipalId,
        at: Timestamp,
    ) -> Self {
        Self {
            conflict_id,
            workspace_id,
            conflict_kind,
            participant_refs,
            status: ConflictStatus::Open,
            state_revision: 0,
            detected_by,
            evidence_refs: Vec::new(),
            reconciliation_ref: None,
            created_at: at,
        }
    }

    pub fn propose_reconciliation(
        mut self,
        reconciliation_ref: String,
    ) -> Result<Self, DomainError> {
        if reconciliation_ref.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "reconciliation_ref must not be blank".into(),
            ));
        }
        match self.status {
            ConflictStatus::Open => {
                self.status = ConflictStatus::ReconciliationProposed;
                self.reconciliation_ref = Some(reconciliation_ref);
                self.state_revision += 1;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot propose reconciliation for conflict in status {:?}",
                self.status
            ))),
        }
    }

    pub fn resolve(mut self) -> Result<Self, DomainError> {
        match self.status {
            ConflictStatus::ReconciliationProposed if self.reconciliation_ref.is_some() => {
                self.status = ConflictStatus::Resolved;
                self.state_revision += 1;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot resolve conflict in status {:?}",
                self.status
            ))),
        }
    }

    pub fn accept_ambiguity(mut self) -> Result<Self, DomainError> {
        match self.status {
            ConflictStatus::ReconciliationProposed if self.reconciliation_ref.is_some() => {
                self.status = ConflictStatus::AcceptedAmbiguity;
                self.state_revision += 1;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(format!(
                "cannot accept ambiguity for conflict in status {:?}",
                self.status
            ))),
        }
    }

    pub fn obsolete(mut self) -> Result<Self, DomainError> {
        match self.status {
            ConflictStatus::Open
            | ConflictStatus::ReconciliationProposed
            | ConflictStatus::Resolved
            | ConflictStatus::AcceptedAmbiguity => {
                self.status = ConflictStatus::Obsolete;
                self.state_revision += 1;
                Ok(self)
            }
            _ => Err(DomainError::PolicyViolation(
                "conflict is already obsolete".into(),
            )),
        }
    }

    pub fn add_evidence(mut self, evidence: EvidenceRef) -> Self {
        self.evidence_refs.push(evidence);
        self.state_revision += 1;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Conflict, ConflictKind, ConflictStatus};
    use crate::{ClaimId, PrincipalId, WorkspaceId, now};

    fn open_conflict() -> Conflict {
        Conflict::new(
            crate::ConflictId::new(),
            WorkspaceId::new(),
            ConflictKind::SemanticContradiction,
            vec![ClaimId::new(), ClaimId::new()],
            PrincipalId::new(),
            now(),
        )
    }

    #[test]
    fn conflict_cannot_be_resolved_without_a_reconciliation_reference() {
        let conflict = open_conflict();

        assert!(conflict.resolve().is_err());
    }

    #[test]
    fn proposed_reconciliation_can_resolve_conflict_without_erasing_history() {
        let conflict = open_conflict()
            .propose_reconciliation("reconciliation-1".to_owned())
            .unwrap()
            .resolve()
            .unwrap();

        assert_eq!(conflict.status, ConflictStatus::Resolved);
        assert_eq!(conflict.state_revision, 2);
        assert_eq!(
            conflict.reconciliation_ref.as_deref(),
            Some("reconciliation-1")
        );
        assert_eq!(conflict.participant_refs.len(), 2);
    }

    #[test]
    fn conflict_lifecycle_increments_state_revision() {
        let conflict = open_conflict();
        assert_eq!(conflict.state_revision, 0);

        let proposed = conflict
            .propose_reconciliation("reconciliation-1".to_owned())
            .unwrap();

        assert_eq!(proposed.state_revision, 1);
    }
}
