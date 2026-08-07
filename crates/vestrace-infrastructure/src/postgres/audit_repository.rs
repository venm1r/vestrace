use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, AuditRepository, RequestContext};
use vestrace_domain::AuditEvent;

pub struct PgAuditRepository {
    pool: PgPool,
}

impl PgAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl AuditRepository for PgAuditRepository {
    async fn record(
        &self,
        _context: &RequestContext,
        event: &AuditEvent,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO audit_events (id, workspace_id, principal_id, action, resource_type, resource_id, payload, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.id.as_uuid())
        .bind(event.workspace_id.as_uuid())
        .bind(event.principal_id.as_uuid())
        .bind(&event.action)
        .bind(&event.resource_type)
        .bind(event.resource_id)
        .bind(&event.payload)
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(())
    }
}
