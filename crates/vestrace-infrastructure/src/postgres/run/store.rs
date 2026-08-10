use async_trait::async_trait;
use sqlx::{FromRow, PgPool};
use vestrace_application::run::ports::{CommitRun, RunSnapshot, RunStorePort};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::id::{
    AgentRunId, AgentRuntimeSnapshotId, BudgetSnapshotId, PlanRevisionId, ResourceUsageSnapshotId,
    RunCheckpointId, RunStepId, WorkspaceId,
};
use vestrace_domain::run::{
    AgentRun, RunCheckpoint, RunEvent, RunExecutionMode, RunStatus, RunStep, RunStepStatus,
    RunTerminalResult, RunVersion,
};
use vestrace_domain::time::Timestamp;

use super::super::PgStore;

#[derive(Clone)]
pub struct PostgresRunStore {
    pool: PgPool,
}

impl PostgresRunStore {
    pub fn new(store: &PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct AgentRunRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    objective: String,
    coordinator_snapshot_id: uuid::Uuid,
    active_plan_revision_id: Option<uuid::Uuid>,
    execution_mode: String,
    status: String,
    current_step_id: Option<uuid::Uuid>,
    checkpoint_id: Option<uuid::Uuid>,
    parent_run_id: Option<uuid::Uuid>,
    parent_step_id: Option<uuid::Uuid>,
    root_run_id: Option<uuid::Uuid>,
    budget_snapshot_id: Option<uuid::Uuid>,
    resource_usage_snapshot_id: Option<uuid::Uuid>,
    run_version: i64,
    result: Option<serde_json::Value>,
    created_at: Timestamp,
    updated_at: Timestamp,
    finished_at: Option<Timestamp>,
}

fn parse_status(s: &str) -> Result<RunStatus, ApplicationError> {
    RunStatus::parse(s).ok_or_else(|| ApplicationError::Storage(format!("unknown run status: {s}")))
}

fn parse_execution_mode(s: &str) -> Result<RunExecutionMode, ApplicationError> {
    RunExecutionMode::parse(s)
        .ok_or_else(|| ApplicationError::Storage(format!("unknown execution mode: {s}")))
}

fn parse_step_status(s: &str) -> Result<RunStepStatus, ApplicationError> {
    RunStepStatus::parse(s)
        .ok_or_else(|| ApplicationError::Storage(format!("unknown step status: {s}")))
}

impl TryFrom<AgentRunRow> for AgentRun {
    type Error = ApplicationError;

    fn try_from(row: AgentRunRow) -> Result<Self, Self::Error> {
        let status = parse_status(&row.status)?;
        let execution_mode = parse_execution_mode(&row.execution_mode)?;
        let run_version =
            u64::try_from(row.run_version).map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let version =
            RunVersion::new(run_version).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let root_run_id = row
            .root_run_id
            .map(AgentRunId::from_uuid)
            .unwrap_or_else(|| AgentRunId::from_uuid(row.id));

        let parent = row
            .parent_run_id
            .zip(row.parent_step_id)
            .map(|(prid, psid)| vestrace_domain::run::ParentRunLink {
                parent_run_id: AgentRunId::from_uuid(prid),
                parent_step_id: RunStepId::from_uuid(psid),
            });

        let result: Option<RunTerminalResult> = row
            .result
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(Self {
            id: AgentRunId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            objective: row.objective,
            coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(row.coordinator_snapshot_id),
            active_plan_revision_id: row.active_plan_revision_id.map(PlanRevisionId::from_uuid),
            execution_mode,
            status,
            current_step_id: row.current_step_id.map(RunStepId::from_uuid),
            checkpoint_id: row.checkpoint_id.map(RunCheckpointId::from_uuid),
            parent,
            root_run_id,
            budget_snapshot_id: row.budget_snapshot_id.map(BudgetSnapshotId::from_uuid),
            resource_usage_snapshot_id: row
                .resource_usage_snapshot_id
                .map(ResourceUsageSnapshotId::from_uuid),
            version,
            result,
            created_at: row.created_at,
            updated_at: row.updated_at,
            finished_at: row.finished_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct RunStepRow {
    id: uuid::Uuid,
    run_id: uuid::Uuid,
    plan_step_reference: Option<String>,
    assigned_actor: serde_json::Value,
    input_references: serde_json::Value,
    status: String,
    attempt: i32,
    output_references: serde_json::Value,
    error: Option<serde_json::Value>,
    created_at: Timestamp,
    started_at: Option<Timestamp>,
    finished_at: Option<Timestamp>,
}

impl TryFrom<RunStepRow> for RunStep {
    type Error = ApplicationError;

    fn try_from(row: RunStepRow) -> Result<Self, Self::Error> {
        let status = parse_step_status(&row.status)?;
        let attempt =
            u32::try_from(row.attempt).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let assigned_actor: vestrace_domain::run::RunActorRef =
            serde_json::from_value(row.assigned_actor)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let input_references: Vec<vestrace_domain::run::RunReference> =
            serde_json::from_value(row.input_references)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let output_references: Vec<vestrace_domain::run::RunReference> =
            serde_json::from_value(row.output_references)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let error: Option<vestrace_domain::run::RunFailure> = row
            .error
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(Self {
            id: RunStepId::from_uuid(row.id),
            run_id: AgentRunId::from_uuid(row.run_id),
            plan_step_reference: row.plan_step_reference,
            assigned_actor,
            input_references,
            status,
            attempt,
            output_references,
            error,
            created_at: row.created_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct RunCheckpointRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    run_id: uuid::Uuid,
    run_version: i64,
    active_plan_revision_id: Option<uuid::Uuid>,
    payload: serde_json::Value,
    created_at: Timestamp,
}

impl TryFrom<RunCheckpointRow> for RunCheckpoint {
    type Error = ApplicationError;

    fn try_from(row: RunCheckpointRow) -> Result<Self, Self::Error> {
        let run_version =
            u64::try_from(row.run_version).map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let version =
            RunVersion::new(run_version).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let payload: vestrace_domain::run::RunCheckpointPayload =
            serde_json::from_value(row.payload)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(Self {
            id: RunCheckpointId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            run_id: AgentRunId::from_uuid(row.run_id),
            run_version: version,
            active_plan_revision_id: row.active_plan_revision_id.map(PlanRevisionId::from_uuid),
            resume_cursor: vestrace_domain::run::ResumeCursor::from_version(version),
            payload,
            created_at: row.created_at,
        })
    }
}

#[async_trait]
impl RunStorePort for PostgresRunStore {
    async fn load(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError> {
        let ws = context.workspace_id.as_uuid();
        let rid = run_id.as_uuid();

        let run_row: Option<AgentRunRow> = sqlx::query_as::<_, AgentRunRow>(
            r#"
            SELECT id, workspace_id, objective, coordinator_snapshot_id,
                   active_plan_revision_id, execution_mode, status,
                   current_step_id, checkpoint_id, parent_run_id, parent_step_id,
                   root_run_id, budget_snapshot_id, resource_usage_snapshot_id,
                   run_version, result, created_at, updated_at, finished_at
            FROM agent_runs
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(ws)
        .bind(rid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let Some(run_row) = run_row else {
            return Ok(None);
        };

        let run = AgentRun::try_from(run_row)?;

        let step_rows: Vec<RunStepRow> = sqlx::query_as::<_, RunStepRow>(
            r#"
            SELECT id, run_id, plan_step_reference, assigned_actor,
                   input_references, status, attempt, output_references, error,
                   created_at, started_at, finished_at
            FROM run_steps
            WHERE run_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(rid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let steps: Vec<RunStep> = step_rows
            .into_iter()
            .map(RunStep::try_from)
            .collect::<Result<_, _>>()?;

        let checkpoint_row: Option<RunCheckpointRow> = sqlx::query_as::<_, RunCheckpointRow>(
            r#"
            SELECT id, workspace_id, run_id, run_version, active_plan_revision_id,
                   payload, created_at
            FROM run_checkpoints
            WHERE run_id = $1
            ORDER BY run_version DESC
            LIMIT 1
            "#,
        )
        .bind(rid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let checkpoint = checkpoint_row.map(RunCheckpoint::try_from).transpose()?;

        Ok(Some(RunSnapshot {
            run,
            steps,
            checkpoint,
        }))
    }

    async fn create(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let ws = context.workspace_id.as_uuid();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let run = &commit.run;
        let run_id = run.id.as_uuid();
        let now = run.created_at;

        let parent_run_id = run.parent.map(|p| p.parent_run_id.as_uuid());
        let parent_step_id = run.parent.map(|p| p.parent_step_id.as_uuid());

        sqlx::query(
            r#"
            INSERT INTO agent_runs
                (id, workspace_id, objective, coordinator_snapshot_id, execution_mode,
                 status, run_version, parent_run_id, parent_step_id, root_run_id,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
            "#,
        )
        .bind(run_id)
        .bind(ws)
        .bind(&run.objective)
        .bind(run.coordinator_snapshot_id.as_uuid())
        .bind(run.execution_mode.as_str())
        .bind(run.status.as_str())
        .bind(run.version.value() as i64)
        .bind(parent_run_id)
        .bind(parent_step_id)
        .bind(run.root_run_id.as_uuid())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        append_event(&mut tx, ws, &commit.event).await?;

        for step in &commit.new_steps {
            insert_step(&mut tx, ws, step).await?;
        }

        if let Some(checkpoint) = &commit.checkpoint {
            insert_checkpoint(&mut tx, ws, checkpoint).await?;
        }

        for item in &commit.work_items {
            insert_work_item(&mut tx, ws, item).await?;
        }

        tx.commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        self.load(context, run.id)
            .await?
            .ok_or_else(|| ApplicationError::Storage("run not found after create".into()))
    }

    async fn commit(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let ws = context.workspace_id.as_uuid();
        let run = &commit.run;
        let run_id = run.id.as_uuid();
        let expected_version = run
            .version
            .previous()
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let new_version = run.version;
        let now = run.updated_at;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let result_json = run
            .result
            .as_ref()
            .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null));

        let result = sqlx::query(
            r#"
            UPDATE agent_runs
            SET status = $3,
                run_version = $4,
                updated_at = $5,
                finished_at = $6,
                current_step_id = $7,
                checkpoint_id = $8,
                active_plan_revision_id = $9,
                result = $10
            WHERE workspace_id = $1
              AND id = $2
              AND run_version = $11
            "#,
        )
        .bind(ws)
        .bind(run_id)
        .bind(run.status.as_str())
        .bind(new_version.value() as i64)
        .bind(now)
        .bind(run.finished_at)
        .bind(run.current_step_id.map(|s| s.as_uuid()))
        .bind(run.checkpoint_id.map(|c| c.as_uuid()))
        .bind(run.active_plan_revision_id.map(|r| r.as_uuid()))
        .bind(result_json)
        .bind(expected_version.value() as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            let current: Option<(i64,)> = sqlx::query_as(
                "SELECT run_version FROM agent_runs WHERE workspace_id = $1 AND id = $2",
            )
            .bind(ws)
            .bind(run_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

            drop(tx);
            return Err(ApplicationError::Conflict(format!(
                "revision_conflict: expected version {}, found {:?}",
                expected_version.value(),
                current.map(|c| c.0)
            )));
        }

        append_event(&mut tx, ws, &commit.event).await?;

        for step in &commit.new_steps {
            insert_step(&mut tx, ws, step).await?;
        }

        if let Some(checkpoint) = &commit.checkpoint {
            insert_checkpoint(&mut tx, ws, checkpoint).await?;
        }

        for item in &commit.work_items {
            insert_work_item(&mut tx, ws, item).await?;
        }

        tx.commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        self.load(context, run.id)
            .await?
            .ok_or_else(|| ApplicationError::Storage("run not found after commit".into()))
    }
}

async fn append_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: uuid::Uuid,
    event: &RunEvent,
) -> Result<(), ApplicationError> {
    let payload_json = serde_json::to_value(&event.payload)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let actor_json =
        serde_json::to_value(&event.actor).map_err(|e| ApplicationError::Storage(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO run_events
            (id, workspace_id, run_id, sequence, run_version, sequence_value,
             event_type, payload_kind, payload, actor,
             correlation_id, causation_event_id, occurred_at, recorded_at)
        VALUES ($1, $2, $3, $4, $5, $5, $6, $6, $7, $8, $9, $10, $11, $11)
        "#,
    )
    .bind(event.id.as_uuid())
    .bind(workspace_id)
    .bind(event.run_id.as_uuid())
    .bind(event.sequence.value() as i64)
    .bind(event.run_version.value() as i64)
    .bind(event.payload.event_type())
    .bind(&payload_json)
    .bind(&actor_json)
    .bind(event.correlation_id.as_uuid())
    .bind(event.causation_event_id.map(|e| e.as_uuid()))
    .bind(event.occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}

async fn insert_step(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    _workspace_id: uuid::Uuid,
    step: &RunStep,
) -> Result<(), ApplicationError> {
    let actor_json = serde_json::to_value(&step.assigned_actor)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let input_json = serde_json::to_value(&step.input_references)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let output_json = serde_json::to_value(&step.output_references)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let error_json = step
        .error
        .as_ref()
        .map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null));

    sqlx::query(
        r#"
        INSERT INTO run_steps
            (id, workspace_id, run_id, plan_step_reference, assigned_actor,
             input_references, status, attempt, output_references, error,
             created_at, started_at, finished_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(step.id.as_uuid())
    .bind(_workspace_id)
    .bind(step.run_id.as_uuid())
    .bind(&step.plan_step_reference)
    .bind(&actor_json)
    .bind(&input_json)
    .bind(step.status.as_str())
    .bind(step.attempt as i32)
    .bind(&output_json)
    .bind(error_json)
    .bind(step.created_at)
    .bind(step.started_at)
    .bind(step.finished_at)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}

async fn insert_checkpoint(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: uuid::Uuid,
    checkpoint: &RunCheckpoint,
) -> Result<(), ApplicationError> {
    let payload_json = serde_json::to_value(&checkpoint.payload)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO run_checkpoints
            (id, workspace_id, run_id, run_version, resume_cursor,
             payload, payload_version, created_at)
        VALUES ($1, $2, $3, $4, $4, $5, 'v1', $6)
        "#,
    )
    .bind(checkpoint.id.as_uuid())
    .bind(workspace_id)
    .bind(checkpoint.run_id.as_uuid())
    .bind(checkpoint.run_version.value() as i64)
    .bind(&payload_json)
    .bind(checkpoint.created_at)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}

async fn insert_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: uuid::Uuid,
    item: &vestrace_application::run::ports::WorkItem,
) -> Result<(), ApplicationError> {
    let (kind, step_id) = match &item.kind {
        vestrace_application::run::ports::WorkItemKind::AdvanceRun => ("advance_run", None),
        vestrace_application::run::ports::WorkItemKind::ResumeRun => ("resume_run", None),
        vestrace_application::run::ports::WorkItemKind::ExecuteStep { step_id } => {
            ("execute_step", Some(step_id.as_uuid()))
        }
    };

    sqlx::query(
        r#"
        INSERT INTO run_work_items
            (id, workspace_id, run_id, step_id, kind, status,
             expected_run_version, available_at, attempt, max_attempts, idempotency_key)
        VALUES ($1, $2, $3, $4, $5, 'ready', $6, $7, $8, 3, $9)
        "#,
    )
    .bind(item.id.as_uuid())
    .bind(workspace_id)
    .bind(item.run_id.as_uuid())
    .bind(step_id)
    .bind(kind)
    .bind(item.expected_run_version.value() as i64)
    .bind(item.available_at)
    .bind(item.attempt as i32)
    .bind(&item.idempotency_key)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}
