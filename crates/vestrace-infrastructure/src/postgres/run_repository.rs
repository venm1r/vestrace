use async_trait::async_trait;
use sqlx::FromRow;
use vestrace_application::{ApplicationError, RequestContext, RunRepository};
use vestrace_domain::{
    id::{AgentRunId, PrincipalId, WorkspaceId},
    run::{AgentRun, RunStatus, RunVersion},
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
struct StoredRun {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    principal_id: uuid::Uuid,
    title: String,
    status: String,
    run_version: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<StoredRun> for AgentRun {
    type Error = ApplicationError;

    fn try_from(stored: StoredRun) -> Result<Self, Self::Error> {
        let status = match stored.status.as_str() {
            "created" => RunStatus::Created,
            "running" => RunStatus::Running,
            "waiting_for_input" => RunStatus::WaitingForInput,
            "waiting_for_approval" => RunStatus::WaitingForApproval,
            "completed" => RunStatus::Completed,
            "failed" => RunStatus::Failed,
            "cancelled" => RunStatus::Cancelled,
            _ => {
                return Err(ApplicationError::Storage(
                    "stored run status is unsupported".to_owned(),
                ));
            }
        };

        let version = u64::try_from(stored.run_version)
            .ok()
            .and_then(|value| RunVersion::new(value).ok())
            .ok_or_else(|| {
                ApplicationError::Storage("stored run version is invalid".to_owned())
            })?;

        Ok(Self {
            id: AgentRunId::from_uuid(stored.id),
            workspace_id: WorkspaceId::from_uuid(stored.workspace_id),
            principal_id: PrincipalId::from_uuid(stored.principal_id),
            title: stored.title,
            status,
            version,
            created_at: stored.created_at,
            updated_at: stored.updated_at,
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
            .map_err(|_| storage_failure())?;

        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id,
                workspace_id,
                principal_id,
                title,
                status,
                run_version,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(run.id.as_uuid())
        .bind(run.workspace_id.as_uuid())
        .bind(run.principal_id.as_uuid())
        .bind(&run.title)
        .bind(status_name(run.status))
        .bind(i64::try_from(run.version.value()).map_err(|_| {
            ApplicationError::Storage("run version exceeds database range".to_owned())
        })?)
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(transaction.connection())
        .await
        .map_err(|_| storage_failure())?;

        transaction.commit().await.map_err(|_| storage_failure())
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
            .map_err(|_| storage_failure())?;

        let stored = sqlx::query_as::<_, StoredRun>(
            r#"
            SELECT id, workspace_id, principal_id, title, status, run_version, created_at, updated_at
            FROM agent_runs
            ORDER BY created_at DESC, id DESC
            LIMIT $1
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(|_| storage_failure())?;

        let runs = stored
            .into_iter()
            .map(AgentRun::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit().await.map_err(|_| storage_failure())?;
        Ok(runs)
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
            .map_err(|_| storage_failure())?;

        let stored = sqlx::query_as::<_, StoredRun>(
            r#"
            SELECT id, workspace_id, principal_id, title, status, run_version, created_at, updated_at
            FROM agent_runs
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(|_| storage_failure())?;

        let run = stored.map(AgentRun::try_from).transpose()?;
        transaction.commit().await.map_err(|_| storage_failure())?;
        Ok(run)
    }
}

fn status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Created => "created",
        RunStatus::Running => "running",
        RunStatus::WaitingForInput => "waiting_for_input",
        RunStatus::WaitingForApproval => "waiting_for_approval",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
    }
}

fn storage_failure() -> ApplicationError {
    ApplicationError::Storage("run storage operation failed".to_owned())
}
