use std::process::{Command, Output};

const UNAVAILABLE_DATABASE_URL: &str =
    "postgres://p0-user:p0-secret-password@127.0.0.1:9/vestrace_p0_unavailable";

fn run(command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .arg(command)
        .env("VESTRACE_DATABASE__URL", UNAVAILABLE_DATABASE_URL)
        .output()
        .expect("vestrace command should start")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn worker_fails_explicitly_when_database_is_unavailable() {
    let output = run("worker");

    assert!(!output.status.success(), "worker unexpectedly succeeded");
    assert!(
        stderr(&output).contains("database is unavailable"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn mcp_fails_explicitly_when_database_is_unavailable() {
    let output = run("mcp");

    assert!(!output.status.success(), "mcp unexpectedly succeeded");
    assert!(
        stderr(&output).contains("database is unavailable"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn rebuild_fails_explicitly_when_database_is_unavailable() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .arg("rebuild")
        .arg("search-documents")
        .env("VESTRACE_DATABASE__URL", UNAVAILABLE_DATABASE_URL)
        .output()
        .expect("vestrace command should start");

    assert!(!output.status.success(), "rebuild unexpectedly succeeded");
    assert!(
        stderr(&output).contains("database is unavailable"),
        "unexpected stderr for rebuild: {}",
        stderr(&output)
    );
}

#[test]
fn migrate_fails_when_database_is_unavailable_and_redacts_secrets() {
    let output = run("migrate");
    let stderr = stderr(&output);

    assert!(!output.status.success(), "migrate unexpectedly succeeded");
    assert!(stderr.contains("database is unavailable"), "{stderr}");
    assert!(!stderr.contains("p0-secret-password"), "{stderr}");
    assert!(!stderr.contains(UNAVAILABLE_DATABASE_URL), "{stderr}");
}

#[test]
fn plan_is_a_read_only_operator_contract_without_database_access() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "plan",
            "--finding-id",
            "018f5b7e-3a2b-7c11-8a22-1234567890ab",
        ])
        .output()
        .expect("vestrace command should start");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout.contains("\"read_only\": true"), "{stdout}");
    assert!(stdout.contains("\"execution_started\": false"), "{stdout}");
}

#[test]
fn repair_contract_does_not_start_without_an_execution_adapter() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "repair",
            "--plan-id",
            "018f5b7e-3a2b-7c11-8a22-1234567890ab",
            "--current-state-ref",
            "state-v1",
        ])
        .output()
        .expect("vestrace command should start");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout.contains("\"read_only\": false"), "{stdout}");
    assert!(stdout.contains("\"execution_started\": false"), "{stdout}");
    assert!(stdout.contains("immutable_repair_plan"), "{stdout}");
}

const SERVER_SOURCE: &str = include_str!("../src/commands/server.rs");
const WORKER_SOURCE: &str = include_str!("../src/commands/worker.rs");

/// Startup recovery only closes the gap it exists for if a deployed process
/// actually runs it, so both long-running entry points must invoke it.
#[test]
fn server_and_worker_run_startup_recovery_before_serving_work() {
    for (name, source) in [("server", SERVER_SOURCE), ("worker", WORKER_SOURCE)] {
        assert!(
            source.contains("config.recovery.enabled"),
            "{name} does not gate on recovery configuration"
        );
        assert!(
            source.contains("config.workspaces"),
            "{name} does not scope startup recovery to configured workspaces"
        );
        assert!(
            source.contains("recovery::run_startup_recovery"),
            "{name} does not run startup recovery"
        );
    }
}

/// A sweep that recovers nothing because no workspace was named is the exact
/// silent failure this gate removes, so the process must refuse to start.
#[test]
fn enabled_recovery_without_workspaces_refuses_to_start() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .arg("worker")
        .env("VESTRACE_DATABASE__URL", UNAVAILABLE_DATABASE_URL)
        .env("VESTRACE_RECOVERY__ENABLED", "true")
        .output()
        .expect("vestrace command should start");

    assert!(!output.status.success(), "worker unexpectedly succeeded");
    assert!(
        stderr(&output).contains("workspaces"),
        "{}",
        stderr(&output)
    );
}

/// The run worker polls workspace-scoped queues, so it must poll the workspaces
/// it was configured to serve rather than a generated identifier.
#[test]
fn worker_polls_configured_workspaces_instead_of_a_generated_one() {
    assert!(
        WORKER_SOURCE.contains(
            "config
        .workspaces"
        ) || WORKER_SOURCE.contains("config.workspaces"),
        "worker does not read the configured workspaces"
    );
    assert!(
        !WORKER_SOURCE.contains("RequestContext::new(WorkspaceId::new()"),
        "worker still polls a randomly generated workspace"
    );
}

/// Deny-all must stay the default: a deployment has to opt in to the weaker
/// configured-capabilities engine explicitly.
#[test]
fn server_defaults_to_the_deny_all_policy_engine() {
    assert!(
        SERVER_SOURCE.contains("PolicyEngineKind::DenyAll => Ok(Arc::new(DenyAllPolicyEngine))"),
        "server no longer maps the default engine to deny-all"
    );
    assert!(
        SERVER_SOURCE.contains("build_policy_engine(&config.policy,"),
        "server does not build its policy engine from configuration"
    );
    // The grant-backed engine has to be reachable from configuration, or the
    // store exists and nothing consults it.
    assert!(
        SERVER_SOURCE.contains("PolicyEngineKind::CapabilityGrants"),
        "server cannot select the capability-grant engine"
    );
}

/// Running on static configured capabilities is not a governed authorization
/// path, so it must announce itself rather than look like normal operation.
#[test]
fn configured_capability_policy_warns_at_startup() {
    assert!(
        SERVER_SOURCE.contains("this is not a capability grant and cannot be revoked"),
        "configured policy engine does not warn about its limits"
    );
}

/// The outbox is only a delivery mechanism if a deployed process drains it.
///
/// Every part of this existed before — the port, the adapter, the backlog
/// invariant — and no binary ever called `claim_pending`, so messages
/// accumulated for the lifetime of the system while the doctor reported the
/// backlog and named no component able to clear it.
#[test]
fn worker_drains_the_outbox() {
    assert!(
        WORKER_SOURCE.contains("build_outbox_dispatcher"),
        "worker builds no outbox dispatcher"
    );
    assert!(
        WORKER_SOURCE.contains("drain_outbox(&outbox, &run_contexts)"),
        "worker does not drain the outbox in its poll loop"
    );
    // Both topics, or a revised memory keeps the meaning it had before the
    // revision while every retrieval still reports success.
    for topic in ["memory.created", "memory.revised"] {
        assert!(
            WORKER_SOURCE.contains(topic),
            "worker does not handle {topic}"
        );
    }
}

/// A stub that leased a job and marked it completed having executed nothing is
/// worse than no consumer at all: it converts undone work into a durable claim
/// that the work was done.
#[test]
fn no_job_worker_completes_work_it_did_not_do() {
    assert!(
        !WORKER_SOURCE.contains("job_worker.process_one()"),
        "worker still runs the stub job worker"
    );
}

/// The subscriber's filters must be attached to the registry, not to the
/// formatting layer.
///
/// A per-layer filter records its decision in a thread-local bitmap that is
/// only written when `Subscriber::enabled` runs — and `enabled` is skipped for
/// any callsite whose cached `Interest` is `always`. `Filtered::on_event` then
/// reads a bit nothing set and drops the event. That discarded every `warn!`
/// and `error!` emitted from the application, infrastructure and HTTP crates
/// inside the async runtime, while startup logging looked perfectly normal.
///
/// A unit test cannot guard this: installing a scoped subscriber disables
/// `Interest` caching, so the failure cannot occur in-process. The wiring is
/// therefore asserted directly.
#[test]
fn subscribers_filter_globally_rather_than_per_layer() {
    let per_layer_filter = SERVER_SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("///"))
        .find(|line| line.contains(".with_filter("));
    assert!(
        per_layer_filter.is_none(),
        "the subscriber attaches a per-layer filter again ({per_layer_filter:?}); log output \
         from every non-binary crate will be silently dropped inside the runtime"
    );
    assert!(
        SERVER_SOURCE.contains("output_filters"),
        "the subscriber no longer builds its global filters"
    );
}

const MEMORY_SERVICE_SOURCE: &str =
    include_str!("../../vestrace-application/src/memory/services.rs");

/// Every outbox topic the write path produces has a handler that consumes it.
///
/// # What this catches
///
/// An outbox message exists because something is waiting to act on it. Two
/// topics — `event.recorded` and `memory.relation.linked` — were written by the
/// memory service and consumed by nobody, so every event recorded since the
/// system began left a row that would never be delivered. The dispatcher counted
/// them as `unhandled` and left them pending, correctly, and the doctor reported
/// a backlog that no amount of draining could clear.
///
/// The producers are gone. This is what stops the next one: a topic added to the
/// write path without a handler fails here, at the point where somebody could
/// still decide whether the message is wanted.
#[test]
fn every_outbox_topic_produced_has_a_handler() {
    let produced: Vec<&str> = MEMORY_SERVICE_SOURCE
        .match_indices("OutboxMessage::new(")
        .filter_map(|(index, _)| {
            // The topic is the second argument, on its own line.
            MEMORY_SERVICE_SOURCE[index..].lines().nth(2).map(str::trim)
        })
        .filter_map(|line| line.strip_prefix('"'))
        .filter_map(|line| line.split('"').next())
        .collect();

    assert!(
        !produced.is_empty(),
        "no outbox topics were found, so this test is asserting nothing"
    );

    for topic in produced {
        assert!(
            WORKER_SOURCE.contains(topic),
            "the write path produces outbox topic `{topic}` and the worker registers no \
             handler for it, so every message on it would sit undelivered forever"
        );
    }
}
