use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{ApplicationError, RequestContext, TriggerRepository};
use vestrace_domain::conversation::ExternalTrigger;

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgTriggerRepository {
    store: PgStore,
}

impl PgTriggerRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl TriggerRepository for PgTriggerRepository {
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<ExternalTrigger>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query(
            "SELECT id, trigger_type, name, enabled, created_at
             FROM external_triggers
             WHERE workspace_id = $1
             ORDER BY created_at DESC, id DESC
             LIMIT $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(ExternalTrigger {
                    id: vestrace_domain::id::TriggerId::from_uuid(
                        row.try_get("id").map_err(storage_error)?,
                    ),
                    workspace_id: context.workspace_id,
                    trigger_type: row.try_get("trigger_type").map_err(storage_error)?,
                    name: row.try_get("name").map_err(storage_error)?,
                    enabled: row.try_get("enabled").map_err(storage_error)?,
                    created_at: row.try_get("created_at").map_err(storage_error)?,
                })
            })
            .collect()
    }
}
