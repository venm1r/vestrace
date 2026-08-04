use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use vestrace_application::{
    ApplicationError, RequestContext, RunCommandCommitter,
};
use vestrace_domain::{
    id::{
        AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId,
    },
    run::{
        AgentRun, RunActor, RunEvent, RunEventEnvelope, RunStatus, RunVersion, replay,
    },
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgRunCommandCommitter {
    store: PgStore,
}

impl PgRunCommandCommitter {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[derive(Debug, FromRow)]
struct StoredRunRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    principal_id: uuid::Uuid,
    title: String,
    status: String,
    run_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<StoredRunRow> for AgentRun {
    type Error = ApplicationError;

    fn try_from(row: StoredRunRow) -> Result<Self, Self::Error> {
        let version = version_from_database(row.run_version)?;
        Ok(Self {
            id: AgentRunId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            principal_id: PrincipalId::from_uuid(row.principal_id),
            title: row.title,
            status: status_from_database(&row.status)?,
            version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct StoredEventRow {
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

impl TryFrom<StoredEventRow> for RunEventEnvelope {
    type Error = ApplicationError;

    fn try_from(row: StoredEventRow) -> Result<Self, Self::Error> {
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
impl RunCommandCommitter for PgRunCommandCommitter {
    async fn commit(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        expected_version: RunVersion,
        events: &[RunEventEnvelope],
        projection: &AgentRun,
    ) -> Result<RunVersion, ApplicationError> {
        validate_batch(context, run_id, expected_version, events, projection)?;

        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let stored_run = load_run_for_update(transaction.connection(), context, run_id).await?;
        let existing_events = load_events(transaction.connection(), context, run_id).await?;

        if expected_version == RunVersion::ZERO {
            if stored_run.is_some() || !existing_events.is_empty() {
                return Err(ApplicationError::Conflict(
                    "run already exists".to_owned(),
                ));
            }
        } else {
            let stored_run = stored_run.ok_or_else(|| {
                ApplicationError::Storage(
                    "owning run does not exist in the current workspace".to_owned(),
                )
            })?;
            validate_existing_projection(stored_run, &existing_events, expected_version)?;
        }

        let mut complete_stream = existing_events;
        complete_stream.extend_from_slice(events);
        let resulting_state = replay(complete_stream)
            .map_err(|error| ApplicationError::Storage(format!(
                "run event replay failed before commit: {error}"
            )))?
            .ok_or_else(|| {
                ApplicationError::Conflict(
                    "run command batch did not produce a projection".to_owned(),
                )
            })?;
        let derived_projection = AgentRun {
            id: resulting_state.id,
            workspace_id: resulting_state.workspace_id,
            principal_id: resulting_state.principal_id,
            title: resulting_state.title,
            status: resulting_state.status,
            version: resulting_state.version,
            created_at: resulting_state.created_at,
            updated_at: resulting_state.updated_at,
        };
        if &derived_projection != projection {
            return Err(ApplicationError::Conflict(
                "run projection does not match the resulting event state".to_owned(),
            ));
        }

        if expected_version == RunVersion::ZERO {
            insert_projection(transaction.connection(), projection).await?;
            insert_events(transaction.connection(), events).await?;
        } else {
            insert_events(transaction.connection(), events).await?;
            update_projection(
                transaction.connection(),
                context,
                run_id,
                expected_version,
                projection,
            )
            .await?;
        }

        transaction.commit().await.map_err(storage_error)?;
        Ok(projection.version)
    }
}

fn validate_batch(
    context: &RequestContext,
    run_id: AgentRunId,
    expected_version: RunVersion,
    events: &[RunEventEnvelope],
    projection: &AgentRun,
) -> Result<(), ApplicationError> {
    if events.is_empty() {
        return Err(ApplicationError::Conflict(
            "run command event batch must not be empty".to_owned(),
        ));
    }
    if projection.id != run_id
        || projection.workspace_id != context.workspace_id
        || projection.principal_id != context.principal_id
    {
        return Err(ApplicationError::Conflict(
            "run projection identity does not match the command context".to_owned(),
        ));
    }

    let mut sequence = expected_version;
    for event in events {
        sequence = sequence.next()?;
        if event.workspace_id != context.workspace_id || event.run_id != run_id {
            return Err(ApplicationError::Conflict(
                "run event identity does not match the command context".to_owned(),
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

    if projection.version != sequence {
        return Err(ApplicationError::Conflict(
            "run projection version does not match the final event sequence".to_owned(),
        ));
    }
    Ok(())
}

async fn load_run_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    context: &RequestContext,
    run_id: AgentRunId,
) -> Result<Option<AgentRun>, ApplicationError> {
    let row = sqlx::query_as::<_, StoredRunRow>(
        "SELECT
             id, workspace_id, principal_id, title, status, run_version,
             created_at, updated_at
         FROM agent_runs
         WHERE workspace_id = $1
           AND id = $2
         FOR UPDATE",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(storage_error)?;

    row.map(AgentRun::try_from).transpose()
}

async fn load_events(
    transaction: &mut Transaction<'_, Postgres>,
    context: &RequestContext,
    run_id: AgentRunId,
) -> Result<Vec<RunEventEnvelope>, ApplicationError> {
    let rows = sqlx::query_as::<_, StoredEventRow>(
        "SELECT
             id, workspace_id, run_id, sequence, event_type, event_version,
             actor, causation_id, correlation_id, payload, occurred_at,
             created_at AS recorded_at
         FROM run_events
         WHERE workspace_id = $1
           AND run_id = $2
         ORDER BY sequence ASC",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_all(&mut **transaction)
    .await
    .map_err(storage_error)?;

    rows.into_iter()
        .map(RunEventEnvelope::try_from)
        .collect()
}

fn validate_existing_projection(
    stored_run: AgentRun,
    existing_events: &[RunEventEnvelope],
    expected_version: RunVersion,
) -> Result<(), ApplicationError> {
    if stored_run.version != expected_version {
        return Err(version_conflict(expected_version, stored_run.version));
    }
    let stream_version = existing_events
        .last()
        .map(|event| event.sequence)
        .unwrap_or(RunVersion::ZERO);
    if stream_version != expected_version {
        return Err(ApplicationError::Storage(format!(
            "run projection and event stream diverged at versions {} and {}",
            stored_run.version.value(),
            stream_version.value()
        )));
    }

    let state = replay(existing_events.to_vec())
        .map_err(|error| ApplicationError::Storage(format!(
            "stored run event replay failed: {error}"
        )))?
        .ok_or_else(|| {
            ApplicationError::Storage(
                "stored run projection has no authoritative events".to_owned(),
            )
        })?;
    let event_projection = AgentRun {
        id: state.id,
        workspace_id: state.workspace_id,
        principal_id: state.principal_id,
        title: state.title,
        status: state.status,
        version: state.version,
        created_at: state.created_at,
        updated_at: state.updated_at,
    };
    if event_projection != stored_run {
        return Err(ApplicationError::Storage(
            "stored run projection does not match its event stream".to_owned(),
        ));
    }
    Ok(())
}

async fn insert_projection(
    transaction: &mut Transaction<'_, Postgres>,
    projection: &AgentRun,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO agent_runs (
             id, workspace_id, principal_id, title, status, run_version,
             created_at, updated_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(projection.id.as_uuid())
    .bind(projection.workspace_id.as_uuid())
    .bind(projection.principal_id.as_uuid())
    .bind(&projection.title)
    .bind(status_name(projection.status))
    .bind(version_to_database(projection.version)?)
    .bind(projection.created_at)
    .bind(projection.updated_at)
    .execute(&mut **transaction)
    .await
    .map_err(database_write_error)?;
    Ok(())
}

async fn update_projection(
    transaction: &mut Transaction<'_, Postgres>,
    context: &RequestContext,
    run_id: AgentRunId,
    expected_version: RunVersion,
    projection: &AgentRun,
) -> Result<(), ApplicationError> {
    let result = sqlx::query(
        "UPDATE agent_runs
         SET status = $1,
             run_version = $2,
             updated_at = $3
         WHERE workspace_id = $4
           AND id = $5
           AND run_version = $6",
    )
    .bind(status_name(projection.status))
    .bind(version_to_database(projection.version)?)
    .bind(projection.updated_at)
    .bind(context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(version_to_database(expected_version)?)
    .execute(&mut **transaction)
    .await
    .map_err(database_write_error)?;

    if result.rows_affected() != 1 {
        return Err(ApplicationError::Conflict(
            "run projection changed before command commit".to_owned(),
        ));
    }
    Ok(())
}

async fn insert_events(
    transaction: &mut Transaction<'_, Postgres>,
    events: &[RunEventEnvelope],
) -> Result<(), ApplicationError> {
    for event in events {
        sqlx::query(
            "INSERT INTO run_events (
                 id, workspace_id, run_id, sequence, event_type, event_version,
                 actor, causation_id, correlation_id, payload, occurred_at, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
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
        .execute(&mut **transaction)
        .await
        .map_err(database_write_error)?;
    }
    Ok(())
}

fn status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Created => "created",
        RunStatus::Ready => "ready",
        RunStatus::Running => "running",
        RunStatus::WaitingForInput => "waiting_for_input",
        RunStatus::WaitingForApproval => "waiting_for_approval",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Stalled => "stalled",
    }
}

fn status_from_database(status: &str) -> Result<RunStatus, ApplicationError> {
    match status {
        "created" => Ok(RunStatus::Created),
        "ready" => Ok(RunStatus::Ready),
        "running" => Ok(RunStatus::Running),
        "waiting_for_input" => Ok(RunStatus::WaitingForInput),
        "waiting_for_approval" => Ok(RunStatus::WaitingForApproval),
        "completed" => Ok(RunStatus::Completed),
        "failed" => Ok(RunStatus::Failed),
        "cancelled" => Ok(RunStatus::Cancelled),
        "stalled" => Ok(RunStatus::Stalled),
        _ => Err(ApplicationError::Storage(
            "stored run status is unsupported".to_owned(),
        )),
    }
}

fn version_to_database(version: RunVersion) -> Result<i64, ApplicationError> {
    i64::try_from(version.value()).map_err(storage_error)
}

fn version_from_database(version: i64) -> Result<RunVersion, ApplicationError> {
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
            "run command commit conflicts with an existing row".to_owned(),
        );
    }
    storage_error(error)
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
