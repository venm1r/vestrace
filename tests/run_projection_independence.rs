//! P1 command commits must depend on the authoritative stream, not its projection.

use std::{str::FromStr, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use vestrace_application::{
    ApplicationError, RequestContext, RunCommandExecutor, RunCommandService, RunRepository,
};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, WorkspaceId},
    now,
    run::{AgentRun, RunActor, RunCommand, RunCommandEnvelope, RunStatus, RunVersion},
};
use vestrace_infrastructure::{PgRunCommandCommitter, PgRunEventStore, PgRunRepository, PgStore};

const WORKSPACE_ID: &str = "63000000-0000-0000-0000-000000000001";
const PRINCIPAL_ID: &str = "63000000-0000-0000-0000-000000000002";
const RUN_ID: &str = "63000000-0000-0000-0000-000000000003";

fn workspace_id() -> WorkspaceId {
    WorkspaceId::from_str(WORKSPACE_ID).unwrap()
}

fn principal_id() -> PrincipalId {
    PrincipalId::from_str(PRINCIPAL_ID).unwrap()
}

fn run_id() -> AgentRunId {
    AgentRunId::from_str(RUN_ID).unwrap()
}

fn context() -> RequestContext {
    RequestContext::new(workspace_id(), principal_id())
}

fn database_timestamp(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    let submicrosecond_nanos = i64::from(timestamp.timestamp_subsec_nanos() % 1_000);
    timestamp - Duration::nanoseconds(submicrosecond_nanos)
}

async fn seed_identity(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug)
         VALUES ($1::uuid, 'projection-independence')",
    )
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES ($1::uuid, $2::uuid, 'projection-independent-principal')",
    )
    .bind(PRINCIPAL_ID)
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();
}

fn service(pool: &sqlx::PgPool) -> RunCommandService {
    let store = PgStore::from_pool(pool.clone());
    RunCommandService::new(
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunCommandCommitter::new(store)),
    )
}

fn command(expected_version: RunVersion, command: RunCommand) -> RunCommandEnvelope {
    RunCommandEnvelope {
        command_id: OperationId::new(),
        idempotency_key: None,
        workspace_id: workspace_id(),
        run_id: run_id(),
        actor: RunActor::Principal(principal_id()),
        expected_version,
        correlation_id: CorrelationId::new(),
        issued_at: database_timestamp(now()),
        command,
    }
}

async fn create_run(service: &RunCommandService) -> AgentRun {
    service
        .execute(
            &context(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_id(),
                    title: "Projection-independent run".to_owned(),
                },
            ),
        )
        .await
        .unwrap()
        .run
}

async fn delete_projection(pool: &sqlx::PgPool) {
    sqlx::query(
        "DELETE FROM agent_runs
         WHERE workspace_id = $1::uuid AND id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn command_commit_recreates_a_deleted_projection_from_the_stream(pool: sqlx::PgPool) {
    seed_identity(&pool).await;
    let service = service(&pool);
    create_run(&service).await;
    service
        .execute(
            &context(),
            command(RunVersion::INITIAL, RunCommand::MarkReady),
        )
        .await
        .unwrap();
    delete_projection(&pool).await;

    let result = service
        .execute(
            &context(),
            command(RunVersion::new(2).unwrap(), RunCommand::Start),
        )
        .await
        .unwrap();

    assert_eq!(result.run.status, RunStatus::Running);
    assert_eq!(result.run.version, RunVersion::new(3).unwrap());

    let repository = PgRunRepository::new(PgStore::from_pool(pool.clone()));
    assert_eq!(
        repository.find_by_id(&context(), run_id()).await.unwrap(),
        Some(result.run)
    );
    let stream_head: i64 = sqlx::query_scalar(
        "SELECT current_version
         FROM run_streams
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stream_head, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_writers_are_serialized_by_the_stream_without_a_projection(pool: sqlx::PgPool) {
    seed_identity(&pool).await;
    let creator = service(&pool);
    create_run(&creator).await;
    delete_projection(&pool).await;

    let first = service(&pool);
    let second = service(&pool);
    let first_command = command(RunVersion::INITIAL, RunCommand::MarkReady);
    let second_command = command(RunVersion::INITIAL, RunCommand::MarkReady);
    let first_context = context();
    let second_context = context();
    let (first_result, second_result) = tokio::join!(
        first.execute(&first_context, first_command),
        second.execute(&second_context, second_command)
    );

    let results = [first_result, second_result];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(ApplicationError::Conflict(_))))
            .count(),
        1
    );

    let stream_head: i64 = sqlx::query_scalar(
        "SELECT current_version
         FROM run_streams
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM run_events
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((stream_head, event_count), (2, 2));

    let repository = PgRunRepository::new(PgStore::from_pool(pool));
    let projection = repository
        .find_by_id(&context(), run_id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(projection.status, RunStatus::Ready);
    assert_eq!(projection.version, RunVersion::new(2).unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_repository_create_explicitly_seeds_a_version_zero_stream(pool: sqlx::PgPool) {
    seed_identity(&pool).await;
    let timestamp = database_timestamp(now());
    let projection = AgentRun {
        id: run_id(),
        workspace_id: workspace_id(),
        principal_id: principal_id(),
        title: "Legacy projection".to_owned(),
        status: RunStatus::Created,
        version: RunVersion::INITIAL,
        created_at: timestamp,
        updated_at: timestamp,
    };
    let repository = PgRunRepository::new(PgStore::from_pool(pool.clone()));

    repository.create(&context(), &projection).await.unwrap();

    let stream_head: i64 = sqlx::query_scalar(
        "SELECT current_version
         FROM run_streams
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM run_events
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((stream_head, event_count), (0, 0));
}
