use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, ModelRecord, ModelRepository, RequestContext};

pub struct PgModelRepository {
    pool: PgPool,
}

impl PgModelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl ModelRepository for PgModelRepository {
    async fn create(
        &self,
        context: &RequestContext,
        model: &ModelRecord,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO models (id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(model.id.as_uuid())
        .bind(model.provider_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(&model.model_name)
        .bind(model.context_window as i32)
        .bind(model.input_cost_per_mtoken)
        .bind(model.output_cost_per_mtoken)
        .bind(model.created_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn list(&self, context: &RequestContext) -> Result<Vec<ModelRecord>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken, created_at
            FROM models
            WHERE workspace_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut models = Vec::new();
        for row in rows {
            models.push(ModelRecord {
                id: vestrace_domain::id::ModelId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                provider_id: vestrace_domain::id::ProviderId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("provider_id")
                        .map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                model_name: row.try_get("model_name").map_err(storage_error)?,
                context_window: row
                    .try_get::<i32, _>("context_window")
                    .map_err(storage_error)? as u32,
                input_cost_per_mtoken: row
                    .try_get("input_cost_per_mtoken")
                    .map_err(storage_error)?,
                output_cost_per_mtoken: row
                    .try_get("output_cost_per_mtoken")
                    .map_err(storage_error)?,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            });
        }
        Ok(models)
    }

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: vestrace_domain::id::ModelId,
    ) -> Result<Option<ModelRecord>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken, created_at
            FROM models
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        match row {
            Some(row) => Ok(Some(ModelRecord {
                id: vestrace_domain::id::ModelId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                provider_id: vestrace_domain::id::ProviderId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("provider_id")
                        .map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                model_name: row.try_get("model_name").map_err(storage_error)?,
                context_window: row
                    .try_get::<i32, _>("context_window")
                    .map_err(storage_error)? as u32,
                input_cost_per_mtoken: row
                    .try_get("input_cost_per_mtoken")
                    .map_err(storage_error)?,
                output_cost_per_mtoken: row
                    .try_get("output_cost_per_mtoken")
                    .map_err(storage_error)?,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            })),
            None => Ok(None),
        }
    }
}
