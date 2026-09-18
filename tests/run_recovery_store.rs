//! PostgreSQL checkpoint, event-range, and projection-rebuild storage contract.

use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use vestrace_application::{
    ApplicationError, RequestContext, RunCheckpoint, RunRecoveryStore, hash_run_state, project_run,
};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId},
    now,
    run::{LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunVersion, replay},
};
use vestrace_infrastructure::{PgRunRecoveryStore, PgStore};

const WORKSPACE_A: &str = "62000000-0000-0000-0000-000000000001";
const WORKSPACE_B: &str = "62000000-0000-0000-0000-000000000002";
const PRINCIPAL_A: &str = "62000000-0000-0000-0000-000000000003";
const PRINCIPAL_B: &str = "62000000-0000-0000-0000-000000000004";
const RUN_ID: &str = "62000000-0000-0000-0000-000000000005";

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

async fn seed_identities(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ($1::uuid, 'recovery-store-a'),
         ($2::uuid, 'recovery-store-b')",
    )
    .bind(WORKSPACE_A)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES
         ($1::uuid, $2::uuid, 'recovery-store-principal-a'),
         ($3::uuid, $4::uuid, 'recovery-store-principal-b')",
    )
    .bind(PRINCIPAL_A)
    .bind(WORKSPACE_A)
    .bind(PRINCIPAL_B)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();
}

fn event(
    sequence: u64,
    payload: LegacyRunEvent,
    occurred_at: DateTime<Utc>,
) -> LegacyRunEventEnvelope {
    LegacyRunEventEnvelope {
        event_id: RunEventId::new(),
        workspace_id: workspace_a(),
        run_id: run_id(),
        sequence: RunVersion::new(sequence).unwrap(),
        event_type: payload.event_type().to_owned(),
        event_version: payload.event_version(),
        actor: RunActor::Principal(principal_a()),
        causation_id: OperationId::new(),
        correlation_id: CorrelationId::new(),
        payload,
        occurred_at,
        recorded_at: occurred_at,
    }
}

fn canonical_events() -> Vec<LegacyRunEventEnvelope> {
    let first = database_timestamp(now());
    let second = database_timestamp(first + Duration::seconds(1));
    vec![
        event(
            1,
            LegacyRunEvent::Created {
                principal_id: principal_a(),
                title: "Recoverable PostgreSQL run".to_owned(),
            },
            first,
        ),
        event(2, LegacyRunEvent::Prepared, second),
    ]
}

async fn seed_stream_and_events(pool: &sqlx::PgPool, events: &[LegacyRunEventEnvelope]) {
    sqlx::query(
        "INSERT INTO run_streams (
             workspace_id, run_id, current_version, created_at, updated_at
         ) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(workspace_a().as_uuid())
    .bind(run_id().as_uuid())
    .bind(i64::try_from(events.last().unwrap().sequence.value()).unwrap())
    .bind(events.first().unwrap().occurred_at)
    .bind(events.last().unwrap().occurred_at)
    .execute(pool)
    .await
    .unwrap();

    for event in events {
        sqlx::query(
            "INSERT INTO run_events (
                 id, workspace_id, run_id, sequence, run_version, sequence_value,
                 event_type, event_version,
                 actor, causation_id, correlation_id, payload,
                 occurred_at, created_at, recorded_at
             ) VALUES ($1, $2, $3, $4, $4, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)",
        )
        .bind(event.event_id.as_uuid())
        .bind(event.workspace_id.as_uuid())
        .bind(event.run_id.as_uuid())
        .bind(i64::try_from(event.sequence.value()).unwrap())
        .bind(&event.event_type)
        .bind(i16::try_from(event.event_version).unwrap())
        .bind(serde_json::to_value(&event.actor).unwrap())
        .bind(event.causation_id.as_uuid())
        .bind(event.correlation_id.as_uuid())
        .bind(serde_json::to_value(&event.payload).unwrap())
        .bind(event.occurred_at)
        .bind(event.recorded_at)
        .execute(pool)
        .await
        .unwrap();
    }
}

fn checkpoint(events: &[LegacyRunEventEnvelope]) -> RunCheckpoint {
    let state = replay(events.to_vec()).unwrap().unwrap();
    RunCheckpoint {
        workspace_id: workspace_a(),
        run_id: run_id(),
        sequence: state.version,
        format_version: 1,
        state_hash: hash_run_state(&state).unwrap(),
        state,
        created_at: database_timestamp(now()),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn checkpoint_round_trips_typed_state(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let events = canonical_events();
    seed_stream_and_events(&pool, &events).await;
    let expected = checkpoint(&events);
    let store = PgRunRecoveryStore::new(PgStore::from_pool(pool));

    store
        .save_checkpoint(&context_a(), &expected)
        .await
        .unwrap();

    assert_eq!(
        store
            .load_checkpoint(&context_a(), run_id(), expected.sequence)
            .await
            .unwrap(),
        Some(expected.clone())
    );
    assert_eq!(
        store
            .load_latest_checkpoint(&context_a(), run_id())
            .await
            .unwrap(),
        Some(expected)
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn identical_checkpoint_write_is_idempotent_but_conflicting_content_is_rejected(
    pool: sqlx::PgPool,
) {
    seed_identities(&pool).await;
    let events = canonical_events();
    seed_stream_and_events(&pool, &events).await;
    let expected = checkpoint(&events);
    let store = PgRunRecoveryStore::new(PgStore::from_pool(pool));

    store
        .save_checkpoint(&context_a(), &expected)
        .await
        .unwrap();
    store
        .save_checkpoint(&context_a(), &expected)
        .await
        .unwrap();

    let mut conflicting = expected;
    conflicting.state.title = "conflicting snapshot".to_owned();
    conflicting.state_hash = hash_run_state(&conflicting.state).unwrap();
    let result = store.save_checkpoint(&context_a(), &conflicting).await;
    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
}

#[sqlx::test(migrations = "./migrations")]
async fn event_ranges_are_ordered_and_bounded(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let events = canonical_events();
    seed_stream_and_events(&pool, &events).await;
    let store = PgRunRecoveryStore::new(PgStore::from_pool(pool));

    assert_eq!(
        store
            .load_events_through(&context_a(), run_id(), RunVersion::INITIAL)
            .await
            .unwrap(),
        vec![events[0].clone()]
    );
    assert_eq!(
        store
            .load_events_after(&context_a(), run_id(), RunVersion::INITIAL)
            .await
            .unwrap(),
        vec![events[1].clone()]
    );
    assert_eq!(
        store
            .load_stream_head(&context_a(), run_id())
            .await
            .unwrap(),
        Some(RunVersion::new(2).unwrap())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn guarded_projection_replacement_recreates_missing_projection(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let events = canonical_events();
    seed_stream_and_events(&pool, &events).await;
    let state = replay(events).unwrap().unwrap();
    let expected = project_run(&state);
    let store = PgRunRecoveryStore::new(PgStore::from_pool(pool.clone()));

    store
        .replace_projection(&context_a(), expected.version, &expected)
        .await
        .unwrap();

    let row: (String, i64) = sqlx::query_as(
        "SELECT status, run_version
         FROM agent_runs
         WHERE workspace_id = $1 AND id = $2",
    )
    .bind(workspace_a().as_uuid())
    .bind(run_id().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("preparing".to_owned(), 2));
}

#[sqlx::test(migrations = "./migrations")]
async fn projection_replacement_rejects_stale_stream_head(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let events = canonical_events();
    seed_stream_and_events(&pool, &events).await;
    let projection = project_run(&replay(events).unwrap().unwrap());
    let store = PgRunRecoveryStore::new(PgStore::from_pool(pool));

    let result = store
        .replace_projection(&context_a(), RunVersion::INITIAL, &projection)
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
}

#[sqlx::test(migrations = "./migrations")]
async fn foreign_workspace_cannot_observe_or_replace_stream_state(pool: sqlx::PgPool) {
    seed_identities(&pool).await;
    let events = canonical_events();
    seed_stream_and_events(&pool, &events).await;
    let projection = project_run(&replay(events).unwrap().unwrap());
    let store = PgRunRecoveryStore::new(PgStore::from_pool(pool));

    assert_eq!(
        store
            .load_stream_head(&context_b(), run_id())
            .await
            .unwrap(),
        None
    );
    assert!(
        store
            .load_latest_checkpoint(&context_b(), run_id())
            .await
            .unwrap()
            .is_none()
    );
    let result = store
        .replace_projection(&context_b(), projection.version, &projection)
        .await;
    assert!(matches!(result, Err(ApplicationError::Domain(_))));
}
