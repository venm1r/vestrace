use crate::{ApplicationError, RequestContext};
use async_trait::async_trait;
use std::sync::Arc;
use vestrace_domain::AuditEvent;

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn record(
        &self,
        context: &RequestContext,
        event: &AuditEvent,
    ) -> Result<(), ApplicationError>;

    /// Most recent events first, capped by `limit`.
    ///
    /// The audit trail is append-only, so reading it never mutates it and a
    /// caller is free to page by asking for fewer rows.
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AuditEvent>, ApplicationError>;
}

pub type SharedAuditRepository = Arc<dyn AuditRepository>;
