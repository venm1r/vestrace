//! P1 command commits must depend on the authoritative stream, not its projection.

use std::{str::FromStr, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use sqlx::types::Uuid;
use vestrace_application::{
    ApplicationError, RequestContext, RunCommandExecutor, RunCommandService, RunRepository,
};
use vestrace_domain::{
    id::{
        AgentRunId, AgentRuntimeSnapshotId, CorrelationId, OperationId, PrincipalId, WorkspaceId,
    },
    now,
    run::{
        AgentRun, RunActor, RunCommand, RunCommandEnvelope, RunExecutionMode, RunStatus, RunVersion,
    },
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

async fn configure_chat_workspace_default(pool: &sqlx::PgPool) {
    let connector_id = Uuid::now_v7();
    let provider_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let execution_guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_binding_revision_id = Uuid::now_v7();
    let model_id = Uuid::now_v7();
    let model_revision_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let connection_qualification_revision_id = Uuid::now_v7();
    let model_qualification_revision_id = Uuid::now_v7();
    let profile_revision = "projection-independence/v1";
    let mut transaction = pool.begin().await.unwrap();
    for (name, value) in [
        ("vestrace.workspace_id", workspace_id().to_string()),
        ("vestrace.principal_id", principal_id().to_string()),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind(name)
            .bind(value)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, 'projection-chat', 'local')")
        .bind(connector_id)
        .bind(workspace_id().as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO providers (id, workspace_id, name, locality) VALUES ($1, $2, 'projection-chat', 'local')")
        .bind(provider_id)
        .bind(workspace_id().as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connections (id, connector_id, workspace_id, principal_id, name, status) VALUES ($1, $2, $3, $4, 'projection-chat', 'active')")
        .bind(connection_id)
        .bind(connector_id)
        .bind(workspace_id().as_uuid())
        .bind(principal_id().as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(execution_guard_id)
        .bind(workspace_id().as_uuid())
        .bind(connection_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_connection_revision_and_advance_head($1, $2, $3, $4, 'lm_studio_local', 'http://127.0.0.1:1234/v1', 'http://127.0.0.1:1234/v1', $5, 'loopback_only', 'none', NULL, 0)")
        .bind(connection_revision_id)
        .bind(workspace_id().as_uuid())
        .bind(connection_id)
        .bind(execution_guard_id)
        .bind(profile_revision)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_no_auth_binding_revision($1, $2, $3, $4)",
    )
    .bind(no_auth_binding_revision_id)
    .bind(workspace_id().as_uuid())
    .bind(connection_id)
    .bind(connection_revision_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("INSERT INTO models (id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken) VALUES ($1, $2, $3, 'projection-chat', 4096, 0, 0)")
        .bind(model_id)
        .bind(provider_id)
        .bind(workspace_id().as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_revision_and_advance_head($1, $2, $3, $4, $5, $6, 'projection-chat', 'chat', NULL::INTEGER, NULL::UUID, NULL::TEXT, NULL::INTEGER, NULL::UUID, NULL::TEXT, 0::BIGINT)")
        .bind(model_revision_id)
        .bind(workspace_id().as_uuid())
        .bind(model_id)
        .bind(connection_id)
        .bind(execution_guard_id)
        .bind(connection_revision_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_set_workspace_model_default($1, $2, 'chat', $3, ARRAY['chat'], 0)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id().as_uuid())
    .bind(model_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_jobs (id, workspace_id, connection_revision_id, profile_revision, state, completed_at) VALUES ($1, $2, $3, $4, 'succeeded', NOW())")
        .bind(qualification_job_id)
        .bind(workspace_id().as_uuid())
        .bind(connection_revision_id)
        .bind(profile_revision)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO qualification_target_bindings (id, workspace_id, qualification_job_id, connection_id, connection_revision_id, branch, no_auth_binding_revision_id) VALUES ($1, $2, $3, $4, $5, 'no_auth', $6)")
        .bind(Uuid::now_v7())
        .bind(workspace_id().as_uuid())
        .bind(qualification_job_id)
        .bind(connection_id)
        .bind(connection_revision_id)
        .bind(no_auth_binding_revision_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connection_qualification_revisions (id, workspace_id, connection_revision_id, qualification_job_id, profile_revision, valid_until, capabilities) VALUES ($1, $2, $3, $4, $5, NOW() + INTERVAL '1 hour', ARRAY['chat'])")
        .bind(connection_qualification_revision_id)
        .bind(workspace_id().as_uuid())
        .bind(connection_revision_id)
        .bind(qualification_job_id)
        .bind(profile_revision)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connection_qualification_heads (workspace_id, connection_revision_id, current_qualification_revision_id, version) VALUES ($1, $2, $3, 1)")
        .bind(workspace_id().as_uuid())
        .bind(connection_revision_id)
        .bind(connection_qualification_revision_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_qualification_revisions (id, workspace_id, model_revision_id, connection_revision_id, connection_qualification_revision_id, qualification_job_id, capabilities, valid_until) VALUES ($1, $2, $3, $4, $5, $6, ARRAY['chat'], NOW() + INTERVAL '1 hour')")
        .bind(model_qualification_revision_id)
        .bind(workspace_id().as_uuid())
        .bind(model_revision_id)
        .bind(connection_revision_id)
        .bind(connection_qualification_revision_id)
        .bind(qualification_job_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_qualification_heads (workspace_id, model_revision_id, current_qualification_revision_id, version) VALUES ($1, $2, $3, 1)")
        .bind(workspace_id().as_uuid())
        .bind(model_revision_id)
        .bind(model_qualification_revision_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
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
    configure_chat_workspace_default(&pool).await;
    let service = service(&pool);
    create_run(&service).await;
    service
        .execute(
            &context(),
            command(RunVersion::INITIAL, RunCommand::Prepare),
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
    configure_chat_workspace_default(&pool).await;
    let creator = service(&pool);
    create_run(&creator).await;
    delete_projection(&pool).await;

    let first = service(&pool);
    let second = service(&pool);
    let first_command = command(RunVersion::INITIAL, RunCommand::Prepare);
    let second_command = command(RunVersion::INITIAL, RunCommand::Prepare);
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
    assert_eq!(projection.status, RunStatus::Preparing);
    assert_eq!(projection.version, RunVersion::new(2).unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_repository_create_explicitly_seeds_a_version_zero_stream(pool: sqlx::PgPool) {
    seed_identity(&pool).await;
    let timestamp = database_timestamp(now());
    let projection = AgentRun {
        id: run_id(),
        workspace_id: workspace_id(),
        objective: "Legacy projection".to_owned(),
        coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(principal_id().as_uuid()),
        active_plan_revision_id: None,
        execution_mode: RunExecutionMode::Autopilot,
        status: RunStatus::Created,
        current_step_id: None,
        checkpoint_id: None,
        parent: None,
        root_run_id: run_id(),
        budget_snapshot_id: None,
        resource_usage_snapshot_id: None,
        version: RunVersion::INITIAL,
        result: None,
        created_at: timestamp,
        updated_at: timestamp,
        finished_at: None,
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
