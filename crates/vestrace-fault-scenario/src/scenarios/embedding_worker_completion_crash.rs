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
//! This is the observation, and it is one job's whole story rather than two
//! unrelated runs. Both children are the same build, given the same reachable
//! provider address, and the second works the job the first died on:
//!
//! - the crash child claims the work and stops existing; the listener counts
//!   nothing;
//! - the lease is then aged the way time would age it;
//! - the successor child claims *that same job* and carries it past the
//!   dispatch; the listener counts one.
//!
//! So the question the pairing answers is the one that costs money: a worker
//! crashing and a worker taking over after it is **one** provider call between
//! them, not two. The successor is also what shows the wire was live, on the
//! very job whose crash counted zero -- a separate control against a separate
//! world would have left that inference to the reader.
//!
//! Every count is taken by a socket in the parent, after each child is gone.
//! None is a number a child reported.
//!
//! What survives is read out of PostgreSQL rather than remembered: after the
//! crash the claim row is still there, owned by a process that no longer
//! exists, still inside its lease -- which is what keeps the next worker out
//! until the lease lapses, asserted here by asking for it and being refused.

use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::RequestContext;
use vestrace_application::embedding::index::EmbeddingIndexRepository;
use vestrace_application::embedding::{EmbeddingWorkKind, EmbeddingWorkRepository};
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::postgres::{
    PgEmbeddingIndexRepository, PgEmbeddingWorkRepository, PgStore,
};

use crate::{ScenarioSettings, embedding_dispatch_crash as dispatch};

const MARKER: &str = "vestrace-fault-scenario: embedding-worker-completion";
const POINT: &str = "after_work_claim";
const BEFORE_CAS: &str = "before_index_cas";
const AFTER_CAS: &str = "after_index_cas";

/// The owner an index build is claimed under.
const INDEX_OWNER: &str = "fault-scenario-index-builder";

/// How many members one chunk asks for. Large enough that these fixtures drain
/// in one pass and small enough that a fixture which grew would loop rather
/// than silently build a partial index.
const CHUNK: u32 = 64;

/// The owner name the crash child claims under.
///
/// Fixed rather than random so the parent can assert *which* process holds the
/// surviving lease rather than merely that someone does. A random name would
/// make the assertion "a claim exists", which a control run also satisfies.
const CRASHED_OWNER: &str = "fault-scenario-worker-that-dies";
const SUCCESSOR_OWNER: &str = "fault-scenario-worker-that-takes-over";

/// The owner the parent claims under to prove the dead worker's lease still
/// excludes a living one. Distinct from the successor's, because that attempt
/// must be refused and this one must not be confused with it.
const PROBE_OWNER: &str = "fault-scenario-worker-probing-the-lease";

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
    // A prefix, not a whole field: the post-CAS marker appends the outcome the
    // compare-and-swap returned, and that outcome is part of what the run is
    // about rather than noise to be stripped before matching.
    let needle = format!("{MARKER} stage={stage}");
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

    // The successor works a job it did not create. Building its own would make
    // the two children two experiments, and the count that matters -- one call
    // for a crash *and* its takeover -- would be the sum of two unrelated runs.
    let successor = std::env::var_os("VESTRACE_WORKER_SUCCESSOR").is_some();
    let fixture = if successor {
        adopt_fixture(
            &owner,
            adopted_identity("WORKSPACE"),
            adopted_identity("JOB"),
        )
        .await
        .unwrap_or_else(|error| die(error))
    } else {
        dispatch::build_fixture(&owner, &runtime)
            .await
            .unwrap_or_else(|error| die(error))
    };

    let context = RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    );
    let work = PgEmbeddingWorkRepository::new(PgStore::from_pool(runtime.clone()));
    let claimed = work
        .claim(
            &context,
            EmbeddingWorkKind::Dispatch,
            if successor {
                SUCCESSOR_OWNER
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
            "the job {} was not among the claimed work {claimed:?}",
            fixture.job_id
        ))
    }

    if successor {
        // Announced before driving, because the drive ends in an abort of its
        // own and a marker written after it would never be flushed.
        announce("successor", fixture.workspace_id, fixture.job_id);
        // `drive` reaches the provider only when given a point past the
        // dispatch commit; with no point it returns early, which is what makes
        // the dispatch scenario's durable-row control silent on the wire. The
        // boundary just after the receipt is the first one past the call.
        dispatch::drive(
            &runtime,
            &fixture,
            Some(
                vestrace_domain::external_effects::EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation,
            ),
        )
        .await
        .unwrap_or_else(|error| die(error));
        die("the successor child must abort inside its drive, not return".to_owned())
    }

    let point = settings.embedding_worker_completion_point();
    if point == BEFORE_CAS || point == AFTER_CAS {
        build_index_to(
            point,
            &runtime,
            &context,
            fixture.workspace_id,
            fixture.job_id,
        )
        .await
    }

    // The claim has committed. Nothing has been dispatched. The process stops
    // here without unwinding, without flushing a transaction and without
    // releasing the lease -- which is what a worker losing power does, and the
    // only way to find out what that leaves behind.
    announce(POINT, fixture.workspace_id, fixture.job_id);
    std::process::abort()
}

/// Drive a real index build to one side of its publication, then stop existing.
///
/// `vestrace_publish_embedding_index_build` is the compare-and-swap: in one
/// call it publishes the generation and marks the attempt published, and the
/// two sides of it are two different worlds to recover from. Before it, a
/// generation is `building` under a live claim and a later worker must be able
/// to take it; after it, the generation is Ready and the attempt is spent even
/// though the process that spent it is gone.
///
/// Nothing here touches a provider. An index is built from projections already
/// in the database, so a crash in it must cost nothing -- which the parent
/// checks by counting.
async fn build_index_to(
    point: &str,
    runtime: &sqlx::PgPool,
    context: &RequestContext,
    workspace: Uuid,
    job: Uuid,
) -> ! {
    let repository = PgEmbeddingIndexRepository::new(PgStore::from_pool(runtime.clone()));
    let plan = repository
        .claim_next_build(context, INDEX_OWNER, CHUNK)
        .await
        .unwrap_or_else(|error| die(format!("the index build claim failed: {error:?}")))
        .unwrap_or_else(|| {
            die("no index build was available, so there is no CAS to crash beside".to_owned())
        });

    // Drained rather than read once: the claim moves the attempt to `building`
    // only once its members have been handed over, and the CAS refuses an
    // attempt that is not in that state.
    let mut after = None;
    loop {
        let chunk = repository
            .load_chunk(context, &plan, after, CHUNK)
            .await
            .unwrap_or_else(|error| die(format!("an index chunk failed to load: {error:?}")));
        if chunk.complete {
            break;
        }
        let Some(last) = chunk.projections.last() else {
            die("an incomplete chunk carried no rows, so draining would not terminate".to_owned())
        };
        after = Some(last.projection_ordinal);
    }

    if point == AFTER_CAS {
        let outcome = repository
            .publish_ready(context, &plan)
            .await
            .unwrap_or_else(|error| die(format!("the index publication failed: {error:?}")));
        // Announced after the CAS committed, so a marker that exists is a
        // marker written by a process that got past it.
        announce(&format!("{AFTER_CAS} outcome={outcome:?}"), workspace, job);
        std::process::abort()
    }

    // Announced before the CAS, and nothing runs between the two.
    announce(BEFORE_CAS, workspace, job);
    std::process::abort()
}

/// One identity the parent handed this child.
///
/// A successor is told which job to work rather than finding one, because
/// "whatever is claimable" would let a mistake in the fixture look like a
/// successful takeover of something else entirely.
fn adopted_identity(name: &str) -> Uuid {
    let variable = format!("VESTRACE_WORKER_{name}");
    std::env::var(&variable)
        .unwrap_or_else(|_| die(format!("the successor child needs {variable}")))
        .parse()
        .unwrap_or_else(|error| die(format!("{variable} is not a uuid: {error}")))
}

/// Rebuild the dispatch fixture for a job that already exists.
///
/// Every field comes out of the rows the first child committed, so the
/// successor drives exactly the effect, evidence and binding snapshot the
/// crashed worker was going to. The intent is `None` because `drive` does not
/// read it: it works from the identities, and inventing one here would be this
/// child asserting something about the first child's world.
async fn adopt_fixture(
    owner: &PgPool,
    workspace: Uuid,
    job: Uuid,
) -> Result<dispatch::Fixture, String> {
    let row: (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT j.space_registration_id, j.model_binding_snapshot_id, j.external_effect_id, \
                j.model_request_evidence_id, s.connection_id, s.connection_revision_id, \
                c.principal_id \
           FROM embedding_jobs j \
           JOIN model_binding_snapshots s \
             ON s.id=j.model_binding_snapshot_id AND s.workspace_id=j.workspace_id \
           JOIN connections c \
             ON c.id=s.connection_id AND c.workspace_id=s.workspace_id \
          WHERE j.workspace_id=$1 AND j.id=$2",
    )
    .bind(workspace)
    .bind(job)
    .fetch_one(owner)
    .await
    .map_err(dispatch::sql)?;
    Ok(dispatch::Fixture {
        workspace_id: workspace,
        principal_id: row.6,
        job_id: job,
        effect_id: row.2,
        snapshot_id: row.1,
        connection_id: row.4,
        connection_revision_id: row.5,
        evidence_id: row.3,
        space_registration_id: row.0,
        intent: None,
    })
}

/// Run the crash child, prove what it left, then let a successor take over.
pub async fn run_parent(settings: &ScenarioSettings) -> Result<String, String> {
    let point = settings.embedding_worker_completion_point();
    if point == BEFORE_CAS || point == AFTER_CAS {
        return run_index_parent(settings, point).await;
    }
    let listener = LoopbackCounter::start().await?;
    let owner = dispatch::connect_owner(settings).await?;

    // --- the crash -------------------------------------------------------
    let crashed = spawn_child(settings, None, &listener.url()).await?;
    if crashed.status.success() {
        return Err("the worker-completion child exited instead of aborting".to_owned());
    }
    let (workspace, job) = identities(&text(&crashed.stderr), POINT)?;
    let crash_requests = listener.count();
    let after_crash = read_survivors(&owner, workspace, job).await?;

    // The lease a dead process holds still excludes a living one. Asked
    // through the same guarded function a real worker uses, under the
    // restricted runtime role, after the holder is gone.
    let principal = principal_of(&owner, workspace, job).await?;
    let probe = claim_as(&owner, workspace, principal, PROBE_OWNER).await?;
    let probe_claimed_the_job = probe.contains(&job);

    // --- the takeover ----------------------------------------------------
    // Aged rather than waited out: the lease is sixty seconds and nothing is
    // learned by spending them. Both timestamps move, because the row carries
    // `claim_deadline > created_at` and time does not move one without the
    // other.
    expire_lease(&owner, workspace, job).await?;
    let successor = spawn_child(settings, Some((workspace, job)), &listener.url()).await?;
    if successor.status.success() {
        return Err(format!(
            "the successor must abort at the boundary past the provider call, not exit: {}",
            text(&successor.stderr)
        ));
    }
    let (successor_workspace, successor_job) = identities(&text(&successor.stderr), "successor")?;
    if (successor_workspace, successor_job) != (workspace, job) {
        return Err(format!(
            "the successor worked {successor_workspace}/{successor_job}, not the crashed \
             job {workspace}/{job}, so this is two experiments rather than one takeover"
        ));
    }
    let successor_requests = listener.count() - crash_requests;
    let after_takeover = read_survivors(&owner, workspace, job).await?;

    Ok(serde_json::json!({
        "scenario": "embedding_worker_completion_crash",
        "point": POINT,
        "proved": true,
        // Counted by a socket in this process, each after its child was gone.
        "crash_requests": crash_requests,
        "successor_requests": successor_requests,
        "total_requests": listener.count(),
        "after_crash": after_crash,
        "probe_claimed_the_job": probe_claimed_the_job,
        "after_takeover": after_takeover,
        "successor_worked_the_crashed_job": true,
    })
    .to_string())
}

/// One child dies on one side of the index publication, and the parent reads
/// which world it left.
///
/// The two sides are the whole point. Before the compare-and-swap the attempt
/// is `building` under a live claim and the generation is still `building`:
/// nothing is drawable, and a later worker must be able to take the claim when
/// it lapses. After it the generation is Ready and the attempt is spent, and
/// that is true even though the process that made it true no longer exists.
///
/// The provider count is asserted at zero for both. An index is built from
/// projections already in the database, so a crash anywhere in it must cost
/// nothing -- and a scenario that did not check would not notice a build that
/// had started calling out.
async fn run_index_parent(
    settings: &ScenarioSettings,
    point: &'static str,
) -> Result<String, String> {
    let listener = LoopbackCounter::start().await?;
    let owner = dispatch::connect_owner(settings).await?;

    let crashed = spawn_child(settings, None, &listener.url()).await?;
    if crashed.status.success() {
        return Err(format!(
            "the {point} child exited instead of aborting: {}",
            text(&crashed.stderr)
        ));
    }
    let stderr = text(&crashed.stderr);
    // The marker for the post-CAS point carries the outcome the CAS returned,
    // so the needle is the stage prefix rather than the whole field.
    let (workspace, job) = identities(&stderr, point)?;
    let requests = listener.count();

    let attempt: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT state, safe_reason FROM embedding_index_build_attempts \
          WHERE workspace_id=$1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(workspace)
    .fetch_optional(&owner)
    .await
    .map_err(dispatch::sql)?;
    let Some((attempt_state, safe_reason)) = attempt else {
        return Err(
            "the crashed builder left no index attempt, so there is no CAS to have been \
             on either side of"
                .to_owned(),
        );
    };
    let generation: (String, i64) = sqlx::query_as(
        "SELECT state, ordinal FROM embedding_corpus_generations \
          WHERE workspace_id=$1 ORDER BY ordinal DESC LIMIT 1",
    )
    .bind(workspace)
    .fetch_one(&owner)
    .await
    .map_err(dispatch::sql)?;
    let ready: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_corpus_generations \
          WHERE workspace_id=$1 AND state='ready'",
    )
    .bind(workspace)
    .fetch_one(&owner)
    .await
    .map_err(dispatch::sql)?;
    let claim_live: Option<bool> = sqlx::query_scalar(
        "SELECT claim_deadline>now() FROM embedding_index_build_attempts \
          WHERE workspace_id=$1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(workspace)
    .fetch_optional(&owner)
    .await
    .map_err(dispatch::sql)?;

    Ok(serde_json::json!({
        "scenario": "embedding_worker_completion_crash",
        "point": point,
        "proved": true,
        "job": job.to_string(),
        // Counted by a socket in this process, after the child was gone. An
        // index build reaches no provider, on either side of the CAS.
        "provider_requests": requests,
        "persisted": {
            "attempt_state": attempt_state,
            "attempt_safe_reason": safe_reason,
            "attempt_claim_live": claim_live,
            "generation_state": generation.0,
            "generation_ordinal": generation.1,
            "ready_generations": ready,
        },
    })
    .to_string())
}

/// The principal the job's own connection belongs to.
async fn principal_of(owner: &PgPool, workspace: Uuid, job: Uuid) -> Result<Uuid, String> {
    sqlx::query_scalar(
        "SELECT c.principal_id FROM connections c \
           JOIN model_binding_snapshots s \
             ON s.connection_id=c.id AND s.workspace_id=c.workspace_id \
           JOIN embedding_jobs j \
             ON j.model_binding_snapshot_id=s.id AND j.workspace_id=s.workspace_id \
          WHERE j.workspace_id=$1 AND j.id=$2",
    )
    .bind(workspace)
    .bind(job)
    .fetch_one(owner)
    .await
    .map_err(dispatch::sql)
}

/// Ask for work as a worker would, and say which jobs came back.
async fn claim_as(
    owner: &PgPool,
    workspace: Uuid,
    principal: Uuid,
    as_owner: &str,
) -> Result<Vec<Uuid>, String> {
    let runtime = dispatch::connect_runtime(owner).await?;
    let work = PgEmbeddingWorkRepository::new(PgStore::from_pool(runtime.clone()));
    let claimed = work
        .claim(
            &RequestContext::new(
                WorkspaceId::from_uuid(workspace),
                PrincipalId::from_uuid(principal),
            ),
            EmbeddingWorkKind::Dispatch,
            as_owner,
            8,
        )
        .await
        .map_err(|error| format!("a claim must answer, not fail: {error:?}"));
    runtime.close().await;
    Ok(claimed?
        .into_iter()
        .map(|claim| claim.job_id.as_uuid())
        .collect())
}

/// End a lease the way time would.
///
/// As the guarded owner with the workspace set, and the affected count is
/// checked. Both tables force row-level security, so an owner statement without
/// `vestrace.workspace_id` matches nothing and *succeeds* -- a step that
/// skipped it would leave the lease live and the takeover below would then be
/// measuring the wrong refusal.
async fn expire_lease(owner: &PgPool, workspace: Uuid, job: Uuid) -> Result<(), String> {
    let mut transaction = owner.begin().await.map_err(dispatch::sql)?;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    sqlx::query("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    let aged = sqlx::query(
        "UPDATE embedding_job_work_claims \
            SET created_at=now()-INTERVAL '2 hours', \
                claim_deadline=now()-INTERVAL '1 hour' \
          WHERE workspace_id=$1 AND job_id=$2 AND work_kind='dispatch'",
    )
    .bind(workspace)
    .bind(job)
    .execute(&mut *transaction)
    .await
    .map_err(dispatch::sql)?
    .rows_affected();
    transaction.commit().await.map_err(dispatch::sql)?;
    if aged != 1 {
        return Err(format!(
            "exactly one lease should have been aged, but {aged} rows changed"
        ));
    }
    Ok(())
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
    successor: Option<(Uuid, Uuid)>,
    dispatch_url: &str,
) -> Result<std::process::Output, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("this executable's own path is unreadable: {error}"))?;
    let url_file = crate::argument(std::env::args().skip(1), "--database-url-file")
        .ok_or_else(|| "database url file is required: pass --database-url-file".to_owned())?;
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .map_err(|_| "VESTRACE_RUNTIME_DATABASE_URL is required".to_owned())?;
    let mut command = tokio::process::Command::new(&program);
    command
        .arg("--database-url-file")
        .arg(&url_file)
        .arg("--scenario")
        .arg("embedding_worker_completion_crash")
        .env_clear()
        .env("VESTRACE_FAULT_CHILD", "1")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        // The child is asked for the boundary this invocation was asked for,
        // not for a constant. A spawner that named one point while the parent
        // read another would file an observation under a boundary the child was
        // never sent to.
        .env(
            "VESTRACE_FAULT_POINT",
            settings.embedding_worker_completion_point(),
        )
        .env("VESTRACE_RUNTIME_DATABASE_URL", runtime_url)
        .env("VESTRACE_EMBEDDING_DISPATCH_URL", dispatch_url);
    if let Some((workspace, job)) = successor {
        command
            .env("VESTRACE_WORKER_SUCCESSOR", "1")
            .env("VESTRACE_WORKER_WORKSPACE", workspace.to_string())
            .env("VESTRACE_WORKER_JOB", job.to_string());
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
