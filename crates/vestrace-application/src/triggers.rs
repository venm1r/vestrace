use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::conversation::ExternalTrigger;

use crate::{ApplicationError, RequestContext};

/// Registered external triggers for a workspace.
///
/// A registry only: it records which triggers exist and whether they are
/// enabled. Nothing here dispatches them — no scheduler or webhook receiver
/// consults this table yet.
#[async_trait]
pub trait TriggerRepository: Send + Sync {
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<ExternalTrigger>, ApplicationError>;
}

pub type SharedTriggerRepository = Arc<dyn TriggerRepository>;
