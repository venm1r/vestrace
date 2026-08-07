use async_trait::async_trait;

use crate::WorkflowExecutionRecord;
use crate::{
    ApplicationError, EvaluationRecord, EvaluationRepository, ExecutionArtifactRecord,
    ExecutionHistoryRepository, ExecutionOutcomeRecord, RequestContext, StepExecutionRecord,
    WorkflowDefinitionRecord, WorkflowRepository, WorkflowRevisionRecord,
};
use vestrace_domain::{
    ExecutionStatus,
    id::{EvaluationId, StepExecutionId, WorkflowExecutionId, WorkflowId},
    time::Timestamp,
};

pub struct NullExecutionHistoryRepository;

impl NullExecutionHistoryRepository {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NullExecutionHistoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionHistoryRepository for NullExecutionHistoryRepository {
    async fn start_workflow_execution(
        &self,
        _context: &RequestContext,
        _record: &WorkflowExecutionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn update_workflow_execution_status(
        &self,
        _context: &RequestContext,
        _id: WorkflowExecutionId,
        _status: ExecutionStatus,
        _completed_at: Option<Timestamp>,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn get_workflow_execution(
        &self,
        _context: &RequestContext,
        _id: WorkflowExecutionId,
    ) -> Result<Option<WorkflowExecutionRecord>, ApplicationError> {
        Ok(None)
    }

    async fn list_workflow_executions(
        &self,
        _context: &RequestContext,
        _workflow_id: WorkflowId,
    ) -> Result<Vec<WorkflowExecutionRecord>, ApplicationError> {
        Ok(vec![])
    }

    async fn record_step(
        &self,
        _context: &RequestContext,
        _record: &StepExecutionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn update_step_status(
        &self,
        _context: &RequestContext,
        _id: StepExecutionId,
        _status: ExecutionStatus,
        _completed_at: Option<Timestamp>,
        _error_message: Option<String>,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn list_steps(
        &self,
        _context: &RequestContext,
        _workflow_execution_id: WorkflowExecutionId,
    ) -> Result<Vec<StepExecutionRecord>, ApplicationError> {
        Ok(vec![])
    }

    async fn record_artifact(
        &self,
        _context: &RequestContext,
        _record: &ExecutionArtifactRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn record_outcome(
        &self,
        _context: &RequestContext,
        _record: &ExecutionOutcomeRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn list_outcomes(
        &self,
        _context: &RequestContext,
        _workflow_execution_id: WorkflowExecutionId,
    ) -> Result<Vec<ExecutionOutcomeRecord>, ApplicationError> {
        Ok(vec![])
    }
}

#[async_trait]
impl WorkflowRepository for NullExecutionHistoryRepository {
    async fn create(
        &self,
        _context: &RequestContext,
        _record: &WorkflowDefinitionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn list(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<WorkflowDefinitionRecord>, ApplicationError> {
        Ok(vec![])
    }

    async fn find_by_id(
        &self,
        _context: &RequestContext,
        _id: WorkflowId,
    ) -> Result<Option<WorkflowDefinitionRecord>, ApplicationError> {
        Ok(None)
    }

    async fn save_revision(
        &self,
        _context: &RequestContext,
        _record: &WorkflowRevisionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn get_revision(
        &self,
        _context: &RequestContext,
        _workflow_id: WorkflowId,
        _revision_number: u32,
    ) -> Result<Option<WorkflowRevisionRecord>, ApplicationError> {
        Ok(None)
    }
}

#[async_trait]
impl EvaluationRepository for NullExecutionHistoryRepository {
    async fn create(
        &self,
        _context: &RequestContext,
        _record: &EvaluationRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }

    async fn list(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<EvaluationRecord>, ApplicationError> {
        Ok(vec![])
    }

    async fn find_by_id(
        &self,
        _context: &RequestContext,
        _id: EvaluationId,
    ) -> Result<Option<EvaluationRecord>, ApplicationError> {
        Ok(None)
    }
}
