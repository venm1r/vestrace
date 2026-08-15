use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use vestrace_application::{
    ApplicationError, RUN_CHECKPOINT_FORMAT_VERSION, RequestContext, RunCheckpoint,
    RunRecoveryStore, hash_run_state,
};
use vestrace_domain::{
    DomainError,
    id::{AgentRunId, CorrelationId, OperationId, RunEventId, WorkspaceId},
    run::{
        AgentRun, LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunState, RunStatus, RunVersion,
    },
};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgRunRecoveryStore {
    store: PgStore,
}

impl PgRunRecoveryStore {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[derive(Debug, FromRow)]
struct CheckpointRow {
    workspace_id: uuid::Uuid,
    run_id: uuid::Uuid,
    sequence: i64,
    format_version: i16,
    state_hash: String,
    state: Value,
    created_at: DateTime<Utc>,
}

impl TryFrom<CheckpointRow> for RunCheckpoint {
    type Error = ApplicationError;

    fn try_from(row: CheckpointRow) -> Result<Self, Self::Error> {
        let sequence = version_from_database(row.sequence)?;
        if sequence == RunVersion::ZERO {
            return Err(storage_corruption(
                "stored run checkpoint sequence must be positive",
            ));
        }
        let format_version = u16::try_from(row.format_version).map_err(storage_error)?;
        if format_version != RUN_CHECKPOINT_FORMAT_VERSION {
            return Err(storage_corruption(
                "stored run checkpoint format is unsupported",
            ));
        }
        let state = serde_json::from_value::<RunState>(row.state).map_err(storage_error)?;
        let checkpoint = RunCheckpoint {
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            run_id: AgentRunId::from_uuid(row.run_id),
            sequence,
            format_version,
            state_hash: row.state_hash,
            state,
            created_at: row.created_at,
        };
        validate_checkpoint_identity(&checkpoint)?;
        Ok(checkpoint)
    }
}

#[derive(Debug, FromRow)]
struct EventRow {
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

impl TryFrom<EventRow> for LegacyRunEventEnvelope {
    type Error = ApplicationError;

    fn try_from(row: EventRow) -> Result<Self, Self::Error> {
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
impl RunRecoveryStore for PgRunRecoveryStore {
    async fn load_stream_head(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunVersion>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let version = sqlx::query_scalar::<_, i64>(
            "SELECT current_version
             FROM run_streams
             WHERE workspace_id = $1
               AND run_id = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .map(version_from_database)
        .transpose()?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(version)
    }

    async fn load_events_through(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        through: RunVersion,
    ) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT
                 id, workspace_id, run_id, sequence, event_type, event_version,
                 actor, causation_id, correlation_id, payload, occurred_at,
                 created_at AS recorded_at
             FROM run_events
             WHERE workspace_id = $1
               AND run_id = $2
               AND sequence <= $3
             ORDER BY sequence ASC",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(version_to_database(through)?)
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        let events = decode_events(rows)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(events)
    }

    async fn load_events_after(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        after: RunVersion,
    ) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT
                 id, workspace_id, run_id, sequence, event_type, event_version,
                 actor, causation_id, correlation_id, payload, occurred_at,
                 created_at AS recorded_at
             FROM run_events
             WHERE workspace_id = $1
               AND run_id = $2
               AND sequence > $3
             ORDER BY sequence ASC",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(version_to_database(after)?)
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        let events = decode_events(rows)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(events)
    }

    async fn load_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        sequence: RunVersion,
    ) -> Result<Option<RunCheckpoint>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query_as::<_, CheckpointRow>(
            "SELECT
                 workspace_id, run_id, sequence, format_version,
                 state_hash, state, created_at
             FROM run_checkpoints
             WHERE workspace_id = $1
               AND run_id = $2
               AND sequence = $3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .bind(version_to_database(sequence)?)
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        let checkpoint = row.map(RunCheckpoint::try_from).transpose()?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(checkpoint)
    }

    async fn load_latest_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunCheckpoint>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query_as::<_, CheckpointRow>(
            "SELECT
                 workspace_id, run_id, sequence, format_version,
                 state_hash, state, created_at
             FROM run_checkpoints
             WHERE workspace_id = $1
               AND run_id = $2
             ORDER BY sequence DESC
             LIMIT 1",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(run_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        let checkpoint = row.map(RunCheckpoint::try_from).transpose()?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(checkpoint)
    }

    async fn save_checkpoint(
        &self,
        context: &RequestContext,
        checkpoint: &RunCheckpoint,
    ) -> Result<(), ApplicationError> {
        validate_checkpoint_for_write(context, checkpoint)?;
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let state = serde_json::to_value(&checkpoint.state).map_err(storage_error)?;
        let result = sqlx::query(
            "INSERT INTO run_checkpoints (
                 workspace_id, run_id, sequence, format_version,
                 state_hash, state, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (workspace_id, run_id, sequence) DO NOTHING",
        )
        .bind(checkpoint.workspace_id.as_uuid())
        .bind(checkpoint.run_id.as_uuid())
        .bind(version_to_database(checkpoint.sequence)?)
        .bind(i16::try_from(checkpoint.format_version).map_err(storage_error)?)
        .bind(&checkpoint.state_hash)
        .bind(state)
        .bind(checkpoint.created_at)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 0 {
            let existing = load_checkpoint_for_update(
                transaction.connection(),
                checkpoint.workspace_id,
                checkpoint.run_id,
                checkpoint.sequence,
            )
            .await?
            .ok_or_else(|| storage_corruption("checkpoint conflict row is not visible"))?;
            if existing != *checkpoint {
                return Err(ApplicationError::Conflict(
                    "run checkpoint sequence already stores different content".to_owned(),
                ));
            }
        }

        transaction.commit().await.map_err(storage_error)?;
        Ok(())
    }

    async fn replace_projection(
        &self,
        context: &RequestContext,
        expected_stream_version: RunVersion,
        projection: &AgentRun,
    ) -> Result<(), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let actual = sqlx::query_scalar::<_, i64>(
            "SELECT current_version
             FROM run_streams
             WHERE workspace_id = $1
               AND run_id = $2
             FOR UPDATE",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(projection.id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .ok_or_else(|| DomainError::NotFound("run stream does not exist".to_owned()))?;
        let actual = version_from_database(actual)?;
        if actual != expected_stream_version {
            return Err(version_conflict(expected_stream_version, actual));
        }
        if projection.workspace_id != context.workspace_id
            || projection.version != expected_stream_version
        {
            return Err(ApplicationError::Conflict(
                "run projection identity or version does not match the stream".to_owned(),
            ));
        }

        sqlx::query(
            // `objective` is NOT NULL. Like `PgRunRepository`, the projection
            // writes the objective into both columns; `title` is the legacy
            // compatibility name for the same text.
            // `finished_at` is required for terminal statuses by
            // chk_agent_runs_terminal_has_finished_at, so the projection must
            // carry it through rather than rebuilding a run without it.
            // A rebuilt projection must be indistinguishable from the one the
            // command committer writes, so it sets the same columns.
            "INSERT INTO agent_runs (
                 id, workspace_id, principal_id, title, objective,
                 coordinator_snapshot_id, execution_mode, status, run_version,
                 root_run_id, created_at, updated_at, finished_at
             ) VALUES ($1, $2, $3, $4, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (workspace_id, id) DO UPDATE
             SET principal_id = EXCLUDED.principal_id,
                 title = EXCLUDED.title,
                 objective = EXCLUDED.objective,
                 coordinator_snapshot_id = EXCLUDED.coordinator_snapshot_id,
                 execution_mode = EXCLUDED.execution_mode,
                 status = EXCLUDED.status,
                 run_version = EXCLUDED.run_version,
                 root_run_id = EXCLUDED.root_run_id,
                 created_at = EXCLUDED.created_at,
                 updated_at = EXCLUDED.updated_at,
                 finished_at = EXCLUDED.finished_at",
        )
        .bind(projection.id.as_uuid())
        .bind(projection.workspace_id.as_uuid())
        .bind(projection.coordinator_snapshot_id.as_uuid())
        .bind(&projection.objective)
        .bind(projection.coordinator_snapshot_id.as_uuid())
        .bind(projection.execution_mode.as_str())
        .bind(status_name(projection.status))
        .bind(version_to_database(projection.version)?)
        .bind(projection.root_run_id.as_uuid())
        .bind(projection.created_at)
        .bind(projection.updated_at)
        .bind(projection.finished_at)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;
        Ok(())
    }
}

async fn load_checkpoint_for_update(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    run_id: AgentRunId,
    sequence: RunVersion,
) -> Result<Option<RunCheckpoint>, ApplicationError> {
    let row = sqlx::query_as::<_, CheckpointRow>(
        "SELECT
             workspace_id, run_id, sequence, format_version,
             state_hash, state, created_at
         FROM run_checkpoints
         WHERE workspace_id = $1
           AND run_id = $2
           AND sequence = $3
         FOR UPDATE",
    )
    .bind(workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .bind(version_to_database(sequence)?)
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?;
    row.map(RunCheckpoint::try_from).transpose()
}

fn decode_events(rows: Vec<EventRow>) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
    rows.into_iter()
        .map(LegacyRunEventEnvelope::try_from)
        .collect()
}

fn validate_checkpoint_for_write(
    context: &RequestContext,
    checkpoint: &RunCheckpoint,
) -> Result<(), ApplicationError> {
    if checkpoint.workspace_id != context.workspace_id {
        return Err(ApplicationError::Conflict(
            "run checkpoint workspace does not match request context".to_owned(),
        ));
    }
    validate_checkpoint_identity(checkpoint)
}

fn validate_checkpoint_identity(checkpoint: &RunCheckpoint) -> Result<(), ApplicationError> {
    if checkpoint.format_version != RUN_CHECKPOINT_FORMAT_VERSION {
        return Err(storage_corruption("run checkpoint format is unsupported"));
    }
    if checkpoint.sequence == RunVersion::ZERO
        || checkpoint.state.version != checkpoint.sequence
        || checkpoint.state.workspace_id != checkpoint.workspace_id
        || checkpoint.state.id != checkpoint.run_id
    {
        return Err(storage_corruption(
            "run checkpoint identity or version is inconsistent",
        ));
    }
    if hash_run_state(&checkpoint.state)? != checkpoint.state_hash {
        return Err(storage_corruption("run checkpoint hash is invalid"));
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

fn status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Created => "created",
        RunStatus::Preparing => "preparing",
        RunStatus::Running => "running",
        RunStatus::WaitingForInput => "waiting_for_input",
        RunStatus::WaitingForApproval => "waiting_for_approval",
        RunStatus::WaitingForDependency => "waiting_for_dependency",
        RunStatus::Succeeded => "succeeded",
        RunStatus::SucceededWithWarnings => "succeeded_with_warnings",
        RunStatus::Partial => "partial",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Paused => "paused",
        RunStatus::PausedPolicyChanged => "paused_policy_changed",
        RunStatus::Expired => "expired",
    }
}

fn version_conflict(expected: RunVersion, actual: RunVersion) -> ApplicationError {
    ApplicationError::Conflict(format!(
        "run version conflict: expected {}, actual {}",
        expected.value(),
        actual.value()
    ))
}

fn storage_corruption(message: &str) -> ApplicationError {
    ApplicationError::Storage(message.to_owned())
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
