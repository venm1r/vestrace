use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, RequestContext, WorkspaceCounts, WorkspaceCountsProvider,
};

use super::PgStore;

/// Aggregate counts for one workspace.
///
/// Each count is a direct query, so the numbers are what the database holds at
/// the moment of the request rather than a cached approximation.
#[derive(Clone, Debug)]
pub struct PgWorkspaceCounts {
    store: PgStore,
}

impl PgWorkspaceCounts {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

const TERMINAL_STATUSES: &str =
    "('succeeded','succeeded_with_warnings','partial','failed','cancelled','expired')";

#[async_trait]
impl WorkspaceCountsProvider for PgWorkspaceCounts {
    async fn workspace_counts(
        &self,
        context: &RequestContext,
    ) -> Result<WorkspaceCounts, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let query = format!(
            "SELECT
                 (SELECT count(*) FROM agent_runs
                   WHERE workspace_id = $1 AND status NOT IN {TERMINAL_STATUSES}) AS live_runs,
                 (SELECT count(*) FROM agent_runs
                   WHERE workspace_id = $1
                     AND created_at >= date_trunc('day', NOW() AT TIME ZONE 'UTC')) AS runs_today,
                 (SELECT count(*) FROM agents WHERE workspace_id = $1) AS registered_agents,
                 (SELECT count(*) FROM models WHERE workspace_id = $1) AS registered_models"
        );

        let row = sqlx::query(&query)
            .bind(context.workspace_id.as_uuid())
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?;
        let counts = WorkspaceCounts {
            live_runs: row.try_get("live_runs").map_err(storage_error)?,
            runs_today: row.try_get("runs_today").map_err(storage_error)?,
            registered_agents: row.try_get("registered_agents").map_err(storage_error)?,
            registered_models: row.try_get("registered_models").map_err(storage_error)?,
        };
        transaction.commit().await.map_err(storage_error)?;
        Ok(counts)
    }
}
