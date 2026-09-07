//! Real-process fault evidence for the governed embedding dispatch path.
//!
//! This is deliberately ignored: it provisions PostgreSQL, spawns a parent
//! scenario and four aborting children, and relies on the runtime role.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use sqlx::{
    PgPool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
const PROVISIONER: &str = include_str!("../docker/postgres/init-runtime-role.sh");

fn provisioner_sql_from(marker: &str) -> &'static str {
    let start = PROVISIONER
        .find(marker)
        .unwrap_or_else(|| panic!("missing provisioner marker {marker}"));
    PROVISIONER[start..]
        .rsplit_once("\nSQL\n")
        .map(|(sql, _)| sql)
        .expect("the provisioner must contain the SQL heredoc terminator")
}

async fn provisioned_runtime(pool: &PgPool) -> PgPool {
    let extensions = PROVISIONER
        .lines()
        .filter(|line| line.starts_with("CREATE EXTENSION IF NOT EXISTS "))
        .collect::<Vec<_>>()
        .join("\n");
    sqlx::raw_sql(&extensions).execute(pool).await.unwrap();
    sqlx::query("ALTER SCHEMA public OWNER TO vestrace")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I OWNER TO vestrace', current_database()); END $$",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(pool)
    .await
    .expect("the real provisioner must precede runtime migration");
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .unwrap();
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let options = PgConnectOptions::from_str(&runtime_url)
        .expect("runtime database URL")
        .database(&database);
    let runtime = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("runtime must connect to the SQLx database");
    MIGRATOR
        .run(&runtime)
        .await
        .expect("restricted runtime must apply all migrations");
    runtime
}

#[sqlx::test(migrations = false)]
#[ignore = "needs PostgreSQL plus the vestrace runtime role; run with --ignored --nocapture"]
async fn embedding_dispatch_crashes_leave_only_the_accepted_baseline(pool: PgPool) {
    provisioned_runtime(&pool).await.close().await;
    let database_url = ephemeral_database_url(&pool).await;
    let url_file = write_url_file(&database_url);
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");

    for point in [
        "after_intent_persistence",
        "after_authorization_before_dispatch",
        "after_dispatch_before_receipt",
        "after_receipt_before_outcome_confirmation",
    ] {
        let output = Command::new(scenario_binary())
            .arg("--database-url-file")
            .arg(&url_file)
            .arg("--scenario")
            .arg("embedding_dispatch_crash")
            .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
            .env("VESTRACE_FAULT_POINT", point)
            .env("VESTRACE_RUNTIME_DATABASE_URL", &runtime_url)
            .output()
            .expect("the embedding fault scenario must start");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("EMBEDDING_FAULT_OUTPUT point={point} stdout={stdout} stderr={stderr}");
        assert!(
            output.status.success(),
            "the parent must reject any survivor tuple outside the accepted baseline at {point}: {stderr}"
        );
        let observation: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("the parent emits one JSON observation");
        assert_eq!(observation["point"], point);
        assert_eq!(observation["intent_count"], 1);
        let post_receipt = point == "after_receipt_before_outcome_confirmation";
        assert_eq!(observation["authorization_count"], i64::from(post_receipt));
        assert_eq!(observation["admission_count"], i64::from(post_receipt));
        assert_eq!(observation["dispatching_count"], i64::from(post_receipt));
        assert_eq!(observation["receipt_count"], i64::from(post_receipt));
        assert_eq!(observation["loopback_requests"], i64::from(post_receipt));
        assert_eq!(observation["control"]["authorization_count"], 1);
        assert_eq!(observation["control"]["admission_count"], 1);
        assert_eq!(observation["control"]["dispatching_count"], 1);
    }

    let output = Command::new(scenario_binary())
        .arg("--database-url-file")
        .arg(&url_file)
        .arg("--scenario")
        .arg("embedding_dispatch_crash")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env("VESTRACE_FAULT_POINT", "after_outcome_before_run_commit")
        .env("VESTRACE_RUNTIME_DATABASE_URL", &runtime_url)
        .output()
        .expect("the unprovable-point report must start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    let observation: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("the parent emits one JSON observation");
    assert_eq!(observation["point"], "after_outcome_before_run_commit");
    assert_eq!(observation["proved"], false);
    assert!(
        observation["reason"]
            .as_str()
            .expect("reason is text")
            .contains("no worker composes an embedding-job executor")
    );

    let _ = std::fs::remove_file(url_file);
}

#[sqlx::test(migrations = false)]
#[ignore = "needs PostgreSQL plus the vestrace runtime role; run with --ignored --nocapture"]
async fn result_preparation_commit_survives_a_real_child_abort(pool: PgPool) {
    provisioned_runtime(&pool).await.close().await;
    let database_url = ephemeral_database_url(&pool).await;
    let url_file = write_url_file(&database_url);
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let output = Command::new(scenario_binary())
        .arg("--database-url-file")
        .arg(&url_file)
        .arg("--scenario")
        .arg("embedding_result_preparation_crash")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env(
            "VESTRACE_FAULT_POINT",
            "after_result_prepared_before_return",
        )
        .env("VESTRACE_RUNTIME_DATABASE_URL", &runtime_url)
        .output()
        .expect("the result-preparation fault scenario must start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("RESULT_PREPARATION_FAULT_OUTPUT stdout={stdout} stderr={stderr}");
    assert!(
        output.status.success(),
        "the parent must prove its child-abort recovery: {stderr}"
    );
    let observation: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("the parent emits one JSON observation");
    assert_eq!(
        observation["scenario"],
        "embedding_result_preparation_crash"
    );
    assert_eq!(observation["point"], "after_result_prepared_before_return");
    assert_eq!(observation["proved"], true);
    assert_eq!(observation["loopback_requests"], 1);
    assert_eq!(observation["recovery_phase"], "result_prepared");
    assert_eq!(observation["persisted"]["marker"], 1);
    assert_eq!(observation["persisted"]["receipt"], 1);
    assert_eq!(observation["persisted"]["attachments"], 2);
    assert_eq!(observation["persisted"]["ciphertexts"], 2);
    assert_eq!(observation["persisted"]["result_finalizing_projections"], 2);
    assert_eq!(observation["persisted"]["source_dependencies"], 2);
    assert_eq!(observation["persisted"]["job_state"], "running");
    assert_eq!(observation["persisted"]["live_materials"], 0);
    assert_eq!(observation["persisted"]["ordinary_references"], 0);
    let _ = std::fs::remove_file(url_file);
}

fn scenario_binary() -> PathBuf {
    let test_binary = std::env::current_exe().expect("current test binary path");
    let name = format!("vestrace-fault-scenario{}", std::env::consts::EXE_SUFFIX);
    let target_debug = test_binary
        .parent()
        .and_then(Path::parent)
        .expect("integration test binary must be in target/debug/deps");
    let path = target_debug.join(&name);
    assert!(
        path.is_file(),
        "{name} was not found at {}; build it first with cargo build -p vestrace-fault-scenario",
        path.display()
    );
    let workspace = target_debug
        .parent()
        .and_then(Path::parent)
        .expect("target/debug must have the workspace root as its grandparent");
    let scenario_source = workspace
        .join("crates/vestrace-fault-scenario/src/scenarios/embedding_result_preparation_crash.rs");
    assert!(
        std::fs::metadata(&path)
            .and_then(|binary| binary.modified())
            .expect("fault scenario binary mtime")
            >= std::fs::metadata(&scenario_source)
                .and_then(|source| source.modified())
                .expect("fault scenario source mtime"),
        "{} is older than {}; rebuild the fault scenario before this e2e",
        path.display(),
        scenario_source.display()
    );
    path
}

async fn ephemeral_database_url(pool: &PgPool) -> String {
    let base = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .expect("the SQLx database names itself");
    let (base, query) = base
        .split_once('?')
        .map_or((base.as_str(), None), |(base, query)| (base, Some(query)));
    let (prefix, _) = base.rsplit_once('/').expect("DATABASE_URL has a database");
    query.map_or_else(
        || format!("{prefix}/{database}"),
        |query| format!("{prefix}/{database}?{query}"),
    )
}

fn write_url_file(database_url: &str) -> PathBuf {
    let database = database_url.rsplit('/').next().expect("database path");
    let path = std::env::temp_dir().join(format!(
        "vestrace-embedding-fault-{}.url",
        database.replace(['?', '&', '='], "_")
    ));
    std::fs::write(&path, database_url).expect("the database URL file is writable");
    path
}
