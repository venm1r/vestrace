//! What survives a worker dying mid-claim, and what a restarted one may do.
//!
//! The claim lease is the only thing standing between one eligible job and two
//! provider calls. Everything downstream of dispatch is idempotent by identity
//! -- one external effect, one evidence root, one result -- but the provider
//! call itself is not: it is money, and it is made before any of those
//! identities has a durable outcome. So the property that matters is not that a
//! duplicate dispatch is *reconciled*, it is that a second worker never reaches
//! the provider at all.
//!
//! Proven against the real guarded functions under the restricted runtime role,
//! because the lease is enforced in SQL and a Rust-level test would be
//! asserting its own mock. What is *not* here is the provider call count: no
//! provider is reached in this suite, and counting calls needs a party outside
//! the process under test. That is the fault scenario's job, and this suite is
//! what makes its claim about the gate believable.

mod common;

use common::result_preparation_fixture::provision_result_behavior_database;
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::embedding::{
    EmbeddingWorkClaim, EmbeddingWorkKind, EmbeddingWorkOutcome, EmbeddingWorkRepository,
};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_infrastructure::postgres::{PgEmbeddingWorkRepository, PgStore};

const DISPATCH: EmbeddingWorkKind = EmbeddingWorkKind::Dispatch;

fn worker(runtime: &PgPool) -> PgEmbeddingWorkRepository {
    PgEmbeddingWorkRepository::new(PgStore::from_pool(runtime.clone()))
}

/// One claim row for this job, whatever happened to it.
async fn claims(pool: &PgPool, workspace: Uuid) -> Vec<(Uuid, String, Option<String>)> {
    sqlx::query_as(
        "SELECT job_id, claim_owner, last_outcome FROM embedding_job_work_claims \
          WHERE workspace_id=$1 ORDER BY job_id",
    )
    .bind(workspace)
    .fetch_all(pool)
    .await
    .expect("the claim table is readable by the owner")
}

/// Ends a lease the way time would.
///
/// Written as the guarded owner because the runtime role holds no UPDATE on
/// this table -- which is itself the point: a worker cannot extend or end its
/// own lease except through the two guarded functions, so a crashed worker
/// cannot leave one it controls.
async fn expire_lease(pool: &PgPool, workspace: Uuid) {
    // Both timestamps move, because the row carries `claim_deadline >
    // created_at` and time does not move one without the other. A fixture that
    // pushed only the deadline into the past would be arranging a state the
    // schema forbids and would be refused rather than proving anything.
    let affected = as_guarded_owner(
        pool,
        workspace,
        "UPDATE embedding_job_work_claims \
            SET created_at=now()-INTERVAL '2 hours', \
                claim_deadline=now()-INTERVAL '1 hour' \
          WHERE workspace_id=$1",
    )
    .await;
    assert_eq!(affected, 1, "exactly one lease was aged");
}

/// Runs one fixture statement the way only the guarded owner can.
///
/// The workspace is set as well as the role, and the affected count is returned
/// rather than discarded. Both tables force row-level security, so an owner
/// statement without `vestrace.workspace_id` matches nothing and *succeeds* --
/// a fixture that skipped either would silently arrange the opposite of what it
/// meant to, and the assertion afterwards would be measuring nothing.
async fn as_guarded_owner(pool: &PgPool, workspace: Uuid, statement: &str) -> u64 {
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .execute(&mut *owner)
        .await
        .unwrap();
    let affected = sqlx::query(statement)
        .bind(workspace)
        .execute(&mut *owner)
        .await
        .expect("the guarded owner may run this fixture statement")
        .rows_affected();
    owner.commit().await.unwrap();
    affected
}

/// Two workers, one eligible job, one claim.
///
/// The second worker is not told to wait and does not retry: it asks at the
/// same moment and is answered with nothing. A design that returned the job to
/// both and relied on a later uniqueness check would have paid for two provider
/// calls before that check ran.
#[sqlx::test(migrations = false)]
async fn one_eligible_job_yields_one_claim_however_many_workers_ask(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = common::accept_embedding_job(&pool, &runtime).await;
    let workspace = job.context.workspace_id.as_uuid();

    let first = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-a", 8)
        .await
        .expect("an accepted delivery job is claimable");
    assert_eq!(first.len(), 1, "the fixture accepted exactly one job");
    assert_eq!(first[0].job_id, job.job_id);
    assert_eq!(first[0].owner, "worker-a");

    let second = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-b", 8)
        .await
        .expect("a worker finding nothing to do is not an error");
    assert!(
        second.is_empty(),
        "a live lease must hide the job from every other worker: {second:?}"
    );

    let rows = claims(&pool, workspace).await;
    assert_eq!(rows.len(), 1, "one job, one claim row");
    assert_eq!(
        rows[0].1, "worker-a",
        "the loser must not overwrite the owner"
    );
    assert_eq!(rows[0].2, None, "no outcome has been recorded yet");

    // And the job itself is untouched by the losing attempt.
    let state: String =
        sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(workspace)
            .bind(job.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "requested");
    runtime.close().await;
}

/// A worker that died holding a lease loses the job when the lease lapses, and
/// cannot take it back by returning.
///
/// This is the restart case as it actually happens: the process is gone, so it
/// flushes nothing and releases nothing, and the only thing that ends its lease
/// is time. What must not happen is the corpse retiring the successor's work --
/// a `finish` from the old owner after the takeover would delete a claim it no
/// longer holds, and a third worker would then dispatch the same job while the
/// second was still talking to the provider.
#[sqlx::test(migrations = false)]
async fn a_lapsed_lease_is_taken_over_and_the_previous_owner_cannot_retire_it(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = common::accept_embedding_job(&pool, &runtime).await;
    let workspace = job.context.workspace_id.as_uuid();

    let dead = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-that-dies", 8)
        .await
        .expect("the first worker claims")
        .remove(0);

    expire_lease(&pool, workspace).await;

    let successor = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-that-restarts", 8)
        .await
        .expect("a lapsed lease is claimable again");
    assert_eq!(successor.len(), 1);
    assert_eq!(successor[0].owner, "worker-that-restarts");

    let rows = claims(&pool, workspace).await;
    assert_eq!(
        rows.len(),
        1,
        "a takeover updates the lease in place; it must not fork it"
    );
    assert_eq!(rows[0].1, "worker-that-restarts");

    // The dead worker comes back and tries to finish what it started.
    let error = worker(&runtime)
        .finish(&job.context, &dead, EmbeddingWorkOutcome::Completed)
        .await
        .expect_err("a worker must not retire a lease it no longer holds");
    assert!(
        matches!(error, ApplicationError::Conflict(ref code) if code == "EMBEDDING_WORK_CLAIM_CONFLICT"),
        "{error:?}"
    );

    let rows = claims(&pool, workspace).await;
    assert_eq!(
        rows.len(),
        1,
        "the refused finish must leave the successor's claim exactly as it was"
    );
    assert_eq!(rows[0].1, "worker-that-restarts");
    runtime.close().await;
}

/// A retryable failure hands the job back at once rather than holding it for
/// the rest of the lease.
///
/// The distinction matters on restart: a worker that failed for a reason
/// another worker might not hit should not make the backlog wait out a minute
/// of lease it is no longer using. The outcome is recorded on the row, so the
/// next claimant inherits a job that is known to have been tried.
#[sqlx::test(migrations = false)]
async fn a_retryable_failure_releases_the_lease_and_records_that_it_was_tried(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = common::accept_embedding_job(&pool, &runtime).await;
    let workspace = job.context.workspace_id.as_uuid();

    let claim = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-a", 8)
        .await
        .expect("the first worker claims")
        .remove(0);
    worker(&runtime)
        .finish(&job.context, &claim, EmbeddingWorkOutcome::RetryableFailure)
        .await
        .expect("a retryable failure is a lawful outcome");

    let rows = claims(&pool, workspace).await;
    assert_eq!(rows.len(), 1, "the row stays; only its lease ended");
    assert_eq!(rows[0].2.as_deref(), Some("retryable_failure"));

    let again = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-b", 8)
        .await
        .expect("a released job is immediately claimable");
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].job_id, job.job_id);
    assert_eq!(again[0].owner, "worker-b");

    let rows = claims(&pool, workspace).await;
    assert_eq!(rows[0].1, "worker-b");
    assert_eq!(
        rows[0].2, None,
        "a fresh claim must not inherit the previous attempt's outcome"
    );
    runtime.close().await;
}

/// A job that reached a terminal state is never claimed again, whatever
/// happened to its claim row.
///
/// Two independent gates, and this proves the second one alone: the claim row
/// is deleted on completion, so the lease is not what excludes the job -- its
/// state is. A worker restarting into a backlog of finished jobs must not
/// re-dispatch them because their leases have long since gone.
#[sqlx::test(migrations = false)]
async fn a_terminal_job_is_not_claimed_again_even_with_no_lease_at_all(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = common::accept_embedding_job(&pool, &runtime).await;
    let workspace = job.context.workspace_id.as_uuid();

    let claim = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-a", 8)
        .await
        .expect("the first worker claims")
        .remove(0);
    worker(&runtime)
        .finish(&job.context, &claim, EmbeddingWorkOutcome::Completed)
        .await
        .expect("completion is a lawful outcome");
    assert!(
        claims(&pool, workspace).await.is_empty(),
        "completion deletes the claim, so the lease cannot be what excludes the job"
    );

    // The job is still `requested`, so with no lease it is claimable again --
    // which is correct, and is why the state gate exists separately.
    let reclaimed = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-b", 8)
        .await
        .expect("an unfinished job with no lease is claimable");
    assert_eq!(reclaimed.len(), 1);
    worker(&runtime)
        .finish(&job.context, &reclaimed[0], EmbeddingWorkOutcome::Completed)
        .await
        .unwrap();

    let settled = as_guarded_owner(
        &pool,
        workspace,
        "UPDATE embedding_jobs SET state='succeeded' WHERE workspace_id=$1",
    )
    .await;
    assert_eq!(settled, 1, "exactly one job was settled");

    let after = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-c", 8)
        .await
        .expect("claiming is not an error when nothing is eligible");
    assert!(
        after.is_empty(),
        "a terminal job must never be dispatched again: {after:?}"
    );
    runtime.close().await;
}

/// A claim never crosses a workspace, and a worker cannot ask on another's
/// behalf.
///
/// The guarded function compares its argument against the session's own
/// `vestrace.workspace_id` rather than trusting it, so a worker that restarted
/// with a stale or forged workspace reaches nothing rather than reaching
/// someone else's backlog.
#[sqlx::test(migrations = false)]
async fn a_claim_never_crosses_a_workspace(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = common::accept_embedding_job(&pool, &runtime).await;

    let elsewhere = RequestContext::new(
        vestrace_domain::WorkspaceId::new(),
        vestrace_domain::PrincipalId::new(),
    );
    let nothing = worker(&runtime)
        .claim(&elsewhere, DISPATCH, "worker-elsewhere", 8)
        .await
        .expect("an empty workspace has an empty backlog");
    assert!(nothing.is_empty());

    // And a claim taken elsewhere cannot be retired from this workspace: the
    // finish compares the same way.
    let claim = worker(&runtime)
        .claim(&job.context, DISPATCH, "worker-a", 8)
        .await
        .expect("the owning workspace claims")
        .remove(0);
    let error = worker(&runtime)
        .finish(&elsewhere, &claim, EmbeddingWorkOutcome::Completed)
        .await
        .expect_err("a foreign workspace must not retire this claim");
    assert!(
        matches!(error, ApplicationError::Conflict(ref code) if code == "EMBEDDING_WORK_CLAIM_CONFLICT"),
        "{error:?}"
    );
    assert_eq!(
        claims(&pool, job.context.workspace_id.as_uuid())
            .await
            .len(),
        1
    );
    runtime.close().await;
}

/// Claim arguments are bounded, and a malformed one is refused rather than
/// silently clamped.
///
/// A worker whose owner name came from a hostname that grew past the bound
/// should fail loudly on its first claim, not take a lease under a truncated
/// identity that some other host might also produce.
#[sqlx::test(migrations = false)]
async fn a_malformed_claim_is_refused_rather_than_clamped(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = common::accept_embedding_job(&pool, &runtime).await;

    for (owner, limit) in [("", 8_u32), ("   ", 8), ("worker-a", 0), ("worker-a", 129)] {
        let error = worker(&runtime)
            .claim(&job.context, DISPATCH, owner, limit)
            .await
            .expect_err("a malformed claim must be refused");
        assert!(
            matches!(error, ApplicationError::Conflict(ref code) if code == "EMBEDDING_WORK_CLAIM_CONFLICT"),
            "owner={owner:?} limit={limit}: {error:?}"
        );
    }
    let long = "w".repeat(129);
    let error = worker(&runtime)
        .claim(&job.context, DISPATCH, &long, 8)
        .await
        .expect_err("an over-long owner must be refused");
    assert!(
        matches!(error, ApplicationError::Conflict(ref code) if code == "EMBEDDING_WORK_CLAIM_CONFLICT"),
        "{error:?}"
    );

    assert!(
        claims(&pool, job.context.workspace_id.as_uuid())
            .await
            .is_empty(),
        "no refused claim may leave a lease behind"
    );
    runtime.close().await;
}

/// The claim kinds that have no producer yet return nothing rather than
/// claiming over a table that does not answer them.
///
/// A restarted worker polls every kind it knows. If an unproduced kind returned
/// the dispatch backlog, one poll loop would dispatch each job once per kind.
#[sqlx::test(migrations = false)]
async fn only_the_dispatch_kind_answers_and_the_others_return_nothing(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = common::accept_embedding_job(&pool, &runtime).await;

    for kind in [
        EmbeddingWorkKind::ReconcileKeys,
        EmbeddingWorkKind::FinalizeResult,
        EmbeddingWorkKind::BuildIndex,
        EmbeddingWorkKind::CoordinateTransition,
        EmbeddingWorkKind::PropagateErasure,
    ] {
        let claimed: Vec<EmbeddingWorkClaim> = worker(&runtime)
            .claim(&job.context, kind, "worker-a", 8)
            .await
            .unwrap_or_else(|error| panic!("{kind:?} must answer, not fail: {error:?}"));
        assert!(
            claimed.is_empty(),
            "{kind:?} has no producer and must claim nothing: {claimed:?}"
        );
    }
    assert!(
        claims(&pool, job.context.workspace_id.as_uuid())
            .await
            .is_empty(),
        "a kind with no producer must not leave a lease"
    );
    runtime.close().await;
}
