use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, EventRepository};
use vestrace_domain::Event;

pub struct PgEventRepository {
    pool: PgPool,
}

impl PgEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventRepository for PgEventRepository {
    async fn save(&mut self, event: &Event) -> Result<(), ApplicationError> {
        sqlx::query!(
            r#"
            INSERT INTO events (id, workspace_id, session_id, event_type, actor, subject, payload, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            event.id.as_uuid(),
            event.workspace_id.as_uuid(),
            event.session_id.map(|s| s.as_uuid()),
            event.event_type,
            serde_json::to_value(&event.actor).unwrap_or_default(),
            event.subject.as_ref().map(|s| serde_json::to_value(s).unwrap_or_default()),
            event.payload,
            event.created_at.as_datetime()
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::StorageFailure(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: vestrace_domain::id::EventId) -> Result<Option<Event>, ApplicationError> {
        // Query implementation
        Ok(None)
    }
}
