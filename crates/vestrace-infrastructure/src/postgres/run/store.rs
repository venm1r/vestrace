use async_trait::async_trait;
use sqlx::FromRow;
use vestrace_application::run::ports::{CommitRun, RunSnapshot, RunStorePort};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::id::{
    AgentRunId, AgentRuntimeSnapshotId, BudgetSnapshotId, PlanRevisionId, ResourceUsageSnapshotId,
    RunCheckpointId, RunStepId, WorkspaceId,
};
use vestrace_domain::run::{
    AgentRun, RunCheckpoint, RunEvent, RunExecutionMode, RunStatus, RunStep, RunStepStatus,
    RunTerminalResult, RunVersion,
};
use vestrace_domain::time::Timestamp;

use super::super::PgStore;

/// The durable run store.
///
/// Holds the `PgStore` rather than a bare pool so every statement runs inside a
/// workspace-scoped transaction. It previously used the pool directly, which
/// meant `vestrace.workspace_id` was never set: under `FORCE ROW LEVEL
/// SECURITY` the policies then match nothing and every read and write is
/// refused. It failed closed rather than leaking, but it could not work at all.
#[derive(Clone)]
pub struct PostgresRunStore {
    store: PgStore,
}

impl PostgresRunStore {
    pub fn new(store: &PgStore) -> Self {
        Self {
            store: store.clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct AgentRunRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    objective: String,
    coordinator_snapshot_id: uuid::Uuid,
    active_plan_revision_id: Option<uuid::Uuid>,
    execution_mode: String,
    status: String,
    current_step_id: Option<uuid::Uuid>,
    checkpoint_id: Option<uuid::Uuid>,
    parent_run_id: Option<uuid::Uuid>,
    parent_step_id: Option<uuid::Uuid>,
    root_run_id: Option<uuid::Uuid>,
    budget_snapshot_id: Option<uuid::Uuid>,
    resource_usage_snapshot_id: Option<uuid::Uuid>,
    run_version: i64,
    result: Option<serde_json::Value>,
    created_at: Timestamp,
    updated_at: Timestamp,
    finished_at: Option<Timestamp>,
}

fn parse_status(s: &str) -> Result<RunStatus, ApplicationError> {
    RunStatus::parse(s).ok_or_else(|| ApplicationError::Storage(format!("unknown run status: {s}")))
}

fn parse_execution_mode(s: &str) -> Result<RunExecutionMode, ApplicationError> {
    RunExecutionMode::parse(s)
        .ok_or_else(|| ApplicationError::Storage(format!("unknown execution mode: {s}")))
}

fn parse_step_status(s: &str) -> Result<RunStepStatus, ApplicationError> {
    RunStepStatus::parse(s)
        .ok_or_else(|| ApplicationError::Storage(format!("unknown step status: {s}")))
}

impl TryFrom<AgentRunRow> for AgentRun {
    type Error = ApplicationError;

    fn try_from(row: AgentRunRow) -> Result<Self, Self::Error> {
        let status = parse_status(&row.status)?;
        let execution_mode = parse_execution_mode(&row.execution_mode)?;
        let run_version =
            u64::try_from(row.run_version).map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let version =
            RunVersion::new(run_version).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let root_run_id = row
            .root_run_id
            .map(AgentRunId::from_uuid)
            .unwrap_or_else(|| AgentRunId::from_uuid(row.id));

        let parent = row
            .parent_run_id
            .zip(row.parent_step_id)
            .map(|(prid, psid)| vestrace_domain::run::ParentRunLink {
                parent_run_id: AgentRunId::from_uuid(prid),
                parent_step_id: RunStepId::from_uuid(psid),
            });

        let result: Option<RunTerminalResult> = row
            .result
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(Self {
            id: AgentRunId::from_uuid(row.id),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            objective: row.objective,
            coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(row.coordinator_snapshot_id),
            active_plan_revision_id: row.active_plan_revision_id.map(PlanRevisionId::from_uuid),
            execution_mode,
            status,
            current_step_id: row.current_step_id.map(RunStepId::from_uuid),
            checkpoint_id: row.checkpoint_id.map(RunCheckpointId::from_uuid),
            parent,
            root_run_id,
            budget_snapshot_id: row.budget_snapshot_id.map(BudgetSnapshotId::from_uuid),
            resource_usage_snapshot_id: row
                .resource_usage_snapshot_id
                .map(ResourceUsageSnapshotId::from_uuid),
            version,
            result,
            created_at: row.created_at,
            updated_at: row.updated_at,
            finished_at: row.finished_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct RunStepRow {
    id: uuid::Uuid,
    run_id: uuid::Uuid,
    plan_step_reference: Option<String>,
    assigned_actor: serde_json::Value,
    input_references: serde_json::Value,
    status: String,
    attempt: i32,
    output_references: serde_json::Value,
    error: Option<serde_json::Value>,
    created_at: Timestamp,
    started_at: Option<Timestamp>,
    finished_at: Option<Timestamp>,
}

impl TryFrom<RunStepRow> for RunStep {
    type Error = ApplicationError;

    fn try_from(row: RunStepRow) -> Result<Self, Self::Error> {
        let status = parse_step_status(&row.status)?;
        let attempt =
            u32::try_from(row.attempt).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let assigned_actor: vestrace_domain::run::RunActorRef =
            serde_json::from_value(row.assigned_actor)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let input_references: Vec<vestrace_domain::run::RunReference> =
            serde_json::from_value(row.input_references)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let output_references: Vec<vestrace_domain::run::RunReference> =
            serde_json::from_value(row.output_references)
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let error: Option<vestrace_domain::run::RunFailure> = row
            .error
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(Self {
            id: RunStepId::from_uuid(row.id),
            run_id: AgentRunId::from_uuid(row.run_id),
            plan_step_reference: row.plan_step_reference,
            assigned_actor,
            input_references,
            status,
            attempt,
            output_references,
            error,
            created_at: row.created_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
        })
    }
}

/// Mirrors `run_checkpoints` as migrations 0112 and 0131 left it: keyed by
/// `(workspace_id, run_id, sequence)` with no `id` column, and no
/// `active_plan_revision_id`.
#[derive(Debug, FromRow)]
struct RunCheckpointRow {
    workspace_id: uuid::Uuid,
    run_id: uuid::Uuid,
    sequence: i64,
    state: serde_json::Value,
    created_at: Timestamp,
}

/// A stable id for a checkpoint that the table no longer stores one for.
///
/// Derived from `(run_id, sequence)`, which is the checkpoint's actual
/// identity after migration 0112. A random id would make two reads of the same
/// checkpoint compare unequal, which replay and recovery both rely on.
fn checkpoint_identity(run_id: uuid::Uuid, sequence: i64) -> uuid::Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(run_id.as_bytes());
    hasher.update(sequence.to_be_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid::Uuid::from_bytes(bytes)
}

impl TryFrom<RunCheckpointRow> for RunCheckpoint {
    type Error = ApplicationError;

    fn try_from(row: RunCheckpointRow) -> Result<Self, Self::Error> {
        let sequence =
            u64::try_from(row.sequence).map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let version =
            RunVersion::new(sequence).map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let payload: vestrace_domain::run::RunCheckpointPayload = serde_json::from_value(row.state)
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        Ok(Self {
            // The table no longer has an `id` column: a checkpoint is
            // identified by `(workspace_id, run_id, sequence)`. The domain type
            // still carries an id, so it is derived from the sequence rather
            // than invented at random, which would make two reads of the same
            // checkpoint compare unequal.
            id: RunCheckpointId::from_uuid(checkpoint_identity(row.run_id, row.sequence)),
            workspace_id: WorkspaceId::from_uuid(row.workspace_id),
            run_id: AgentRunId::from_uuid(row.run_id),
            run_version: version,
            // Dropped by migration 0131; nothing stores it any more.
            active_plan_revision_id: None,
            resume_cursor: vestrace_domain::run::ResumeCursor::from_version(version),
            payload,
            created_at: row.created_at,
        })
    }
}

#[async_trait]
impl RunStorePort for PostgresRunStore {
    async fn load(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError> {
        let ws = context.workspace_id.as_uuid();
        let rid = run_id.as_uuid();

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let run_row: Option<AgentRunRow> = sqlx::query_as::<_, AgentRunRow>(
            r#"
            SELECT id, workspace_id, objective, coordinator_snapshot_id,
                   active_plan_revision_id, execution_mode, status,
                   current_step_id, checkpoint_id, parent_run_id, parent_step_id,
                   root_run_id, budget_snapshot_id, resource_usage_snapshot_id,
                   run_version, result, created_at, updated_at, finished_at
            FROM agent_runs
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(ws)
        .bind(rid)
        .fetch_optional(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let Some(run_row) = run_row else {
            scoped
                .commit()
                .await
                .map_err(|e| ApplicationError::Storage(e.to_string()))?;
            return Ok(None);
        };

        let run = AgentRun::try_from(run_row)?;

        let step_rows: Vec<RunStepRow> = sqlx::query_as::<_, RunStepRow>(
            r#"
            SELECT id, run_id, plan_step_reference, assigned_actor,
                   input_references, status, attempt, output_references, error,
                   created_at, started_at, finished_at
            FROM run_steps
            WHERE run_id = $1
            ORDER BY created_at
            "#,
        )
        .bind(rid)
        .fetch_all(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let steps: Vec<RunStep> = step_rows
            .into_iter()
            .map(RunStep::try_from)
            .collect::<Result<_, _>>()?;

        let checkpoint_row: Option<RunCheckpointRow> = sqlx::query_as::<_, RunCheckpointRow>(
            r#"
            SELECT workspace_id, run_id, sequence, state, created_at
            FROM run_checkpoints
            WHERE run_id = $1
            ORDER BY sequence DESC
            LIMIT 1
            "#,
        )
        .bind(rid)
        .fetch_optional(scoped.connection())
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        let checkpoint = checkpoint_row.map(RunCheckpoint::try_from).transpose()?;

        Ok(Some(RunSnapshot {
            run,
            steps,
            checkpoint,
        }))
    }

    async fn create(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let ws = context.workspace_id.as_uuid();
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let tx = scoped.connection();

        let run = &commit.run;
        let run_id = run.id.as_uuid();
        let now = run.created_at;

        let parent_run_id = run.parent.map(|p| p.parent_run_id.as_uuid());
        let parent_step_id = run.parent.map(|p| p.parent_step_id.as_uuid());

        // `principal_id` and `title` are NOT NULL in `agent_runs` and were both
        // absent from this statement, so this store could never insert a run
        // against the real schema. The coordinator path was not merely unwired
        // — it was broken, and no test caught it because the coordinator's own
        // tests use an in-memory store rather than PostgreSQL.
        //
        // `principal_id` comes from the request context: the durable `AgentRun`
        // carries no principal, and the run belongs to whoever asked for it.
        // `title` takes the objective, which is what the read path already
        // reports as the title.
        sqlx::query(
            r#"
            INSERT INTO agent_runs
                (id, workspace_id, principal_id, title, objective,
                 coordinator_snapshot_id, execution_mode,
                 status, run_version, parent_run_id, parent_step_id, root_run_id,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
            "#,
        )
        .bind(run_id)
        .bind(ws)
        .bind(context.principal_id.as_uuid())
        .bind(&run.objective)
        .bind(run.coordinator_snapshot_id.as_uuid())
        .bind(run.execution_mode.as_str())
        .bind(run.status.as_str())
        .bind(run.version.value() as i64)
        .bind(parent_run_id)
        .bind(parent_step_id)
        .bind(run.root_run_id.as_uuid())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        // Migration 0112 states that `run_streams` is authoritative and that
        // `agent_runs` and `run_checkpoints` are derived from it. This store
        // never advanced the stream, so its version stayed at the 0 the
        // `agent_runs_seed_run_stream` trigger seeds, while `agent_runs`
        // advanced — and `PgRunRecoveryStore` reads the run's version from the
        // stream, so recovery would have acted on a version of 0.
        //
        // `DO UPDATE`, not `DO NOTHING`: the trigger has already inserted the
        // row by the time this runs, so `DO NOTHING` would leave it at 0.
        //
        // The version tracked is the appended event's, because the stream is a
        // record of events and not of the projection built from them.
        advance_stream(tx, ws, run_id, commit.event.run_version, now).await?;

        append_event(tx, ws, &commit.event).await?;

        for step in &commit.new_steps {
            insert_step(tx, ws, step).await?;
        }

        if let Some(checkpoint) = &commit.checkpoint {
            insert_checkpoint(tx, ws, checkpoint).await?;
        }

        for item in &commit.work_items {
            insert_work_item(tx, ws, item).await?;
        }

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        self.load(context, run.id)
            .await?
            .ok_or_else(|| ApplicationError::Storage("run not found after create".into()))
    }

    async fn commit(
        &self,
        context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let ws = context.workspace_id.as_uuid();
        let run = &commit.run;
        let run_id = run.id.as_uuid();
        let expected_version = run
            .version
            .previous()
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let new_version = run.version;
        let now = run.updated_at;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;
        let tx = scoped.connection();

        let result_json = run
            .result
            .as_ref()
            .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null));

        let result = sqlx::query(
            r#"
            UPDATE agent_runs
            SET status = $3,
                run_version = $4,
                updated_at = $5,
                finished_at = $6,
                current_step_id = $7,
                checkpoint_id = $8,
                active_plan_revision_id = $9,
                result = $10
            WHERE workspace_id = $1
              AND id = $2
              AND run_version = $11
            "#,
        )
        .bind(ws)
        .bind(run_id)
        .bind(run.status.as_str())
        .bind(new_version.value() as i64)
        .bind(now)
        .bind(run.finished_at)
        .bind(run.current_step_id.map(|s| s.as_uuid()))
        .bind(run.checkpoint_id.map(|c| c.as_uuid()))
        .bind(run.active_plan_revision_id.map(|r| r.as_uuid()))
        .bind(result_json)
        .bind(expected_version.value() as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            let current: Option<(i64,)> = sqlx::query_as(
                "SELECT run_version FROM agent_runs WHERE workspace_id = $1 AND id = $2",
            )
            .bind(ws)
            .bind(run_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

            drop(scoped);
            return Err(ApplicationError::Conflict(format!(
                "revision_conflict: expected version {}, found {:?}",
                expected_version.value(),
                current.map(|c| c.0)
            )));
        }

        // Keeps the authoritative stream level with the event just appended;
        // see the note in `create`.
        advance_stream(tx, ws, run_id, commit.event.run_version, now).await?;

        append_event(tx, ws, &commit.event).await?;

        for step in &commit.new_steps {
            insert_step(tx, ws, step).await?;
        }

        if let Some(checkpoint) = &commit.checkpoint {
            insert_checkpoint(tx, ws, checkpoint).await?;
        }

        for item in &commit.work_items {
            insert_work_item(tx, ws, item).await?;
        }

        scoped
            .commit()
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))?;

        self.load(context, run.id)
            .await?
            .ok_or_else(|| ApplicationError::Storage("run not found after commit".into()))
    }
}

/// Schema version stamped on every canonical run event.
///
/// Matches the legacy writer's `event_version()`, so a reader cannot tell the
/// two writers apart by this column and a historical row stays interpretable.
const CANONICAL_RUN_EVENT_VERSION: i16 = 1;

/// Move `run_streams.current_version` to the version of the event being
/// appended.
///
/// The row normally exists already — `agent_runs_seed_run_stream` inserts it at
/// version 0 — but it is upserted so a run whose projection predates the
/// trigger still gets a stream rather than silently having none.
async fn advance_stream(
    connection: &mut sqlx::PgConnection,
    workspace_id: uuid::Uuid,
    run_id: uuid::Uuid,
    version: RunVersion,
    at: Timestamp,
) -> Result<(), ApplicationError> {
    sqlx::query(
        r#"
        INSERT INTO run_streams (workspace_id, run_id, current_version, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT (workspace_id, run_id)
        DO UPDATE SET current_version = EXCLUDED.current_version,
                      updated_at = EXCLUDED.updated_at
        "#,
    )
    .bind(workspace_id)
    .bind(run_id)
    .bind(version.value() as i64)
    .bind(at)
    .execute(&mut *connection)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    Ok(())
}

async fn append_event(
    connection: &mut sqlx::PgConnection,
    workspace_id: uuid::Uuid,
    event: &RunEvent,
) -> Result<(), ApplicationError> {
    let payload_json = serde_json::to_value(&event.payload)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let actor_json =
        serde_json::to_value(&event.actor).map_err(|e| ApplicationError::Storage(e.to_string()))?;

    // `event_version` and `causation_id` are NOT NULL with no default, and
    // neither was supplied here, so this statement could never insert against
    // the real schema. The legacy committer wrote both, which is why only this
    // path was broken.
    //
    // `causation_id` falls back to the event's own id when the event was not
    // caused by another: the column cannot be null, and pointing an uncaused
    // event at itself says "this is where the chain starts" rather than
    // inventing a link to an unrelated event.
    let causation_id = event
        .causation_event_id
        .map(|id| id.as_uuid())
        .unwrap_or_else(|| event.id.as_uuid());

    sqlx::query(
        r#"
        INSERT INTO run_events
            (id, workspace_id, run_id, sequence, run_version, sequence_value,
             event_type, event_version, payload_kind, payload, actor,
             correlation_id, causation_id, occurred_at, recorded_at)
        VALUES ($1, $2, $3, $4, $5, $5, $6, $7, $6, $8, $9, $10, $11, $12, $12)
        "#,
    )
    .bind(event.id.as_uuid())
    .bind(workspace_id)
    .bind(event.run_id.as_uuid())
    .bind(event.sequence.value() as i64)
    .bind(event.run_version.value() as i64)
    .bind(event.payload.event_type())
    .bind(CANONICAL_RUN_EVENT_VERSION)
    .bind(&payload_json)
    .bind(&actor_json)
    .bind(event.correlation_id.as_uuid())
    .bind(causation_id)
    .bind(event.occurred_at)
    .execute(&mut *connection)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}

async fn insert_step(
    connection: &mut sqlx::PgConnection,
    _workspace_id: uuid::Uuid,
    step: &RunStep,
) -> Result<(), ApplicationError> {
    let actor_json = serde_json::to_value(&step.assigned_actor)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let input_json = serde_json::to_value(&step.input_references)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let output_json = serde_json::to_value(&step.output_references)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;
    let error_json = step
        .error
        .as_ref()
        .map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null));

    // `step_number` is NOT NULL with no default and unique per run, and was
    // absent here. It is derived in the statement rather than in Rust so two
    // concurrent inserts inside one transaction cannot pick the same number.
    //
    // An upsert, not a plain insert: `CommitRun::new_steps` carries steps to
    // *persist*, which includes ones that already exist with a changed status —
    // `ExecuteStepHandler` puts the running and then the finished step there.
    // A plain insert therefore failed with a duplicate key the moment a step
    // was executed, and the run could never progress past its first step.
    //
    // `step_number`, `run_id` and `created_at` are deliberately not updated:
    // a step's position and origin do not change, only its progress does.
    sqlx::query(
        r#"
        INSERT INTO run_steps
            (id, workspace_id, run_id, step_number, plan_step_reference, assigned_actor,
             input_references, status, attempt, output_references, error,
             created_at, started_at, finished_at)
        VALUES ($1, $2, $3,
                (SELECT COALESCE(MAX(step_number), 0) + 1 FROM run_steps WHERE run_id = $3),
                $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        ON CONFLICT (id) DO UPDATE SET
            status = EXCLUDED.status,
            attempt = EXCLUDED.attempt,
            output_references = EXCLUDED.output_references,
            error = EXCLUDED.error,
            started_at = EXCLUDED.started_at,
            finished_at = EXCLUDED.finished_at,
            updated_at = NOW()
        "#,
    )
    .bind(step.id.as_uuid())
    .bind(_workspace_id)
    .bind(step.run_id.as_uuid())
    .bind(&step.plan_step_reference)
    .bind(&actor_json)
    .bind(&input_json)
    .bind(step.status.as_str())
    .bind(step.attempt as i32)
    .bind(&output_json)
    .bind(error_json)
    .bind(step.created_at)
    .bind(step.started_at)
    .bind(step.finished_at)
    .execute(&mut *connection)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}

async fn insert_checkpoint(
    connection: &mut sqlx::PgConnection,
    workspace_id: uuid::Uuid,
    checkpoint: &RunCheckpoint,
) -> Result<(), ApplicationError> {
    let payload_json = serde_json::to_value(&checkpoint.payload)
        .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    // Migration 0112 renamed `run_version` to `sequence` and `state_snapshot`
    // to `state`, dropped `id`, and added a mandatory `state_hash`; migration
    // 0131 dropped `resume_cursor`, `payload` and `payload_version`. This
    // statement still named the pre-0112 columns and could never execute.
    //
    // `state_hash` pins the stored state, so a checkpoint that was altered in
    // place can be detected rather than resumed from.
    let state_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(payload_json.to_string().as_bytes());
        format!("{:x}", hasher.finalize())
    };

    sqlx::query(
        r#"
        INSERT INTO run_checkpoints
            (workspace_id, run_id, sequence, state, state_hash, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (workspace_id, run_id, sequence) DO NOTHING
        "#,
    )
    .bind(workspace_id)
    .bind(checkpoint.run_id.as_uuid())
    .bind(checkpoint.run_version.value() as i64)
    .bind(&payload_json)
    .bind(&state_hash)
    .bind(checkpoint.created_at)
    .execute(&mut *connection)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}

async fn insert_work_item(
    connection: &mut sqlx::PgConnection,
    workspace_id: uuid::Uuid,
    item: &vestrace_application::run::ports::WorkItem,
) -> Result<(), ApplicationError> {
    let (kind, step_id) = match &item.kind {
        vestrace_application::run::ports::WorkItemKind::AdvanceRun => ("advance_run", None),
        vestrace_application::run::ports::WorkItemKind::ResumeRun => ("resume_run", None),
        vestrace_application::run::ports::WorkItemKind::ExecuteStep { step_id } => {
            ("execute_step", Some(step_id.as_uuid()))
        }
    };

    sqlx::query(
        r#"
        INSERT INTO run_work_items
            (id, workspace_id, run_id, step_id, kind, status,
             expected_run_version, available_at, attempt, max_attempts, idempotency_key)
        VALUES ($1, $2, $3, $4, $5, 'ready', $6, $7, $8, 3, $9)
        "#,
    )
    .bind(item.id.as_uuid())
    .bind(workspace_id)
    .bind(item.run_id.as_uuid())
    .bind(step_id)
    .bind(kind)
    .bind(item.expected_run_version.value() as i64)
    .bind(item.available_at)
    .bind(item.attempt as i32)
    .bind(&item.idempotency_key)
    .execute(&mut *connection)
    .await
    .map_err(|e| ApplicationError::Storage(e.to_string()))?;

    Ok(())
}
