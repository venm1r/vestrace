use crate::{
    id::{KekId, MemoryGrantId, MemoryId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

mod federation;
mod sharing;

pub use federation::*;
pub use sharing::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceKek {
    pub id: KekId,
    pub workspace_id: WorkspaceId,
    pub key_alias: String,
    pub algorithm: String,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CrossWorkspaceMemoryGrant {
    pub id: MemoryGrantId,
    pub owner_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub memory_id: MemoryId,
    pub granted_at: Timestamp,
}
