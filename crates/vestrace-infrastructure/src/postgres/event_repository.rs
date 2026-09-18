use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{ApplicationError, EventRepository, RequestContext};
use vestrace_domain::{
    Event,
    id::{EventId, SessionId, WorkspaceId},
};

use super::PgStore;

pub struct PgEventRepository {
    store: PgStore,
}

impl PgEventRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl EventRepository for PgEventRepository {
    async fn save(&self, context: &RequestContext, event: &Event) -> Result<(), ApplicationError> {
        if event.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "an event cannot be recorded into another workspace".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO events
                (id, workspace_id, session_id, event_type, actor, subject, payload, created_at, occurred_at, recorded_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
        .bind(event.occurred_at)
        .bind(event.recorded_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: EventId,
    ) -> Result<Option<Event>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The workspace predicate is new. This selected on `id` alone, so an
        // event id from any tenant fetched that tenant's event — and the policy
        // did not stop it either, because `events` enables row level security
        // without forcing it and the runtime role owns the table.
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, session_id, event_type, actor, subject, payload,
                   created_at, occurred_at, recorded_at
            FROM events
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
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
    let occurred_at = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("occurred_at")
        .map_err(storage_error)?;
    let recorded_at = row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("recorded_at")
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
        occurred_at,
        recorded_at,
    })
}
