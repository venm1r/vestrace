use crate::{
    id::{AgentRunId, InteractionSessionId, ProductReleaseId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

pub mod transfer;

pub use transfer::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProductRelease {
    pub id: ProductReleaseId,
    pub version: String,
    pub manifest: serde_json::Value,
    pub created_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InteractionSession {
    pub id: InteractionSessionId,
    pub workspace_id: WorkspaceId,
    pub run_id: Option<AgentRunId>,
    pub client_type: String,
    pub status: SessionStatus,
    pub created_at: Timestamp,
}
