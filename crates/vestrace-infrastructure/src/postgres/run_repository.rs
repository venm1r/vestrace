use async_trait::async_trait;
use sqlx::FromRow;
use vestrace_application::{ApplicationError, RequestContext, RunRepository};
use vestrace_domain::{
    id::{AgentRunId, AgentRuntimeSnapshotId, WorkspaceId},
    run::{AgentRun, RunExecutionMode, RunStatus, RunVersion},
    time::Timestamp,
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgRunRepository {
    store: PgStore,
}

impl PgRunRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[derive(Debug, FromRow)]
struct AgentRunRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    principal_id: uuid::Uuid,
    title: String,
    objective: String,
    coordinator_snapshot_id: Option<uuid::Uuid>,
    execution_mode: String,
    status: String,
    run_version: i64,
    root_run_id: uuid::Uuid,
    created_at: Timestamp,
    updated_at: Timestamp,
    finished_at: Option<Timestamp>,
}

impl TryFrom<AgentRunRow> for AgentRun {
    type Error = ApplicationError;

    fn try_from(row: AgentRunRow) -> Result<Self, Self::Error> {
        let status = parse_status(&row.status)?;
        let run_version = u64::try_from(row.run_version)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let execution_mode = RunExecutionMode::parse(&row.execution_mode).ok_or_else(|| {
            ApplicationError::Storage(format!("invalid execution_mode: {}", row.execution_mode))
        })?;

        Ok(Self {
            id: AgentRunId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            objective: row.objective,
            coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(
                row.coordinator_snapshot_id.unwrap_or(row.principal_id),
            ),
            active_plan_revision_id: None,
            execution_mode,
            status,
            current_step_id: None,
            checkpoint_id: None,
            parent: None,
            root_run_id: AgentRunId::from_uuid(row.root_run_id),
            budget_snapshot_id: None,
            resource_usage_snapshot_id: None,
            version: RunVersion::new(run_version)?,
            result: None,
            created_at: row.created_at,
            updated_at: row.updated_at,
            finished_at: row.finished_at,
        })
    }
}

#[async_trait]
impl RunRepository for PgRunRepository {
    async fn create(
        &self,
        context: &RequestContext,
        run: &AgentRun,
    ) -> Result<(), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            "INSERT INTO run_streams (
                 workspace_id, run_id, current_version, created_at, updated_at
             ) VALUES ($1, $2, 0, $3, $4)",
        )
        .bind(run.workspace_id.as_uuid())
        .bind(run.id.as_uuid())
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        sqlx::query(
            "INSERT INTO agent_runs (
                 id,
                 workspace_id,
                 principal_id,
                 title,
                 objective,
                 coordinator_snapshot_id,
                 execution_mode,
                 status,
                 run_version,
                 root_run_id,
                 created_at,
                 updated_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(run.id.as_uuid())
        .bind(run.workspace_id.as_uuid())
        .bind(context.principal_id.as_uuid())
        .bind(&run.objective)
        .bind(&run.objective)
        .bind(run.coordinator_snapshot_id.as_uuid())
        .bind(run.execution_mode.as_str())
        .bind(status_as_str(run.status))
        .bind(
            i64::try_from(run.version.value())
                .map_err(|error| ApplicationError::Storage(error.to_string()))?,
        )
        .bind(run.root_run_id.as_uuid())
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;
        Ok(())
    }

    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query_as::<_, AgentRunRow>(
            "SELECT
                 id,
                 workspace_id,
                 principal_id,
                 title,
                 objective,
                 coordinator_snapshot_id,
                 execution_mode,
                 status,
                 run_version,
                 root_run_id,
                 created_at,
                 updated_at,
                 finished_at
             FROM agent_runs
             WHERE workspace_id = $1
             ORDER BY created_at ASC, id ASC
             LIMIT $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        rows.into_iter().map(AgentRun::try_from).collect()
    }

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query_as::<_, AgentRunRow>(
            "SELECT
                 id,
                 workspace_id,
                 principal_id,
                 title,
                 objective,
                 coordinator_snapshot_id,
                 execution_mode,
                 status,
                 run_version,
                 root_run_id,
                 created_at,
                 updated_at,
                 finished_at
             FROM agent_runs
             WHERE workspace_id = $1
               AND id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        row.map(AgentRun::try_from).transpose()
    }
}

fn status_as_str(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Created => "created",
        RunStatus::Preparing => "preparing",
        RunStatus::Running => "running",
        RunStatus::WaitingForInput => "waiting_for_input",
        RunStatus::WaitingForApproval => "waiting_for_approval",
        RunStatus::WaitingForDependency => "waiting_for_dependency",
        RunStatus::Succeeded => "succeeded",
        RunStatus::SucceededWithWarnings => "succeeded_with_warnings",
        RunStatus::Partial => "partial",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Paused => "paused",
        RunStatus::PausedPolicyChanged => "paused_policy_changed",
        RunStatus::Expired => "expired",
    }
}

fn parse_status(value: &str) -> Result<RunStatus, ApplicationError> {
    match value {
        "created" => Ok(RunStatus::Created),
        "preparing" => Ok(RunStatus::Preparing),
        "running" => Ok(RunStatus::Running),
        "waiting_for_input" => Ok(RunStatus::WaitingForInput),
        "waiting_for_approval" => Ok(RunStatus::WaitingForApproval),
        "waiting_for_dependency" => Ok(RunStatus::WaitingForDependency),
        "succeeded" => Ok(RunStatus::Succeeded),
        "succeeded_with_warnings" => Ok(RunStatus::SucceededWithWarnings),
        "partial" => Ok(RunStatus::Partial),
        "failed" => Ok(RunStatus::Failed),
        "cancelled" => Ok(RunStatus::Cancelled),
        "paused" => Ok(RunStatus::Paused),
        "paused_policy_changed" => Ok(RunStatus::PausedPolicyChanged),
        "expired" => Ok(RunStatus::Expired),
        _ => Err(ApplicationError::Storage(
            "stored run status is not supported".to_owned(),
        )),
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
