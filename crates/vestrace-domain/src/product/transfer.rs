use crate::{
    id::{ArtifactId, ProductApiTransferId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferType {
    Upload,
    Download,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Initiated,
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProductApiTransfer {
    pub id: ProductApiTransferId,
    pub workspace_id: WorkspaceId,
    pub artifact_id: Option<ArtifactId>,
    pub transfer_type: TransferType,
    pub status: TransferStatus,
    pub byte_offset: u64,
    pub total_bytes: u64,
    pub created_at: Timestamp,
}
