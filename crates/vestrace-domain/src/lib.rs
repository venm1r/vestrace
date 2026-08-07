#![forbid(unsafe_code)]

pub mod a2a;
pub mod ag_ui;
pub mod artifact;
pub mod budget;
pub mod cognitive;
pub mod connection;
pub mod conversation;
pub mod diagnostics;
pub mod enterprise;
pub mod error;
pub mod event;
pub mod execution;
pub mod id;
pub mod job;
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
pub mod state_engine;
pub mod time;
pub mod tool;
pub mod webhook;

pub use cognitive::{
    AgentDefinition, AgentRevision, AgentRole, BudgetPolicy, CapabilitySet,
    CompositeImplementation, HttpImplementation, HumanImplementation, LoopPolicy,
    McpToolImplementation, MemoryScope, PromptImplementation, SkillDefinition, SkillDependency,
    SkillExample, SkillImplementation, SkillKind, SkillRevision, WorkflowDefinition, WorkflowNode,
    WorkflowNodeKind, WorkflowRevision, WorkflowTransition, WorkflowValidationError,
    WorkflowValidationReport,
};
pub use error::DomainError;
pub use event::{ActorRef, Event, SubjectRef};
pub use execution::{
    ArtifactKind, ExecutionArtifact, ExecutionOutcome, ExecutionStatus, OutcomeKind, StepExecution,
    StepKind, WorkflowExecution,
};
pub use id::*;
pub use job::{Job, JobState};
pub use memory::*;
pub use models::{
    ModelCostProfile, ModelProfile, ModelRouter, ProviderLocality, RejectedCandidate,
    RoutingCandidate, RoutingDecision, RoutingStrategy, TaskRequirements,
};
pub use policy::{ActivationDecision, MemoryWritePolicy};
pub use provenance::{Derivation, DerivationMethod, EvidenceRole, MemorySource};
pub use relation::{KnowledgeRelation, RelationType};
pub use retrieval::{
    ContextItem, ContextPack, ContextSection, RepresentationLevel, RetrievalCandidate,
    RetrievalIntent, ScoreComponents, TimePerspective,
};
pub use security::{
    ApprovalKind, ApprovalRecord, ApprovalStatus, AuditEvent, Capability, DataDestination,
    Sensitivity,
};
pub use time::{Timestamp, now};

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
