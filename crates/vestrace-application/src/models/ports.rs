use std::sync::Arc;

use async_trait::async_trait;

use crate::{ApplicationError, RequestContext};
use vestrace_domain::{
    ProviderLocality,
    id::{ModelId, ProviderId, WorkspaceId},
    time::Timestamp,
};

pub use super::bindings::{
    LEGACY_RUN_MODEL_DEFAULT_PURPOSE, ModelBindingResolver, SharedModelBindingResolver,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRecord {
    pub id: ProviderId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub locality: ProviderLocality,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelRecord {
    pub id: ModelId,
    pub provider_id: ProviderId,
    pub workspace_id: WorkspaceId,
    pub model_name: String,
    pub context_window: u32,
    pub input_cost_per_mtoken: f32,
    pub output_cost_per_mtoken: f32,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait ProviderRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        id: ProviderId,
        name: &str,
        locality: ProviderLocality,
    ) -> Result<(), ApplicationError>;
    async fn list(&self, context: &RequestContext)
    -> Result<Vec<ProviderRecord>, ApplicationError>;
}

#[async_trait]
pub trait ModelRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        model: &ModelRecord,
    ) -> Result<(), ApplicationError>;
    async fn list(&self, context: &RequestContext) -> Result<Vec<ModelRecord>, ApplicationError>;
    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: ModelId,
    ) -> Result<Option<ModelRecord>, ApplicationError>;
}

pub type SharedProviderRepository = Arc<dyn ProviderRepository>;
pub type SharedModelRepository = Arc<dyn ModelRepository>;
