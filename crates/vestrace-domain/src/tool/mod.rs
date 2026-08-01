use crate::{
    id::{ToolDefinitionId, ToolInvocationId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolBindingKind {
    Native,
    Http,
    Mcp,
    SandboxCommand,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolDefinition {
    pub id: ToolDefinitionId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: String,
    pub binding_kind: ToolBindingKind,
    pub parameters_schema: serde_json::Value,
    pub created_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationStatus {
    Prepared,
    Committed,
    Verified,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolInvocation {
    pub id: ToolInvocationId,
    pub tool_id: ToolDefinitionId,
    pub workspace_id: WorkspaceId,
    pub arguments: serde_json::Value,
    pub status: ToolInvocationStatus,
    pub output: Option<serde_json::Value>,
    pub created_at: Timestamp,
}
