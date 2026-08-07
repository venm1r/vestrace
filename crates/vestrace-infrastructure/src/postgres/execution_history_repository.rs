use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{
    ApplicationError, ExecutionArtifactRecord, ExecutionHistoryRepository, ExecutionOutcomeRecord,
    RequestContext, StepExecutionRecord, WorkflowExecutionRecord,
};
use vestrace_domain::{
    ArtifactKind, ExecutionStatus, OutcomeKind, StepKind,
    id::{
        AgentId, ExecutionArtifactId, ExecutionOutcomeId, ModelExecutionAttemptId, StepExecutionId,
        ToolInvocationId, WorkflowExecutionId, WorkflowId, WorkflowNodeId, WorkflowRevisionId,
        WorkspaceId,
    },
};

pub struct PgExecutionHistoryRepository {
    pool: PgPool,
}

impl PgExecutionHistoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn parse_status(s: &str) -> ExecutionStatus {
    match s {
        "queued" => ExecutionStatus::Queued,
        "running" => ExecutionStatus::Running,
        "waiting" => ExecutionStatus::Waiting,
        "succeeded" => ExecutionStatus::Succeeded,
        "failed" => ExecutionStatus::Failed,
        "cancelled" => ExecutionStatus::Cancelled,
        _ => ExecutionStatus::Queued,
    }
}

fn status_str(s: ExecutionStatus) -> &'static str {
    match s {
        ExecutionStatus::Queued => "queued",
        ExecutionStatus::Running => "running",
        ExecutionStatus::Waiting => "waiting",
        ExecutionStatus::Succeeded => "succeeded",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::Cancelled => "cancelled",
    }
}

fn parse_step_kind(s: &str) -> StepKind {
    match s {
        "agent" => StepKind::Agent,
        "skill" => StepKind::Skill,
        "tool" => StepKind::Tool,
        "decision" => StepKind::Decision,
        "parallel" => StepKind::Parallel,
        "join" => StepKind::Join,
        "human_approval" => StepKind::HumanApproval,
        "sub_workflow" => StepKind::SubWorkflow,
        "end" => StepKind::End,
        _ => StepKind::End,
    }
}

fn step_kind_str(k: &StepKind) -> &'static str {
    match k {
        StepKind::Agent => "agent",
        StepKind::Skill => "skill",
        StepKind::Tool => "tool",
        StepKind::Decision => "decision",
        StepKind::Parallel => "parallel",
        StepKind::Join => "join",
        StepKind::HumanApproval => "human_approval",
        StepKind::SubWorkflow => "sub_workflow",
        StepKind::End => "end",
    }
}

fn artifact_kind_str(k: &ArtifactKind) -> &'static str {
    match k {
        ArtifactKind::Input => "input",
        ArtifactKind::Output => "output",
        ArtifactKind::Intermediate => "intermediate",
    }
}

fn parse_outcome_kind(s: &str) -> OutcomeKind {
    match s {
        "failure" => OutcomeKind::Failure,
        "cancelled" => OutcomeKind::Cancelled,
        _ => OutcomeKind::Success,
    }
}

fn outcome_kind_str(k: &OutcomeKind) -> &'static str {
    match k {
        OutcomeKind::Success => "success",
        OutcomeKind::Failure => "failure",
        OutcomeKind::Cancelled => "cancelled",
    }
}

#[async_trait]
impl ExecutionHistoryRepository for PgExecutionHistoryRepository {
    async fn start_workflow_execution(
        &self,
        _context: &RequestContext,
        record: &WorkflowExecutionRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO workflow_executions
                (id, workspace_id, workflow_id, workflow_revision, workflow_revision_id,
                 status, attempt, started_at, completed_at, correlation_id, causation_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.workflow_id.as_uuid())
        .bind(record.workflow_revision as i32)
        .bind(record.workflow_revision_id.as_uuid())
        .bind(status_str(record.status))
        .bind(record.attempt as i32)
        .bind(record.started_at)
        .bind(record.completed_at)
        .bind(&record.correlation_id)
        .bind(&record.causation_id)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn update_workflow_execution_status(
        &self,
        _context: &RequestContext,
        id: WorkflowExecutionId,
        status: ExecutionStatus,
        completed_at: Option<vestrace_domain::time::Timestamp>,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            UPDATE workflow_executions
            SET status = $2, completed_at = $3
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(status_str(status))
        .bind(completed_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn get_workflow_execution(
        &self,
        _context: &RequestContext,
        id: WorkflowExecutionId,
    ) -> Result<Option<WorkflowExecutionRecord>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, workflow_id, workflow_revision, workflow_revision_id,
                   status, attempt, started_at, completed_at, correlation_id, causation_id
            FROM workflow_executions
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(|r| WorkflowExecutionRecord {
            id: WorkflowExecutionId::from_uuid(r.get::<uuid::Uuid, _>("id")),
            workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
            workflow_id: WorkflowId::from_uuid(r.get("workflow_id")),
            workflow_revision: r.get::<i32, _>("workflow_revision") as u32,
            workflow_revision_id: WorkflowRevisionId::from_uuid(r.get("workflow_revision_id")),
            status: parse_status(r.get("status")),
            attempt: r.get::<i32, _>("attempt") as u32,
            started_at: r.get("started_at"),
            completed_at: r.get("completed_at"),
            correlation_id: r.get("correlation_id"),
            causation_id: r.get("causation_id"),
        }))
    }

    async fn list_workflow_executions(
        &self,
        context: &RequestContext,
        workflow_id: WorkflowId,
    ) -> Result<Vec<WorkflowExecutionRecord>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, workflow_id, workflow_revision, workflow_revision_id,
                   status, attempt, started_at, completed_at, correlation_id, causation_id
            FROM workflow_executions
            WHERE workspace_id = $1 AND workflow_id = $2
            ORDER BY started_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(workflow_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|r| WorkflowExecutionRecord {
                id: WorkflowExecutionId::from_uuid(r.get("id")),
                workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
                workflow_id: WorkflowId::from_uuid(r.get("workflow_id")),
                workflow_revision: r.get::<i32, _>("workflow_revision") as u32,
                workflow_revision_id: WorkflowRevisionId::from_uuid(r.get("workflow_revision_id")),
                status: parse_status(r.get("status")),
                attempt: r.get::<i32, _>("attempt") as u32,
                started_at: r.get("started_at"),
                completed_at: r.get("completed_at"),
                correlation_id: r.get("correlation_id"),
                causation_id: r.get("causation_id"),
            })
            .collect())
    }

    async fn record_step(
        &self,
        _context: &RequestContext,
        record: &StepExecutionRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO step_executions
                (id, workflow_execution_id, workspace_id, node_id, node_label, kind,
                 agent_ref, attempt, status, input_artifact_id, output_artifact_id,
                 model_attempt_id, tool_invocation_id, error_message, started_at, completed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workflow_execution_id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.node_id.as_uuid())
        .bind(&record.node_label)
        .bind(step_kind_str(&record.kind))
        .bind(record.agent_ref.map(|a| a.as_uuid()))
        .bind(record.attempt as i32)
        .bind(status_str(record.status))
        .bind(record.input_artifact_id.map(|a| a.as_uuid()))
        .bind(record.output_artifact_id.map(|a| a.as_uuid()))
        .bind(record.model_attempt_id.map(|m| m.as_uuid()))
        .bind(record.tool_invocation_id.map(|t| t.as_uuid()))
        .bind(&record.error_message)
        .bind(record.started_at)
        .bind(record.completed_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn update_step_status(
        &self,
        _context: &RequestContext,
        id: StepExecutionId,
        status: ExecutionStatus,
        completed_at: Option<vestrace_domain::time::Timestamp>,
        error_message: Option<String>,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            UPDATE step_executions
            SET status = $2, completed_at = $3, error_message = COALESCE($4, error_message)
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(status_str(status))
        .bind(completed_at)
        .bind(error_message)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn list_steps(
        &self,
        _context: &RequestContext,
        workflow_execution_id: WorkflowExecutionId,
    ) -> Result<Vec<StepExecutionRecord>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, workflow_execution_id, workspace_id, node_id, node_label, kind,
                   agent_ref, attempt, status, input_artifact_id, output_artifact_id,
                   model_attempt_id, tool_invocation_id, error_message, started_at, completed_at
            FROM step_executions
            WHERE workflow_execution_id = $1
            ORDER BY started_at ASC
            "#,
        )
        .bind(workflow_execution_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|r| StepExecutionRecord {
                id: StepExecutionId::from_uuid(r.get("id")),
                workflow_execution_id: WorkflowExecutionId::from_uuid(
                    r.get("workflow_execution_id"),
                ),
                workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
                node_id: WorkflowNodeId::from_uuid(r.get("node_id")),
                node_label: r.get("node_label"),
                kind: parse_step_kind(r.get("kind")),
                agent_ref: r
                    .get::<Option<uuid::Uuid>, _>("agent_ref")
                    .map(AgentId::from_uuid),
                attempt: r.get::<i32, _>("attempt") as u32,
                status: parse_status(r.get("status")),
                input_artifact_id: r
                    .get::<Option<uuid::Uuid>, _>("input_artifact_id")
                    .map(ExecutionArtifactId::from_uuid),
                output_artifact_id: r
                    .get::<Option<uuid::Uuid>, _>("output_artifact_id")
                    .map(ExecutionArtifactId::from_uuid),
                model_attempt_id: r
                    .get::<Option<uuid::Uuid>, _>("model_attempt_id")
                    .map(ModelExecutionAttemptId::from_uuid),
                tool_invocation_id: r
                    .get::<Option<uuid::Uuid>, _>("tool_invocation_id")
                    .map(ToolInvocationId::from_uuid),
                error_message: r.get("error_message"),
                started_at: r.get("started_at"),
                completed_at: r.get("completed_at"),
            })
            .collect())
    }

    async fn record_artifact(
        &self,
        _context: &RequestContext,
        record: &ExecutionArtifactRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO execution_artifacts
                (id, workspace_id, step_execution_id, kind, content_ref, content_type, byte_size, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.step_execution_id.as_uuid())
        .bind(artifact_kind_str(&record.kind))
        .bind(&record.content_ref)
        .bind(&record.content_type)
        .bind(record.byte_size as i64)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn record_outcome(
        &self,
        _context: &RequestContext,
        record: &ExecutionOutcomeRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO execution_outcomes
                (id, workspace_id, workflow_execution_id, step_execution_id,
                 outcome_kind, summary, error_code, error_detail, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.workflow_execution_id.as_uuid())
        .bind(record.step_execution_id.map(|s| s.as_uuid()))
        .bind(outcome_kind_str(&record.outcome_kind))
        .bind(&record.summary)
        .bind(&record.error_code)
        .bind(&record.error_detail)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn list_outcomes(
        &self,
        _context: &RequestContext,
        workflow_execution_id: WorkflowExecutionId,
    ) -> Result<Vec<ExecutionOutcomeRecord>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, workflow_execution_id, step_execution_id,
                   outcome_kind, summary, error_code, error_detail, created_at
            FROM execution_outcomes
            WHERE workflow_execution_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(workflow_execution_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|r| ExecutionOutcomeRecord {
                id: ExecutionOutcomeId::from_uuid(r.get("id")),
                workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
                workflow_execution_id: WorkflowExecutionId::from_uuid(
                    r.get("workflow_execution_id"),
                ),
                step_execution_id: r
                    .get::<Option<uuid::Uuid>, _>("step_execution_id")
                    .map(StepExecutionId::from_uuid),
                outcome_kind: parse_outcome_kind(r.get("outcome_kind")),
                summary: r.get("summary"),
                error_code: r.get("error_code"),
                error_detail: r.get("error_detail"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
}
