use crate::{
    id::{InteractionSessionId, AgentRunId, ProductReleaseId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProductRelease {
    pub id: ProductReleaseId,
    pub version: String,
    pub manifest: serde_json::Value,
    pub created_at: Timestamp,
}

pub mod transfer;

pub use transfer::*;
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
