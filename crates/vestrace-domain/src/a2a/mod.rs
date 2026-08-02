use crate::{
    id::{RemoteAgentInvocationId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvocationStatus {
    Initiated,
    Pending,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoteAgentInvocation {
    pub id: RemoteAgentInvocationId,
    pub workspace_id: WorkspaceId,
    pub target_agent_url: String,
    pub protocol_version: String,
    pub status: InvocationStatus,
    pub payload: serde_json::Value,
    pub created_at: Timestamp,
}
