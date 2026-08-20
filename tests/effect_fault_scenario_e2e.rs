//! The external-effect fault suite, run against a real ephemeral deployment.
//!
//! Every other test of this machinery supplies its observations from a mock
//! that returns `FaultObservation::expected(point)`, which is an attestation
//! wearing the costume of an execution. This one drives
//! `ExternalEffectFaultSuiteService` through the real
//! `ConfiguredEffectFaultScenarioExecutor` and the real
//! `ProcessFaultInjectionRuntime`, which invokes `vestrace-fault-scenario`
//! once per point. That program spawns a child, drives the external-effect
//! lifecycle through the deployment's own services against this database,
//! calls `std::process::abort()` at the requested point, runs the worker's
//! recovery and outcome delivery, and reads back what survived.
//!
//! # Why this is `#[ignore]`d
//!
//! It needs Docker for a PostgreSQL the migrations can actually run against,
//! and it aborts five child processes. Neither belongs in a default
//! `cargo test`, and `docker-compose.yml` publishes no host port for Postgres,
//! so there is no checked-in database for it to find. It is run deliberately,
//! against a database provisioned for the purpose:
//!
//! ```text
//! cargo build -p vestrace-fault-scenario
//! DATABASE_URL=postgres://user:pass@127.0.0.1:PORT/vestrace \
//!   cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e \
//!   -- --ignored --nocapture
//! ```
//!
//! # Why the assertion is on the suite's own verdict
//!
//! The last line of the test is `assert!(decision.is_passed())`, and it fails
//! today. That is deliberate. A harness that printed the verdict and passed
//! regardless would report that the fault suite had been *run*, which is the
//! exact substitution the scenario exists to refuse — the release gate would
//! then be satisfied by a harness rather than by a system. The suite's verdict
//! is this test's verdict.
//!
//! The way to turn it green is to change the system it is observing. It is not
//! permitted to be turned green by changing the scenario program, the adapter
//! stub, or the fixtures: every field of every observation is read back from
//! persisted rows or counted by the party that was dispatched to, and tuning
//! any of them toward `FaultObservation::expected` would delete the only thing
//! this harness produces.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use vestrace_application::{
    ConfiguredEffectFaultScenarioExecutor, ExternalEffectFaultSuiteService,
    FaultInjectionEnvironment, FaultInjectionSettings, ProcessFaultInjectionRuntime,
};
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation};

/// Longer than any single point should need, and short enough that a wedged
/// child is a failed run rather than a hung one. The first invocation pays for
/// the whole migration set; the other four do not.
const POINT_TIMEOUT: Duration = Duration::from_secs(300);

/// Any non-blank string satisfies `FaultInjectionSettings`, and the scenario
/// program never reads it. It is spelled out rather than left implicit because
/// the qualification evidence this suite feeds names the target it qualified,
/// and a digest naming a real build would be a claim this harness cannot
/// support: nothing here verifies that the scenario binary was built from the
/// tree it is being run against.
const UNVERIFIED_TARGET_DIGEST: &str = "sha256:local-debug-build-not-digest-verified";

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs Docker for PostgreSQL and aborts five child processes; run with --ignored"]
async fn the_fault_suite_runs_against_an_ephemeral_deployment(pool: PgPool) {
    let scenario_binary = scenario_binary();
    let database_url = ephemeral_database_url(&pool).await;
    let url_file = write_url_file(&database_url);

    // `env_clear()` in `execute_fault_command` means the scenario program
    // cannot inherit a connection string, which is a property worth keeping: a
    // program whose job is to kill things mid-transaction must not silently
    // acquire the credentials of whatever ran it. The path travels in argv; the
    // URL does not.
    let settings = FaultInjectionSettings::new(
        true,
        UNVERIFIED_TARGET_DIGEST,
        FaultInjectionEnvironment::Ephemeral,
    )
    .expect("fault injection settings");
    let runtime = ProcessFaultInjectionRuntime::new(
        settings.clone(),
        scenario_binary.to_string_lossy().into_owned(),
        [
            "--database-url-file".to_owned(),
            url_file.to_string_lossy().into_owned(),
        ],
        POINT_TIMEOUT,
    )
    .expect("process fault runtime");
    let executor = ConfiguredEffectFaultScenarioExecutor::new(settings, Arc::new(runtime));
    let suite = ExternalEffectFaultSuiteService::new(Arc::new(executor));

    let report = suite.run().await;
    let _ = std::fs::remove_file(&url_file);
    let report = report.expect("the fault suite could not be run at all");

    // Printed before anything is asserted, so the record survives the failure.
    // The failing assertion below is the finding; these lines are its evidence.
    println!(
        "FAULT_SUITE_SOURCE binary={} points={}",
        scenario_binary.display(),
        EffectFaultPoint::required_points().len()
    );
    for observation in report.observations() {
        let expected = FaultObservation::expected(observation.point);
        println!(
            "FAULT_SUITE_OBSERVED point={} status={:?} receipt_persisted={} \
             reconciliation_started={} retry_attempted={} | expected status={:?} \
             receipt_persisted={} reconciliation_started={} retry_attempted={}",
            point_name(observation.point),
            observation.status,
            observation.receipt_persisted,
            observation.reconciliation_started,
            observation.retry_attempted,
            expected.status,
            expected.receipt_persisted,
            expected.reconciliation_started,
            expected.retry_attempted,
        );
    }
    let decision = report.decision();
    println!(
        "FAULT_SUITE_DECISION passed={} failures={}",
        decision.is_passed(),
        decision.failures().len()
    );
    for failure in decision.failures() {
        println!("FAULT_SUITE_FAILURE {failure}");
    }

    // Harness integrity, asserted separately from the verdict: a run that
    // silently observed four points, or answered one point with another point's
    // observation, would produce a verdict about something other than the five
    // required points, and that must be ruled out before the verdict is worth
    // reading at all.
    let required = EffectFaultPoint::required_points();
    assert_eq!(
        report.observations().len(),
        required.len(),
        "the suite must produce one observation per required point"
    );
    for (observation, point) in report.observations().iter().zip(required) {
        assert_eq!(
            observation.point, point,
            "observations must be in required-point order"
        );
    }

    assert!(
        decision.is_passed(),
        "the external effect fault suite does not pass against a real deployment: {:#?}",
        decision.failures()
    );
}

/// The scenario binary as cargo built it, beside this test binary.
///
/// It is a separate package with its own `[[bin]]`, so `CARGO_BIN_EXE_` does
/// not reach it and the path has to be derived. It is deliberately not built
/// from inside the test: a nested `cargo build` would contend for the same
/// target directory lock as the run that started this test.
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
    panic!(
        "{name} was not found beside {}; build it first with \
         `cargo build -p vestrace-fault-scenario`",
        test_binary.display()
    );
}

/// The URL of the database `#[sqlx::test]` provisioned for this run.
///
/// `sqlx::test` creates and later drops a database of its own from
/// `DATABASE_URL` and hands back a pool, not a URL — and the scenario program
/// needs a URL, because it is a separate process. The credentials come from
/// `DATABASE_URL` and the database name is asked of the connection itself
/// rather than reconstructed from the macro's naming scheme, which is a private
/// detail of sqlx.
async fn ephemeral_database_url(pool: &PgPool) -> String {
    let base = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set: this test needs a PostgreSQL to provision from");
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .expect("the provisioned database must name itself");

    let (base, query) = match base.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (base.as_str(), None),
    };
    let (prefix, _) = base
        .rsplit_once('/')
        .expect("DATABASE_URL must carry a database path");
    match query {
        Some(query) => format!("{prefix}/{database}?{query}"),
        None => format!("{prefix}/{database}"),
    }
}

/// Put the URL in a file and hand over the path.
///
/// A path in argv is not a credential; a URL in argv is, because argv is
/// world-readable on a normal host. The file is named after the ephemeral
/// database so two runs cannot collide, and it is removed as soon as the suite
/// has finished with it.
fn write_url_file(database_url: &str) -> PathBuf {
    let database = database_url
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or(database_url);
    let database = database.split_once('?').map_or(database, |(name, _)| name);
    let path = std::env::temp_dir().join(format!("vestrace-fault-{database}.url"));
    std::fs::write(&path, database_url).expect("the database url file must be writable");
    path
}

/// The names the scenario program prints and `fault_runtime` parses.
///
/// Spelled out rather than using `{:?}` so the printed record uses the wire
/// vocabulary, which is what a reader comparing this output against the
/// scenario's own report has in front of them.
fn point_name(point: EffectFaultPoint) -> &'static str {
    match point {
        EffectFaultPoint::AfterIntentPersistence => "after_intent_persistence",
        EffectFaultPoint::AfterAuthorizationBeforeDispatch => "after_authorization_before_dispatch",
        EffectFaultPoint::AfterDispatchBeforeReceipt => "after_dispatch_before_receipt",
        EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation => {
            "after_receipt_before_outcome_confirmation"
        }
        EffectFaultPoint::AfterOutcomeBeforeRunCommit => "after_outcome_before_run_commit",
    }
}
