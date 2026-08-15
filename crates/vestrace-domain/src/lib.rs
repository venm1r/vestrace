#![forbid(unsafe_code)]

pub mod a2a;
pub mod ag_ui;
pub mod artifact;
pub mod budget;
pub mod claim;
pub mod cognitive;
pub mod conformance;
pub mod connection;
pub mod conversation;
pub mod enterprise;
pub mod error;
pub mod evaluation;
pub mod event;
pub mod execution;
pub mod external_effects;
pub mod health;
pub mod id;
pub mod identity;
pub mod job;
pub mod learning;
pub mod memory;
pub mod models;
pub mod observability;
pub mod package;
pub mod planning;
pub mod policy;
pub mod product;
pub mod provenance;
pub mod relation;
pub mod release;
pub mod retrieval;
pub mod run;
pub mod security;
pub mod settings;
pub mod state_engine;
pub mod time;
pub mod tool;
pub mod trust;
pub mod webhook;

pub use claim::{
    AssessmentKind, Claim, ClaimAssessment, ClaimEvidenceLink, ClaimStatus, CognitiveMutation,
    Conflict, ConflictKind, ConflictStatus, MutationKind, MutationTargetKind, ReconciliationClass,
    ReconciliationOutcome, ReconciliationRecord, SourceClassification, SupersessionLink,
    SupersessionTargetKind,
};
pub use cognitive::{
    AgentDefinition, AgentRevision, AgentRole, BudgetPolicy, CapabilitySet,
    CompositeImplementation, HttpImplementation, HumanImplementation, LoopPolicy,
    McpToolImplementation, MemoryScope, PromptImplementation, SkillDefinition, SkillDependency,
    SkillExample, SkillImplementation, SkillKind, SkillRevision, WorkflowDefinition, WorkflowNode,
    WorkflowNodeKind, WorkflowRevision, WorkflowTransition, WorkflowValidationError,
    WorkflowValidationReport,
};
pub use error::DomainError;
pub use evaluation::{
    EvaluationAuthority, EvaluationFact, EvaluationMetric, EvaluationResult, EvaluationTarget,
    EvaluatorKind, EvaluatorRef,
};
pub use event::{ActorRef, Event, SubjectRef};
pub use execution::{
    ArtifactKind, ExecutionArtifact, ExecutionOutcome, ExecutionStatus, OutcomeKind, StepExecution,
    StepKind, WorkflowExecution,
};
pub use external_effects::{
    AdapterContractError, AdapterDispatchResult, AdapterError, DeliverySemantics, DryRunMode,
    EffectAuthorization, EffectFaultPoint, EffectLifecycleStatus, EffectPrecondition,
    EffectReversibility, ExternalEffectAdapter, ExternalEffectAdapterDescriptor,
    ExternalEffectIntent, ExternalEffectReceipt, FaultObservation, IdempotencyProfile,
    RetryDecision,
};
pub use health::{
    FindingDisposition, FindingLifecycleStatus, HealthFinding, HealthOccurrence, HealthProjection,
    HealthProjectionState, HealthScope, HealthSeverity, HealthState, InvariantDefinition,
    InvariantRegistry, RepairExecution, RepairExecutionError, RepairExecutionResult, RepairPlan,
    RepairRisk, Repairability, VerificationResult, VerificationRun,
};
pub use id::*;
pub use job::{Job, JobState};
pub use learning::{
    LearnedProjection, LearningChange, LearningProposal, LearningProposalStatus, LearningTarget,
    ProjectionAuthority, ProjectionGenerator, ProjectionKind,
};
pub use memory::*;
pub use models::{
    ModelCostProfile, ModelProfile, ModelRouter, ProviderLocality, RejectedCandidate,
    RoutingCandidate, RoutingDecision, RoutingStrategy, TaskRequirements,
};
pub use policy::{ActivationDecision, MemoryWritePolicy};
pub use provenance::{Derivation, DerivationMethod, EvidenceRef, EvidenceRole, MemorySource};
pub use relation::{KnowledgeRelation, RelationType};
pub use release::VestraceCapabilityManifest;
pub use retrieval::{
    ContextItem, ContextPack, ContextSection, RepresentationLevel, RetrievalCandidate,
    RetrievalIntent, ScoreComponents, TimePerspective,
};
pub use security::{
    ApprovalKind, ApprovalRecord, ApprovalStatus, AuditEvent, AuthorizationRequest,
    BudgetConstraint, BudgetReservation, Capability, CapabilityGrant, CapabilityGrantSpec,
    CapabilityGrantStatus, DELEGATION_OPERATION, DataDestination, DelegatedCapability,
    DelegationContract, GrantCondition, HierarchicalBudget, MAX_DELEGATION_DEPTH, PolicyDecision,
    PolicyDecisionReason, PolicyDecisionResult, PolicyInputState, ResourceKind, ResourceScope,
    RiskCategory, Sensitivity, evaluate_capability_grants,
};
pub use settings::{LogLevel, WorkspaceSettings};
pub use time::{Timestamp, now};
pub use trust::*;

pub fn crate_name() -> &'static str {
    "vestrace-domain"
}

#[cfg(test)]
mod tests {
    use super::WorkspaceId;

    #[test]
    fn crate_is_linkable() {
        assert_eq!(super::crate_name(), "vestrace-domain");
    }

    #[test]
    fn workspace_id_round_trips_as_string() {
        let id = WorkspaceId::new();

        assert_eq!(id.to_string().parse::<WorkspaceId>().unwrap(), id);
    }
}
