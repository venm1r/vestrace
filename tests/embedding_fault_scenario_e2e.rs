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
    // Every scenario source, not one of them. A binary older than the scenario
    // under test would run the previous build and report an observation of code
    // that is no longer there -- and the guard naming a single file could not
    // see that for any of the others.
    let built = std::fs::metadata(&path)
        .and_then(|binary| binary.modified())
        .expect("fault scenario binary mtime");
    for source in [
        "embedding_result_preparation_crash.rs",
        "embedding_result_finalization_crash.rs",
        "embedding_worker_completion_crash.rs",
    ] {
        let scenario_source = workspace.join(format!(
            "crates/vestrace-fault-scenario/src/scenarios/{source}"
        ));
        assert!(
            built
                >= std::fs::metadata(&scenario_source)
                    .and_then(|source| source.modified())
                    .expect("fault scenario source mtime"),
            "{} is older than {}; rebuild the fault scenario before this e2e",
            path.display(),
            scenario_source.display()
        );
    }
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

#[sqlx::test(migrations = false)]
#[ignore = "needs PostgreSQL plus the vestrace runtime role; run with --ignored --nocapture"]
async fn result_finalization_survives_a_real_child_abort(pool: PgPool) {
    provisioned_runtime(&pool).await.close().await;
    let database_url = ephemeral_database_url(&pool).await;
    let url_file = write_url_file(&database_url);
    let runtime_url =
        std::env::var("VESTRACE_RUNTIME_DATABASE_URL").expect("runtime role is required");
    let output = Command::new(scenario_binary())
        .arg("--database-url-file")
        .arg(&url_file)
        .arg("--scenario")
        .arg("embedding_result_finalization_crash")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env("VESTRACE_FAULT_POINT", "finalization_checkpoint_matrix")
        .env("VESTRACE_RUNTIME_DATABASE_URL", runtime_url)
        .output()
        .expect("finalization scenario starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "finalization proof failed: {stderr}"
    );
    let observation: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("one JSON observation");
    assert_eq!(
        observation["scenario"],
        "embedding_result_finalization_crash"
    );
    assert_eq!(observation["proved"], true);
    assert_eq!(observation["loopback_requests"], 1);
    assert_eq!(observation["output_count"], 2);
    assert_eq!(observation["published_replay_without_vault"], true);
    let checkpoints = observation["checkpoints"].as_array().unwrap();
    assert_eq!(checkpoints.len(), 5);
    for (index, point) in [
        "host_bound_before_sql",
        "strict_receipt_subset",
        "all_bound_before_publish",
        "sql_before_commit",
        "after_commit",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(checkpoints[index]["point"], *point);
        assert_eq!(checkpoints[index]["aborted"], true);
        assert!(checkpoints[index]["pid"].as_u64().unwrap() > 0);
        assert_eq!(checkpoints[index]["sql_bindings"], [0, 1, 2, 2, 2][index]);
        assert_eq!(checkpoints[index]["host_bound"], [1, 1, 2, 2, 2][index]);
        assert_eq!(
            checkpoints[index]["publications"],
            if index == 4 { 1 } else { 0 }
        );
    }
    assert_eq!(checkpoints[3]["sql_rollback_exact"], true);
    let sql_legs = observation["sql_legs"]
        .as_array()
        .expect("individual SQL leg observations");
    let expected = [
        "publication_insert",
        "event_insert",
        "material_0",
        "attachment_0",
        "intent_0",
        "projection_0",
        "material_1",
        "attachment_1",
        "intent_1",
        "projection_1",
        "corpus_update",
        "generation_update",
        "ready_stale",
        "blocker_0",
        "blocker_1",
        "job_succeeded",
    ];
    assert_eq!(sql_legs.len(), expected.len());
    for (leg, stage) in sql_legs.iter().zip(expected) {
        assert_eq!(leg["stage"], stage);
        assert_eq!(leg["after_row_observed"], true);
        assert_eq!(leg["child_killed"], true);
        assert_eq!(leg["backend_ended"], true);
        assert_eq!(leg["rollback_exact"], true);
        assert!(leg["pid"].as_u64().unwrap() > 0);
        assert!(leg["backend_pid"].as_u64().unwrap() > 0);
    }
    let _ = std::fs::remove_file(url_file);
}

/// A crash and the takeover after it cost one provider call between them.
///
/// One job, two processes, and a listener outside both. The crash child claims
/// real work and stops existing: nothing arrives. Its lease is then aged the
/// way time would age it, and a successor claims *that same job* and carries it
/// past the dispatch: exactly one request arrives.
///
/// Both halves matter. The zero alone would be satisfied by a child that was
/// never wired to a provider, and the one alone says nothing about what a crash
/// costs. Together they answer the question that costs money: a worker dying
/// and a worker replacing it is one call, not two.
#[sqlx::test(migrations = false)]
#[ignore = "needs PostgreSQL plus the vestrace runtime role; run with --ignored --nocapture"]
async fn a_crash_and_its_takeover_cost_one_provider_call_between_them(pool: PgPool) {
    provisioned_runtime(&pool).await.close().await;
    let database_url = ephemeral_database_url(&pool).await;
    let url_file = write_url_file(&database_url);
    let runtime_url =
        std::env::var("VESTRACE_RUNTIME_DATABASE_URL").expect("runtime role is required");

    let output = Command::new(scenario_binary())
        .arg("--database-url-file")
        .arg(&url_file)
        .arg("--scenario")
        .arg("embedding_worker_completion_crash")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env("VESTRACE_FAULT_POINT", "after_work_claim")
        .env("VESTRACE_RUNTIME_DATABASE_URL", runtime_url)
        .output()
        .expect("the worker-completion scenario must start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("WORKER_COMPLETION_OUTPUT stdout={stdout} stderr={stderr}");
    assert!(
        output.status.success(),
        "the parent must prove its child-abort observation: {stderr}"
    );
    let observation: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("the parent emits one JSON observation");

    assert_eq!(observation["scenario"], "embedding_worker_completion_crash");
    assert_eq!(observation["point"], "after_work_claim");
    assert_eq!(observation["proved"], true);

    // Counted from outside, each after its child was gone.
    assert_eq!(
        observation["crash_requests"], 0,
        "a worker that died at its claim must never have reached the provider"
    );
    assert_eq!(
        observation["successor_requests"], 1,
        "the takeover must reach the provider, or the zero above proves nothing"
    );
    assert_eq!(
        observation["total_requests"], 1,
        "a crash and its takeover are one provider call between them"
    );
    assert_eq!(
        observation["successor_worked_the_crashed_job"], true,
        "one job's story, not two runs"
    );

    // What the crash left behind.
    let after_crash = &observation["after_crash"];
    assert_eq!(
        after_crash["claim_owner"], "fault-scenario-worker-that-dies",
        "the surviving lease must name the process that is gone"
    );
    assert_eq!(
        after_crash["lease_live"], true,
        "the lease outlives its holder; only time ends it"
    );
    assert_eq!(
        after_crash["last_outcome"],
        serde_json::Value::Null,
        "a process that stopped existing recorded no outcome"
    );
    assert_eq!(
        after_crash["job_state"], "requested",
        "claiming a job does not advance it"
    );
    assert_eq!(
        after_crash["dispatching_transitions"], 0,
        "nothing was dispatched"
    );
    assert_eq!(after_crash["receipts"], 0, "and nothing was received");

    // While that lease was live it kept a living worker out.
    assert_eq!(
        observation["probe_claimed_the_job"], false,
        "a live lease must exclude the next worker even when its holder is gone"
    );

    // And after the takeover: one dispatch, one receipt, the lease now the
    // successor's. Two dispatching transitions here would be the duplicate this
    // whole design exists to prevent.
    let after_takeover = &observation["after_takeover"];
    assert_eq!(
        after_takeover["claim_owner"], "fault-scenario-worker-that-takes-over",
        "the lease moved to the worker that finished the job"
    );
    assert_eq!(
        after_takeover["dispatching_transitions"], 1,
        "exactly one dispatch for the job, across both workers"
    );
    assert_eq!(
        after_takeover["receipts"], 1,
        "and exactly one receipt to go with it"
    );

    let _ = std::fs::remove_file(url_file);
}

/// The index publication compare-and-swap has two sides, and a crash lands on
/// exactly one of them.
///
/// `vestrace_publish_embedding_index_build` publishes the generation and marks
/// the attempt published in one call. Before it, the attempt is `building`
/// under a live claim and nothing is drawable -- a later worker must be able to
/// take the claim when it lapses. After it, the generation is Ready and the
/// attempt is spent, and stays that way although the process that spent it is
/// gone.
///
/// Both sides are run in the same test rather than in two, because the property
/// is that they *differ*: an observation that reported the same world on either
/// side of a compare-and-swap would be describing the fixture, not the swap.
#[sqlx::test(migrations = false)]
#[ignore = "needs PostgreSQL plus the vestrace runtime role; run with --ignored --nocapture"]
async fn an_index_build_crash_lands_on_one_side_of_its_publication(pool: PgPool) {
    provisioned_runtime(&pool).await.close().await;
    let database_url = ephemeral_database_url(&pool).await;
    let url_file = write_url_file(&database_url);
    let runtime_url =
        std::env::var("VESTRACE_RUNTIME_DATABASE_URL").expect("runtime role is required");

    let observe = |point: &'static str| {
        let url_file = url_file.clone();
        let runtime_url = runtime_url.clone();
        move || {
            let output = Command::new(scenario_binary())
                .arg("--database-url-file")
                .arg(&url_file)
                .arg("--scenario")
                .arg("embedding_worker_completion_crash")
                .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
                .env("VESTRACE_FAULT_POINT", point)
                .env("VESTRACE_RUNTIME_DATABASE_URL", &runtime_url)
                .output()
                .expect("the index-CAS scenario must start");
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            println!("INDEX_CAS_OUTPUT point={point} stdout={stdout} stderr={stderr}");
            assert!(
                output.status.success(),
                "the parent must prove its child-abort observation at {point}: {stderr}"
            );
            serde_json::from_str::<serde_json::Value>(stdout.trim())
                .expect("the parent emits one JSON observation")
        }
    };

    // Before the swap: claimed, building, nothing drawable.
    let before = observe("before_index_cas")();
    assert_eq!(before["point"], "before_index_cas");
    assert_eq!(before["proved"], true);
    assert_eq!(
        before["persisted"]["attempt_state"], "building",
        "the attempt the dead builder held is still its own"
    );
    assert_eq!(
        before["persisted"]["attempt_claim_live"], true,
        "and its claim outlives it, exactly as a work lease does"
    );
    assert_eq!(
        before["persisted"]["generation_state"], "building",
        "a generation whose index never published is not Ready"
    );
    assert_eq!(
        before["persisted"]["ready_generations"], 0,
        "so nothing in this workspace can be drawn from"
    );

    // After it: published and durable without its author.
    let after = observe("after_index_cas")();
    assert_eq!(after["point"], "after_index_cas");
    assert_eq!(after["proved"], true);
    assert_eq!(
        after["persisted"]["attempt_state"], "published",
        "the swap is durable although the process that made it is gone"
    );
    assert_eq!(after["persisted"]["generation_state"], "ready");
    assert_eq!(
        after["persisted"]["ready_generations"], 1,
        "exactly one generation is drawable, and it is the one that was built"
    );

    // Neither side reached a provider. An index is built from projections
    // already in the database; a crash in it must cost nothing, and a scenario
    // that did not count would not notice a build that started calling out.
    for (point, observation) in [("before", &before), ("after", &after)] {
        assert_eq!(
            observation["provider_requests"], 0,
            "an index build must reach no provider, {point} the swap"
        );
    }

    // And the two sides are different worlds, which is the whole claim.
    assert_ne!(
        before["persisted"]["attempt_state"],
        after["persisted"]["attempt_state"]
    );
    assert_ne!(
        before["persisted"]["generation_state"],
        after["persisted"]["generation_state"]
    );

    let _ = std::fs::remove_file(url_file);
}
