//! PostgreSQL run event-store behavior for R1.2.

use std::str::FromStr;

use vestrace_application::{RequestContext, RunEventStore};
use vestrace_domain::{
    id::{AgentRunId, PrincipalId, WorkspaceId},
    run::{RunActor, RunEvent},
};
use vestrace_infrastructure::{PgRunEventStore, PgStore};

const WORKSPACE_ID: &str = "52000000-0000-0000-0000-000000000001";
const PRINCIPAL_ID: &str = "52000000-0000-0000-0000-000000000002";
const RUN_ID: &str = "52000000-0000-0000-0000-000000000003";
const EMPTY_RUN_ID: &str = "52000000-0000-0000-0000-000000000004";

async fn seed_owner(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug)
         VALUES ($1::uuid, 'run-event-store')",
    )
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES ($1::uuid, $2::uuid, 'run-event-store-principal')",
    )
    .bind(PRINCIPAL_ID)
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_runs (
             id, workspace_id, principal_id, title, status, run_version
         ) VALUES
         ($1::uuid, $2::uuid, $3::uuid, 'loaded', 'created', 1),
         ($4::uuid, $2::uuid, $3::uuid, 'empty', 'created', 1)",
    )
    .bind(RUN_ID)
    .bind(WORKSPACE_ID)
    .bind(PRINCIPAL_ID)
    .bind(EMPTY_RUN_ID)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn load_stream_returns_full_envelopes_in_sequence_order(pool: sqlx::PgPool) {
    seed_owner(&pool).await;

    let principal_id = PrincipalId::from_str(PRINCIPAL_ID).unwrap();
    let actor = serde_json::to_value(RunActor::Principal(principal_id)).unwrap();
    let created = serde_json::to_value(RunEvent::Created {
        principal_id,
        title: "loaded".to_owned(),
    })
    .unwrap();
    let ready = serde_json::to_value(RunEvent::MarkedReady).unwrap();

    for (
        event_id,
        sequence,
        event_type,
        causation_id,
        correlation_id,
        payload,
        occurred_at,
        recorded_at,
    ) in [
        (
            "52000000-0000-0000-0000-000000000012",
            2_i64,
            "run.marked_ready",
            "52000000-0000-0000-0000-000000000022",
            "52000000-0000-0000-0000-000000000032",
            ready,
            "2026-08-04T10:02:00Z",
            "2026-08-04T10:02:01Z",
        ),
        (
            "52000000-0000-0000-0000-000000000011",
            1_i64,
            "run.created",
            "52000000-0000-0000-0000-000000000021",
            "52000000-0000-0000-0000-000000000031",
            created,
            "2026-08-04T10:01:00Z",
            "2026-08-04T10:01:01Z",
        ),
    ] {
        sqlx::query(
            "INSERT INTO run_events (
                 id, workspace_id, run_id, sequence, event_type, event_version,
                 actor, causation_id, correlation_id, payload, occurred_at, created_at
             ) VALUES (
                 $1::uuid, $2::uuid, $3::uuid, $4, $5, 1,
                 $6, $7::uuid, $8::uuid, $9, $10::timestamptz, $11::timestamptz
             )",
        )
        .bind(event_id)
        .bind(WORKSPACE_ID)
        .bind(RUN_ID)
        .bind(sequence)
        .bind(event_type)
        .bind(actor.clone())
        .bind(causation_id)
        .bind(correlation_id)
        .bind(payload)
        .bind(occurred_at)
        .bind(recorded_at)
        .execute(&pool)
        .await
        .unwrap();
    }

    let workspace_id = WorkspaceId::from_str(WORKSPACE_ID).unwrap();
    let run_id = AgentRunId::from_str(RUN_ID).unwrap();
    let context = RequestContext::new(workspace_id, principal_id);
    let store = PgRunEventStore::new(PgStore::from_pool(pool));

    let events = store.load_stream(&context, run_id).await.unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence.value(), 1);
    assert_eq!(events[1].sequence.value(), 2);
    assert_eq!(events[0].event_type, "run.created");
    assert_eq!(events[1].event_type, "run.marked_ready");
    assert_eq!(events[0].actor, RunActor::Principal(principal_id));
    assert!(matches!(
        &events[0].payload,
        RunEvent::Created { principal_id: stored, title }
            if stored == principal_id && title == "loaded"
    ));
    assert_eq!(
        events[0].occurred_at.to_rfc3339(),
        "2026-08-04T10:01:00+00:00"
    );
    assert_eq!(
        events[0].recorded_at.to_rfc3339(),
        "2026-08-04T10:01:01+00:00"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn load_stream_returns_an_empty_vector_when_no_events_exist(pool: sqlx::PgPool) {
    seed_owner(&pool).await;

    let workspace_id = WorkspaceId::from_str(WORKSPACE_ID).unwrap();
    let principal_id = PrincipalId::from_str(PRINCIPAL_ID).unwrap();
    let run_id = AgentRunId::from_str(EMPTY_RUN_ID).unwrap();
    let context = RequestContext::new(workspace_id, principal_id);
    let store = PgRunEventStore::new(PgStore::from_pool(pool));

    let events = store.load_stream(&context, run_id).await.unwrap();

    assert!(events.is_empty());
}
