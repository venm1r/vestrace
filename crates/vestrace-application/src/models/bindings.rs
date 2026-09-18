use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{ModelBindingSnapshot, id::AgentRunId};

use crate::{ApplicationError, RequestContext, UnitOfWork};

/// The legacy Run path resolves only this conversational workspace default.
///
/// Legacy create-run commands carry no model selection, so this is deliberately
/// a temporary compatibility rule rather than permanent product policy. It is
/// expected to disappear when Runs carry an explicit model selection.
pub const LEGACY_RUN_MODEL_DEFAULT_PURPOSE: &str = "chat";

/// Resolves and durably pins the model binding for legacy Run acceptance.
#[async_trait]
pub trait ModelBindingResolver: Send + Sync {
    async fn resolve_for_run_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        run_id: AgentRunId,
    ) -> Result<ModelBindingSnapshot, ApplicationError>;
}

pub type SharedModelBindingResolver = Arc<dyn ModelBindingResolver>;
