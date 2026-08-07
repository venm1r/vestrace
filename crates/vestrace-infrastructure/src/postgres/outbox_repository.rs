use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, OutboxMessage, OutboxRepository};
use vestrace_domain::{WorkspaceId, id::OutboxId};

pub struct PgOutboxRepository {
    pool: PgPool,
}

impl PgOutboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl OutboxRepository for PgOutboxRepository {
    async fn save(&self, message: &OutboxMessage) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO outbox (id, workspace_id, topic, payload, processed_at, created_at)
            VALUES ($1, $2, $3, $4, NULL, $5)
            "#,
        )
        .bind(message.id.as_uuid())
        .bind(message.workspace_id.as_uuid())
        .bind(&message.topic)
        .bind(&message.payload)
        .bind(message.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn claim_pending(&self, limit: u32) -> Result<Vec<OutboxMessage>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, topic, payload, created_at
            FROM outbox
            WHERE processed_at IS NULL
            ORDER BY created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.into_iter().map(parse_outbox_row).collect()
    }

    async fn mark_processed(&self, id: OutboxId) -> Result<(), ApplicationError> {
        sqlx::query("UPDATE outbox SET processed_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }
}

fn parse_outbox_row(row: sqlx::postgres::PgRow) -> Result<OutboxMessage, ApplicationError> {
    let id: uuid::Uuid = row.try_get("id").map_err(storage_error)?;
    let workspace_id: uuid::Uuid = row.try_get("workspace_id").map_err(storage_error)?;
    let topic: String = row.try_get("topic").map_err(storage_error)?;
    let payload: serde_json::Value = row.try_get("payload").map_err(storage_error)?;
    let created_at = row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        .map_err(storage_error)?;

    Ok(OutboxMessage {
        id: OutboxId::from_uuid(id),
        workspace_id: WorkspaceId::from_uuid(workspace_id),
        topic,
        payload,
        created_at,
    })
}
