use crate::{
    id::{
        AgentRunId, ApprovalRecordId, CorrelationId, OperationId, PrincipalId, RequestId,
        RunEventId, RunStepId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::{RunActor, RunVersion, StallReason};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PendingRunEvent {
    pub event: RunEvent,
    pub occurred_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunEventEnvelope {
    pub event_id: RunEventId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub sequence: RunVersion,
    pub event_type: String,
    pub event_version: u16,
    pub actor: RunActor,
    pub causation_id: OperationId,
    pub correlation_id: CorrelationId,
    pub payload: RunEvent,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunEvent {
    Created {
        principal_id: PrincipalId,
        title: String,
    },
    MarkedReady,
    Started,
    StepStarted {
        step_id: RunStepId,
        kind: String,
        label: Option<String>,
    },
    StepCompleted {
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
    Resumed,
    Completed {
        summary: Option<String>,
    },
    Failed {
        code: String,
        message: String,
        retryable: bool,
    },
    Cancelled {
        reason: Option<String>,
    },
    Stalled {
        reason: StallReason,
    },
}

impl RunEvent {
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Created { .. } => "run.created",
            Self::MarkedReady => "run.marked_ready",
            Self::Started => "run.started",
            Self::StepStarted { .. } => "step.started",
            Self::StepCompleted { .. } => "step.completed",
            Self::StepFailed { .. } => "step.failed",
            Self::WaitingForInput { .. } => "run.waiting_for_input",
            Self::WaitingForApproval { .. } => "run.waiting_for_approval",
            Self::Resumed => "run.resumed",
            Self::Completed { .. } => "run.completed",
            Self::Failed { .. } => "run.failed",
            Self::Cancelled { .. } => "run.cancelled",
            Self::Stalled { .. } => "run.stalled",
        }
    }

    pub const fn event_version(&self) -> u16 {
        1
    }
}
