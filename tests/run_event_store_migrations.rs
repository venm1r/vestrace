//! R1.2 run event-store migration contract.

mod support;

use sqlx::types::{
    Uuid,
    chrono::{DateTime, Utc},
};
use support::assert_sqlstate;

const WORKSPACE_A: &str = "51000000-0000-0000-0000-000000000001";
const WORKSPACE_B: &str = "51000000-0000-0000-0000-000000000002";
const PRINCIPAL_A: &str = "51000000-0000-0000-0000-000000000003";
const PRINCIPAL_B: &str = "51000000-0000-0000-0000-000000000004";
const RUN_A: &str = "51000000-0000-0000-0000-000000000005";
const RUN_B: &str = "51000000-0000-0000-0000-000000000006";

async fn seed_run_owners(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ($1::uuid, 'run-event-workspace-a'),
         ($2::uuid, 'run-event-workspace-b')",
    )
    .bind(WORKSPACE_A)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES
         ($1::uuid, $2::uuid, 'run-event-principal-a'),
         ($3::uuid, $4::uuid, 'run-event-principal-b')",
    )
    .bind(PRINCIPAL_A)
    .bind(WORKSPACE_A)
    .bind(PRINCIPAL_B)
    .bind(WORKSPACE_B)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_runs (
             id, workspace_id, principal_id, title, objective, status, run_version
         ) VALUES
         ($1::uuid, $2::uuid, $3::uuid, 'run-a', 'run-a', 'created', 1),
         ($4::uuid, $5::uuid, $6::uuid, 'run-b', 'run-b', 'created', 1)",
    )
    .bind(RUN_A)
    .bind(WORKSPACE_A)
    .bind(PRINCIPAL_A)
    .bind(RUN_B)
    .bind(WORKSPACE_B)
    .bind(PRINCIPAL_B)
    .execute(pool)
    .await
    .unwrap();
}

fn insert_event_sql() -> &'static str {
    "INSERT INTO run_events (
         id, workspace_id, run_id, sequence, run_version, sequence_value,
         event_type, event_version,
         actor, causation_id, correlation_id, payload,
         occurred_at, created_at, recorded_at
     ) VALUES (
         $1::uuid, $2::uuid, $3::uuid, $4, $4, $4, $5, $6,
         '{\"system\":{\"component\":\"migration-test\"}}'::jsonb,
         $7::uuid, $8::uuid, '{}'::jsonb, now(), now(), now()
     )"
}

#[sqlx::test(migrations = "./migrations")]
async fn run_events_expose_the_r1_envelope_columns(pool: sqlx::PgPool) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'run_events'
           AND column_name IN (
               'event_version',
               'actor',
               'causation_id',
               'correlation_id',
               'occurred_at'
           )
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        columns,
        vec![
            "actor",
            "causation_id",
            "correlation_id",
            "event_version",
            "occurred_at",
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn run_event_envelope_columns_are_required(pool: sqlx::PgPool) {
    let nullable: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'run_events'
           AND column_name IN (
               'event_version',
               'actor',
               'causation_id',
               'correlation_id',
               'occurred_at'
           )
           AND is_nullable = 'YES'
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(
        nullable.is_empty(),
        "nullable envelope columns: {nullable:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn run_events_reject_non_positive_sequence(pool: sqlx::PgPool) {
    seed_run_owners(&pool).await;

    let result = sqlx::query(insert_event_sql())
        .bind("51000000-0000-0000-0000-000000000010")
        .bind(WORKSPACE_A)
        .bind(RUN_A)
        .bind(0_i64)
        .bind("run.created")
        .bind(1_i16)
        .bind("51000000-0000-0000-0000-000000000011")
        .bind("51000000-0000-0000-0000-000000000012")
        .execute(&pool)
        .await;

    assert_sqlstate(result, "23514");
}

#[sqlx::test(migrations = "./migrations")]
async fn run_events_reject_non_positive_event_version(pool: sqlx::PgPool) {
    seed_run_owners(&pool).await;

    let result = sqlx::query(insert_event_sql())
        .bind("51000000-0000-0000-0000-000000000020")
        .bind(WORKSPACE_A)
        .bind(RUN_A)
        .bind(1_i64)
        .bind("run.created")
        .bind(0_i16)
        .bind("51000000-0000-0000-0000-000000000021")
        .bind("51000000-0000-0000-0000-000000000022")
        .execute(&pool)
        .await;

    assert_sqlstate(result, "23514");
}

#[sqlx::test(migrations = "./migrations")]
async fn run_events_reject_blank_event_type(pool: sqlx::PgPool) {
    seed_run_owners(&pool).await;

    let result = sqlx::query(insert_event_sql())
        .bind("51000000-0000-0000-0000-000000000030")
        .bind(WORKSPACE_A)
        .bind(RUN_A)
        .bind(1_i64)
        .bind("   ")
        .bind(1_i16)
        .bind("51000000-0000-0000-0000-000000000031")
        .bind("51000000-0000-0000-0000-000000000032")
        .execute(&pool)
        .await;

    assert_sqlstate(result, "23514");
}

#[sqlx::test(migrations = "./migrations")]
async fn run_events_reject_a_run_owned_by_another_workspace(pool: sqlx::PgPool) {
    seed_run_owners(&pool).await;

    let result = sqlx::query(insert_event_sql())
        .bind("51000000-0000-0000-0000-000000000040")
        .bind(WORKSPACE_A)
        .bind(RUN_B)
        .bind(1_i64)
        .bind("run.created")
        .bind(1_i16)
        .bind("51000000-0000-0000-0000-000000000041")
        .bind("51000000-0000-0000-0000-000000000042")
        .execute(&pool)
        .await;

    assert_sqlstate(result, "23503");
}

#[sqlx::test(migrations = "./migrations")]
async fn run_event_migration_backfills_legacy_rows_deterministically(pool: sqlx::PgPool) {
    seed_run_owners(&pool).await;

    // A genuinely legacy row predates every column later migrations backfill,
    // including the run_version/sequence_value pair migration 0020 introduced.
    for column in [
        "event_version",
        "actor",
        "causation_id",
        "correlation_id",
        "occurred_at",
        "run_version",
        "sequence_value",
    ] {
        sqlx::query(&format!(
            "ALTER TABLE run_events ALTER COLUMN {column} DROP NOT NULL"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO run_events (
             id, workspace_id, run_id, sequence, event_type, payload, created_at
         ) VALUES (
             '51000000-0000-0000-0000-000000000050',
             $1::uuid,
             $2::uuid,
             1,
             'run.created',
             '{}'::jsonb,
             '2026-08-04T10:00:00Z'::timestamptz
         )",
    )
    .bind(WORKSPACE_A)
    .bind(RUN_A)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(
        "../migrations/0111_run_event_store_foundation.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let row: (i16, serde_json::Value, Uuid, Uuid, DateTime<Utc>) = sqlx::query_as(
        "SELECT event_version, actor, causation_id, correlation_id, occurred_at
             FROM run_events
             WHERE id = '51000000-0000-0000-0000-000000000050'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.0, 1);
    assert_eq!(
        row.1,
        serde_json::json!({"system": {"component": "legacy_migration"}})
    );
    assert_eq!(
        row.2,
        Uuid::parse_str("51000000-0000-0000-0000-000000000050").unwrap()
    );
    assert_eq!(row.3, Uuid::parse_str(RUN_A).unwrap());
    assert_eq!(row.4.to_rfc3339(), "2026-08-04T10:00:00+00:00");
}
