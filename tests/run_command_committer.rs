//! PostgreSQL event + projection transaction contract for R1.3.

use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use vestrace_application::{
    ApplicationError, RequestContext, RunCommandCommitter, RunCommandExecutor, RunCommandService,
    RunEventStore, RunRepository,
};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId},
    now,
    run::{
        AgentRun, RunActor, RunCommand, RunCommandEnvelope, RunEvent, RunEventEnvelope, RunStatus,
        RunVersion,
    },
};
use vestrace_infrastructure::{PgRunCommandCommitter, PgRunEventStore, PgRunRepository, PgStore};

const WORKSPACE_A: &str = "55000000-0000-0000-0000-000000000001";
const WORKSPACE_B: &str = "55000000-0000-0000-0000-000000000002";
const PRINCIPAL_A: &str = "55000000-0000-0000-0000-000000000003";
const PRINCIPAL_B: &str = "55000000-0000-0000-0000-000000000004";
const RUN_ID: &str = "55000000-0000-0000-0000-000000000005";

async fn seed_identities(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ($1::uuid, 'command-commit-a'),
         ($2::uuid, 'command-commit-b')",
    )
    .bind(WORKSPACE_A)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES
         ($1::uuid, $2::uuid, 'command-commit-principal-a'),
         ($3::uuid, $4::uuid, 'command-commit-principal-b')",
    )
    .bind(PRINCIPAL_A)
    .bind(WORKSPACE_A)
    .bind(PRINCIPAL_B)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();
}

fn workspace_a() -> WorkspaceId {
    WorkspaceId::from_str(WORKSPACE_A).unwrap()
}

fn workspace_b() -> WorkspaceId {
    WorkspaceId::from_str(WORKSPACE_B).unwrap()
}

fn principal_a() -> PrincipalId {
    PrincipalId::from_str(PRINCIPAL_A).unwrap()
}

fn principal_b() -> PrincipalId {
    PrincipalId::from_str(PRINCIPAL_B).unwrap()
}

fn run_id() -> AgentRunId {
    AgentRunId::from_str(RUN_ID).unwrap()
}

fn context_a() -> RequestContext {
    RequestContext::new(workspace_a(), principal_a())
}

fn context_b() -> RequestContext {
    RequestContext::new(workspace_b(), principal_b())
}

fn database_timestamp(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    let submicrosecond_nanos = i64::from(timestamp.timestamp_subsec_nanos() % 1_000);
    timestamp - Duration::nanoseconds(submicrosecond_nanos)
}

fn command(expected_version: RunVersion, command: RunCommand) -> RunCommandEnvelope {
    RunCommandEnvelope {
        command_id: OperationId::new(),
        idempotency_key: None,
        workspace_id: workspace_a(),
        run_id: run_id(),
        actor: RunActor::Principal(principal_a()),
        expected_version,
        correlation_id: CorrelationId::new(),
        issued_at: database_timestamp(now()),
        command,
    }
}

fn service(pool: &sqlx::PgPool) -> RunCommandService {
    let store = PgStore::from_pool(pool.clone());
    RunCommandService::new(
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunCommandCommitter::new(store)),
    )
}

fn mark_ready_event(
    event_id: RunEventId,
    workspace_id: WorkspaceId,
    actor: RunActor,
) -> RunEventEnvelope {
    let occurred_at = database_timestamp(now());
    let payload = RunEvent::MarkedReady;
    RunEventEnvelope {
        event_id,
        workspace_id,
        run_id: run_id(),
        sequence: RunVersion::new(2).unwrap(),
        event_type: payload.event_type().to_owned(),
        event_version: payload.event_version(),
        actor,
        causation_id: OperationId::new(),
        correlation_id: CorrelationId::new(),
        payload,
        occurred_at,
        recorded_at: occurred_at,
    }
}

fn ready_projection(created: &AgentRun, updated_at: chrono::DateTime<chrono::Utc>) -> AgentRun {
    AgentRun {
        status: RunStatus::Ready,
        version: RunVersion::new(2).unwrap(),
        updated_at,
        ..created.clone()
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn create_command_commits_event_and_projection_atomically(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let service = service(&pool);

    let result = service
        .execute(
            &context_a(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_a(),
                    title: "Atomic projection".to_owned(),
                },
            ),
        )
        .await
        .unwrap();

    let store = PgStore::from_pool(pool);
    let repository = PgRunRepository::new(store.clone());
    let event_store = PgRunEventStore::new(store);
    assert_eq!(
        repository.find_by_id(&context_a(), run_id()).await.unwrap(),
        Some(result.run.clone())
    );
    assert_eq!(
        event_store
            .load_stream(&context_a(), run_id())
            .await
            .unwrap(),
        result.events
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn existing_command_advances_projection_and_preserves_created_at(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let service = service(&pool);
    let created = service
        .execute(
            &context_a(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_a(),
                    title: "Advance projection".to_owned(),
                },
            ),
        )
        .await
        .unwrap();

    let ready = service
        .execute(
            &context_a(),
            command(RunVersion::INITIAL, RunCommand::MarkReady),
        )
        .await
        .unwrap();

    assert_eq!(ready.run.status, RunStatus::Ready);
    assert_eq!(ready.run.version.value(), 2);
    assert_eq!(ready.run.created_at, created.run.created_at);
    assert_eq!(ready.run.updated_at, ready.events[0].occurred_at);

    let store = PgStore::from_pool(pool);
    let repository = PgRunRepository::new(store.clone());
    let event_store = PgRunEventStore::new(store);
    assert_eq!(
        repository.find_by_id(&context_a(), run_id()).await.unwrap(),
        Some(ready.run)
    );
    assert_eq!(
        event_store
            .load_stream(&context_a(), run_id())
            .await
            .unwrap()
            .len(),
        2
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_event_id_rolls_back_projection_update(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let service = service(&pool);
    let created = service
        .execute(
            &context_a(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_a(),
                    title: "Rollback projection".to_owned(),
                },
            ),
        )
        .await
        .unwrap();

    let duplicate = mark_ready_event(
        created.events[0].event_id,
        workspace_a(),
        RunActor::Principal(principal_a()),
    );
    let projection = ready_projection(&created.run, duplicate.occurred_at);
    let committer = PgRunCommandCommitter::new(PgStore::from_pool(pool.clone()));

    let result = committer
        .commit(
            &context_a(),
            run_id(),
            RunVersion::INITIAL,
            &[duplicate],
            &projection,
        )
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    let store = PgStore::from_pool(pool);
    let repository = PgRunRepository::new(store.clone());
    let event_store = PgRunEventStore::new(store);
    assert_eq!(
        repository.find_by_id(&context_a(), run_id()).await.unwrap(),
        Some(created.run)
    );
    assert_eq!(
        event_store
            .load_stream(&context_a(), run_id())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn projection_identity_mismatch_is_rejected_without_writes(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let occurred_at = database_timestamp(now());
    let payload = RunEvent::Created {
        principal_id: principal_a(),
        title: "Mismatch".to_owned(),
    };
    let event = RunEventEnvelope {
        event_id: RunEventId::new(),
        workspace_id: workspace_a(),
        run_id: run_id(),
        sequence: RunVersion::INITIAL,
        event_type: payload.event_type().to_owned(),
        event_version: payload.event_version(),
        actor: RunActor::Principal(principal_a()),
        causation_id: OperationId::new(),
        correlation_id: CorrelationId::new(),
        payload,
        occurred_at,
        recorded_at: occurred_at,
    };
    let projection = AgentRun {
        id: run_id(),
        workspace_id: workspace_b(),
        principal_id: principal_a(),
        title: "Mismatch".to_owned(),
        status: RunStatus::Created,
        version: RunVersion::INITIAL,
        created_at: occurred_at,
        updated_at: occurred_at,
    };
    let committer = PgRunCommandCommitter::new(PgStore::from_pool(pool.clone()));

    let result = committer
        .commit(
            &context_a(),
            run_id(),
            RunVersion::ZERO,
            &[event],
            &projection,
        )
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    let run_count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    let event_count: i64 = sqlx::query_scalar("SELECT count(*) FROM run_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(run_count, 0);
    assert_eq!(event_count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_writers_at_one_version_yield_one_success(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let service = service(&pool);
    let created = service
        .execute(
            &context_a(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_a(),
                    title: "Concurrent projection".to_owned(),
                },
            ),
        )
        .await
        .unwrap();

    let event_a = mark_ready_event(
        RunEventId::new(),
        workspace_a(),
        RunActor::Principal(principal_a()),
    );
    let event_b = mark_ready_event(
        RunEventId::new(),
        workspace_a(),
        RunActor::Principal(principal_a()),
    );
    let projection_a = ready_projection(&created.run, event_a.occurred_at);
    let projection_b = ready_projection(&created.run, event_b.occurred_at);
    let committer_a = PgRunCommandCommitter::new(PgStore::from_pool(pool.clone()));
    let committer_b = PgRunCommandCommitter::new(PgStore::from_pool(pool.clone()));
    let context = context_a();
    let target_run_id = run_id();
    let events_a = [event_a];
    let events_b = [event_b];

    let (result_a, result_b) = tokio::join!(
        committer_a.commit(
            &context,
            target_run_id,
            RunVersion::INITIAL,
            &events_a,
            &projection_a,
        ),
        committer_b.commit(
            &context,
            target_run_id,
            RunVersion::INITIAL,
            &events_b,
            &projection_b,
        )
    );

    let successes = usize::from(result_a.is_ok()) + usize::from(result_b.is_ok());
    let conflicts = usize::from(matches!(result_a, Err(ApplicationError::Conflict(_))))
        + usize::from(matches!(result_b, Err(ApplicationError::Conflict(_))));
    assert_eq!(successes, 1);
    assert_eq!(conflicts, 1);

    let store = PgStore::from_pool(pool);
    let repository = PgRunRepository::new(store.clone());
    let event_store = PgRunEventStore::new(store);
    assert_eq!(
        repository
            .find_by_id(&context_a(), run_id())
            .await
            .unwrap()
            .unwrap()
            .version
            .value(),
        2
    );
    assert_eq!(
        event_store
            .load_stream(&context_a(), run_id())
            .await
            .unwrap()
            .len(),
        2
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn foreign_workspace_cannot_commit_to_an_existing_run(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let service = service(&pool);
    let created = service
        .execute(
            &context_a(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_a(),
                    title: "Workspace boundary".to_owned(),
                },
            ),
        )
        .await
        .unwrap();

    let event = mark_ready_event(
        RunEventId::new(),
        workspace_b(),
        RunActor::Principal(principal_b()),
    );
    let projection = AgentRun {
        workspace_id: workspace_b(),
        principal_id: principal_b(),
        status: RunStatus::Ready,
        version: RunVersion::new(2).unwrap(),
        updated_at: event.occurred_at,
        ..created.run.clone()
    };
    let committer = PgRunCommandCommitter::new(PgStore::from_pool(pool.clone()));

    let result = committer
        .commit(
            &context_b(),
            run_id(),
            RunVersion::INITIAL,
            &[event],
            &projection,
        )
        .await;

    assert!(matches!(
        result,
        Err(ApplicationError::Storage(_) | ApplicationError::Conflict(_))
    ));
    let store = PgStore::from_pool(pool);
    let repository = PgRunRepository::new(store.clone());
    let event_store = PgRunEventStore::new(store);
    assert_eq!(
        repository.find_by_id(&context_a(), run_id()).await.unwrap(),
        Some(created.run)
    );
    assert_eq!(
        event_store
            .load_stream(&context_a(), run_id())
            .await
            .unwrap()
            .len(),
        1
    );
}
