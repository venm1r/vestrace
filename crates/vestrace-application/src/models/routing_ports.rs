use std::sync::Arc;

use async_trait::async_trait;

use crate::{ApplicationError, RequestContext};
use vestrace_domain::{
    id::{ModelExecutionId, ModelId, RoutingDecisionId, WorkspaceId},
    time::Timestamp,
};

#[derive(Clone, Debug, PartialEq)]
pub struct RoutingDecisionRecord {
    pub id: RoutingDecisionId,
    pub workspace_id: WorkspaceId,
    pub selected_model_id: Option<ModelId>,
    pub intent: String,
    pub rationale: String,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait RoutingDecisionRepository: Send + Sync {
    async fn record(
        &self,
        context: &RequestContext,
        decision: &RoutingDecisionRecord,
    ) -> Result<(), ApplicationError>;
    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<RoutingDecisionRecord>, ApplicationError>;
}

pub type SharedRoutingDecisionRepository = Arc<dyn RoutingDecisionRepository>;

#[derive(Clone, Debug, PartialEq)]
pub struct ModelExecutionRecord {
    pub id: ModelExecutionId,
    pub workspace_id: WorkspaceId,
    pub model_id: ModelId,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub latency_ms: u32,
    pub status: String,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait ModelExecutionRepository: Send + Sync {
    async fn record(
        &self,
        context: &RequestContext,
        execution: &ModelExecutionRecord,
    ) -> Result<(), ApplicationError>;
    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<ModelExecutionRecord>, ApplicationError>;
}

pub type SharedModelExecutionRepository = Arc<dyn ModelExecutionRepository>;
