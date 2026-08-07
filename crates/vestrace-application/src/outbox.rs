use crate::ApplicationError;
use async_trait::async_trait;
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

#[async_trait]
pub trait OutboxRepository: Send + Sync {
    async fn save(&self, message: &OutboxMessage) -> Result<(), ApplicationError>;
    async fn claim_pending(&self, limit: u32) -> Result<Vec<OutboxMessage>, ApplicationError>;
    async fn mark_processed(&self, id: OutboxId) -> Result<(), ApplicationError>;
}
