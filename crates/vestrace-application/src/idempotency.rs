use crate::ApplicationError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use vestrace_domain::{WorkspaceId, time::Timestamp};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub idempotency_key: String,
    pub workspace_id: WorkspaceId,
    pub request_hash: String,
    pub response_payload: Option<serde_json::Value>,
    pub status: String,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

#[async_trait]
pub trait IdempotencyRepository: Send + Sync {
    async fn find_by_key(
        &self,
        workspace_id: WorkspaceId,
        key: &str,
    ) -> Result<Option<IdempotencyRecord>, ApplicationError>;

    async fn save(&self, record: &IdempotencyRecord) -> Result<(), ApplicationError>;
}
