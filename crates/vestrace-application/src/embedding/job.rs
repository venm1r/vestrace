use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    AuditEvent, AuthorizationRequest, Capability, EmbeddingJobId, EmbeddingSpaceId,
    ExternalEffectIntent, ModelRequestEvidenceId, PolicyDecision, PolicyDecisionResult,
    ResourceScope, RiskCategory, embedding::EmbeddingJobKind,
};

use crate::{
    ApplicationError, GovernedMutationReceipt, IdempotencyRecord, OutboxMessage, RequestContext,
};

/// One embedding job, accepted with every identity it will ever own.
///
/// Spec line 219: a job owns "exactly one immutable embedding-kind
/// `ModelBindingSnapshot`, one external-effect identity, and one
/// `ModelRequestEvidence` identity", and an ordinary job's snapshot is
/// "resolved from the current tuple at acceptance and never changed
/// thereafter". Every one of those is stated by the caller here and proved
/// consistent by the guarded function; none is resolved later.
///
/// The intent is carried rather than constructed inside the repository.
/// `ExternalEffectIntent::new` allocates a fresh id, so building one at the
/// storage boundary would give a same-key replay a different effect than the
/// job it is being accepted for — the same trap the Run-step path documents.
pub struct AcceptEmbeddingJob {
    pub job_id: EmbeddingJobId,
    pub space_registration_id: EmbeddingSpaceId,
    pub kind: EmbeddingJobKind,
    pub model_binding_snapshot_id: uuid::Uuid,
    pub intent: ExternalEffectIntent,
    pub model_request_evidence_id: ModelRequestEvidenceId,
    /// Spec line 257: only the authorized duplicate-charge acknowledgement sets
    /// this, and only against a terminal `InconclusiveUnknown` predecessor. An
    /// ordinary job retries nothing, and the guarded function refuses a
    /// predecessor in any other state rather than trusting the caller.
    pub retries_unknown_embedding_job_id: Option<EmbeddingJobId>,
    /// The predecessor version the acknowledging operator read.
    ///
    /// Line 257 requires the acknowledgement to be expected-version checked.
    /// Present exactly when a predecessor is named: the guarded function refuses
    /// a successor without one and refuses a version without a successor, so the
    /// pair cannot drift apart in either direction.
    pub expected_predecessor_version: Option<u64>,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// The only durable sources that can prove a governed embedding job never
/// crossed the provider boundary.  A string supplied by a caller is not one.
#[derive(Clone, Debug)]
pub enum PreDispatchTerminationEvidence {
    /// The configured policy engine allowed this workspace-scoped cancellation.
    CancellationAuthorization(Box<PolicyDecision>),
    /// The owning effect has an exact persisted denied authorization record.
    ExternalEffectDenied { authorization_id: uuid::Uuid },
    /// The admission wait itself reached its database-time deadline.
    AdmissionTimeout { wait_id: uuid::Uuid },
}

impl PreDispatchTerminationEvidence {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CancellationAuthorization(_) => "cancellation_authorization",
            Self::ExternalEffectDenied { .. } => "external_effect_denied",
            Self::AdmissionTimeout { .. } => "admission_timeout",
        }
    }

    pub fn id(&self) -> uuid::Uuid {
        match self {
            Self::CancellationAuthorization(decision) => decision.id.as_uuid(),
            Self::ExternalEffectDenied { authorization_id } => *authorization_id,
            Self::AdmissionTimeout { wait_id } => *wait_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreDispatchTerminalState {
    Cancelled,
    FailedDefinite,
}

impl PreDispatchTerminalState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::FailedDefinite => "failed_definite",
        }
    }
}

/// Typed, expected-version termination.  `receipt_id` is allocated before the
/// write so retries converge on one durable command identity instead of asking
/// a shared governed-mutation replay to decide whether it can reapply.
#[derive(Clone, Debug)]
pub struct TerminateEmbeddingJobPreDispatch {
    pub receipt_id: uuid::Uuid,
    pub job_id: EmbeddingJobId,
    pub expected_version: u64,
    pub idempotency_key: String,
    pub terminal_state: PreDispatchTerminalState,
    pub evidence: PreDispatchTerminationEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingJobTerminationReceipt {
    pub receipt_id: uuid::Uuid,
    pub job_id: EmbeddingJobId,
    pub version: u64,
    pub terminal_state: PreDispatchTerminalState,
    pub evidence_kind: String,
    pub evidence_id: uuid::Uuid,
}

/// The configured-policy cancellation entry point.  Definite failure is
/// intentionally absent: it is reconciliation over durable effect/wait facts,
/// never a second request authorization.
pub struct EmbeddingJobTerminationService {
    repository: SharedEmbeddingJobRepository,
    policy: crate::SharedPolicyDecisionEngine,
}

impl EmbeddingJobTerminationService {
    pub fn new(
        repository: SharedEmbeddingJobRepository,
        policy: crate::SharedPolicyDecisionEngine,
    ) -> Self {
        Self { repository, policy }
    }

    pub async fn cancel(
        &self,
        context: RequestContext,
        job_id: EmbeddingJobId,
        expected_version: u64,
        idempotency_key: String,
    ) -> Result<EmbeddingJobTerminationReceipt, ApplicationError> {
        let request = AuthorizationRequest::new(
            Capability::ExecutionWrite,
            "embedding.job.cancel",
            ResourceScope::workspace().to_string(),
            RiskCategory::Low,
        );
        let decision = self.policy.decide(&context, request.clone()).await?;
        validate_cancellation_decision(&context, &request, &decision)?;
        self.repository
            .terminate_pre_dispatch(
                context,
                TerminateEmbeddingJobPreDispatch {
                    receipt_id: uuid::Uuid::now_v7(),
                    job_id,
                    expected_version,
                    idempotency_key,
                    terminal_state: PreDispatchTerminalState::Cancelled,
                    evidence: PreDispatchTerminationEvidence::CancellationAuthorization(Box::new(
                        decision,
                    )),
                },
            )
            .await
    }
}

pub fn validate_cancellation_decision(
    context: &RequestContext,
    request: &AuthorizationRequest,
    decision: &PolicyDecision,
) -> Result<(), ApplicationError> {
    if decision.result != PolicyDecisionResult::Allow
        || decision.workspace_id != context.workspace_id
        || decision.subject_id != context.principal_id
        || decision.capability != request.capability
        || decision.operation != request.operation
        || decision.resource_scope != request.resource_scope
        || decision.input_state.workspace_id != context.workspace_id
        || decision.input_state.subject_id != context.principal_id
        || decision.input_state.capability != request.capability
        || decision.input_state.operation != request.operation
        || decision.input_state.resource_scope != request.resource_scope
        || decision.input_state.requested_risk != request.requested_risk
        || decision.input_state.context_risk != request.context_risk
        || decision.input_state.effective_risk != request.effective_risk()
        || decision.input_state.requested_budget_units != request.requested_budget_units
        || decision.input_state.conditions != request.conditions
    {
        return Err(ApplicationError::Policy(
            "embedding cancellation authorization is not bound to its exact request context"
                .to_owned(),
        ));
    }
    Ok(())
}

/// Accepts embedding jobs and records their durable pre-dispatch terminal facts.
///
/// Rediscovery, dispatch and recovery remain on `ProviderDispatchRepository`:
/// the termination method establishes an immutable terminal fence, but never
/// becomes a second provider-dispatch authority for embeddings.
#[async_trait]
pub trait EmbeddingJobRepository: Send + Sync {
    /// The default refuses. An unconfigured authority that answered `Ok` would
    /// let a caller believe a job exists and then wait forever for a dispatch
    /// nothing will make.
    async fn accept_governed(
        &self,
        _context: RequestContext,
        _command: AcceptEmbeddingJob,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding job acceptance is not configured".to_owned(),
        ))
    }

    async fn terminate_pre_dispatch(
        &self,
        _context: RequestContext,
        _command: TerminateEmbeddingJobPreDispatch,
    ) -> Result<EmbeddingJobTerminationReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding job pre-dispatch termination is not configured".to_owned(),
        ))
    }
}

pub type SharedEmbeddingJobRepository = Arc<dyn EmbeddingJobRepository>;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use vestrace_domain::{
        PolicyDecisionId, PolicyDecisionReason, PolicyInputState, PrincipalId, WorkspaceId,
    };

    use super::*;
    use crate::{
        ConfiguredCapabilityPolicyEngine, PolicyDecisionEngine, SharedPolicyDecisionEngine,
    };

    struct RecordingRepository {
        commands: Mutex<Vec<TerminateEmbeddingJobPreDispatch>>,
    }

    #[async_trait]
    impl EmbeddingJobRepository for RecordingRepository {
        async fn terminate_pre_dispatch(
            &self,
            _context: RequestContext,
            command: TerminateEmbeddingJobPreDispatch,
        ) -> Result<EmbeddingJobTerminationReceipt, ApplicationError> {
            let receipt = EmbeddingJobTerminationReceipt {
                receipt_id: command.receipt_id,
                job_id: command.job_id,
                version: command.expected_version + 1,
                terminal_state: command.terminal_state,
                evidence_kind: command.evidence.kind().to_owned(),
                evidence_id: command.evidence.id(),
            };
            self.commands.lock().unwrap().push(command);
            Ok(receipt)
        }
    }

    struct DefaultRepository;

    #[async_trait]
    impl EmbeddingJobRepository for DefaultRepository {}

    struct AdversarialPolicy {
        mutate: fn(&mut PolicyDecision),
    }

    #[async_trait]
    impl PolicyDecisionEngine for AdversarialPolicy {
        async fn decide(
            &self,
            context: &RequestContext,
            request: AuthorizationRequest,
        ) -> Result<PolicyDecision, ApplicationError> {
            let mut decision = allowed_decision(context, &request);
            (self.mutate)(&mut decision);
            Ok(decision)
        }
    }

    fn context() -> RequestContext {
        RequestContext::new(WorkspaceId::new(), PrincipalId::new())
    }

    fn allowed_decision(
        context: &RequestContext,
        request: &AuthorizationRequest,
    ) -> PolicyDecision {
        PolicyDecision {
            id: PolicyDecisionId::new(),
            policy_id: None,
            policy_version: "test-policy".to_owned(),
            workspace_id: context.workspace_id,
            subject_id: context.principal_id,
            capability: request.capability.clone(),
            operation: request.operation.clone(),
            resource_scope: request.resource_scope.clone(),
            result: PolicyDecisionResult::Allow,
            reason: PolicyDecisionReason::ConfiguredAllowance,
            input_state: PolicyInputState::from_request(
                context.workspace_id,
                context.principal_id,
                request,
            ),
            matched_grant_id: None,
            decided_at: vestrace_domain::time::now(),
        }
    }

    fn configured_cancellation_policy() -> SharedPolicyDecisionEngine {
        Arc::new(
            ConfiguredCapabilityPolicyEngine::new(
                "cancellation-test-v1",
                [Capability::ExecutionWrite],
                RiskCategory::Low,
            )
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn configured_allowed_cancellation_calls_the_repository_with_the_exact_command() {
        let context = context();
        let repository = Arc::new(RecordingRepository {
            commands: Mutex::new(Vec::new()),
        });
        let service = EmbeddingJobTerminationService::new(
            repository.clone(),
            configured_cancellation_policy(),
        );
        let job_id = EmbeddingJobId::new();

        let receipt = service
            .cancel(context.clone(), job_id, 7, "cancel-7".to_owned())
            .await
            .unwrap();

        let commands = repository.commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        let command = &commands[0];
        assert_eq!(command.job_id, job_id);
        assert_eq!(command.expected_version, 7);
        assert_eq!(command.idempotency_key, "cancel-7");
        assert_eq!(command.terminal_state, PreDispatchTerminalState::Cancelled);
        let PreDispatchTerminationEvidence::CancellationAuthorization(decision) = &command.evidence
        else {
            panic!("configured cancellation must carry its authorization decision");
        };
        assert!(decision.is_allowed());
        assert_eq!(decision.workspace_id, context.workspace_id);
        assert_eq!(decision.subject_id, context.principal_id);
        assert_eq!(decision.capability, Capability::ExecutionWrite);
        assert_eq!(decision.operation, "embedding.job.cancel");
        assert_eq!(
            decision.resource_scope,
            ResourceScope::workspace().to_string()
        );
        assert_eq!(
            decision.input_state,
            PolicyInputState::from_request(
                context.workspace_id,
                context.principal_id,
                &AuthorizationRequest::new(
                    Capability::ExecutionWrite,
                    "embedding.job.cancel",
                    ResourceScope::workspace().to_string(),
                    RiskCategory::Low,
                ),
            )
        );
        assert_eq!(receipt.receipt_id, command.receipt_id);
        assert_eq!(receipt.version, 8);
    }

    #[tokio::test]
    async fn denied_cancellation_never_calls_the_repository() {
        let context = context();
        let repository = Arc::new(RecordingRepository {
            commands: Mutex::new(Vec::new()),
        });
        let service = EmbeddingJobTerminationService::new(
            repository.clone(),
            Arc::new(crate::DenyAllPolicyEngine),
        );

        let result = service
            .cancel(context, EmbeddingJobId::new(), 1, "denied".to_owned())
            .await;

        assert!(matches!(result, Err(ApplicationError::Policy(_))));
        assert!(repository.commands.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn mismatched_allowed_policy_decisions_never_call_the_repository() {
        type PolicyMutation = fn(&mut PolicyDecision);
        let cases: [(&str, PolicyMutation); 9] = [
            ("workspace", |decision| {
                decision.workspace_id = WorkspaceId::new()
            }),
            ("principal", |decision| {
                decision.subject_id = PrincipalId::new()
            }),
            ("capability", |decision| {
                decision.capability = Capability::ExecutionRead
            }),
            ("operation", |decision| {
                decision.operation = "other.operation".to_owned()
            }),
            ("scope", |decision| {
                decision.resource_scope = "workspace://other".to_owned()
            }),
            ("input state", |decision| {
                decision.input_state.workspace_id = WorkspaceId::new()
            }),
            ("input scope", |decision| {
                decision.input_state.resource_scope = "workspace://other".to_owned()
            }),
            ("risk", |decision| {
                decision.input_state.requested_risk = RiskCategory::Critical
            }),
            ("effective risk", |decision| {
                decision.input_state.effective_risk = RiskCategory::Critical
            }),
        ];

        for (name, mutate) in cases {
            let context = context();
            let repository = Arc::new(RecordingRepository {
                commands: Mutex::new(Vec::new()),
            });
            let service = EmbeddingJobTerminationService::new(
                repository.clone(),
                Arc::new(AdversarialPolicy { mutate }),
            );

            let result = service
                .cancel(
                    context,
                    EmbeddingJobId::new(),
                    1,
                    format!("mismatched-{name}"),
                )
                .await;

            assert!(
                matches!(result, Err(ApplicationError::Policy(_))),
                "a {name} mismatch must be refused"
            );
            assert!(
                repository.commands.lock().unwrap().is_empty(),
                "a {name} mismatch must not reach durable termination"
            );
        }
    }

    #[tokio::test]
    async fn default_repository_fails_closed_after_a_valid_authorization() {
        let service = EmbeddingJobTerminationService::new(
            Arc::new(DefaultRepository),
            configured_cancellation_policy(),
        );

        let result = service
            .cancel(
                context(),
                EmbeddingJobId::new(),
                1,
                "unconfigured".to_owned(),
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::Unavailable(_))));
    }
}
