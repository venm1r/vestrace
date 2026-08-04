use serde_json::Value;
use vestrace_domain::{
    id::{AgentRunId, OperationId, PrincipalId, WorkspaceId},
    run::{AgentRun, RunEventEnvelope, RunVersion},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateRunCommand {
    pub title: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunCommandReceipt {
    pub command_id: OperationId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub command_type: String,
    pub idempotency_key: Option<String>,
    pub request_payload: Value,
    pub run_id: AgentRunId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunCommandCommitOutcome {
    Committed(RunVersion),
    Replayed(AgentRun),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunCommandResult {
    pub run: AgentRun,
    pub events: Vec<RunEventEnvelope>,
    pub replayed: bool,
}
