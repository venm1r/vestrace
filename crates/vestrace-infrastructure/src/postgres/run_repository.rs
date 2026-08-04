use async_trait::async_trait;
use sqlx::FromRow;
use vestrace_application::{ApplicationError, RequestContext, RunRepository};
use vestrace_domain::{
    id::{AgentRunId, PrincipalId, WorkspaceId},
    run::{AgentRun, RunStatus, RunVersion},
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
    status: String,
    run_version: i64,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl TryFrom<AgentRunRow> for AgentRun {
    type Error = ApplicationError;

    fn try_from(row: AgentRunRow) -> Result<Self, Self::Error> {
        let status = parse_status(&row.status)?;
        let run_version = u64::try_from(row.run_version)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        Ok(Self {
            id: AgentRunId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            principal_id: PrincipalId::from_uuid(row.principal_id),
            title: row.title,
            status,
            version: RunVersion::new(run_version)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
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
                 status,
                 run_version,
                 created_at,
                 updated_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(run.id.as_uuid())
        .bind(run.workspace_id.as_uuid())
        .bind(run.principal_id.as_uuid())
        .bind(&run.title)
        .bind(status_as_str(run.status))
        .bind(
            i64::try_from(run.version.value())
                .map_err(|error| ApplicationError::Storage(error.to_string()))?,
        )
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
                 status,
                 run_version,
                 created_at,
                 updated_at
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
                 status,
                 run_version,
                 created_at,
                 updated_at
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
        RunStatus::Ready => "ready",
        RunStatus::Running => "running",
        RunStatus::WaitingForInput => "waiting_for_input",
        RunStatus::WaitingForApproval => "waiting_for_approval",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Stalled => "stalled",
    }
}

fn parse_status(value: &str) -> Result<RunStatus, ApplicationError> {
    match value {
        "created" => Ok(RunStatus::Created),
        "ready" => Ok(RunStatus::Ready),
        "running" => Ok(RunStatus::Running),
        "waiting_for_input" => Ok(RunStatus::WaitingForInput),
        "waiting_for_approval" => Ok(RunStatus::WaitingForApproval),
        "completed" => Ok(RunStatus::Completed),
        "failed" => Ok(RunStatus::Failed),
        "cancelled" => Ok(RunStatus::Cancelled),
        "stalled" => Ok(RunStatus::Stalled),
        _ => Err(ApplicationError::Storage(
            "stored run status is not supported".to_owned(),
        )),
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
