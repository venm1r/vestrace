use crate::{
    id::{ArtifactId, ArtifactRevisionId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStatus {
    Quarantined,
    Active,
    Archived,
    Purged,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Artifact {
    pub id: ArtifactId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub status: ArtifactStatus,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactRevision {
    pub id: ArtifactRevisionId,
    pub artifact_id: ArtifactId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub media_type: String,
    pub content_hash: String,
    pub byte_size: u64,
    pub created_at: Timestamp,
}
