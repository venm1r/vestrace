use serde::{Deserialize, Serialize};
use vestrace_domain::{WorkspaceId, id::OutboxId, time::Timestamp};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub id: OutboxId,
    pub workspace_id: WorkspaceId,
    pub topic: String,
    pub payload: serde_json::Value,
    pub created_at: Timestamp,
}
