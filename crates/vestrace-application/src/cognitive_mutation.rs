use crate::{ApplicationError, PolicyEngine, RequestContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vestrace_domain::{
    CognitiveMutation, CognitiveMutationId, ConflictId, DomainError, EvidenceRef, MutationKind,
    MutationTargetKind, PrincipalId, ReconciliationClass, ReconciliationOutcome,
    ReconciliationRecord, now, time::Timestamp,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CognitiveMutationCommand {
    pub mutation_id: CognitiveMutationId,
    pub target_kind: MutationTargetKind,
    pub target_id: String,
    pub expected_state_revision: u32,
    pub mutation_kind: MutationKind,
    pub reason: String,
    pub provenance_refs: Vec<EvidenceRef>,
    pub reconciliation: Option<ReconciliationRequest>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationRequest {
    pub reconciliation_id: String,
    pub conflict_id: ConflictId,
    pub reconciliation_class: ReconciliationClass,
    pub outcome: ReconciliationOutcome,
    pub input_evidence_refs: Vec<EvidenceRef>,
    pub basis_refs: Vec<EvidenceRef>,
    pub policy_version: Option<String>,
    pub human_decision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CognitiveMutationResult {
    pub mutation: CognitiveMutation,
    pub reconciliation: Option<ReconciliationRecord>,
    pub replayed: bool,
}

#[async_trait]
pub trait CognitiveMutationRepository: Send + Sync {
    async fn apply_transactional(
        &self,
        context: &RequestContext,
        mutation: CognitiveMutation,
        reconciliation: Option<ReconciliationRecord>,
        request_hash: String,
        idempotency_key: String,
    ) -> Result<CognitiveMutationResult, ApplicationError>;
}

pub struct CognitiveMutationService<R, P> {
    repository: R,
    policy: P,
}

impl<R, P> CognitiveMutationService<R, P>
where
    R: CognitiveMutationRepository,
    P: PolicyEngine,
{
    pub fn new(repository: R, policy: P) -> Self {
        Self { repository, policy }
    }

    pub async fn apply(
        &self,
        context: &RequestContext,
        command: CognitiveMutationCommand,
    ) -> Result<CognitiveMutationResult, ApplicationError> {
        validate_command(&command)?;
        self.policy
            .evaluate(context, vestrace_domain::Capability::MemoryWrite)
            .await?;

        let request_hash = request_hash(&command)?;
        let at = now();
        let mut mutation = CognitiveMutation::new(
            command.mutation_id,
            context.principal_id,
            context.workspace_id,
            command.target_kind,
            command.target_id.clone(),
            command.expected_state_revision,
            command.mutation_kind,
            command.reason,
            at,
        );
        for evidence in command.provenance_refs {
            mutation = mutation.with_provenance(evidence);
        }

        let reconciliation = command
            .reconciliation
            .map(|request| request.into_record(context.workspace_id, context.principal_id, at))
            .transpose()?;

        self.repository
            .apply_transactional(
                context,
                mutation,
                reconciliation,
                request_hash,
                command.idempotency_key,
            )
            .await
    }
}

impl ReconciliationRequest {
    fn into_record(
        self,
        workspace_id: vestrace_domain::WorkspaceId,
        resolved_by: PrincipalId,
        at: Timestamp,
    ) -> Result<ReconciliationRecord, ApplicationError> {
        let record = match self.reconciliation_class {
            ReconciliationClass::Deterministic => {
                if self.outcome != ReconciliationOutcome::Resolved {
                    return Err(invalid("deterministic reconciliation must resolve"));
                }
                ReconciliationRecord::deterministic(
                    self.reconciliation_id,
                    workspace_id,
                    self.conflict_id,
                    self.input_evidence_refs,
                    self.basis_refs,
                    resolved_by,
                    at,
                )?
            }
            ReconciliationClass::PolicyGuided => {
                if self.outcome != ReconciliationOutcome::Resolved {
                    return Err(invalid("policy-guided reconciliation must resolve"));
                }
                ReconciliationRecord::policy_guided(
                    self.reconciliation_id,
                    workspace_id,
                    self.conflict_id,
                    self.input_evidence_refs,
                    self.policy_version.unwrap_or_default(),
                    resolved_by,
                    at,
                )?
            }
            ReconciliationClass::Semantic => {
                let record = ReconciliationRecord::semantic(
                    self.reconciliation_id,
                    workspace_id,
                    self.conflict_id,
                    self.input_evidence_refs,
                    self.basis_refs,
                    resolved_by,
                    at,
                )?;
                match self.outcome {
                    ReconciliationOutcome::Resolved => record,
                    ReconciliationOutcome::AcceptedAmbiguity => record.accept_ambiguity()?,
                    ReconciliationOutcome::HumanDeferred => record.defer_to_human()?,
                    ReconciliationOutcome::Obsolete => {
                        return Err(invalid("semantic reconciliation cannot be obsolete"));
                    }
                }
            }
            ReconciliationClass::HumanRequired => {
                if self.outcome != ReconciliationOutcome::Resolved {
                    return Err(invalid("human-required reconciliation must resolve"));
                }
                ReconciliationRecord::human_required(
                    self.reconciliation_id,
                    workspace_id,
                    self.conflict_id,
                    self.input_evidence_refs,
                    self.human_decision.unwrap_or_default(),
                    resolved_by,
                    at,
                )?
            }
        };
        Ok(record)
    }
}

fn validate_command(command: &CognitiveMutationCommand) -> Result<(), ApplicationError> {
    if command.target_id.trim().is_empty() {
        return Err(invalid("mutation target_id must not be blank"));
    }
    if command.reason.trim().is_empty() {
        return Err(invalid("mutation reason must not be blank"));
    }
    if command.idempotency_key.trim().is_empty() {
        return Err(invalid("idempotency key must not be blank"));
    }
    if let Some(reconciliation) = &command.reconciliation {
        if command.target_kind != MutationTargetKind::Conflict {
            return Err(invalid("reconciliation target must be a conflict"));
        }
        let target_id: ConflictId = command
            .target_id
            .parse()
            .map_err(|_| invalid("conflict target_id must be a UUID"))?;
        if reconciliation.conflict_id != target_id {
            return Err(invalid("reconciliation conflict_id must match target_id"));
        }
    }
    Ok(())
}

fn request_hash(command: &CognitiveMutationCommand) -> Result<String, ApplicationError> {
    let bytes = serde_json::to_vec(command)
        .map_err(|error| ApplicationError::Internal(error.to_string()))?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn invalid(message: impl Into<String>) -> ApplicationError {
    ApplicationError::Domain(DomainError::InvalidArgument(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::CapabilitySetPolicyEngine;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use vestrace_domain::{
        Capability, CognitiveMutation, CognitiveMutationId, ConflictId, EventId, EvidenceRef,
        MutationKind, MutationTargetKind, PrincipalId, ReconciliationClass, ReconciliationOutcome,
        WorkspaceId,
    };

    #[derive(Clone, Default)]
    struct RecordingMutationRepository {
        applied: Arc<Mutex<Vec<AppliedMutation>>>,
    }

    #[derive(Clone)]
    struct AppliedMutation {
        mutation: CognitiveMutation,
        reconciliation: Option<ReconciliationRecord>,
        request_hash: String,
        idempotency_key: String,
    }

    #[async_trait]
    impl CognitiveMutationRepository for RecordingMutationRepository {
        async fn apply_transactional(
            &self,
            _context: &RequestContext,
            mutation: CognitiveMutation,
            reconciliation: Option<ReconciliationRecord>,
            request_hash: String,
            idempotency_key: String,
        ) -> Result<CognitiveMutationResult, ApplicationError> {
            let result = CognitiveMutationResult {
                mutation: mutation.clone(),
                reconciliation: reconciliation.clone(),
                replayed: false,
            };
            self.applied.lock().unwrap().push(AppliedMutation {
                mutation,
                reconciliation,
                request_hash,
                idempotency_key,
            });
            Ok(result)
        }
    }

    fn context() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    fn command() -> CognitiveMutationCommand {
        CognitiveMutationCommand {
            mutation_id: CognitiveMutationId::new(),
            target_kind: MutationTargetKind::Conflict,
            target_id: ConflictId::new().to_string(),
            expected_state_revision: 0,
            mutation_kind: MutationKind::ResolveConflict,
            reason: "reconcile conflicting claims".into(),
            provenance_refs: vec![EvidenceRef::event(EventId::new())],
            reconciliation: None,
            idempotency_key: "idem-1".into(),
        }
    }

    #[tokio::test]
    async fn mutation_service_requires_memory_write_capability() {
        let repository = RecordingMutationRepository::default();
        let service = CognitiveMutationService::new(repository, CapabilitySetPolicyEngine::new([]));

        let result = service.apply(&context(), command()).await;

        assert!(matches!(result, Err(ApplicationError::Policy(_))));
    }

    #[tokio::test]
    async fn mutation_service_passes_hashed_request_to_repository() {
        let repository = RecordingMutationRepository::default();
        let applied = repository.applied.clone();
        let service = CognitiveMutationService::new(
            repository,
            CapabilitySetPolicyEngine::new([Capability::MemoryWrite]),
        );

        service.apply(&context(), command()).await.unwrap();

        let applied = applied.lock().unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].idempotency_key, "idem-1");
        assert_eq!(applied[0].request_hash.len(), 64);
        assert!(applied[0].reconciliation.is_none());
        assert_eq!(applied[0].mutation.provenance_refs.len(), 1);
    }

    #[tokio::test]
    async fn deterministic_reconciliation_preserves_basis_and_conflict_identity() {
        let repository = RecordingMutationRepository::default();
        let applied = repository.applied.clone();
        let service = CognitiveMutationService::new(
            repository,
            CapabilitySetPolicyEngine::new([Capability::MemoryWrite]),
        );
        let context = context();
        let mut command = command();
        let conflict_id: ConflictId = command.target_id.parse().unwrap();
        let basis = EvidenceRef::event(EventId::new());
        command.reconciliation = Some(ReconciliationRequest {
            reconciliation_id: "reconciliation-1".into(),
            conflict_id,
            reconciliation_class: ReconciliationClass::Deterministic,
            outcome: ReconciliationOutcome::Resolved,
            input_evidence_refs: command.provenance_refs.clone(),
            basis_refs: vec![basis.clone()],
            policy_version: None,
            human_decision: None,
        });

        service.apply(&context, command).await.unwrap();

        let applied = applied.lock().unwrap();
        let record = applied[0].reconciliation.as_ref().unwrap();
        assert_eq!(record.workspace_id, context.workspace_id);
        assert_eq!(record.conflict_id, conflict_id);
        assert_eq!(record.basis_refs, vec![basis]);
    }

    #[tokio::test]
    async fn policy_guided_reconciliation_requires_exact_policy_version() {
        let service = CognitiveMutationService::new(
            RecordingMutationRepository::default(),
            CapabilitySetPolicyEngine::new([Capability::MemoryWrite]),
        );
        let mut command = command();
        let conflict_id: ConflictId = command.target_id.parse().unwrap();
        command.reconciliation = Some(ReconciliationRequest {
            reconciliation_id: "reconciliation-policy".into(),
            conflict_id,
            reconciliation_class: ReconciliationClass::PolicyGuided,
            outcome: ReconciliationOutcome::Resolved,
            input_evidence_refs: Vec::new(),
            basis_refs: Vec::new(),
            policy_version: None,
            human_decision: None,
        });

        let result = service.apply(&context(), command).await;

        assert!(matches!(result, Err(ApplicationError::Domain(_))));
    }

    #[tokio::test]
    async fn human_required_reconciliation_requires_human_decision() {
        let service = CognitiveMutationService::new(
            RecordingMutationRepository::default(),
            CapabilitySetPolicyEngine::new([Capability::MemoryWrite]),
        );
        let mut command = command();
        let conflict_id: ConflictId = command.target_id.parse().unwrap();
        command.reconciliation = Some(ReconciliationRequest {
            reconciliation_id: "reconciliation-human".into(),
            conflict_id,
            reconciliation_class: ReconciliationClass::HumanRequired,
            outcome: ReconciliationOutcome::Resolved,
            input_evidence_refs: Vec::new(),
            basis_refs: Vec::new(),
            policy_version: None,
            human_decision: None,
        });

        let result = service.apply(&context(), command).await;

        assert!(matches!(result, Err(ApplicationError::Domain(_))));
    }
}
