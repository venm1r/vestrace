//! A real process dies holding a real work claim, and the provider is never
//! reached.
//!
//! `crates/vestrace-infrastructure/tests/embedding_worker_restart.rs` proves
//! the claim lease in SQL: one job yields one claim, a lapsed lease is taken
//! over in place, a dead owner cannot retire its successor's work. What it
//! cannot prove is the thing the lease exists for. It makes no provider call,
//! so "a second worker never reaches the provider" is, in that suite, an
//! argument rather than an observation.
//!
//! This is the observation. Two children of the same build do the same real
//! setup against the same database and are given the same reachable provider
//! address:
//!
//! - the control claims the work and goes on to dispatch, and the listener
//!   counts one request;
//! - the crash child claims the work and stops existing, and the listener
//!   counts none.
//!
//! The count is taken by a socket in the parent, after the child is gone. It is
//! never a number a child reported. That is the whole point of the pairing: a
//! zero from a child that was never wired to a provider proves nothing, and the
//! control is what shows the wire was live.
//!
//! What survives is then read out of PostgreSQL rather than remembered: the
//! claim row is still there, still owned by a process that no longer exists,
//! still inside its lease -- which is exactly the state that keeps the next
//! worker out until the lease lapses, and exactly the state the restart suite
//! reasons from.

use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::RequestContext;
use vestrace_application::embedding::{EmbeddingWorkKind, EmbeddingWorkRepository};
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::postgres::{PgEmbeddingWorkRepository, PgStore};

use crate::{ScenarioSettings, embedding_dispatch_crash as dispatch};

const MARKER: &str = "vestrace-fault-scenario: embedding-worker-completion";
const POINT: &str = "after_work_claim";

/// The owner name the crash child claims under.
///
/// Fixed rather than random so the parent can assert *which* process holds the
/// surviving lease rather than merely that someone does. A random name would
/// make the assertion "a claim exists", which a control run also satisfies.
const CRASHED_OWNER: &str = "fault-scenario-worker-that-dies";
const CONTROL_OWNER: &str = "fault-scenario-worker-control";

/// Counts what arrives, from outside every process under test.
///
/// The dispatch scenario has one of these and keeps it private, and that file
/// is outside this package's change scope, so this is its own rather than a
/// widening of somebody else's. Both count the same thing the same way: a
/// connection accepted is a request that arrived.
struct LoopbackCounter {
    url: String,
    count: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl LoopbackCounter {
    async fn start() -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| format!("loopback listener bind failed: {error}"))?;
        let url = format!(
            "http://{}/embedding",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                observed.fetch_add(1, Ordering::SeqCst);
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        });
        Ok(Self { url, count, task })
    }

    fn url(&self) -> String {
        self.url.clone()
    }

    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl Drop for LoopbackCounter {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn die(error: String) -> ! {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{MARKER} setup-failure: {error}");
    let _ = stderr.flush();
    std::process::abort()
}

/// The one line the parent learns identities from.
///
/// Identities only. Every field of the observation is read back out of the
/// database or counted by the listener; nothing a child says about what
/// happened is believed.
fn announce(stage: &str, workspace: Uuid, job: Uuid) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(
        stderr,
        "{MARKER} stage={stage} workspace={workspace} job={job} pid={}",
        std::process::id()
    );
    let _ = stderr.flush();
}

fn identities(stderr: &str, stage: &str) -> Result<(Uuid, Uuid), String> {
    let needle = format!("{MARKER} stage={stage} ");
    let line = stderr
        .lines()
        .find(|line| line.contains(&needle))
        .ok_or_else(|| {
            format!(
                "the child never announced stage {stage}, so it stopped somewhere other than \
                 the boundary it was asked for and there is no observation to take; its \
                 stderr was: {}",
                stderr.trim()
            )
        })?;
    let field = |name: &str| -> Result<Uuid, String> {
        line.split_whitespace()
            .find_map(|token| token.strip_prefix(name))
            .ok_or_else(|| format!("the {stage} marker names no {name}: {line}"))?
            .parse()
            .map_err(|error| format!("the {stage} marker's {name} is not a uuid: {error}"))
    };
    Ok((field("workspace=")?, field("job=")?))
}

/// Build the world, claim the work, and either stop existing or go on.
///
/// The claim goes through `PgEmbeddingWorkRepository` against the real guarded
/// function under the restricted runtime role -- the same call
/// `EmbeddingWorkCycle` makes. A child that inserted a claim row directly would
/// be proving something about its own INSERT.
pub async fn run_child(settings: &ScenarioSettings) -> ! {
    settings.embedding_worker_completion_point();
    let owner = dispatch::connect_owner(settings)
        .await
        .unwrap_or_else(|error| die(error));
    let runtime = dispatch::connect_runtime(&owner)
        .await
        .unwrap_or_else(|error| die(error));
    let fixture = dispatch::build_fixture(&owner, &runtime)
        .await
        .unwrap_or_else(|error| die(error));

    let control = std::env::var_os("VESTRACE_EMBEDDING_CONTROL").is_some();
    let context = RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    );
    let work = PgEmbeddingWorkRepository::new(PgStore::from_pool(runtime.clone()));
    let claimed = work
        .claim(
            &context,
            EmbeddingWorkKind::Dispatch,
            if control {
                CONTROL_OWNER
            } else {
                CRASHED_OWNER
            },
            8,
        )
        .await
        .unwrap_or_else(|error| die(format!("the work claim was refused: {error:?}")));
    if !claimed
        .iter()
        .any(|claim| claim.job_id.as_uuid() == fixture.job_id)
    {
        die(format!(
            "the accepted job {} was not among the claimed work {claimed:?}",
            fixture.job_id
        ))
    }

    if control {
        // Announced before driving, because the drive ends in an abort of its
        // own and a marker written after it would never be flushed.
        announce("control", fixture.workspace_id, fixture.job_id);
        // The control carries on past the dispatch the crash child stops short
        // of, as far as the boundary just after the provider has answered. Its
        // whole job is to show that the address the crash child was given is
        // reachable, so that child's zero means "did not call" rather than
        // "could not have". `drive` reaches the provider only when it is given
        // a point past the dispatch commit -- with no point it returns early,
        // which is what makes the durable-row control of the dispatch scenario
        // silent on the wire.
        dispatch::drive(
            &runtime,
            &fixture,
            Some(vestrace_domain::external_effects::EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation),
        )
        .await
        .unwrap_or_else(|error| die(error));
        die("the control child must abort inside its drive, not return".to_owned())
    }

    // The claim has committed. Nothing has been dispatched. The process stops
    // here without unwinding, without flushing a transaction and without
    // releasing the lease -- which is what a worker losing power does, and the
    // only way to find out what that leaves behind.
    announce(POINT, fixture.workspace_id, fixture.job_id);
    std::process::abort()
}

/// Run the control, then the crash child, then read what is left.
pub async fn run_parent(settings: &ScenarioSettings) -> Result<String, String> {
    settings.embedding_worker_completion_point();
    let listener = LoopbackCounter::start().await?;

    // The control aborts too. It has to: the only path in this build that
    // reaches the provider is one that ends at a fault point, and a control
    // that exited cleanly would be a different code path from the one under
    // test.
    let control = spawn_child(settings, true, &listener.url()).await?;
    if control.status.success() {
        return Err(format!(
            "the control child must abort at the boundary past the provider call, not              exit: {}",
            text(&control.stderr)
        ));
    }
    let (control_workspace, control_job) = identities(&text(&control.stderr), "control")?;
    let after_control = listener.count();
    if after_control != 1 {
        return Err(format!(
            "the control child must reach the provider exactly once so the crash child's \
             zero means something; the listener counted {after_control}"
        ));
    }

    let crashed = spawn_child(settings, false, &listener.url()).await?;
    if crashed.status.success() {
        return Err("the worker-completion child exited instead of aborting".to_owned());
    }
    let (workspace, job) = identities(&text(&crashed.stderr), POINT)?;
    let crash_requests = listener.count() - after_control;

    let owner = dispatch::connect_owner(settings).await?;
    let survivors = read_survivors(&owner, workspace, job).await?;

    // The lease the dead process holds still excludes a living one. Asked
    // through the same guarded function a real worker would use, under the
    // restricted runtime role, after the holder is gone.
    let runtime = dispatch::connect_runtime(&owner).await?;
    let work = PgEmbeddingWorkRepository::new(PgStore::from_pool(runtime.clone()));
    let principal: Uuid = sqlx::query_scalar(
        "SELECT c.principal_id FROM connections c \
           JOIN model_binding_snapshots s \
             ON s.connection_id=c.id AND s.workspace_id=c.workspace_id \
           JOIN embedding_jobs j \
             ON j.model_binding_snapshot_id=s.id AND j.workspace_id=s.workspace_id \
          WHERE j.workspace_id=$1 AND j.id=$2",
    )
    .bind(workspace)
    .bind(job)
    .fetch_one(&owner)
    .await
    .map_err(dispatch::sql)?;
    let successor = work
        .claim(
            &RequestContext::new(
                WorkspaceId::from_uuid(workspace),
                PrincipalId::from_uuid(principal),
            ),
            EmbeddingWorkKind::Dispatch,
            "fault-scenario-worker-after-the-crash",
            8,
        )
        .await
        .map_err(|error| format!("the successor's claim must answer, not fail: {error:?}"))?;
    let successor_claimed_the_job = successor.iter().any(|claim| claim.job_id.as_uuid() == job);
    runtime.close().await;

    Ok(serde_json::json!({
        "scenario": "embedding_worker_completion_crash",
        "point": POINT,
        "proved": true,
        // Counted by a socket in this process, after the child was gone.
        "control_requests": after_control,
        "crash_requests": crash_requests,
        "control": {
            "workspace_distinct": control_workspace != workspace,
            "job_distinct": control_job != job,
        },
        "persisted": survivors,
        "successor_claimed_the_job": successor_claimed_the_job,
    })
    .to_string())
}

/// What the database still holds for the crashed attempt.
///
/// Read as the owner because these are the rows the runtime role is not
/// permitted to read freely, and because a reading taken through the same
/// restricted path the child used could be shaped by that path's own policy.
async fn read_survivors(
    owner: &PgPool,
    workspace: Uuid,
    job: Uuid,
) -> Result<serde_json::Value, String> {
    let claim: Option<(String, bool, Option<String>)> = sqlx::query_as(
        "SELECT claim_owner, claim_deadline>now(), last_outcome \
           FROM embedding_job_work_claims \
          WHERE workspace_id=$1 AND job_id=$2 AND work_kind='dispatch'",
    )
    .bind(workspace)
    .bind(job)
    .fetch_optional(owner)
    .await
    .map_err(dispatch::sql)?;
    let Some((claim_owner, lease_live, last_outcome)) = claim else {
        return Err(
            "the crashed worker's claim did not survive, so the lease that should exclude \
             the next worker is gone"
                .to_owned(),
        );
    };

    let job_state: String =
        sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(workspace)
            .bind(job)
            .fetch_one(owner)
            .await
            .map_err(dispatch::sql)?;
    // Column names taken from the writes in `embedding_dispatch_crash::drive`,
    // which is what puts these rows there: the lifecycle table keys on
    // `effect_id` and names its column `status`.
    let dispatching: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_effect_lifecycle_transitions t \
           JOIN embedding_jobs j ON j.external_effect_id=t.effect_id \
          WHERE j.workspace_id=$1 AND j.id=$2 AND t.status='dispatching'",
    )
    .bind(workspace)
    .bind(job)
    .fetch_one(owner)
    .await
    .map_err(dispatch::sql)?;
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_effect_receipts r \
           JOIN embedding_jobs j ON j.external_effect_id=r.effect_id \
          WHERE j.workspace_id=$1 AND j.id=$2",
    )
    .bind(workspace)
    .bind(job)
    .fetch_one(owner)
    .await
    .map_err(dispatch::sql)?;

    Ok(serde_json::json!({
        "claim_owner": claim_owner,
        "lease_live": lease_live,
        "last_outcome": last_outcome,
        "job_state": job_state,
        "dispatching_transitions": dispatching,
        "receipts": receipts,
    }))
}

/// Start this same binary as a child of itself.
///
/// The same build on both sides on purpose: a separate child binary could drift
/// from the parent's idea of the cycle and nothing would notice.
async fn spawn_child(
    settings: &ScenarioSettings,
    control: bool,
    dispatch_url: &str,
) -> Result<std::process::Output, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("this executable's own path is unreadable: {error}"))?;
    let url_file = crate::argument(std::env::args().skip(1), "--database-url-file")
        .ok_or_else(|| "database url file is required: pass --database-url-file".to_owned())?;
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .map_err(|_| "VESTRACE_RUNTIME_DATABASE_URL is required".to_owned())?;
    let _ = settings;
    let mut command = tokio::process::Command::new(&program);
    command
        .arg("--database-url-file")
        .arg(&url_file)
        .arg("--scenario")
        .arg("embedding_worker_completion_crash")
        .env_clear()
        .env("VESTRACE_FAULT_CHILD", "1")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env("VESTRACE_FAULT_POINT", POINT)
        .env("VESTRACE_RUNTIME_DATABASE_URL", runtime_url)
        .env("VESTRACE_EMBEDDING_DISPATCH_URL", dispatch_url);
    if control {
        command.env("VESTRACE_EMBEDDING_CONTROL", "1");
    }
    // Windows resolves DLLs through the inherited environment, so the cleared
    // environment keeps the few variables a process needs to start at all.
    for inherited in ["PATH", "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "HOME"] {
        if let Some(value) = std::env::var_os(inherited) {
            command.env(inherited, value);
        }
    }
    command
        .output()
        .await
        .map_err(|error| format!("the worker-completion child did not start: {error}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
