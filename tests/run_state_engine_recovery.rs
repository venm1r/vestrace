//! P1/PR1 end-to-end State Engine recovery acceptance.

use std::{str::FromStr, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use sqlx::types::Uuid;
use vestrace_application::{
    ApplicationError, RequestContext, RunCommandExecutor, RunCommandService, RunRecoveryOperations,
    RunRecoveryService, RunRepository,
};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunStepId, WorkspaceId},
    now,
    run::{RunActor, RunCommand, RunCommandEnvelope, RunStatus, RunVersion},
};
use vestrace_infrastructure::{
    PgRunCommandCommitter, PgRunEventStore, PgRunRecoveryStore, PgRunRepository, PgStore,
};

const WORKSPACE_ID: &str = "64000000-0000-0000-0000-000000000001";
const PRINCIPAL_ID: &str = "64000000-0000-0000-0000-000000000002";
const RUN_ID: &str = "64000000-0000-0000-0000-000000000003";

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
         VALUES ($1::uuid, 'state-engine-recovery')",
    )
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES ($1::uuid, $2::uuid, 'state-engine-recovery-principal')",
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
    let profile_revision = "state-engine-recovery/v1";
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
    sqlx::query("INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, 'state-engine-chat', 'local')")
        .bind(connector_id)
        .bind(workspace_id().as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO providers (id, workspace_id, name, locality) VALUES ($1, $2, 'state-engine-chat', 'local')")
        .bind(provider_id)
        .bind(workspace_id().as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO connections (id, connector_id, workspace_id, principal_id, name, status) VALUES ($1, $2, $3, $4, 'state-engine-chat', 'active')")
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
    sqlx::query("INSERT INTO models (id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken) VALUES ($1, $2, $3, 'state-engine-chat', 4096, 0, 0)")
        .bind(model_id)
        .bind(provider_id)
        .bind(workspace_id().as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_revision_and_advance_head($1, $2, $3, $4, $5, $6, 'state-engine-chat', 'chat', NULL::INTEGER, NULL::UUID, NULL::TEXT, NULL::INTEGER, NULL::UUID, NULL::TEXT, 0::BIGINT)")
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

fn services(pool: &sqlx::PgPool) -> (RunCommandService, RunRecoveryService, PgRunRepository) {
    let store = PgStore::from_pool(pool.clone());
    let command_service = RunCommandService::new(
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunCommandCommitter::new(store.clone())),
    );
    let recovery_service =
        RunRecoveryService::new(Arc::new(PgRunRecoveryStore::new(store.clone())));
    let repository = PgRunRepository::new(store);
    (command_service, recovery_service, repository)
}

async fn execute(service: &RunCommandService, expected_version: u64, command_value: RunCommand) {
    service
        .execute(
            &context(),
            command(RunVersion::new(expected_version).unwrap(), command_value),
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn checkpoint_plus_tail_rebuilds_a_deleted_projection_exactly(pool: sqlx::PgPool) {
    seed_identity(&pool).await;
    configure_chat_workspace_default(&pool).await;
    let (commands, recovery, repository) = services(&pool);
    let step_id = RunStepId::new();

    commands
        .execute(
            &context(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_id(),
                    title: "Recoverable State Engine run".to_owned(),
                },
            ),
        )
        .await
        .unwrap();
    execute(&commands, 1, RunCommand::Prepare).await;
    execute(&commands, 2, RunCommand::Start).await;

    let checkpoint = recovery
        .create_checkpoint(&context(), run_id())
        .await
        .unwrap();
    assert_eq!(checkpoint.sequence, RunVersion::new(3).unwrap());

    execute(
        &commands,
        3,
        RunCommand::StartStep {
            step_id,
            kind: "tool".to_owned(),
            label: Some("recoverable step".to_owned()),
        },
    )
    .await;
    execute(
        &commands,
        4,
        RunCommand::CompleteStep {
            step_id,
            output_references: vec!["artifact://result".to_owned()],
        },
    )
    .await;
    execute(
        &commands,
        5,
        RunCommand::Succeed {
            summary: Some("recovered".to_owned()),
        },
    )
    .await;

    let expected_state = recovery.restore(&context(), run_id()).await.unwrap();
    let expected_projection = repository
        .find_by_id(&context(), run_id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(expected_state.version, RunVersion::new(6).unwrap());
    assert_eq!(expected_projection.status, RunStatus::Succeeded);

    sqlx::query(
        "DELETE FROM agent_runs
         WHERE workspace_id = $1::uuid AND id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(&pool)
    .await
    .unwrap();

    let preserved: (i64, i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT count(*) FROM run_streams WHERE workspace_id = $1::uuid AND run_id = $2::uuid),
             (SELECT count(*) FROM run_events WHERE workspace_id = $1::uuid AND run_id = $2::uuid),
             (SELECT count(*) FROM run_checkpoints WHERE workspace_id = $1::uuid AND run_id = $2::uuid)",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(preserved, (1, 6, 1));
    assert!(
        repository
            .find_by_id(&context(), run_id())
            .await
            .unwrap()
            .is_none()
    );

    let validated = recovery
        .validate_checkpoint(&context(), run_id(), checkpoint.sequence)
        .await
        .unwrap();
    assert_eq!(validated, checkpoint);
    assert_eq!(
        recovery.restore(&context(), run_id()).await.unwrap(),
        expected_state
    );

    let rebuilt = recovery
        .rebuild_projection(&context(), run_id())
        .await
        .unwrap();
    assert_eq!(rebuilt, expected_projection);
    assert_eq!(
        repository.find_by_id(&context(), run_id()).await.unwrap(),
        Some(expected_projection)
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn corrupted_checkpoint_is_rejected_instead_of_falling_back_to_projection(
    pool: sqlx::PgPool,
) {
    seed_identity(&pool).await;
    configure_chat_workspace_default(&pool).await;
    let (commands, recovery, _) = services(&pool);

    commands
        .execute(
            &context(),
            command(
                RunVersion::ZERO,
                RunCommand::Create {
                    principal_id: principal_id(),
                    title: "Corruption detection".to_owned(),
                },
            ),
        )
        .await
        .unwrap();
    execute(&commands, 1, RunCommand::Prepare).await;
    let checkpoint = recovery
        .create_checkpoint(&context(), run_id())
        .await
        .unwrap();

    sqlx::query(
        "UPDATE run_checkpoints
         SET state = jsonb_set(state, '{title}', '\"tampered\"'::jsonb)
         WHERE workspace_id = $1::uuid
           AND run_id = $2::uuid
           AND sequence = $3",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .bind(i64::try_from(checkpoint.sequence.value()).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let result = recovery.restore(&context(), run_id()).await;
    assert!(matches!(result, Err(ApplicationError::Storage(_))));
}
