#![forbid(unsafe_code)]

pub mod cognitive;
pub mod cognitive_mutation;
pub mod cognitive_ports;
mod context;
pub mod diagnostics;
mod error;
pub mod execution_ports;
pub mod external_effects;
mod health;
pub mod idempotency;
pub mod jobs;
pub mod memory;
pub mod models;
mod null_execution_history;
pub mod operator;
pub mod outbox;
mod ports;
pub mod providers;
pub mod qualification;
pub mod recovery;
pub mod retrieval;
pub mod run;
pub mod runs;
pub mod security;

pub use cognitive::{
    AgentRecord, AgentRepository, SharedAgentRepository, SharedSkillRepository, SkillRecord,
    SkillRepository,
};
pub use cognitive_mutation::{
    CognitiveMutationCommand, CognitiveMutationRepository, CognitiveMutationResult,
    CognitiveMutationService, ReconciliationRequest,
};
pub use cognitive_ports::{
    EvaluationFactRecord, EvaluationRecord, EvaluationRepository, LearnedProjectionRecord,
    LearningProposalRecord, LearningRepository, SharedEvaluationRepository,
    SharedLearningRepository, SharedWorkflowRepository, UnavailableLearningRepository,
    WorkflowDefinitionRecord, WorkflowRepository, WorkflowRevisionRecord,
};
pub use context::RequestContext;
pub use diagnostics::{DiagnosticsRepository, DoctorService, SharedDiagnosticsRepository};
pub use error::ApplicationError;
pub use execution_ports::{
    ExecutionArtifactRecord, ExecutionHistoryRepository, ExecutionOutcomeRecord,
    SharedExecutionHistoryRepository, StepExecutionRecord, WorkflowExecutionRecord,
};
pub use external_effects::ExternalEffectService;
pub use health::HealthRepository;
pub use idempotency::{IdempotencyRecord, IdempotencyRepository};
pub use jobs::*;
pub use memory::*;
pub use models::{
    ModelExecutionRecord, ModelExecutionRepository, ModelRecord, ModelRepository, ProviderRecord,
    ProviderRepository, RoutingDecisionRecord, RoutingDecisionRepository,
    SharedModelExecutionRepository, SharedModelRepository, SharedProviderRepository,
    SharedRoutingDecisionRepository,
};
pub use null_execution_history::NullExecutionHistoryRepository;
pub use operator::{HealthOperatorService, RepairPlanRequest, RepairRequest};
pub use outbox::{OutboxMessage, OutboxRepository};
pub use ports::{TransactionManager, UnitOfWork};
pub use providers::*;
pub use qualification::{
    QualificationRepository, QualificationRuntime, RuntimeQualificationDecision,
    RuntimeQualificationEvidence, SharedQualificationRepository, evaluate_runtime_qualification,
};
pub use recovery::{RecoveryRepository, SharedRecoveryRepository};
pub use retrieval::{
    ContextPackBuilder, ExactRetriever, NormalizedRetrievalRequest, RetrievalJournal,
    RetrievalRequest, RetrievalResult, RetrievalService, SharedExactRetriever,
    SharedRetrievalJournal, SharedStructuredRetriever, SharedTextRetriever, SharedVectorRetriever,
    StructuredRetriever, TextRetriever, VectorRetriever, reciprocal_rank_fusion, rerank,
};
pub use runs::*;
pub use security::{
    AuditRepository, AuthorizationBoundary, BudgetPolicyEngine, DenyAllPolicyEngine,
    GrantPolicyEngine, PolicyDecisionEngine, PolicyEngine, RedactionRule, RedactionService,
    SharedAuditRepository, SharedHierarchicalBudget, SharedPolicyDecisionEngine,
};

#[cfg(test)]
mod tests {
    use super::{ApplicationError, RequestContext};
    use vestrace_domain::{DomainError, PrincipalId, WorkspaceId};

    #[test]
    fn request_context_is_workspace_bound() {
        let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

        assert_ne!(ctx.workspace_id.as_uuid(), uuid::Uuid::nil());
    }

    #[test]
    fn application_error_preserves_domain_failure() {
        let error = ApplicationError::from(DomainError::InvalidArgument(
            "workspace name is required".to_owned(),
        ));

        assert!(matches!(
            error,
            ApplicationError::Domain(DomainError::InvalidArgument(message))
                if message == "workspace name is required"
        ));
    }
}
