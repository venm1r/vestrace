use crate::{ApplicationError, RequestContext, UnitOfWork};
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
        context: &RequestContext,
        key: &str,
    ) -> Result<Option<IdempotencyRecord>, ApplicationError>;

    async fn save(
        &self,
        context: &RequestContext,
        record: &IdempotencyRecord,
    ) -> Result<(), ApplicationError>;

    /// Persist a key inside a transaction the caller already owns.
    async fn save_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        record: &IdempotencyRecord,
    ) -> Result<(), ApplicationError>;
}
