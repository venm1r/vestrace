use vestrace_domain::{id::OutboxId, time::Timestamp, WorkspaceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub id: OutboxId,
    pub workspace_id: WorkspaceId,
    pub topic: String,
    pub payload: serde_json::Value,
    pub created_at: Timestamp,
}
