use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, EventRepository};
use vestrace_domain::{
    Event,
    id::{EventId, SessionId, WorkspaceId},
};

pub struct PgEventRepository {
    pool: PgPool,
}

impl PgEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl EventRepository for PgEventRepository {
    async fn save(&self, event: &Event) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO events (id, workspace_id, session_id, event_type, actor, subject, payload, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.id.as_uuid())
        .bind(event.workspace_id.as_uuid())
        .bind(event.session_id.map(|s| s.as_uuid()))
        .bind(&event.event_type)
        .bind(serde_json::to_value(&event.actor).unwrap_or_default())
        .bind(event.subject.as_ref().map(|s| serde_json::to_value(s).unwrap_or_default()))
        .bind(&event.payload)
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: EventId) -> Result<Option<Event>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, session_id, event_type, actor, subject, payload, created_at
            FROM events
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(parse_event_row).transpose()
    }
}

fn parse_event_row(row: sqlx::postgres::PgRow) -> Result<Event, ApplicationError> {
    let id: uuid::Uuid = row.try_get("id").map_err(storage_error)?;
    let workspace_id: uuid::Uuid = row.try_get("workspace_id").map_err(storage_error)?;
    let session_id: Option<uuid::Uuid> = row.try_get("session_id").map_err(storage_error)?;
    let event_type: String = row.try_get("event_type").map_err(storage_error)?;
    let actor_value: serde_json::Value = row.try_get("actor").map_err(storage_error)?;
    let subject_value: Option<serde_json::Value> = row.try_get("subject").map_err(storage_error)?;
    let payload: serde_json::Value = row.try_get("payload").map_err(storage_error)?;
    let created_at = row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        .map_err(storage_error)?;

    let actor = serde_json::from_value(actor_value).map_err(storage_error)?;
    let subject = subject_value
        .map(serde_json::from_value)
        .transpose()
        .map_err(storage_error)?;

    Ok(Event {
        id: EventId::from_uuid(id),
        workspace_id: WorkspaceId::from_uuid(workspace_id),
        session_id: session_id.map(SessionId::from_uuid),
        event_type,
        actor,
        subject,
        payload,
        created_at,
    })
}
