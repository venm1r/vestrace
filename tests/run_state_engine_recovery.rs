//! P1/PR1 end-to-end State Engine recovery acceptance.

use std::{str::FromStr, sync::Arc};

use chrono::{DateTime, Duration, Utc};
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
    execute(&commands, 1, RunCommand::MarkReady).await;
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
        RunCommand::Complete {
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
    assert_eq!(expected_projection.status, RunStatus::Completed);

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
    execute(&commands, 1, RunCommand::MarkReady).await;
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
