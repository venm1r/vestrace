#![forbid(unsafe_code)]

pub mod ag_ui;
pub mod artifacts;
pub mod capability_restoration;
pub mod cognitive;
pub mod cognitive_mutation;
pub mod cognitive_ports;
pub mod conformance_cases;
pub mod connections;
mod context;
pub mod crypto_qualification;
pub mod diagnostics;
pub mod effect_outcome_delivery;
pub mod effect_recovery;
pub mod effect_repository;
mod error;
pub mod execution_ports;
pub mod external_effects;
pub mod fault_admission;
pub mod fault_bundle;
pub mod fault_evidence;
pub mod fault_gate_evidence;
pub mod fault_qualification;
pub mod fault_runtime;
pub mod fault_suite;
mod health;
pub mod idempotency;
pub mod identity;
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
pub mod recovery_reconciliation;
pub mod release_approval;
pub mod retrieval;
pub mod run;
pub mod runs;
pub mod secrets;
pub mod security;
pub mod settings;
pub mod triggers;
pub mod trust_restoration;
pub mod v1_release_evidence;

pub use ag_ui::{AgUiEndpoint, AgUiRepository, AgUiRunEvent, SharedAgUiRepository};
pub use artifacts::{
    ArtifactContent, ArtifactListing, ArtifactRepository, SharedArtifactRepository, StoredArtifact,
};
pub use capability_restoration::{
    CapabilityRestorationDecision, CapabilityRestorationPolicy, CapabilityRestorationService,
    RestorationBlockReason, RestorationEvidence, RestorationStage,
};
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
pub use connections::{ConnectionListing, ConnectionRepository, SharedConnectionRepository};
pub use context::RequestContext;
pub use crypto_qualification::{
    CryptoAdapterQualificationEvidence, CryptoAdapterQualificationProbe,
    CryptoAdapterQualificationService, CryptoAdapterQualificationTarget, CryptoCustody,
    CryptoQualificationCheck, CryptoQualificationDecision, CryptoQualificationFailure,
};
pub use diagnostics::{
    RuntimeEvidenceProvider, SharedRuntimeEvidenceProvider, SharedWorkspaceCountsProvider,
    WorkspaceCounts, WorkspaceCountsProvider,
};
pub use effect_outcome_delivery::{
    EffectOutcomeDeliveryService, OutcomeDeliveryReport, deliver_effect_outcomes,
};
pub use effect_recovery::{
    DISPATCH_CONSIDERED_LOST_AFTER, ExternalEffectReadBackAdapter, ExternalEffectRecoveryReport,
    ExternalEffectRecoveryService, RECONCILIATION_RETRY_AFTER,
};
pub use effect_repository::{
    ExternalEffectRecoveryCandidate, ExternalEffectRepository, SharedExternalEffectRepository,
    UndeliveredOutcome,
};
pub use error::ApplicationError;
pub use execution_ports::{
    ExecutionArtifactRecord, ExecutionHistoryRepository, ExecutionOutcomeRecord,
    SharedExecutionHistoryRepository, StepExecutionRecord, WorkflowExecutionRecord,
};
pub use external_effects::{ExternalEffectService, PerformExternalEffectService};
pub use fault_admission::ExternalEffectFaultEvidenceAdmissionService;
pub use fault_bundle::ExternalEffectQualificationBundleService;
pub use fault_evidence::{
    ExternalEffectFaultObservationEvidence, ExternalEffectFaultSuiteEvidence,
    FaultSuiteEvidenceRepository,
};
pub use fault_gate_evidence::ExternalEffectFaultGateEvidenceService;
pub use fault_qualification::ExternalEffectFaultQualificationService;
pub use fault_runtime::{
    ConfiguredEffectFaultScenarioExecutor, DockerFaultInjectionRuntime, FaultInjectionDriver,
    FaultInjectionEnvironment, FaultInjectionRuntime, FaultInjectionSettings,
    ProcessFaultInjectionRuntime,
};
pub use fault_suite::{
    EffectFaultScenarioExecutor, ExternalEffectFaultSuiteReport, ExternalEffectFaultSuiteService,
};
pub use health::{
    HealthFindingRepository, HealthInspectionService, HealthMonitorService, HealthRepository,
    InspectedFinding, InvariantObservation, InvariantObserver, MonitoredFinding,
    SharedHealthFindingRepository, SharedInvariantObserver, standard_invariants,
};
pub use idempotency::{IdempotencyRecord, IdempotencyRepository};
pub use identity::{
    AccessTokenAuthenticator, AccessTokenStore, AuthenticatedPrincipal,
    SharedAccessTokenAuthenticator, SharedAccessTokenStore, SharedTokenEntropySource,
    TokenEntropySource,
};
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
pub use outbox::{
    DrainReport, OutboxDispatcher, OutboxHandler, OutboxMessage, OutboxRepository,
    SharedOutboxHandler, SharedOutboxRepository,
};
pub use ports::{TransactionManager, UnitOfWork};
pub use providers::*;
pub use qualification::{
    QualificationRepository, QualificationRuntime, RuntimeQualificationDecision,
    RuntimeQualificationEvidence, SharedQualificationRepository, evaluate_runtime_qualification,
};
pub use recovery::{RecoveryRepository, SharedRecoveryRepository};
pub use recovery_reconciliation::ExternalEffectReconciliationService;
pub use release_approval::{
    ReleaseApprovalDecision, ReleaseApprovalFailure, ReleaseApprovalService,
    ReleaseSignatureEvidence,
};
pub use retrieval::{
    ContextPackBuilder, ExactRetriever, NormalizedRetrievalRequest, RetrievalJournal,
    RetrievalRequest, RetrievalResult, RetrievalService, SharedExactRetriever,
    SharedRetrievalJournal, SharedStructuredRetriever, SharedTextRetriever, SharedVectorRetriever,
    StructuredRetriever, TextRetriever, VectorRetriever, reciprocal_rank_fusion, rerank,
};
pub use runs::*;
pub use secrets::{SecretDescriptor, SecretMaterial, SecretStore, SharedSecretStore};
pub use security::{
    AuditRepository, AuthorizationBoundary, BudgetPolicyEngine, CapabilityGrantRepository,
    ConfiguredCapabilityPolicyEngine, DenyAllPolicyEngine, GrantPolicyEngine, PolicyDecisionEngine,
    PolicyEngine, RedactionRule, RedactionService, SharedAuditRepository,
    SharedCapabilityGrantRepository, SharedHierarchicalBudget, SharedPolicyDecisionEngine,
    StoredGrantPolicyEngine,
};
pub use settings::{
    SharedWorkspaceSettingsRepository, WorkspaceSettingsRepository, WorkspaceSettingsService,
};
pub use triggers::{SharedTriggerRepository, TriggerRepository};
pub use trust_restoration::ProgressiveTrustRestorationService;
pub use v1_release_evidence::{
    ExactEnvironmentReleaseDecision, ExactEnvironmentReleaseEvidence,
    ExactEnvironmentReleaseFailure, ExactEnvironmentReleaseTarget, V1ReleaseEvidenceProbe,
    V1ReleaseEvidenceService,
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
