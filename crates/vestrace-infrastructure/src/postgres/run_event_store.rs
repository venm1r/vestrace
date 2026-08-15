use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use vestrace_application::{ApplicationError, RequestContext, RunEventStore};
use vestrace_domain::{
    DomainError,
    id::{AgentRunId, CorrelationId, OperationId, RunEventId, WorkspaceId},
    run::{LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunVersion},
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgRunEventStore {
    store: PgStore,
}

impl PgRunEventStore {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[derive(Debug, FromRow)]
struct RunEventRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    run_id: uuid::Uuid,
    sequence: i64,
    event_type: String,
    event_version: i16,
    actor: Value,
    causation_id: uuid::Uuid,
    correlation_id: uuid::Uuid,
    payload: Value,
    occurred_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
}

impl TryFrom<RunEventRow> for LegacyRunEventEnvelope {
    type Error = ApplicationError;

    fn try_from(row: RunEventRow) -> Result<Self, Self::Error> {
        let sequence = version_from_database(row.sequence)?;
        if sequence == RunVersion::ZERO {
            return Err(storage_corruption(
                "stored run event sequence must be positive",
            ));
        }
        let event_version = u16::try_from(row.event_version).map_err(storage_error)?;
        let payload =
            serde_json::from_value::<LegacyRunEvent>(row.payload).map_err(storage_error)?;
        if row.event_type != payload.event_type() || event_version != payload.event_version() {
            return Err(storage_corruption(
                "stored run event metadata does not match its payload",
            ));
        }
        Ok(Self {
            event_id: RunEventId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            run_id: AgentRunId::from_uuid(row.run_id),
            sequence,
            event_type: row.event_type,
            event_version,
            actor: serde_json::from_value::<RunActor>(row.actor).map_err(storage_error)?,
            causation_id: OperationId::from_uuid(row.causation_id),
            correlation_id: CorrelationId::from_uuid(row.correlation_id),
            payload,
            occurred_at: row.occurred_at,
            recorded_at: row.recorded_at,
        })
    }
}

#[async_trait]
impl RunEventStore for PgRunEventStore {
    async fn load_stream(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let events = load_events(transaction.connection(), context, run_id).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(events)
    }

    async fn append(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        expected_version: RunVersion,
        events: &[LegacyRunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError> {
        let final_version = validate_event_batch(context, run_id, expected_version, events)?;
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let actual_version = lock_or_create_stream(
            transaction.connection(),
            context,
            run_id,
            expected_version,
            events[0].recorded_at,
        )
        .await?;
        if actual_version != expected_version {
            return Err(version_conflict(expected_version, actual_version));
        }

        let stored_head = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(sequence), 0)
             FROM run_events
             WHERE workspace_id = $1
               AND run_id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        let stored_head = version_from_database(stored_head)?;
        if stored_head != actual_version {
            return Err(storage_corruption(&format!(
                "run stream metadata and events diverged at versions {} and {}",
                actual_version.value(),
                stored_head.value()
            )));
        }

        insert_events(transaction.connection(), events).await?;
        advance_stream(
            transaction.connection(),
            context,
            run_id,
            expected_version,
            final_version,
            events
                .last()
                .expect("validated non-empty batch")
                .recorded_at,
        )
        .await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(final_version)
    }
}

async fn lock_or_create_stream(
    connection: &mut PgConnection,
    context: &RequestContext,
    run_id: AgentRunId,
    expected_version: RunVersion,
    created_at: DateTime<Utc>,
) -> Result<RunVersion, ApplicationError> {
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT current_version
         FROM run_streams
         WHERE workspace_id = $1
           AND run_id = $2
         FOR UPDATE",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?;

    if let Some(version) = existing {
        return version_from_database(version);
    }
    if expected_version != RunVersion::ZERO {
        return Err(DomainError::NotFound("run stream does not exist".to_owned()).into());
    }

    sqlx::query(
        "INSERT INTO run_streams (
             workspace_id, run_id, current_version, created_at, updated_at
         ) VALUES ($1, $2, 0, $3, $3)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(created_at)
    .execute(&mut *connection)
    .await
    .map_err(database_write_error)?;
    Ok(RunVersion::ZERO)
}

async fn load_events(
    connection: &mut PgConnection,
    context: &RequestContext,
    run_id: AgentRunId,
) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
    let rows = sqlx::query_as::<_, RunEventRow>(
        "SELECT
             id, workspace_id, run_id, sequence, event_type, event_version,
             actor, causation_id, correlation_id, payload, occurred_at,
             recorded_at
         FROM run_events
         WHERE workspace_id = $1
           AND run_id = $2
         ORDER BY sequence ASC",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    rows.into_iter()
        .map(LegacyRunEventEnvelope::try_from)
        .collect()
}

fn validate_event_batch(
    context: &RequestContext,
    run_id: AgentRunId,
    expected_version: RunVersion,
    events: &[LegacyRunEventEnvelope],
) -> Result<RunVersion, ApplicationError> {
    if events.is_empty() {
        return Err(ApplicationError::Conflict(
            "run event append batch must not be empty".to_owned(),
        ));
    }
    let mut sequence = expected_version;
    for event in events {
        sequence = sequence.next()?;
        if event.workspace_id != context.workspace_id || event.run_id != run_id {
            return Err(ApplicationError::Conflict(
                "run event identity does not match append context".to_owned(),
            ));
        }
        if event.sequence != sequence {
            return Err(ApplicationError::Conflict(format!(
                "run event sequence must be {}, got {}",
                sequence.value(),
                event.sequence.value()
            )));
        }
        if event.event_type != event.payload.event_type() {
            return Err(ApplicationError::Conflict(
                "run event type does not match payload".to_owned(),
            ));
        }
        if event.event_version != event.payload.event_version() {
            return Err(ApplicationError::Conflict(
                "run event version does not match payload".to_owned(),
            ));
        }
    }
    Ok(sequence)
}

async fn insert_events(
    connection: &mut PgConnection,
    events: &[LegacyRunEventEnvelope],
) -> Result<(), ApplicationError> {
    for event in events {
        sqlx::query(
            "INSERT INTO run_events (
                 id, workspace_id, run_id, sequence, run_version, sequence_value,
                 event_type, event_version, actor, causation_id, correlation_id,
                 payload, occurred_at, recorded_at
             ) VALUES ($1, $2, $3, $4, $4, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(event.event_id.as_uuid())
        .bind(event.workspace_id.as_uuid())
        .bind(event.run_id.as_uuid())
        .bind(version_to_database(event.sequence)?)
        .bind(&event.event_type)
        .bind(i16::try_from(event.event_version).map_err(storage_error)?)
        .bind(serde_json::to_value(&event.actor).map_err(storage_error)?)
        .bind(event.causation_id.as_uuid())
        .bind(event.correlation_id.as_uuid())
        .bind(serde_json::to_value(&event.payload).map_err(storage_error)?)
        .bind(event.occurred_at)
        .bind(event.recorded_at)
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
    }
    Ok(())
}

async fn advance_stream(
    connection: &mut PgConnection,
    context: &RequestContext,
    run_id: AgentRunId,
    expected_version: RunVersion,
    new_version: RunVersion,
    updated_at: DateTime<Utc>,
) -> Result<(), ApplicationError> {
    let result = sqlx::query(
        "UPDATE run_streams
         SET current_version = $1,
             updated_at = $2
         WHERE workspace_id = $3
           AND run_id = $4
           AND current_version = $5",
    )
    .bind(version_to_database(new_version)?)
    .bind(updated_at)
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(version_to_database(expected_version)?)
    .execute(&mut *connection)
    .await
    .map_err(database_write_error)?;
    if result.rows_affected() != 1 {
        return Err(ApplicationError::Conflict(
            "run stream changed before event append".to_owned(),
        ));
    }
    Ok(())
}

fn version_to_database(version: RunVersion) -> Result<i64, ApplicationError> {
    i64::try_from(version.value()).map_err(storage_error)
}

fn version_from_database(version: i64) -> Result<RunVersion, ApplicationError> {
    if version == 0 {
        return Ok(RunVersion::ZERO);
    }
    let value = u64::try_from(version).map_err(storage_error)?;
    RunVersion::new(value).map_err(ApplicationError::from)
}

fn version_conflict(expected: RunVersion, actual: RunVersion) -> ApplicationError {
    ApplicationError::Conflict(format!(
        "run version conflict: expected {}, actual {}",
        expected.value(),
        actual.value()
    ))
}

fn database_write_error(error: sqlx::Error) -> ApplicationError {
    if error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .as_deref()
        == Some("23505")
    {
        return ApplicationError::Conflict(
            "run event append conflicts with an existing row".to_owned(),
        );
    }
    storage_error(error)
}

fn storage_corruption(message: &str) -> ApplicationError {
    ApplicationError::Storage(message.to_owned())
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
