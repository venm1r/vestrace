//! PostgreSQL event + projection transaction contract for R1.3.

use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::types::Uuid;
use vestrace_application::{
    ApplicationError, RequestContext, RunCommandCommitter, RunCommandExecutor, RunCommandService,
    RunEventStore, RunRepository,
};
use vestrace_domain::{
    id::{
        AgentRunId, AgentRuntimeSnapshotId, CorrelationId, OperationId, PrincipalId, RunEventId,
        WorkspaceId,
    },
    now,
    run::{
        AgentRun, LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunCommand, RunCommandEnvelope,
        RunExecutionMode, RunStatus, RunVersion,
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

async fn configure_chat_workspace_default(pool: &sqlx::PgPool) {
    let connector_id = Uuid::now_v7();
    let provider_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let execution_guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_binding_revision_id = Uuid::now_v7();
    let model_id = Uuid::now_v7();
    let model_revision_id = Uuid::now_v7();
    let default_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let connection_qualification_revision_id = Uuid::now_v7();
    let model_qualification_revision_id = Uuid::now_v7();
    let profile_revision = "run-command-committer/v1";
    let mut transaction = pool.begin().await.unwrap();

    for (name, value) in [
        ("vestrace.workspace_id", workspace_a().to_string()),
        ("vestrace.principal_id", principal_a().to_string()),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind(name)
            .bind(value)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }

    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type)
         VALUES ($1, $2, 'run-command-committer-chat', 'local')",
    )
    .bind(connector_id)
    .bind(workspace_a().as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO providers (id, workspace_id, name, locality)
         VALUES ($1, $2, 'run-command-committer-chat', 'local')",
    )
    .bind(provider_id)
    .bind(workspace_a().as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name, status)
         VALUES ($1, $2, $3, $4, 'run-command-committer-chat', 'active')",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_a().as_uuid())
    .bind(principal_a().as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(execution_guard_id)
        .bind(workspace_a().as_uuid())
        .bind(connection_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(
             $1, $2, $3, $4, 'lm_studio_local', 'http://127.0.0.1:1234/v1',
             'http://127.0.0.1:1234/v1', $5, 'loopback_only', 'none', NULL, 0)",
    )
    .bind(connection_revision_id)
    .bind(workspace_a().as_uuid())
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
    .bind(workspace_a().as_uuid())
    .bind(connection_id)
    .bind(connection_revision_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO models
             (id, provider_id, workspace_id, model_name, context_window,
              input_cost_per_mtoken, output_cost_per_mtoken)
         VALUES ($1, $2, $3, 'run-command-committer-chat', 4096, 0, 0)",
    )
    .bind(model_id)
    .bind(provider_id)
    .bind(workspace_a().as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_revision_and_advance_head(
             $1, $2, $3, $4, $5, $6, 'run-command-committer-chat', 'chat',
             NULL::INTEGER, NULL::UUID, NULL::TEXT, NULL::INTEGER, NULL::UUID,
             NULL::TEXT, 0::BIGINT)",
    )
    .bind(model_revision_id)
    .bind(workspace_a().as_uuid())
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
    .bind(default_id)
    .bind(workspace_a().as_uuid())
    .bind(model_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();

    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO qualification_jobs
             (id, workspace_id, connection_revision_id, profile_revision, state, completed_at)
         VALUES ($1, $2, $3, $4, 'succeeded', NOW())",
    )
    .bind(qualification_job_id)
    .bind(workspace_a().as_uuid())
    .bind(connection_revision_id)
    .bind(profile_revision)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_target_bindings
             (id, workspace_id, qualification_job_id, connection_id, connection_revision_id,
              branch, no_auth_binding_revision_id)
         VALUES ($1, $2, $3, $4, $5, 'no_auth', $6)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_a().as_uuid())
    .bind(qualification_job_id)
    .bind(connection_id)
    .bind(connection_revision_id)
    .bind(no_auth_binding_revision_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions
             (id, workspace_id, connection_revision_id, qualification_job_id, profile_revision,
              valid_until, capabilities)
         VALUES ($1, $2, $3, $4, $5, NOW() + INTERVAL '1 hour', ARRAY['chat'])",
    )
    .bind(connection_qualification_revision_id)
    .bind(workspace_a().as_uuid())
    .bind(connection_revision_id)
    .bind(qualification_job_id)
    .bind(profile_revision)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_heads
             (workspace_id, connection_revision_id, current_qualification_revision_id, version)
         VALUES ($1, $2, $3, 1)",
    )
    .bind(workspace_a().as_uuid())
    .bind(connection_revision_id)
    .bind(connection_qualification_revision_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_revisions
             (id, workspace_id, model_revision_id, connection_revision_id,
              connection_qualification_revision_id, qualification_job_id, capabilities, valid_until)
         VALUES ($1, $2, $3, $4, $5, $6, ARRAY['chat'], NOW() + INTERVAL '1 hour')",
    )
    .bind(model_qualification_revision_id)
    .bind(workspace_a().as_uuid())
    .bind(model_revision_id)
    .bind(connection_revision_id)
    .bind(connection_qualification_revision_id)
    .bind(qualification_job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_heads
             (workspace_id, model_revision_id, current_qualification_revision_id, version)
         VALUES ($1, $2, $3, 1)",
    )
    .bind(workspace_a().as_uuid())
    .bind(model_revision_id)
    .bind(model_qualification_revision_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
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

fn prepared_event(
    event_id: RunEventId,
    workspace_id: WorkspaceId,
    actor: RunActor,
) -> LegacyRunEventEnvelope {
    let occurred_at = database_timestamp(now());
    let payload = LegacyRunEvent::Prepared;
    LegacyRunEventEnvelope {
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

fn prepared_projection(created: &AgentRun, updated_at: chrono::DateTime<chrono::Utc>) -> AgentRun {
    AgentRun {
        status: RunStatus::Preparing,
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
    configure_chat_workspace_default(&pool).await;
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
            command(RunVersion::INITIAL, RunCommand::Prepare),
        )
        .await
        .unwrap();

    assert_eq!(ready.run.status, RunStatus::Preparing);
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

    let duplicate = prepared_event(
        created.events[0].event_id,
        workspace_a(),
        RunActor::Principal(principal_a()),
    );
    let projection = prepared_projection(&created.run, duplicate.occurred_at);
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
    let payload = LegacyRunEvent::Created {
        principal_id: principal_a(),
        title: "Mismatch".to_owned(),
    };
    let event = LegacyRunEventEnvelope {
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
        objective: "Mismatch".to_owned(),
        coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(principal_a().as_uuid()),
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
        created_at: occurred_at,
        updated_at: occurred_at,
        finished_at: None,
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
    configure_chat_workspace_default(&pool).await;
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

    let event_a = prepared_event(
        RunEventId::new(),
        workspace_a(),
        RunActor::Principal(principal_a()),
    );
    let event_b = prepared_event(
        RunEventId::new(),
        workspace_a(),
        RunActor::Principal(principal_a()),
    );
    let projection_a = prepared_projection(&created.run, event_a.occurred_at);
    let projection_b = prepared_projection(&created.run, event_b.occurred_at);
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

    let event = prepared_event(
        RunEventId::new(),
        workspace_b(),
        RunActor::Principal(principal_b()),
    );
    let projection = AgentRun {
        workspace_id: workspace_b(),
        coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(principal_b().as_uuid()),
        status: RunStatus::Preparing,
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
