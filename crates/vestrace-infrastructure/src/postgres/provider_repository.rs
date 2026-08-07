use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, ProviderRecord, ProviderRepository, RequestContext};
use vestrace_domain::{ProviderLocality, time::now};

pub struct PgProviderRepository {
    pool: PgPool,
}

impl PgProviderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl ProviderRepository for PgProviderRepository {
    async fn create(
        &self,
        context: &RequestContext,
        id: vestrace_domain::id::ProviderId,
        name: &str,
        locality: ProviderLocality,
    ) -> Result<(), ApplicationError> {
        let locality_str = match locality {
            ProviderLocality::Local => "local",
            ProviderLocality::Remote => "remote",
        };
        sqlx::query(
            r#"
            INSERT INTO providers (id, workspace_id, name, locality, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(name)
        .bind(locality_str)
        .bind(now())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<ProviderRecord>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, name, locality, created_at
            FROM providers
            WHERE workspace_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut providers = Vec::new();
        for row in rows {
            let locality_str: String = row.try_get("locality").map_err(storage_error)?;
            let locality = match locality_str.as_str() {
                "local" => ProviderLocality::Local,
                _ => ProviderLocality::Remote,
            };
            providers.push(ProviderRecord {
                id: vestrace_domain::id::ProviderId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("id").map_err(storage_error)?,
                ),
                workspace_id: vestrace_domain::WorkspaceId::from_uuid(
                    row.try_get::<uuid::Uuid, _>("workspace_id")
                        .map_err(storage_error)?,
                ),
                name: row.try_get("name").map_err(storage_error)?,
                locality,
                created_at: row.try_get("created_at").map_err(storage_error)?,
            });
        }
        Ok(providers)
    }
}
