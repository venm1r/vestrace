use vestrace_domain::{
    id::{AgentRunId, AgentRuntimeSnapshotId, RunEventId, RunStepId},
    run::{
        ParentRunLink, RunActorRef, RunCheckpointPayloadV1, RunFailure, RunReference, RunStatus,
        RunStepStatus, RunTerminalResult, RunVersion,
    },
};

pub struct CreateRun {
    pub objective: String,
    pub coordinator_snapshot_id: AgentRuntimeSnapshotId,
    pub execution_mode: vestrace_domain::run::RunExecutionMode,
    pub parent: Option<ParentRunLink>,
    pub idempotency_key: String,
}

pub struct TransitionRun {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub target: RunStatus,
    pub result: Option<RunTerminalResult>,
    pub actor: RunActorRef,
    pub causation_event_id: Option<RunEventId>,
    pub idempotency_key: String,
}

pub struct AddRunSteps {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub steps: Vec<NewRunStepDto>,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

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
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

pub struct ResumeRun {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}

pub struct CancelRun {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub reason: Option<String>,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}
