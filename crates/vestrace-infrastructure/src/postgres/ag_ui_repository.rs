use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    AgUiEndpoint, AgUiRepository, AgUiRunEvent, ApplicationError, RequestContext,
};
use vestrace_domain::id::AgentRunId;
use vestrace_domain::time::Timestamp;

use super::PgStore;

/// Reads the AG-UI endpoint registry from migration 0110 and the run event log.
#[derive(Clone, Debug)]
pub struct PgAgUiRepository {
    store: PgStore,
}

impl PgAgUiRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl AgUiRepository for PgAgUiRepository {
    async fn list_endpoints(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<AgUiEndpoint>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, name, endpoint_url, enabled, created_at
            FROM ag_ui_endpoints
            WHERE workspace_id = $1
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(AgUiEndpoint {
                    id: row.try_get("id").map_err(storage_error)?,
                    name: row.try_get("name").map_err(storage_error)?,
                    endpoint_url: row.try_get("endpoint_url").map_err(storage_error)?,
                    enabled: row.try_get("enabled").map_err(storage_error)?,
                    created_at: row.try_get("created_at").map_err(storage_error)?,
                })
            })
            .collect()
    }

    async fn events_since(
        &self,
        context: &RequestContext,
        run_id: Option<AgentRunId>,
        since: Timestamp,
        limit: u32,
    ) -> Result<Vec<AgUiRunEvent>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // Oldest first, so a caller advancing its cursor to the last row of a
        // batch does not skip the events in between. `> $2` is strict, so the
        // event a cursor already points at is not redelivered.
        //
        // `$3::uuid IS NULL` keeps this one statement rather than two: the
        // narrowed and unnarrowed forms then cannot drift apart in their
        // ordering or their workspace predicate.
        let rows = sqlx::query(
            r#"
            SELECT run_id, event_type, sequence, created_at
            FROM run_events
            WHERE workspace_id = $1
              AND created_at > $2
              AND ($3::uuid IS NULL OR run_id = $3::uuid)
            ORDER BY created_at ASC, sequence ASC
            LIMIT $4
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(since)
        .bind(run_id.map(|id| id.as_uuid()))
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(AgUiRunEvent {
                    run_id: row.try_get("run_id").map_err(storage_error)?,
                    event_type: row.try_get("event_type").map_err(storage_error)?,
                    sequence: row.try_get("sequence").map_err(storage_error)?,
                    created_at: row.try_get("created_at").map_err(storage_error)?,
                })
            })
            .collect()
    }
}
