use crate::{
    id::{AgentRunId, ApprovalRecordId, JobId, PrincipalId, RequestId, RunStepId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::status::RunStepStatus;
use super::{RunStatus, RunVersion};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunActor {
    Principal(PrincipalId),
    System { component: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StallReason {
    NoProgress,
    VersionConflictLimit,
    InvalidState,
    ManualInterventionRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunCompletion {
    Succeeded {
        summary: Option<String>,
    },
    SucceededWithWarnings {
        summary: String,
        warnings: Vec<String>,
    },
    Partial {
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
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunWait {
    Input {
        request_id: RequestId,
        prompt: String,
    },
    Approval {
        approval_id: ApprovalRecordId,
    },
    Job {
        job_id: JobId,
    },
    Event {
        event_type: String,
        correlation_id: crate::id::CorrelationId,
    },
    Timer {
        resume_at: Timestamp,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunStepState {
    pub id: RunStepId,
    pub kind: String,
    pub label: Option<String>,
    pub status: RunStepStatus,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunState {
    pub id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub title: String,
    pub status: RunStatus,
    pub version: RunVersion,
    pub active_step: Option<RunStepState>,
    pub wait: Option<RunWait>,
    pub completion: Option<RunCompletion>,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub updated_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}
