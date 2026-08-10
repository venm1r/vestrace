use crate::{
    id::{
        AgentRunId, ApprovalRecordId, CorrelationId, OperationId, PrincipalId, RequestId,
        RunEventId, RunStepId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::RunActor;
use super::RunVersion;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PendingRunEvent {
    pub event: LegacyRunEvent,
    pub occurred_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LegacyRunEventEnvelope {
    pub event_id: RunEventId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub sequence: RunVersion,
    pub event_type: String,
    pub event_version: u16,
    pub actor: RunActor,
    pub causation_id: OperationId,
    pub correlation_id: CorrelationId,
    pub payload: LegacyRunEvent,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegacyRunEvent {
    Created {
        principal_id: PrincipalId,
        title: String,
    },
    Prepared,
    Started,
    StepStarted {
        step_id: RunStepId,
        kind: String,
        label: Option<String>,
    },
    StepSucceeded {
        step_id: RunStepId,
        output_references: Vec<String>,
    },
    StepFailed {
        step_id: RunStepId,
        code: String,
        message: String,
        retryable: bool,
    },
    WaitingForInput {
        request_id: RequestId,
        prompt: String,
    },
    WaitingForApproval {
        approval_id: ApprovalRecordId,
    },
    WaitingForDependency {
        dependency_run_id: AgentRunId,
    },
    Resumed,
    Succeeded {
        summary: Option<String>,
    },
    SucceededWithWarnings {
        summary: String,
        warnings: Vec<String>,
    },
    PartialCompleted {
        summary: String,
        remaining_work: Vec<String>,
    },
    Failed {
        code: String,
        message: String,
        retryable: bool,
    },
    Cancelled {
        reason: Option<String>,
    },
    Paused,
    Expired,
}

impl LegacyRunEvent {
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Created { .. } => "run.created",
            Self::Prepared => "run.prepared",
            Self::Started => "run.started",
            Self::StepStarted { .. } => "step.started",
            Self::StepSucceeded { .. } => "step.succeeded",
            Self::StepFailed { .. } => "step.failed",
            Self::WaitingForInput { .. } => "run.waiting_for_input",
            Self::WaitingForApproval { .. } => "run.waiting_for_approval",
            Self::WaitingForDependency { .. } => "run.waiting_for_dependency",
            Self::Resumed => "run.resumed",
            Self::Succeeded { .. } => "run.succeeded",
            Self::SucceededWithWarnings { .. } => "run.succeeded_with_warnings",
            Self::PartialCompleted { .. } => "run.partial_completed",
            Self::Failed { .. } => "run.failed",
            Self::Cancelled { .. } => "run.cancelled",
            Self::Paused => "run.paused",
            Self::Expired => "run.expired",
        }
    }

    pub const fn event_version(&self) -> u16 {
        1
    }
}
