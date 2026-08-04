use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;
use vestrace_application::{ApplicationError, RequestContext, RunEventStore};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, RunEventId, WorkspaceId},
    run::{RunActor, RunEvent, RunEventEnvelope, RunVersion},
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

impl TryFrom<RunEventRow> for RunEventEnvelope {
    type Error = ApplicationError;

    fn try_from(row: RunEventRow) -> Result<Self, Self::Error> {
        let sequence = u64::try_from(row.sequence).map_err(storage_error)?;
        let event_version = u16::try_from(row.event_version).map_err(storage_error)?;

        Ok(Self {
            event_id: RunEventId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            run_id: AgentRunId::from_uuid(row.run_id),
            sequence: RunVersion::new(sequence)?,
            event_type: row.event_type,
            event_version,
            actor: serde_json::from_value::<RunActor>(row.actor).map_err(storage_error)?,
            causation_id: OperationId::from_uuid(row.causation_id),
            correlation_id: CorrelationId::from_uuid(row.correlation_id),
            payload: serde_json::from_value::<RunEvent>(row.payload).map_err(storage_error)?,
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
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query_as::<_, RunEventRow>(
            "SELECT
                 id,
                 workspace_id,
                 run_id,
                 sequence,
                 event_type,
                 event_version,
                 actor,
                 causation_id,
                 correlation_id,
                 payload,
                 occurred_at,
                 created_at AS recorded_at
             FROM run_events
             WHERE workspace_id = $1
               AND run_id = $2
             ORDER BY sequence ASC",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;

        let events = rows
            .into_iter()
            .map(RunEventEnvelope::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        transaction.commit().await.map_err(storage_error)?;
        Ok(events)
    }

    async fn append(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        expected_version: RunVersion,
        events: &[RunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError> {
        validate_batch(context, run_id, expected_version, events)?;

        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let owner = sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT id
             FROM agent_runs
             WHERE workspace_id = $1
               AND id = $2
             FOR UPDATE",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;

        if owner.is_none() {
            return Err(ApplicationError::Storage(
                "owning run does not exist in the current workspace".to_owned(),
            ));
        }

        let actual_sequence = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(sequence), 0)::BIGINT
             FROM run_events
             WHERE workspace_id = $1
               AND run_id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        let actual_version = version_from_sequence(actual_sequence)?;

        if actual_version != expected_version {
            return Err(version_conflict(expected_version, actual_version));
        }

        for event in events {
            let sequence = i64::try_from(event.sequence.value()).map_err(storage_error)?;
            let event_version = i16::try_from(event.event_version).map_err(storage_error)?;
            let actor = serde_json::to_value(&event.actor).map_err(storage_error)?;
            let payload = serde_json::to_value(&event.payload).map_err(storage_error)?;

            sqlx::query(
                "INSERT INTO run_events (
                     id,
                     workspace_id,
                     run_id,
                     sequence,
                     event_type,
                     event_version,
                     actor,
                     causation_id,
                     correlation_id,
                     payload,
                     occurred_at,
                     created_at
                 ) VALUES (
                     $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
                 )",
            )
            .bind(event.event_id.as_uuid())
            .bind(event.workspace_id.as_uuid())
            .bind(event.run_id.as_uuid())
            .bind(sequence)
            .bind(&event.event_type)
            .bind(event_version)
            .bind(actor)
            .bind(event.causation_id.as_uuid())
            .bind(event.correlation_id.as_uuid())
            .bind(payload)
            .bind(event.occurred_at)
            .bind(event.recorded_at)
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        }

        let resulting_version = events
            .last()
            .map(|event| event.sequence)
            .ok_or_else(|| ApplicationError::Conflict("empty event batch".to_owned()))?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(resulting_version)
    }
}

fn validate_batch(
    context: &RequestContext,
    run_id: AgentRunId,
    expected_version: RunVersion,
    events: &[RunEventEnvelope],
) -> Result<(), ApplicationError> {
    if events.is_empty() {
        return Err(ApplicationError::Conflict(
            "run event batch must not be empty".to_owned(),
        ));
    }

    let mut expected_sequence = expected_version.next()?;
    for event in events {
        if event.workspace_id != context.workspace_id {
            return Err(ApplicationError::Conflict(
                "run event workspace does not match request context".to_owned(),
            ));
        }
        if event.run_id != run_id {
            return Err(ApplicationError::Conflict(
                "run event aggregate does not match requested run".to_owned(),
            ));
        }
        if event.sequence != expected_sequence {
            return Err(ApplicationError::Conflict(format!(
                "run event sequence must be {}, got {}",
                expected_sequence.value(),
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
        expected_sequence = expected_sequence.next()?;
    }

    Ok(())
}

fn version_from_sequence(sequence: i64) -> Result<RunVersion, ApplicationError> {
    if sequence == 0 {
        return Ok(RunVersion::ZERO);
    }

    let sequence = u64::try_from(sequence).map_err(storage_error)?;
    RunVersion::new(sequence).map_err(ApplicationError::from)
}

fn version_conflict(expected: RunVersion, actual: RunVersion) -> ApplicationError {
    ApplicationError::Conflict(format!(
        "run version conflict: expected {}, actual {}",
        expected.value(),
        actual.value()
    ))
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
