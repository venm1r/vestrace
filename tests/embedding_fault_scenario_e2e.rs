//! Real-process fault evidence for the governed embedding dispatch path.
//!
//! This is deliberately ignored: it provisions PostgreSQL, spawns a parent
//! scenario and four aborting children, and relies on the runtime role.

use std::path::{Path, PathBuf};
use std::process::Command;

use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs PostgreSQL plus the vestrace runtime role; run with --ignored --nocapture"]
async fn embedding_dispatch_crashes_leave_only_the_accepted_baseline(pool: PgPool) {
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

fn scenario_binary() -> PathBuf {
    let test_binary = std::env::current_exe().expect("current test binary path");
    let name = format!("vestrace-fault-scenario{}", std::env::consts::EXE_SUFFIX);
    let mut directory: Option<&Path> = test_binary.parent();
    while let Some(candidate) = directory {
        let path = candidate.join(&name);
        if path.is_file() {
            return path;
        }
        directory = candidate.parent();
    }
    panic!("{name} was not found; build it first with cargo build -p vestrace-fault-scenario");
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
