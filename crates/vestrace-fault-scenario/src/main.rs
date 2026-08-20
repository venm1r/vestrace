//! One invocation: crash the external-effect lifecycle at one point, then say
//! what is still there.
//!
//! The process is its own child. `VESTRACE_FAULT_CHILD` decides which half runs,
//! so the code that dies and the code that reads back what survived are the same
//! build of the same binary — a separate child binary could drift from the
//! parent's idea of the lifecycle and nothing would notice.
//!
//! # What the parent is not allowed to do
//!
//! Every field of the printed observation comes from `observe`, which reads
//! persisted rows, or from the stub, which counted what reached it. The parent
//! knows a great deal by this point — which point it asked for, that the child
//! aborted, that it ran recovery — and none of that may become a field. The
//! parent's beliefs are exactly the thing this scenario exists to distrust.

use std::process::Stdio;
use std::sync::Arc;

use secrecy::SecretString;
use vestrace_application::{
    EffectOutcomeDeliveryService, ExternalEffectRecoveryService, RequestContext,
    SharedExternalEffectRepository, deliver_effect_outcomes,
};
use vestrace_domain::ExternalEffectId;
use vestrace_domain::external_effects::FaultObservation;
use vestrace_domain::now;
use vestrace_fault_scenario::{AdapterStub, ScenarioSettings, child, observe, report};
use vestrace_infrastructure::{
    DatabaseConfig, HttpExternalEffectReadBackAdapter, PgExternalEffectRepository, PgRunEventStore,
    PgRunRecoveryStore, PgStore,
};

#[tokio::main]
async fn main() {
    let settings = match ScenarioSettings::from_env_and_args(std::env::args().skip(1), &|name| {
        std::env::var(name).ok()
    }) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    if settings.is_child() {
        let dispatch_url = match child::dispatch_url_argument(std::env::args().skip(1)) {
            Ok(url) => url,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        };
        // Diverges: the child always ends in `abort()`.
        child::run_child(&settings, &dispatch_url).await;
    }

    match run_parent(&settings).await {
        // The only thing this process ever writes to stdout, and it writes it
        // once. `ProcessFaultObservation` is handed the whole of stdout.
        Ok(observation) => println!("{}", report::render(&observation)),
        Err(error) => {
            // Nothing on stdout: a partial or absent observation must be a
            // failed invocation rather than a report the reader would accept.
            eprintln!("vestrace-fault-scenario: {error}");
            std::process::exit(1);
        }
    }
}

/// Start the stub, drive the child, recover, and read the result back.
///
/// The stub is shut down on both paths. Leaving its accept task running would
/// hold the port for the rest of the process, which matters because the
/// invoking runtime may run all five points back to back.
async fn run_parent(settings: &ScenarioSettings) -> Result<FaultObservation, String> {
    let stub = AdapterStub::start().await?;
    let observation = observe_one_fault(settings, &stub).await;
    stub.shutdown().await;
    observation
}

async fn observe_one_fault(
    settings: &ScenarioSettings,
    stub: &AdapterStub,
) -> Result<FaultObservation, String> {
    let store = prepare_fixture(settings).await?;
    let context = child::scenario_context();

    let effect_id = run_the_child(settings, stub).await?;

    // The deployment's own recovery, not a rehearsal of it: the same
    // `ExternalEffectRecoveryService` over the same `PgExternalEffectRepository`
    // and the same `HttpExternalEffectReadBackAdapter` the worker builds at
    // `crates/vestrace-cli/src/commands/worker.rs:369`. If reconciliation is
    // going to move an effect that a crash left unresolved, this is what moves
    // it, and a hand-written approximation here would be observing the
    // approximation.
    reconcile(&store, &context, stub).await?;
    // Recovery settles the outcome; delivery is what tells the run. They are
    // separate passes in the worker for a reason — the debt outlives a failed
    // delivery — and running only the first would leave the lifecycle half
    // finished at exactly the place the last fault point asks about.
    deliver_outcomes(&store, &context).await?;

    // `dispatch_count` is read here, after everything that could dispatch has
    // finished, and it is the stub's number rather than the parent's.
    observe(
        &store,
        &context,
        effect_id,
        settings.point(),
        stub.dispatch_count(),
    )
    .await
}

/// The database the child will write into and the parent will read out of.
///
/// "Clean" is a property of the isolation rather than of a delete this function
/// performs: `ScenarioSettings` refuses anything but an ephemeral isolation, so
/// the database is a throwaway one. What this does is make sure the schema is
/// there before either half needs it — the child migrates too, but the parent
/// must be able to read even from a child that died before its own migration
/// finished, and reporting "relation does not exist" as an observation failure
/// would hide the fault the run was about.
async fn prepare_fixture(settings: &ScenarioSettings) -> Result<PgStore, String> {
    let store = PgStore::connect(&DatabaseConfig {
        url: SecretString::from(settings.database_url().to_owned()),
        max_connections: 4,
    })
    .await
    .map_err(|error| format!("the scenario database is unreachable: {error}"))?;
    store
        .migrate()
        .await
        .map_err(|error| format!("the scenario database could not be migrated: {error}"))?;
    Ok(store)
}

/// Run this same executable as the child, and return the effect it announced.
///
/// # Why the id comes back from the child
///
/// An effect id is generated inside `ExternalEffectIntent::new` and cannot be
/// chosen by a caller, so the parent cannot hand the child a fixture id; it
/// learns the id from the one marker line the child writes to stderr before
/// anything can fail. That line is an identifier, not an observation — every
/// field of the report is still read back out of the database or counted by the
/// stub.
///
/// # Why the exit code is not asserted
///
/// The child ends in `abort()`, and what `abort()` becomes at the process
/// boundary is platform-shaped: a signal on Unix, a large status on Windows,
/// and different again under a runtime that reports the signal its own way.
/// Pinning a number would make this scenario fail on a host rather than on a
/// defect. What must hold is weaker and is the thing that actually matters: the
/// child did not finish. A child that exited successfully completed its
/// lifecycle without crashing, and an observation taken from it would describe a
/// run that never faulted — so that is refused rather than reported.
async fn run_the_child(
    settings: &ScenarioSettings,
    stub: &AdapterStub,
) -> Result<ExternalEffectId, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("this executable's own path is unreadable: {error}"))?;
    let url_file = argument(std::env::args().skip(1), "--database-url-file")
        .ok_or_else(|| "database url file is required: pass --database-url-file".to_owned())?;

    let output = tokio::process::Command::new(&program)
        .arg("--database-url-file")
        .arg(&url_file)
        // A loopback address is not a credential, so unlike the database URL it
        // may travel in argv where any process on the host can read it.
        .arg("--dispatch-url")
        .arg(stub.dispatch_url())
        .env("VESTRACE_FAULT_CHILD", "1")
        // Restated from the parsed settings rather than left to inheritance, so
        // the child is driven by the point this process actually accepted. The
        // isolation is a literal for the same reason it is safe to be one:
        // `ScenarioSettings` refused to construct at all unless it was already
        // this value.
        .env("VESTRACE_FAULT_POINT", report::point_name(settings.point()))
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .stdin(Stdio::null())
        // Piped rather than inherited: the child must not be able to put
        // anything on this process's stdout, which carries exactly one object.
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| format!("the scenario child could not be started: {error}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        return Err(format!(
            "the scenario child exited successfully, so nothing crashed and there is no fault \
             to observe; its stderr was: {}",
            stderr.trim()
        ));
    }

    effect_id_from(&stderr)
}

/// The effect the child announced, from the one line it prints for the purpose.
///
/// The last such line wins, because a child that somehow announced twice has
/// only its most recent intent in hand.
fn effect_id_from(stderr: &str) -> Result<ExternalEffectId, String> {
    let announced = stderr
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(child::EFFECT_ID_MARKER))
        .ok_or_else(|| {
            format!(
                "the scenario child never announced an effect id, so it died before reaching \
                 its lifecycle at all and there is nothing to observe; its stderr was: {}",
                stderr.trim()
            )
        })?;
    announced.trim().parse().map_err(|error| {
        format!("the scenario child announced an unreadable effect id {announced:?}: {error}")
    })
}

/// The reconciliation sweep, built the way the worker builds it.
async fn reconcile(
    store: &PgStore,
    context: &RequestContext,
    stub: &AdapterStub,
) -> Result<(), String> {
    let repository: SharedExternalEffectRepository =
        Arc::new(PgExternalEffectRepository::new(store.clone()));
    let read_back = HttpExternalEffectReadBackAdapter::new(stub.read_back_url())
        .map_err(|error| format!("effect read-back is not configurable: {error}"))?;
    let recovery = ExternalEffectRecoveryService::new(repository, Arc::new(read_back));
    recovery
        .sweep(context, now())
        .await
        .map_err(|error| format!("the reconciliation sweep failed: {error}"))?;
    Ok(())
}

/// Outcome delivery, built the way the worker builds it.
///
/// A settled outcome that nothing delivered stays owed, and the report says so
/// by leaving the debt where it is. Nothing here inspects the report: what it
/// counted is the parent's knowledge, and the observation is read out of the
/// database afterwards.
async fn deliver_outcomes(store: &PgStore, context: &RequestContext) -> Result<(), String> {
    let delivery = Arc::new(EffectOutcomeDeliveryService::new(
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunRecoveryStore::new(store.clone())),
    ));
    deliver_effect_outcomes(&delivery, context)
        .await
        .map_err(|error| format!("the settled outcomes could not be delivered: {error}"))?;
    Ok(())
}

/// The value following a named flag, or `None`.
///
/// The last occurrence wins, matching how `ScenarioSettings` and
/// `dispatch_url_argument` read their own flags — three readers of the same argv
/// disagreeing about which occurrence counts would be worse than any of the
/// three choices.
fn argument(args: impl Iterator<Item = String>, name: &str) -> Option<String> {
    let mut args = args;
    let mut value = None;
    while let Some(arg) = args.next() {
        if arg == name {
            value = args.next();
        }
    }
    value
}
