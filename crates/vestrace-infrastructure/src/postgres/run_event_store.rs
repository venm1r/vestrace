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
        let mut transaction = self.store.begin_scoped(context).await.map_err(storage_error)?;
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
        _context: &RequestContext,
        _run_id: AgentRunId,
        _expected_version: RunVersion,
        _events: &[RunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError> {
        Err(ApplicationError::Internal(
            "run event append is not implemented".to_owned(),
        ))
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
