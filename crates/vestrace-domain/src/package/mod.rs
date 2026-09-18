use crate::{
    id::{AgentPackageId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentPackage {
    pub id: AgentPackageId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub manifest: serde_json::Value,
    pub created_at: Timestamp,
}
