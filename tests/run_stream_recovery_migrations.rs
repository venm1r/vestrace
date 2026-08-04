//! P1/PR1 stream authority and checkpoint migration contract.

mod support;

use support::assert_sqlstate;

const WORKSPACE_ID: &str = "61000000-0000-0000-0000-000000000001";
const PRINCIPAL_ID: &str = "61000000-0000-0000-0000-000000000002";
const RUN_ID: &str = "61000000-0000-0000-0000-000000000003";
const EVENT_ID: &str = "61000000-0000-0000-0000-000000000004";
const CAUSATION_ID: &str = "61000000-0000-0000-0000-000000000005";
const CORRELATION_ID: &str = "61000000-0000-0000-0000-000000000006";

async fn seed_identity(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug)
         VALUES ($1::uuid, 'run-stream-recovery')",
    )
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier)
         VALUES ($1::uuid, $2::uuid, 'run-stream-principal')",
    )
    .bind(PRINCIPAL_ID)
    .bind(WORKSPACE_ID)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_stream_projection_event_and_checkpoint(pool: &sqlx::PgPool) {
    seed_identity(pool).await;

    sqlx::query(
        "INSERT INTO run_streams (
             workspace_id, run_id, current_version, created_at, updated_at
         ) VALUES ($1::uuid, $2::uuid, 1, now(), now())",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_runs (
             id, workspace_id, principal_id, title, status, run_version
         ) VALUES ($1::uuid, $2::uuid, $3::uuid, 'recoverable', 'created', 1)",
    )
    .bind(RUN_ID)
    .bind(WORKSPACE_ID)
    .bind(PRINCIPAL_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO run_events (
             id, workspace_id, run_id, sequence, event_type, event_version,
             actor, causation_id, correlation_id, payload, occurred_at, created_at
         ) VALUES (
             $1::uuid, $2::uuid, $3::uuid, 1, 'run.created', 1,
             '{\"system\":{\"component\":\"migration-contract\"}}'::jsonb,
             $4::uuid, $5::uuid,
             '{\"created\":{\"principal_id\":\"61000000-0000-0000-0000-000000000002\",\"title\":\"recoverable\"}}'::jsonb,
             now(), now()
         )",
    )
    .bind(EVENT_ID)
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .bind(CAUSATION_ID)
    .bind(CORRELATION_ID)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO run_checkpoints (
             workspace_id, run_id, sequence, format_version, state_hash, state, created_at
         ) VALUES (
             $1::uuid, $2::uuid, 1, 1,
             '0000000000000000000000000000000000000000000000000000000000000000',
             '{}'::jsonb,
             now()
         )",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn recovery_tables_exist(pool: sqlx::PgPool) {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name
         FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name IN ('run_streams', 'run_checkpoints')
         ORDER BY table_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(tables, vec!["run_checkpoints", "run_streams"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn run_events_belong_to_streams_not_projections(pool: sqlx::PgPool) {
    let referenced_table: String = sqlx::query_scalar(
        "SELECT confrelid::regclass::text
         FROM pg_constraint
         WHERE conrelid = 'run_events'::regclass
           AND conname = 'run_events_workspace_run_fkey'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(referenced_table, "run_streams");
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_projection_preserves_stream_events_and_checkpoints(pool: sqlx::PgPool) {
    seed_stream_projection_event_and_checkpoint(&pool).await;

    sqlx::query(
        "DELETE FROM agent_runs
         WHERE workspace_id = $1::uuid AND id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(&pool)
    .await
    .unwrap();

    let stream_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run_streams
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run_events
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    let checkpoint_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run_checkpoints
         WHERE workspace_id = $1::uuid AND run_id = $2::uuid",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!((stream_count, event_count, checkpoint_count), (1, 1, 1));
}

#[sqlx::test(migrations = "./migrations")]
async fn checkpoints_reject_invalid_format_and_hash(pool: sqlx::PgPool) {
    seed_identity(&pool).await;
    sqlx::query(
        "INSERT INTO run_streams (
             workspace_id, run_id, current_version, created_at, updated_at
         ) VALUES ($1::uuid, $2::uuid, 0, now(), now())",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(&pool)
    .await
    .unwrap();

    let invalid_format = sqlx::query(
        "INSERT INTO run_checkpoints (
             workspace_id, run_id, sequence, format_version, state_hash, state, created_at
         ) VALUES ($1::uuid, $2::uuid, 1, 2, repeat('0', 64), '{}'::jsonb, now())",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(&pool)
    .await;
    assert_sqlstate(invalid_format, "23514");

    let invalid_hash = sqlx::query(
        "INSERT INTO run_checkpoints (
             workspace_id, run_id, sequence, format_version, state_hash, state, created_at
         ) VALUES ($1::uuid, $2::uuid, 1, 1, 'not-a-sha256', '{}'::jsonb, now())",
    )
    .bind(WORKSPACE_ID)
    .bind(RUN_ID)
    .execute(&pool)
    .await;
    assert_sqlstate(invalid_hash, "23514");
}
