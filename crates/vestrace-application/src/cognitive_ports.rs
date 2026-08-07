use std::sync::Arc;

use async_trait::async_trait;

use crate::{ApplicationError, RequestContext};
use vestrace_domain::{
    id::{ModelId, WorkflowId, WorkflowRevisionId, WorkspaceId},
    time::Timestamp,
};

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowDefinitionRecord {
    pub id: WorkflowId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub current_revision: u32,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowRevisionRecord {
    pub revision_id: WorkflowRevisionId,
    pub workflow_id: WorkflowId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub definition: serde_json::Value,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait WorkflowRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), ApplicationError>;

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<WorkflowDefinitionRecord>, ApplicationError>;

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: WorkflowId,
    ) -> Result<Option<WorkflowDefinitionRecord>, ApplicationError>;

    async fn save_revision(
        &self,
        context: &RequestContext,
        record: &WorkflowRevisionRecord,
    ) -> Result<(), ApplicationError>;

    async fn get_revision(
        &self,
        context: &RequestContext,
        workflow_id: WorkflowId,
        revision_number: u32,
    ) -> Result<Option<WorkflowRevisionRecord>, ApplicationError>;
}

pub type SharedWorkflowRepository = Arc<dyn WorkflowRepository>;

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationRecord {
    pub id: vestrace_domain::id::EvaluationId,
    pub workspace_id: WorkspaceId,
    pub model_id: Option<ModelId>,
    pub name: String,
    pub status: String,
    pub score: Option<f64>,
    pub summary: Option<String>,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait EvaluationRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        record: &EvaluationRecord,
    ) -> Result<(), ApplicationError>;

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<EvaluationRecord>, ApplicationError>;

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: vestrace_domain::id::EvaluationId,
    ) -> Result<Option<EvaluationRecord>, ApplicationError>;
}

pub type SharedEvaluationRepository = Arc<dyn EvaluationRepository>;
