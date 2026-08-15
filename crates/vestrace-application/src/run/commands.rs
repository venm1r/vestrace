use vestrace_domain::{
    id::{AgentRunId, AgentRuntimeSnapshotId, RunEventId, RunStepId},
    run::{
        ParentRunLink, RunActorRef, RunCheckpointPayloadV1, RunFailure, RunReference, RunStatus,
        RunStepStatus, RunTerminalResult, RunVersion,
    },
};

#[derive(Clone, Debug)]
pub struct CreateRun {
    /// The caller's correlation id, so an HTTP request can be tied to the run
    /// events it caused. Absent means "no caller supplied one", and a fresh id
    /// is minted rather than leaving the event uncorrelated.
    pub correlation_id: Option<vestrace_domain::id::CorrelationId>,
    pub objective: String,
    pub coordinator_snapshot_id: AgentRuntimeSnapshotId,
    pub execution_mode: vestrace_domain::run::RunExecutionMode,
    pub parent: Option<ParentRunLink>,
    pub idempotency_key: String,
}

pub struct TransitionRun {
    /// The caller's correlation id, so an HTTP request can be tied to the run
    /// events it caused. Absent means "no caller supplied one", and a fresh id
    /// is minted rather than leaving the event uncorrelated.
    pub correlation_id: Option<vestrace_domain::id::CorrelationId>,
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub target: RunStatus,
    pub result: Option<RunTerminalResult>,
    pub actor: RunActorRef,
    pub causation_event_id: Option<RunEventId>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct AddRunSteps {
    /// The caller's correlation id, so an HTTP request can be tied to the run
    /// events it caused. Absent means "no caller supplied one", and a fresh id
    /// is minted rather than leaving the event uncorrelated.
    pub correlation_id: Option<vestrace_domain::id::CorrelationId>,
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub steps: Vec<NewRunStepDto>,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct NewRunStepDto {
    pub id: RunStepId,
    pub plan_step_reference: Option<String>,
    pub assigned_actor: RunActorRef,
    pub input_references: Vec<RunReference>,
}

pub struct TransitionRunStep {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub expected_version: RunVersion,
    pub target: RunStepStatus,
    pub failure: Option<RunFailure>,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

pub struct CreateCheckpoint {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub payload: RunCheckpointPayloadV1,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

pub struct PauseRun {
    /// The caller's correlation id, so an HTTP request can be tied to the run
    /// events it caused. Absent means "no caller supplied one", and a fresh id
    /// is minted rather than leaving the event uncorrelated.
    pub correlation_id: Option<vestrace_domain::id::CorrelationId>,
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

pub struct ResumeRun {
    /// The caller's correlation id, so an HTTP request can be tied to the run
    /// events it caused. Absent means "no caller supplied one", and a fresh id
    /// is minted rather than leaving the event uncorrelated.
    pub correlation_id: Option<vestrace_domain::id::CorrelationId>,
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

/// Grant a pending approval.
///
/// Carries `approver_id` separately from `actor`: the actor is who issued the
/// HTTP request, and the approver is who the record says granted it. They are
/// the same principal today, but conflating them in the type would make it
/// impossible to represent an approval granted on someone's behalf without
/// silently misattributing it.
pub struct ApproveRun {
    /// The caller's correlation id, so an HTTP request can be tied to the run
    /// events it caused. Absent means "no caller supplied one", and a fresh id
    /// is minted rather than leaving the event uncorrelated.
    pub correlation_id: Option<vestrace_domain::id::CorrelationId>,
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub approver_id: vestrace_domain::id::PrincipalId,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

pub struct CancelRun {
    /// The caller's correlation id, so an HTTP request can be tied to the run
    /// events it caused. Absent means "no caller supplied one", and a fresh id
    /// is minted rather than leaving the event uncorrelated.
    pub correlation_id: Option<vestrace_domain::id::CorrelationId>,
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub reason: Option<String>,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}
