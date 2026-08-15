use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, ModelExecutionRecord, ModelExecutionRepository, RequestContext,
};

pub struct PgModelExecutionRepository {
    store: PgStore,
}

impl PgModelExecutionRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

use super::PgStore;

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl ModelExecutionRepository for PgModelExecutionRepository {
    async fn record(
        &self,
        context: &RequestContext,
        execution: &ModelExecutionRecord,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        sqlx::query(
            r#"
            INSERT INTO model_executions (id, workspace_id, model_id, prompt_tokens, completion_tokens, latency_ms, status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(execution.id.as_uuid())
        .bind(execution.workspace_id.as_uuid())
        .bind(execution.model_id.as_uuid())
        .bind(execution.prompt_tokens as i32)
        .bind(execution.completion_tokens as i32)
        .bind(execution.latency_ms as i32)
        .bind(&execution.status)
        .bind(execution.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<ModelExecutionRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, model_id, prompt_tokens, completion_tokens, latency_ms, status, created_at
            FROM model_executions
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        let mut executions = Vec::new();
        for row in rows {
            executions.push(ModelExecutionRecord {
                id: vestrace_domain::id::ModelExecutionId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                model_id: vestrace_domain::id::ModelId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("model_id")
                        .map_err(storage_error)?,
                ),
                prompt_tokens: row
                    .try_get::<i32, _>("prompt_tokens")
                    .map_err(storage_error)? as u32,
                completion_tokens: row
                    .try_get::<i32, _>("completion_tokens")
                    .map_err(storage_error)? as u32,
                latency_ms: row.try_get::<i32, _>("latency_ms").map_err(storage_error)? as u32,
                status: row.try_get("status").map_err(storage_error)?,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            });
        }

        scoped.commit().await.map_err(storage_error)?;
        Ok(executions)
    }
}
