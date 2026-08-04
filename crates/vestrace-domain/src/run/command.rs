use crate::{
    id::{
        AgentRunId, ApprovalRecordId, CorrelationId, OperationId, PrincipalId, RequestId,
        RunStepId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::{RunActor, RunVersion, StallReason};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunCommandEnvelope {
    pub command_id: OperationId,
    pub idempotency_key: Option<String>,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub actor: RunActor,
    pub expected_version: RunVersion,
    pub correlation_id: CorrelationId,
    pub issued_at: Timestamp,
    pub command: RunCommand,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunCommand {
    Create {
        principal_id: PrincipalId,
        title: String,
    },
    MarkReady,
    Start,
    StartStep {
        step_id: RunStepId,
        kind: String,
        label: Option<String>,
    },
    CompleteStep {
        step_id: RunStepId,
        output_references: Vec<String>,
    },
    FailStep {
        step_id: RunStepId,
        code: String,
        message: String,
        retryable: bool,
    },
    WaitForInput {
        request_id: RequestId,
        prompt: String,
    },
    WaitForApproval {
        approval_id: ApprovalRecordId,
    },
    Resume,
    Complete {
        summary: Option<String>,
    },
    Fail {
        code: String,
        message: String,
        retryable: bool,
    },
    Cancel {
        reason: Option<String>,
    },
    MarkStalled {
        reason: StallReason,
    },
}
