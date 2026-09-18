use std::sync::Arc;

use async_trait::async_trait;

use crate::{ApplicationError, RequestContext};
use vestrace_domain::{
    ArtifactKind, ExecutionStatus, OutcomeKind, StepKind,
    id::{
        AgentId, AgentRunId, ExecutionArtifactId, ExecutionOutcomeId, ModelExecutionAttemptId,
        StepExecutionId, ToolInvocationId, WorkflowExecutionId, WorkflowId, WorkflowNodeId,
        WorkflowRevisionId, WorkspaceId,
    },
    time::Timestamp,
};

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowExecutionRecord {
    pub id: WorkflowExecutionId,
    pub workspace_id: WorkspaceId,
    pub workflow_id: WorkflowId,
    pub workflow_revision: u32,
    pub workflow_revision_id: WorkflowRevisionId,
    pub status: ExecutionStatus,
    pub attempt: u32,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub run_id: Option<AgentRunId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StepExecutionRecord {
    pub id: StepExecutionId,
    pub workflow_execution_id: WorkflowExecutionId,
    pub workspace_id: WorkspaceId,
    pub node_id: WorkflowNodeId,
    pub node_label: String,
    pub kind: StepKind,
    pub agent_ref: Option<AgentId>,
    pub attempt: u32,
    pub status: ExecutionStatus,
    pub input_artifact_id: Option<ExecutionArtifactId>,
    pub output_artifact_id: Option<ExecutionArtifactId>,
    pub model_attempt_id: Option<ModelExecutionAttemptId>,
    pub tool_invocation_id: Option<ToolInvocationId>,
    pub error_message: Option<String>,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
    pub run_id: Option<AgentRunId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionArtifactRecord {
    pub id: ExecutionArtifactId,
    pub workspace_id: WorkspaceId,
    pub step_execution_id: StepExecutionId,
    pub kind: ArtifactKind,
    pub content_ref: String,
    pub content_type: String,
    pub byte_size: u64,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionOutcomeRecord {
    pub id: ExecutionOutcomeId,
    pub workspace_id: WorkspaceId,
    pub workflow_execution_id: WorkflowExecutionId,
    pub step_execution_id: Option<StepExecutionId>,
    pub outcome_kind: OutcomeKind,
    pub summary: String,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait ExecutionHistoryRepository: Send + Sync {
    async fn start_workflow_execution(
        &self,
        context: &RequestContext,
        record: &WorkflowExecutionRecord,
    ) -> Result<(), ApplicationError>;

    async fn update_workflow_execution_status(
        &self,
        context: &RequestContext,
        id: WorkflowExecutionId,
        status: ExecutionStatus,
        completed_at: Option<Timestamp>,
    ) -> Result<(), ApplicationError>;

    async fn get_workflow_execution(
        &self,
        context: &RequestContext,
        id: WorkflowExecutionId,
    ) -> Result<Option<WorkflowExecutionRecord>, ApplicationError>;

    async fn list_workflow_executions(
        &self,
        context: &RequestContext,
        workflow_id: WorkflowId,
    ) -> Result<Vec<WorkflowExecutionRecord>, ApplicationError>;

    async fn record_step(
        &self,
        context: &RequestContext,
        record: &StepExecutionRecord,
    ) -> Result<(), ApplicationError>;

    async fn update_step_status(
        &self,
        context: &RequestContext,
        id: StepExecutionId,
        status: ExecutionStatus,
        completed_at: Option<Timestamp>,
        error_message: Option<String>,
    ) -> Result<(), ApplicationError>;

    async fn list_steps(
        &self,
        context: &RequestContext,
        workflow_execution_id: WorkflowExecutionId,
    ) -> Result<Vec<StepExecutionRecord>, ApplicationError>;

    async fn record_artifact(
        &self,
        context: &RequestContext,
        record: &ExecutionArtifactRecord,
    ) -> Result<(), ApplicationError>;

    async fn record_outcome(
        &self,
        context: &RequestContext,
        record: &ExecutionOutcomeRecord,
    ) -> Result<(), ApplicationError>;

    async fn list_outcomes(
        &self,
        context: &RequestContext,
        workflow_execution_id: WorkflowExecutionId,
    ) -> Result<Vec<ExecutionOutcomeRecord>, ApplicationError>;
}

pub type SharedExecutionHistoryRepository = Arc<dyn ExecutionHistoryRepository>;
