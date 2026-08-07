use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, IdempotencyRecord, IdempotencyRepository};
use vestrace_domain::{WorkspaceId, time::Timestamp};

pub struct PgIdempotencyRepository {
    pool: PgPool,
}

impl PgIdempotencyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl IdempotencyRepository for PgIdempotencyRepository {
    async fn find_by_key(
        &self,
        workspace_id: WorkspaceId,
        key: &str,
    ) -> Result<Option<IdempotencyRecord>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT idempotency_key, workspace_id, request_hash, response_payload, status, created_at, expires_at
            FROM idempotency_keys
            WHERE workspace_id = $1 AND idempotency_key = $2
            "#,
        )
        .bind(workspace_id.as_uuid())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(parse_record).transpose()
    }

    async fn save(&self, record: &IdempotencyRecord) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO idempotency_keys (idempotency_key, workspace_id, request_hash, response_payload, status, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (workspace_id, idempotency_key) DO NOTHING
            "#,
        )
        .bind(&record.idempotency_key)
        .bind(record.workspace_id.as_uuid())
        .bind(&record.request_hash)
        .bind(&record.response_payload)
        .bind(&record.status)
        .bind(record.created_at)
        .bind(record.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Conflict(e.to_string()))?;

        Ok(())
    }
}

fn parse_record(row: sqlx::postgres::PgRow) -> Result<IdempotencyRecord, ApplicationError> {
    let idempotency_key: String = row.try_get("idempotency_key").map_err(storage_error)?;
    let workspace_id: uuid::Uuid = row.try_get("workspace_id").map_err(storage_error)?;
    let request_hash: String = row.try_get("request_hash").map_err(storage_error)?;
    let response_payload: Option<serde_json::Value> =
        row.try_get("response_payload").map_err(storage_error)?;
    let status: String = row.try_get("status").map_err(storage_error)?;
    let created_at: Timestamp = row.try_get("created_at").map_err(storage_error)?;
    let expires_at: Timestamp = row.try_get("expires_at").map_err(storage_error)?;

    Ok(IdempotencyRecord {
        idempotency_key,
        workspace_id: WorkspaceId::from_uuid(workspace_id),
        request_hash,
        response_payload,
        status,
        created_at,
        expires_at,
    })
}
