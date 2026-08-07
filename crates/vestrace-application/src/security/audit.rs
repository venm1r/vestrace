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
}

pub type SharedAuditRepository = Arc<dyn AuditRepository>;
