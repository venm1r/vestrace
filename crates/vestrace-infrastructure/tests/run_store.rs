//! `PostgresRunStore` against the real schema.
//!
//! The coordinator's own tests use an in-memory store, so nothing exercised
//! this adapter against PostgreSQL. It was missing `principal_id` and `title`,
//! both NOT NULL on `agent_runs`, and therefore could never insert a run — a
//! defect invisible to every green test run until the HTTP surface was moved
//! onto this path.

use sqlx::PgPool;
use vestrace_application::RequestContext;
use vestrace_application::run::ports::{CommitRun, RunStorePort, WorkItem, WorkItemKind};
use vestrace_domain::id::{AgentRuntimeSnapshotId, PrincipalId, WorkItemId, WorkspaceId};
use vestrace_domain::run::{
    AgentRun, NewAgentRun, ResumeCursor, RunActorRef, RunEvent, RunEventPayload, RunExecutionMode,
    RunVersion,
};
use vestrace_infrastructure::{PgStore, PostgresRunStore};

async fn seed(pool: &PgPool) -> (WorkspaceId, PrincipalId) {
    let workspace = WorkspaceId::new();
    let principal = PrincipalId::new();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace.as_uuid())
        .bind(format!("runs-{}", workspace.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal.as_uuid())
        .bind(workspace.as_uuid())
        .bind(format!("principal-{}", principal.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
    (workspace, principal)
}

fn creation(workspace: WorkspaceId, objective: &str) -> (AgentRun, RunEvent) {
    let at = vestrace_domain::time::now();
    let run = AgentRun::create(
        NewAgentRun {
            id: vestrace_domain::id::AgentRunId::new(),
            workspace_id: workspace,
            objective: objective.to_string(),
            coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
            execution_mode: RunExecutionMode::Supervised,
            parent: None,
            budget_snapshot_id: None,
            resource_usage_snapshot_id: None,
        },
        at,
    )
    .unwrap();

    let event = RunEvent::new(
        run.id,
        workspace,
        RunVersion::INITIAL,
        ResumeCursor::from_version(RunVersion::INITIAL),
        RunActorRef::System,
        RunEventPayload::RunCreated {
            objective: objective.to_string(),
            execution_mode: run.execution_mode,
            coordinator_snapshot_id: run.coordinator_snapshot_id,
            parent: None,
        },
        vestrace_domain::id::CorrelationId::new(),
        None,
        at,
    )
    .unwrap();

    (run, event)
}

/// The defect this file exists for.
#[sqlx::test(migrations = "../../migrations")]
async fn a_run_can_actually_be_created(pool: PgPool) {
    let (workspace, principal) = seed(&pool).await;
    let store = PostgresRunStore::new(&PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, principal);
    let (run, event) = creation(workspace, "store a real run");
    let run_id = run.id;

    let snapshot = store
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![WorkItem {
                    id: WorkItemId::new(),
                    run_id,
                    kind: WorkItemKind::AdvanceRun,
                    expected_run_version: RunVersion::INITIAL,
                    available_at: vestrace_domain::time::now(),
                    idempotency_key: format!("advance:{}", run_id.as_uuid()),
                    attempt: 1,
                }],
            },
        )
        .await
        .unwrap();

    assert_eq!(snapshot.run.id, run_id);
    assert_eq!(snapshot.run.objective, "store a real run");
}

/// The run must be attributed to the principal that asked for it. Nothing in
/// the durable `AgentRun` carries a principal, so it comes from the request
/// context, and getting it wrong would misattribute every run in the system.
#[sqlx::test(migrations = "../../migrations")]
async fn the_stored_run_is_attributed_to_the_requesting_principal(pool: PgPool) {
    let (workspace, principal) = seed(&pool).await;
    let store = PostgresRunStore::new(&PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, principal);
    let (run, event) = creation(workspace, "attributed run");
    let run_id = run.id;

    store
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![],
            },
        )
        .await
        .unwrap();

    let stored: uuid::Uuid =
        sqlx::query_scalar("SELECT principal_id FROM agent_runs WHERE id = $1")
            .bind(run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, principal.as_uuid());
}

/// A created run must enqueue the work that advances it, or the worker never
/// sees it — which is the condition this whole change set exists to remove.
#[sqlx::test(migrations = "../../migrations")]
async fn creation_enqueues_the_work_that_advances_the_run(pool: PgPool) {
    let (workspace, principal) = seed(&pool).await;
    let store = PostgresRunStore::new(&PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, principal);
    let (run, event) = creation(workspace, "queued run");
    let run_id = run.id;

    store
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![WorkItem {
                    id: WorkItemId::new(),
                    run_id,
                    kind: WorkItemKind::AdvanceRun,
                    expected_run_version: RunVersion::INITIAL,
                    available_at: vestrace_domain::time::now(),
                    idempotency_key: format!("advance:{}", run_id.as_uuid()),
                    attempt: 1,
                }],
            },
        )
        .await
        .unwrap();

    let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_work_items WHERE run_id = $1")
        .bind(run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 1);
}

/// The event written on creation must carry the correlation it was given, or an
/// HTTP request cannot be traced to the run it caused.
#[sqlx::test(migrations = "../../migrations")]
async fn the_creation_event_preserves_its_correlation(pool: PgPool) {
    let (workspace, principal) = seed(&pool).await;
    let store = PostgresRunStore::new(&PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, principal);
    let (run, mut event) = creation(workspace, "correlated run");
    let run_id = run.id;
    let correlation = vestrace_domain::id::CorrelationId::new();
    event.correlation_id = correlation;

    store
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![],
            },
        )
        .await
        .unwrap();

    let stored: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT correlation_id FROM run_events WHERE run_id = $1")
            .bind(run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, Some(correlation.as_uuid()));
}

/// `run_streams` is authoritative per migration 0112, and `PgRunRecoveryStore`
/// reads a run's version from it. If a commit advanced `agent_runs` alone,
/// recovery would act on a stale version.
#[sqlx::test(migrations = "../../migrations")]
async fn advancing_a_run_advances_the_authoritative_stream(pool: PgPool) {
    let (workspace, principal) = seed(&pool).await;
    let store = PostgresRunStore::new(&PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, principal);
    let (run, event) = creation(workspace, "advancing run");
    let run_id = run.id;

    let snapshot = store
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![],
            },
        )
        .await
        .unwrap();

    let opened: i64 = sqlx::query_scalar(
        "SELECT current_version FROM run_streams WHERE workspace_id = $1 AND run_id = $2",
    )
    .bind(workspace.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        opened, 1,
        "creation must open the stream at the initial version"
    );

    let next = snapshot.run.version.next().unwrap();
    let mut advanced = snapshot.run.clone();
    advanced.version = next;
    advanced.status = vestrace_domain::run::RunStatus::Running;
    advanced.updated_at = vestrace_domain::time::now();

    let event = RunEvent::new(
        run_id,
        workspace,
        next,
        ResumeCursor::from_version(next),
        RunActorRef::System,
        RunEventPayload::RunStatusChanged {
            from: vestrace_domain::run::RunStatus::Created,
            to: vestrace_domain::run::RunStatus::Running,
            result: None,
        },
        vestrace_domain::id::CorrelationId::new(),
        None,
        vestrace_domain::time::now(),
    )
    .unwrap();

    store
        .commit(
            &context,
            CommitRun {
                run: advanced,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![],
            },
        )
        .await
        .unwrap();

    let streamed: i64 = sqlx::query_scalar(
        "SELECT current_version FROM run_streams WHERE workspace_id = $1 AND run_id = $2",
    )
    .bind(workspace.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let derived: i64 = sqlx::query_scalar("SELECT run_version FROM agent_runs WHERE id = $1")
        .bind(run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(streamed, next.value() as i64);
    assert_eq!(
        streamed, derived,
        "the authoritative stream and the derived projection must not diverge"
    );
}

/// Steps are the newly reachable path: before `POST /v1/runs/{id}/steps` no run
/// ever acquired one, so this insert had never executed against the schema.
/// `step_number` is NOT NULL with no default and unique per run.
#[sqlx::test(migrations = "../../migrations")]
async fn steps_are_stored_and_numbered_within_their_run(pool: PgPool) {
    let (workspace, principal) = seed(&pool).await;
    let store = PostgresRunStore::new(&PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, principal);
    let (run, event) = creation(workspace, "run with steps");
    let run_id = run.id;

    let snapshot = store
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![],
            },
        )
        .await
        .unwrap();

    let at = vestrace_domain::time::now();
    let steps: Vec<_> = (0..2)
        .map(|_| {
            vestrace_domain::run::RunStep::create(
                vestrace_domain::run::NewRunStep {
                    id: vestrace_domain::id::RunStepId::new(),
                    run_id,
                    plan_step_reference: None,
                    assigned_actor: RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()),
                    input_references: vec![],
                },
                at,
            )
            .unwrap()
        })
        .collect();

    let next = snapshot.run.version.next().unwrap();
    let mut advanced = snapshot.run.clone();
    advanced.version = next;
    advanced.updated_at = at;

    let event = RunEvent::new(
        run_id,
        workspace,
        next,
        ResumeCursor::from_version(next),
        RunActorRef::System,
        RunEventPayload::StepsAdded {
            steps: steps.clone(),
        },
        vestrace_domain::id::CorrelationId::new(),
        None,
        at,
    )
    .unwrap();

    let after = store
        .commit(
            &context,
            CommitRun {
                run: advanced,
                event,
                new_steps: steps,
                checkpoint: None,
                work_items: vec![],
            },
        )
        .await
        .unwrap();

    assert_eq!(after.steps.len(), 2, "both steps must be readable back");

    // Numbered from one and distinct, which the unique constraint requires and
    // which two inserts in one transaction could otherwise violate.
    let numbers: Vec<i32> = sqlx::query_scalar(
        "SELECT step_number FROM run_steps WHERE run_id = $1 ORDER BY step_number",
    )
    .bind(run_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(numbers, vec![1, 2]);

    // An agent-assigned step is the one that invokes a model, so the actor must
    // survive the round trip intact.
    assert!(
        after
            .steps
            .iter()
            .all(|step| matches!(step.assigned_actor, RunActorRef::AgentSnapshot(_)))
    );
}

/// `CommitRun::new_steps` carries steps to persist, which includes ones that
/// already exist with a changed status — `ExecuteStepHandler` puts the running
/// and then the finished step there. A plain insert failed with a duplicate key
/// the first time a step executed, so no run could progress past step one.
#[sqlx::test(migrations = "../../migrations")]
async fn re_persisting_a_step_updates_it_rather_than_failing(pool: PgPool) {
    let (workspace, principal) = seed(&pool).await;
    let store = PostgresRunStore::new(&PgStore::from_pool(pool.clone()));
    let context = RequestContext::new(workspace, principal);
    let (run, event) = creation(workspace, "run whose step advances");
    let run_id = run.id;

    let mut snapshot = store
        .create(
            &context,
            CommitRun {
                run,
                event,
                new_steps: vec![],
                checkpoint: None,
                work_items: vec![],
            },
        )
        .await
        .unwrap();

    let at = vestrace_domain::time::now();
    let mut step = vestrace_domain::run::RunStep::create(
        vestrace_domain::run::NewRunStep {
            id: vestrace_domain::id::RunStepId::new(),
            run_id,
            plan_step_reference: None,
            assigned_actor: RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()),
            input_references: vec![],
        },
        at,
    )
    .unwrap();

    // Persist the step three times, as an executing run does: added, running,
    // then finished.
    for status in [
        vestrace_domain::run::RunStepStatus::Pending,
        vestrace_domain::run::RunStepStatus::Running,
        vestrace_domain::run::RunStepStatus::Succeeded,
    ] {
        step.status = status;
        if status.is_terminal() {
            // The schema requires a finish time on a terminal step.
            step.finished_at = Some(vestrace_domain::time::now());
        }

        let next = snapshot.run.version.next().unwrap();
        let mut advanced = snapshot.run.clone();
        advanced.version = next;
        advanced.updated_at = vestrace_domain::time::now();

        let event = RunEvent::new(
            run_id,
            workspace,
            next,
            ResumeCursor::from_version(next),
            RunActorRef::System,
            RunEventPayload::StepsAdded {
                steps: vec![step.clone()],
            },
            vestrace_domain::id::CorrelationId::new(),
            None,
            vestrace_domain::time::now(),
        )
        .unwrap();

        snapshot = store
            .commit(
                &context,
                CommitRun {
                    run: advanced,
                    event,
                    new_steps: vec![step.clone()],
                    checkpoint: None,
                    work_items: vec![],
                },
            )
            .await
            .unwrap();
    }

    // One row, at the latest status — not three rows and not a duplicate-key
    // failure.
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_steps WHERE run_id = $1")
        .bind(run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(snapshot.steps.len(), 1);
    assert_eq!(
        snapshot.steps[0].status,
        vestrace_domain::run::RunStepStatus::Succeeded
    );

    // The step keeps its position across updates.
    let number: i32 = sqlx::query_scalar("SELECT step_number FROM run_steps WHERE run_id = $1")
        .bind(run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(number, 1);
}
