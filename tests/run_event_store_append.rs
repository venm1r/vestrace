//! Optimistic PostgreSQL run event append behavior for R1.2.

use std::{str::FromStr, sync::Arc};

use tokio::sync::Barrier;
use vestrace_application::{ApplicationError, RequestContext, RunEventStore};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId},
    now,
    run::{RunActor, RunEvent, RunEventEnvelope, RunVersion},
};
use vestrace_infrastructure::{PgRunEventStore, PgStore};

const WORKSPACE_ID: &str = "53000000-0000-0000-0000-000000000001";
const OTHER_WORKSPACE_ID: &str = "53000000-0000-0000-0000-000000000002";
const PRINCIPAL_ID: &str = "53000000-0000-0000-0000-000000000003";
const RUN_ID: &str = "53000000-0000-0000-0000-000000000004";
const OTHER_RUN_ID: &str = "53000000-0000-0000-0000-000000000005";

async fn seed_owner(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ($1::uuid, 'run-event-append'),
         ($2::uuid, 'run-event-append-other')",
    )
    .bind(WORKSPACE_ID)
    .bind(OTHER_WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES ($1::uuid, $2::uuid, 'run-event-append-principal')",
    )
    .bind(PRINCIPAL_ID)
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_runs (
             id, workspace_id, principal_id, title, status, run_version
         ) VALUES ($1::uuid, $2::uuid, $3::uuid, 'append', 'created', 1)",
    )
    .bind(RUN_ID)
    .bind(WORKSPACE_ID)
    .bind(PRINCIPAL_ID)
    .execute(pool)
    .await
    .unwrap();
}

fn context() -> RequestContext {
    RequestContext::new(
        WorkspaceId::from_str(WORKSPACE_ID).unwrap(),
        PrincipalId::from_str(PRINCIPAL_ID).unwrap(),
    )
}

fn run_id() -> AgentRunId {
    AgentRunId::from_str(RUN_ID).unwrap()
}

fn event(
    workspace_id: WorkspaceId,
    run_id: AgentRunId,
    sequence: u64,
    payload: RunEvent,
) -> RunEventEnvelope {
    let at = now();
    RunEventEnvelope {
        event_id: RunEventId::new(),
        workspace_id,
        run_id,
        sequence: RunVersion::new(sequence).unwrap(),
        event_type: payload.event_type().to_owned(),
        event_version: payload.event_version(),
        actor: RunActor::Principal(PrincipalId::from_str(PRINCIPAL_ID).unwrap()),
        causation_id: OperationId::new(),
        correlation_id: CorrelationId::new(),
        payload,
        occurred_at: at,
        recorded_at: at,
    }
}

fn first_event() -> RunEventEnvelope {
    event(
        WorkspaceId::from_str(WORKSPACE_ID).unwrap(),
        run_id(),
        1,
        RunEvent::Created {
            principal_id: PrincipalId::from_str(PRINCIPAL_ID).unwrap(),
            title: "append".to_owned(),
        },
    )
}

fn second_event() -> RunEventEnvelope {
    event(
        WorkspaceId::from_str(WORKSPACE_ID).unwrap(),
        run_id(),
        2,
        RunEvent::MarkedReady,
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn append_stores_the_first_event_at_version_zero(pool: sqlx::PgPool) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));

    let version = store
        .append(&context(), run_id(), RunVersion::ZERO, &[first_event()])
        .await
        .unwrap();

    assert_eq!(version, RunVersion::INITIAL);
    let stored = store.load_stream(&context(), run_id()).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].sequence, RunVersion::INITIAL);
}

#[sqlx::test(migrations = "./migrations")]
async fn append_continues_a_stream_from_the_expected_version(pool: sqlx::PgPool) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));

    store
        .append(&context(), run_id(), RunVersion::ZERO, &[first_event()])
        .await
        .unwrap();
    let version = store
        .append(&context(), run_id(), RunVersion::INITIAL, &[second_event()])
        .await
        .unwrap();

    assert_eq!(version.value(), 2);
    let stored = store.load_stream(&context(), run_id()).await.unwrap();
    assert_eq!(
        stored
            .iter()
            .map(|event| event.sequence.value())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn append_rejects_a_stale_expected_version_after_reading_the_stream_head(
    pool: sqlx::PgPool,
) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));

    store
        .append(&context(), run_id(), RunVersion::ZERO, &[first_event()])
        .await
        .unwrap();
    let result = store
        .append(&context(), run_id(), RunVersion::ZERO, &[first_event()])
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    assert_eq!(
        store.load_stream(&context(), run_id()).await.unwrap().len(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn append_rejects_an_empty_batch(pool: sqlx::PgPool) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));

    let result = store
        .append(&context(), run_id(), RunVersion::ZERO, &[])
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    assert!(
        store
            .load_stream(&context(), run_id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn append_rejects_mixed_workspace_identity(pool: sqlx::PgPool) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));
    let invalid = event(
        WorkspaceId::from_str(OTHER_WORKSPACE_ID).unwrap(),
        run_id(),
        1,
        RunEvent::MarkedReady,
    );

    let result = store
        .append(&context(), run_id(), RunVersion::ZERO, &[invalid])
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    assert!(
        store
            .load_stream(&context(), run_id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn append_rejects_mixed_run_identity(pool: sqlx::PgPool) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));
    let invalid = event(
        WorkspaceId::from_str(WORKSPACE_ID).unwrap(),
        AgentRunId::from_str(OTHER_RUN_ID).unwrap(),
        1,
        RunEvent::MarkedReady,
    );

    let result = store
        .append(&context(), run_id(), RunVersion::ZERO, &[invalid])
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    assert!(
        store
            .load_stream(&context(), run_id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn append_rejects_sequence_gaps_without_partial_writes(pool: sqlx::PgPool) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));
    let third = event(
        WorkspaceId::from_str(WORKSPACE_ID).unwrap(),
        run_id(),
        3,
        RunEvent::Started,
    );

    let result = store
        .append(
            &context(),
            run_id(),
            RunVersion::ZERO,
            &[first_event(), third],
        )
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    assert!(
        store
            .load_stream(&context(), run_id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn database_failure_rolls_back_the_entire_batch(pool: sqlx::PgPool) {
    seed_owner(&pool).await;
    let store = PgRunEventStore::new(PgStore::from_pool(pool));
    let first = first_event();
    let mut second = second_event();
    second.event_id = first.event_id;

    let result = store
        .append(
            &context(),
            run_id(),
            RunVersion::ZERO,
            &[first, second],
        )
        .await;

    assert!(matches!(result, Err(ApplicationError::Storage(_))));
    assert!(
        store
            .load_stream(&context(), run_id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_writers_cannot_claim_the_same_stream_version(pool: sqlx::PgPool) {
    seed_owner(&pool).await;

    let store_a = PgRunEventStore::new(PgStore::from_pool(pool.clone()));
    let store_b = PgRunEventStore::new(PgStore::from_pool(pool));
    let barrier = Arc::new(Barrier::new(3));

    let barrier_a = Arc::clone(&barrier);
    let writer_a = tokio::spawn(async move {
        barrier_a.wait().await;
        store_a
            .append(&context(), run_id(), RunVersion::ZERO, &[first_event()])
            .await
    });

    let barrier_b = Arc::clone(&barrier);
    let writer_b = tokio::spawn(async move {
        barrier_b.wait().await;
        store_b
            .append(&context(), run_id(), RunVersion::ZERO, &[first_event()])
            .await
    });

    barrier.wait().await;
    let result_a = writer_a.await.unwrap();
    let result_b = writer_b.await.unwrap();

    let successes = [result_a.as_ref(), result_b.as_ref()]
        .into_iter()
        .filter(|result| result.is_ok())
        .count();
    let conflicts = [result_a.as_ref(), result_b.as_ref()]
        .into_iter()
        .filter(|result| matches!(result, Err(ApplicationError::Conflict(_))))
        .count();

    assert_eq!(successes, 1);
    assert_eq!(conflicts, 1);

    let verifier = PgRunEventStore::new(PgStore::from_pool(
        sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap(),
    ));
    assert_eq!(
        verifier
            .load_stream(&context(), run_id())
            .await
            .unwrap()
            .len(),
        1
    );
}
