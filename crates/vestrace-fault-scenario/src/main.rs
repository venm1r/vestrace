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
use std::time::Duration;

use secrecy::SecretString;
use vestrace_application::{
    EffectOutcomeDeliveryService, ExternalEffectReadBackAdapter, ExternalEffectReadBackRegistry,
    ExternalEffectRecoveryService, ExternalEffectRepository, RequestContext,
    SharedExternalEffectRepository, WORKER_PRESENCE_LAPSE_AFTER, deliver_effect_outcomes,
};
use vestrace_domain::external_effects::{
    EffectFaultPoint, ExternalEffectAdapter, FaultObservation,
};
use vestrace_domain::now;
use vestrace_domain::{ExternalEffectId, WorkerId};
use vestrace_fault_scenario::settings::Scenario;
use vestrace_fault_scenario::{AdapterStub, ScenarioSettings, child, observe, report};

use vestrace_infrastructure::{
    DatabaseConfig, HttpExternalEffectReadBackAdapter, HttpWebhookEffectAdapter,
    PgExternalEffectRepository, PgRunEventStore, PgRunRecoveryStore, PgStore,
};

#[path = "scenarios/credential_intent_crash.rs"]
mod credential_intent_crash;
#[path = "scenarios/embedding_dispatch_crash.rs"]
mod embedding_dispatch_crash;
#[path = "scenarios/material_intent_crash.rs"]
mod material_intent_crash;

/// Plumbing the two key-intent scenarios share.
///
/// Inline here rather than its own file because `scenarios/` holds exactly the
/// two scenario sources the plan names, and neither is a home for machinery the
/// other also needs.
mod intent_support {
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    use sqlx::PgPool;
    use uuid::Uuid;
    use vestrace_domain::external_effects::EffectFaultPoint;
    use vestrace_fault_scenario::ScenarioSettings;

    pub const MARKER_PREFIX: &str = "vestrace-fault-scenario: intent-crash ";
    pub const BOOTSTRAP_KEY_ID: &str = "material-vault-bootstrap";
    pub const BOOTSTRAP_SCOPE: &str = "material-vault-bootstrap";
    pub const BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

    /// Where the host vault lives for this invocation.
    ///
    /// Derived from the database-url file rather than passed as new arguments:
    /// both halves of the same run compute the same paths without widening the
    /// invocation contract, and the isolation that makes the database throwaway
    /// makes these directories throwaway with it. The vault must outlive the
    /// child — a key created before the crash is exactly what several boundaries
    /// are about — so it cannot be a temporary directory the child owns.
    pub fn vault_roots() -> Result<(PathBuf, PathBuf), String> {
        let url_file = crate::argument(std::env::args().skip(1), "--database-url-file")
            .ok_or_else(|| "database url file is required: pass --database-url-file".to_owned())?;
        let base = Path::new(&url_file);
        let stem = base
            .file_name()
            .ok_or_else(|| format!("database url file {url_file} has no file name"))?
            .to_string_lossy()
            .into_owned();
        let parent = base.parent().unwrap_or_else(|| Path::new("."));
        let vault_root = parent.join(format!("{stem}.vault"));
        let bootstrap_root = parent.join(format!("{stem}.bootstrap"));
        write_bootstrap_key(&bootstrap_root)?;
        std::fs::create_dir_all(&vault_root)
            .map_err(|error| format!("the scenario vault root is unusable: {error}"))?;
        Ok((vault_root, bootstrap_root))
    }

    /// The mounted bootstrap reference the vault resolves its envelope key from.
    ///
    /// Fixed bytes, because this is a throwaway ephemeral installation and a
    /// scenario that generated a fresh bootstrap key per process would hand the
    /// parent a vault it could not open after the child wrote into it.
    fn write_bootstrap_key(root: &Path) -> Result<(), String> {
        let key_root = root.join(BOOTSTRAP_KEY_ID);
        let version_root = key_root.join("v1");
        std::fs::create_dir_all(&version_root)
            .map_err(|error| format!("the scenario bootstrap mount is unusable: {error}"))?;
        for (path, bytes) in [
            (key_root.join("scope"), BOOTSTRAP_SCOPE.as_bytes().to_vec()),
            (key_root.join("purpose"), b"storage".to_vec()),
            (
                key_root.join("algorithm"),
                BOOTSTRAP_ALGORITHM.as_bytes().to_vec(),
            ),
            (version_root.join("state"), b"active".to_vec()),
            (version_root.join("private.pkcs8"), vec![0x5A; 32]),
        ] {
            std::fs::write(&path, bytes)
                .map_err(|error| format!("the scenario bootstrap key is unwritable: {error}"))?;
        }
        Ok(())
    }

    pub async fn tenancy(
        pool: &PgPool,
        workspace_id: Uuid,
        principal_id: Uuid,
        label: &str,
    ) -> Result<(), String> {
        sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
            .bind(workspace_id)
            .bind(format!("{label}-{workspace_id}"))
            .execute(pool)
            .await
            .map_err(|error| format!("the scenario workspace could not be created: {error}"))?;
        sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
            .bind(principal_id)
            .bind(workspace_id)
            .bind(format!("{label}-principal-{principal_id}"))
            .execute(pool)
            .await
            .map_err(|error| format!("the scenario principal could not be created: {error}"))?;
        Ok(())
    }

    /// One guarded transition in its own committed transaction, which is why a
    /// crash leaves exactly the state after the last completed step.
    pub async fn guarded(
        pool: &PgPool,
        workspace_id: Uuid,
        principal_id: Uuid,
        query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> Result<(), String> {
        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| format!("the scenario transaction could not begin: {error}"))?;
        for (setting, value) in [
            ("vestrace.workspace_id", workspace_id.to_string()),
            ("vestrace.principal_id", principal_id.to_string()),
        ] {
            sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
                .bind(setting)
                .bind(value)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| format!("the scenario request context is unusable: {error}"))?;
        }
        query
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("a guarded transition was refused: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("the scenario transaction could not commit: {error}"))
    }

    /// Run this same executable as the child at the requested boundary.
    ///
    /// The exit code is not asserted, for the reason the external-effect
    /// scenario already documents: what `abort()` becomes at the process
    /// boundary is platform-shaped. What is asserted is portable — the child did
    /// not finish, and it announced the boundary it was asked about. A child
    /// that exited cleanly never crashed, and a child that died before its
    /// boundary describes where it broke rather than where it was told to.
    pub async fn run_the_child(
        settings: &ScenarioSettings,
        point: EffectFaultPoint,
    ) -> Result<String, String> {
        let program = std::env::current_exe()
            .map_err(|error| format!("this executable's own path is unreadable: {error}"))?;
        let url_file = crate::argument(std::env::args().skip(1), "--database-url-file")
            .ok_or_else(|| "database url file is required: pass --database-url-file".to_owned())?;
        let scenario = match settings.scenario() {
            vestrace_fault_scenario::settings::Scenario::MaterialIntent => "material_intent_crash",
            vestrace_fault_scenario::settings::Scenario::CredentialIntent => {
                "credential_intent_crash"
            }
            vestrace_fault_scenario::settings::Scenario::ExternalEffect => {
                return Err("the intent child runs only for an intent scenario".to_owned());
            }
            vestrace_fault_scenario::settings::Scenario::EmbeddingDispatch => {
                return Err("the intent child runs only for an intent scenario".to_owned());
            }
        };
        let output = tokio::process::Command::new(&program)
            .arg("--database-url-file")
            .arg(&url_file)
            .arg("--scenario")
            .arg(scenario)
            .env("VESTRACE_FAULT_CHILD", "1")
            // Restated from the parsed settings rather than inherited, so the
            // child is driven by the boundary this process actually accepted.
            .env("VESTRACE_FAULT_POINT", point.as_str())
            .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
            .stdin(Stdio::null())
            // Piped rather than inherited: the child must not be able to put
            // anything on this process's stdout, which carries exactly one
            // object.
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|error| format!("the scenario child could not be started: {error}"))?;

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.status.success() {
            return Err(format!(
                "the scenario child exited successfully, so nothing crashed and there is no \
                 fault to observe; its stderr was: {}",
                stderr.trim()
            ));
        }
        Ok(stderr)
    }

    /// Recover the crashed child's intent identity, and the tenancy it wrote
    /// under.
    ///
    /// The identity comes off the child's marker line; the workspace and
    /// principal come out of the database rows that identity points at. The
    /// parent supplies none of the three.
    pub async fn identify(
        pool: &PgPool,
        stderr: &str,
        lifecycle: &str,
    ) -> Result<(Uuid, Uuid, Uuid), String> {
        let expected = format!("{MARKER_PREFIX}{lifecycle} intent=");
        let line = stderr
            .lines()
            .find(|line| line.starts_with(&expected))
            .ok_or_else(|| {
                format!(
                    "the scenario child announced no {lifecycle} intent, so it stopped before \
                     its boundary; its stderr was: {}",
                    stderr.trim()
                )
            })?;
        let intent_id: Uuid = line
            .trim_start_matches(&expected)
            .split_whitespace()
            .next()
            .ok_or_else(|| "the scenario child's marker has no intent id".to_owned())?
            .parse()
            .map_err(|error| format!("the scenario child's intent id is unreadable: {error}"))?;

        let table = table_for(lifecycle)?;
        let workspace_id: Uuid =
            sqlx::query_scalar(&format!("SELECT workspace_id FROM {table} WHERE id = $1"))
                .bind(intent_id)
                .fetch_optional(pool)
                .await
                .map_err(|error| format!("the crashed intent could not be read back: {error}"))?
                .ok_or_else(|| {
                    "the scenario child announced an intent it never persisted".to_owned()
                })?;
        let principal_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM principals WHERE workspace_id = $1 ORDER BY id LIMIT 1",
        )
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("the scenario principal could not be read back: {error}"))?
        .ok_or_else(|| "the scenario child persisted no principal".to_owned())?;
        Ok((intent_id, workspace_id, principal_id))
    }

    fn table_for(lifecycle: &str) -> Result<&'static str, String> {
        match lifecycle {
            "material" => Ok("material_key_creation_intents"),
            "credential" => Ok("credential_key_creation_intents"),
            other => Err(format!("unknown intent lifecycle '{other}'")),
        }
    }

    pub async fn state_of(pool: &PgPool, table: &str, intent_id: Uuid) -> Result<String, String> {
        sqlx::query_scalar(&format!("SELECT state FROM {table} WHERE id = $1"))
            .bind(intent_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("the intent state could not be read: {error}"))?
            .ok_or_else(|| "the intent row is absent".to_owned())
    }

    /// How many intents share this one's reserved key.
    ///
    /// A resumption that minted a second identity would show up here as two.
    pub async fn identities_for_key(
        pool: &PgPool,
        table: &str,
        intent_id: Uuid,
    ) -> Result<i64, String> {
        sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE material_key_id = \
             (SELECT material_key_id FROM {table} WHERE id = $1)"
        ))
        .bind(intent_id)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("the intent identities could not be counted: {error}"))
    }
}

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

    // The key-intent lifecycles are their own scenarios, dispatched before any
    // external-effect code runs so that `settings.point()`, which is defined
    // only for the external-effect vocabulary, is never consulted for them.
    match settings.scenario() {
        Scenario::ExternalEffect => {}
        Scenario::MaterialIntent => {
            if settings.is_child() {
                material_intent_crash::run_child(&settings).await;
            }
            finish(material_intent_crash::run_parent(&settings).await);
        }
        Scenario::CredentialIntent => {
            if settings.is_child() {
                credential_intent_crash::run_child(&settings).await;
            }
            finish(credential_intent_crash::run_parent(&settings).await);
        }
        Scenario::EmbeddingDispatch => {
            if settings.is_child() {
                embedding_dispatch_crash::run_child(&settings).await;
            }
            finish(embedding_dispatch_crash::run_parent(&settings).await);
        }
    }

    if settings.is_child() {
        let dispatch_url = match child::dispatch_url_argument(std::env::args().skip(1)) {
            Ok(url) => url,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        };
        let dispatch_owner = match argument(std::env::args().skip(1), "--worker-id")
            .ok_or_else(|| "worker id is required: pass --worker-id".to_owned())
            .and_then(|value| {
                value
                    .parse::<WorkerId>()
                    .map_err(|error| format!("worker id is invalid: {error}"))
            }) {
            Ok(worker_id) => worker_id,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        };
        // Diverges: the child always ends in `abort()`.
        child::run_child(&settings, &dispatch_url, dispatch_owner).await;
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

/// The single object an intent scenario writes to stdout, or nothing at all.
///
/// Same contract as the external-effect path: a partial or absent observation
/// is a failed invocation rather than a report a reader would accept.
fn finish(rendered: Result<String, String>) -> ! {
    match rendered {
        Ok(observation) => {
            println!("{observation}");
            std::process::exit(0)
        }
        Err(error) => {
            eprintln!("vestrace-fault-scenario: {error}");
            std::process::exit(1)
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
    let dispatch_owner = WorkerId::new();
    let started_at = now();
    PgExternalEffectRepository::new(store.clone())
        .record_worker_presence(&context, dispatch_owner, started_at, started_at)
        .await
        .map_err(|error| format!("the scenario worker presence could not be recorded: {error}"))?;

    let effect_id = run_the_child(settings, stub, dispatch_owner).await?;
    await_dead_owner_evidence(settings.point()).await?;

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
/// defect. What is asserted instead is portable and is the thing that actually
/// matters: the child did not finish, *and* it got as far as the point it was
/// asked about. A child that exited successfully completed its lifecycle
/// without crashing, and an observation taken from it would describe a run that
/// never faulted. A child that aborted before its point describes where it
/// broke rather than where it was told to crash, which is worse: it wears a
/// system finding's clothes. Both are refused rather than reported.
async fn run_the_child(
    settings: &ScenarioSettings,
    stub: &AdapterStub,
    dispatch_owner: WorkerId,
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
        .arg("--worker-id")
        .arg(dispatch_owner.to_string())
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

    // A crash is necessary but not sufficient. A child that failed to connect,
    // failed to authorize or failed to dispatch also aborts, also announces an
    // effect id, and leaves a database the parent would happily read — filing
    // "it never got there" under the fault point it never got to. The child says
    // which stage it finished and this refuses anything else, so a run that
    // stopped short is exit 1 with nothing on stdout rather than an observation.
    child::confirm_reached_point(&stderr, settings.point())?;

    effect_id_from(&stderr)
}

/// Wait for the durable presence owned by the crashed child to become lapsed.
///
/// Only point 3 leaves an in-flight dispatch without a receipt. The other four
/// points have no need to pay the liveness window before recovery reads them.
/// The extra second crosses the repository's strict `last_reported_at < cutoff`
/// boundary without inventing a second worker-liveness cadence.
async fn await_dead_owner_evidence(point: EffectFaultPoint) -> Result<(), String> {
    if point != EffectFaultPoint::AfterDispatchBeforeReceipt {
        return Ok(());
    }

    let lapse = WORKER_PRESENCE_LAPSE_AFTER
        .to_std()
        .map_err(|error| format!("the worker presence lapse window is invalid: {error}"))?;
    tokio::time::sleep(lapse + Duration::from_secs(1)).await;
    Ok(())
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
    let dispatch = HttpWebhookEffectAdapter::new(
        "fault-scenario-webhook",
        stub.dispatch_url(),
        stub.read_back_url(),
    )
    .map_err(|error| format!("effect adapter is not configurable: {error}"))?;
    let read_backs = ExternalEffectReadBackRegistry::new([(
        "fault-scenario-webhook".to_owned(),
        dispatch.descriptor().clone(),
        Arc::new(read_back) as Arc<dyn ExternalEffectReadBackAdapter>,
    )])
    .map_err(|error| format!("effect read-back is not configurable: {error}"))?;
    let recovery = ExternalEffectRecoveryService::new(repository, read_backs);
    recovery
        .sweep(context, now())
        .await
        .map_err(|error| format!("the reconciliation sweep failed: {error}"))?;
    Ok(())
}

/// Outcome delivery, built the way the worker builds it.
///
/// A settled outcome that nothing delivered stays owed, and the report says so
/// by leaving the debt where it is.
///
/// # Why the counts are printed
///
/// Point 5's `reconciliation_started: true` and `receipt_persisted: true` are
/// its only two agreements with the suite, and they exist *because delivery
/// deferred*: the child's `execution_ref` names a run with no event stream, so
/// the debt stays owed, `notified_at` stays NULL, and `observe`'s debt query can
/// still see the reconciliation. A pass that delivered instead would pay the
/// debt before the parent looked and flip that field to `false` — the same
/// observation, a different world. This report is the only artefact that says
/// which of the two happened, so it goes to stderr on every run rather than
/// living in a document's prose.
///
/// It goes to **stderr**, and no field of the observation is derived from it.
/// What the parent counted is the parent's knowledge; the observation is still
/// read out of the database afterwards.
async fn deliver_outcomes(store: &PgStore, context: &RequestContext) -> Result<(), String> {
    let delivery = Arc::new(EffectOutcomeDeliveryService::new(
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunRecoveryStore::new(store.clone())),
    ));
    let report = deliver_effect_outcomes(&delivery, context)
        .await
        .map_err(|error| format!("the settled outcomes could not be delivered: {error}"))?;
    eprintln!(
        "vestrace-fault-scenario: outcome delivery: delivered={} deferred={} unattributable={}",
        report.delivered, report.deferred, report.unattributable
    );
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
