# P06 Follow-Up: Closing the Connector/Provider and Qualification Gaps — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** make a Connection and its Models reach a real `qualified` state end to end on a fresh workspace, surviving a restart — by closing three of the four backend gaps P06's Task 8 found live (connector/provider auto-materialization, the qualification-job executor, and the console fixes needed to exercise them). The fourth gap (composing `GovernedRunStepInputAuthority` so AG-UI can execute) was scoped out of this plan during its own writing — see the spec's §3.4 — because it needs real design work (sampling defaults, effect-intent derivation, a new pinned-binding read port) discovered only by tracing every field its acceptance command needs, not a composition this package can respond to. This plan does not make AG-UI produce a real completion; it removes three of the four things stopping it.

**Architecture:** Auto-materialize the `connectors`/`providers` compatibility rows transactionally inside existing governed-creation SQL (no new HTTP routes). Add a new worker poll loop, modeled directly on the existing embedding-work claim/finish pattern, driving the already-complete `QualificationJobService`/`PgQualificationProbeRunner` machinery. Two small console changes (a real no-auth binding id, an embedding-kind selector) are needed to actually exercise all of this through the browser.

**Tech Stack:** Rust (axum, sqlx/Postgres, tokio), React + TypeScript for the console, Playwright (MCP) for browser evidence, Docker Compose for the restart-cycle evidence.

**Spec:** `docs/superpowers/specs/2026-09-18-vestrace-v1-p06-followup-real-execution-design.md`

## Global Constraints

- `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test`, `VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test` — always `127.0.0.1`, never `localhost`, for every Rust test in this plan.
- Migrations 0001-0216 are protected/immutable; this plan's only new migration is 0217.
- Every new file under `docs/`, `migrations/`, or `crates/` must be added to `scripts/p05-scope.mjs` (sorted) and synced into `docs/development-evidence/v1-g0-05-preflight.json`'s `change_scope_paths` in the same step it is created. Any file that is edited for the first time this program (even if it was already dirty from earlier work) must also be added — `verify-dirty-baseline.mjs` flags any edit to a file not in scope, new or old.
- Commit messages end with `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`.
- The console SDK's governed-mutation idempotency header is `idempotency-key` (already correct as of P06 Task 3) — no task in this plan needs to touch it, but any new SDK method must use the existing `governedPost`/`request` helpers, never invent its own header handling.
- `ConnectionKind`'s LM Studio value serializes to `'l_m_studio_local'` (not `'lm_studio_local'`) — already correct in the current console source; no task in this plan should reintroduce the wrong literal.
- `cargo fmt -p <crate> -- --check` must pass before any task is reported done — a prior P06 task shipped an import-ordering defect caught only by this check.

---

### Task 1: Migration — qualification job work claims

**Files:**
- Create: `migrations/0217_qualification_job_work_claims.sql`
- Test: `crates/vestrace-infrastructure/tests/qualification_baseline_repository.rs` (extend, or add a new sibling test file `crates/vestrace-infrastructure/tests/qualification_work_claims.rs` if the existing file's fixtures don't fit — implementer's call, see Step 4)

**Interfaces:**
- Consumes: the existing `qualification_jobs` table (`migrations/0177_qualification_jobs_and_revisions.sql`: `id, workspace_id, connection_revision_id, profile_revision, state, requested_at, completed_at`, `state IN ('requested','running','succeeded','failed_definite','inconclusive_unknown','cancelled')`).
- Produces: table `qualification_job_work_claims` and SQL functions `vestrace_claim_qualification_work(workspace_id, owner, limit) RETURNS TABLE(job_id, claim_owner, claim_deadline)` and `vestrace_finish_qualification_work(workspace_id, job, owner, outcome) RETURNS BOOLEAN`. Task 2 calls both by exact name.

This migration is modeled directly on the existing, proven pattern for embedding work claims: `migrations/0199_embedding_executor_work.sql`'s `embedding_job_work_claims` table and its `vestrace_claim_embedding_work`/`vestrace_finish_embedding_work` functions. Read that file first — this task produces the qualification-specific analog, simplified because qualification has only one kind of work (no `work_kind` column is needed).

- [ ] **Step 1: Write the migration file**

```sql
-- Qualification jobs have no lease/claim mechanism today: nothing in any
-- production binary ever executes QualificationJobService::run_next_probe;
-- its only callers are integration tests. This migration adds the same
-- claim/finish shape migrations/0199_embedding_executor_work.sql already
-- proved for embedding work, simplified to one work kind.

CREATE TABLE qualification_job_work_claims (
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    claim_owner TEXT NOT NULL CHECK (length(btrim(claim_owner)) BETWEEN 1 AND 128),
    claim_deadline TIMESTAMPTZ NOT NULL,
    last_outcome TEXT CHECK (last_outcome IN ('completed', 'retryable_failure', 'definite_failure')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, job_id),
    FOREIGN KEY (workspace_id, job_id) REFERENCES qualification_jobs(workspace_id, id) ON DELETE RESTRICT,
    CHECK (claim_deadline > created_at)
);

CREATE INDEX qualification_job_work_claims_available
    ON qualification_job_work_claims(workspace_id, claim_deadline, job_id);

ALTER TABLE qualification_job_work_claims ENABLE ROW LEVEL SECURITY;

CREATE POLICY qualification_job_work_claims_workspace_isolation ON qualification_job_work_claims
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE FUNCTION vestrace_claim_qualification_work(
    target_workspace UUID, target_owner TEXT, target_limit INTEGER
) RETURNS TABLE(job_id UUID, claim_owner TEXT, claim_deadline TIMESTAMPTZ)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE deadline TIMESTAMPTZ := now() + INTERVAL '60 seconds';
BEGIN
    IF target_workspace IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128
       OR target_limit IS NULL OR target_limit NOT BETWEEN 1 AND 128 THEN
        RAISE EXCEPTION 'qualification work claim arguments are malformed' USING ERRCODE = '22023';
    END IF;
    RETURN QUERY
    WITH candidates AS (
        SELECT job.id
          FROM qualification_jobs AS job
         WHERE job.workspace_id = target_workspace
           AND job.state IN ('requested', 'running')
           AND NOT EXISTS (
               SELECT 1 FROM qualification_job_work_claims AS claim
                WHERE claim.workspace_id = job.workspace_id AND claim.job_id = job.id
                  AND claim.claim_deadline > now())
         ORDER BY job.requested_at, job.id
         FOR UPDATE SKIP LOCKED
         LIMIT target_limit
    ), claimed AS (
        INSERT INTO qualification_job_work_claims(
            workspace_id, job_id, claim_owner, claim_deadline, last_outcome, created_at, updated_at)
        SELECT target_workspace, id, target_owner, deadline, NULL, now(), now() FROM candidates
        ON CONFLICT (workspace_id, job_id) DO UPDATE
           SET claim_owner = EXCLUDED.claim_owner, claim_deadline = EXCLUDED.claim_deadline,
               last_outcome = NULL, updated_at = now()
         WHERE qualification_job_work_claims.claim_deadline <= now()
        RETURNING qualification_job_work_claims.job_id,
                  qualification_job_work_claims.claim_owner, qualification_job_work_claims.claim_deadline
    )
    SELECT job_id, claim_owner, claim_deadline FROM claimed ORDER BY job_id;
END $$;

CREATE FUNCTION vestrace_finish_qualification_work(
    target_workspace UUID, target_job UUID, target_owner TEXT, target_outcome TEXT
) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128
       OR target_outcome NOT IN ('completed', 'retryable_failure', 'definite_failure') THEN
        RAISE EXCEPTION 'qualification work completion arguments are malformed' USING ERRCODE = '22023';
    END IF;
    IF target_outcome = 'retryable_failure' THEN
        UPDATE qualification_job_work_claims SET claim_deadline = now(), last_outcome = target_outcome, updated_at = now()
         WHERE workspace_id = target_workspace AND job_id = target_job
           AND claim_owner = target_owner AND claim_deadline > now();
    ELSE
        DELETE FROM qualification_job_work_claims
         WHERE workspace_id = target_workspace AND job_id = target_job
           AND claim_owner = target_owner AND claim_deadline > now();
    END IF;
    RETURN FOUND;
END $$;
```

- [ ] **Step 2: Check for a function-ownership-assignment convention and mirror it if active**

Some earlier migrations end with a call like `SELECT vestrace_assign_p03_function_owner('some_function(...)'::REGPROCEDURE);` to hand a new `SECURITY DEFINER` function's ownership to a guarded role. Run:

```bash
grep -n "vestrace_assign_p0.*_function_owner\|vestrace_assign_.*_function_owner" migrations/0210_managed_backup_archive_retention.sql migrations/0213_managed_restore_refusal.sql migrations/0216_installation_drain_request.sql
```

If any of the three most recent migrations use this pattern for their own new functions, add the equivalent call(s) at the end of `0217_qualification_job_work_claims.sql` for `vestrace_claim_qualification_work` and `vestrace_finish_qualification_work`, using the exact function-owner helper name those recent migrations use. If none of them use it (the pattern may be specific to a different migration family), skip this step — `migrations/0199_embedding_executor_work.sql`'s own claim/finish functions (the direct precedent for this task) do not appear to need it either.

- [ ] **Step 3: Register the new migration in the sqlx migrator**

Per this repo's standing convention (`crates/vestrace-infrastructure/src/postgres/pool.rs`: "a new migration file is unreachable until pool.rs allows its (version, predecessor) arm"), open `pool.rs` and find where migration 0216 was registered when it was added (search for `216` in that file). Add the equivalent `(217, 216)` (or whatever the actual pinned-pair pattern looks like at the call site you find) so migration 0217 is reachable. This step is easy to get silently wrong — the failure mode is a migration that exists on disk but is never applied, with no obviously-related error. Confirm by running Step 5's test only after this step.

- [ ] **Step 4: Write a test proving the claim/finish functions work**

Add to `crates/vestrace-infrastructure/tests/qualification_baseline_repository.rs` if its existing fixtures (workspace/connection/qualification-job setup) are reusable, otherwise create `crates/vestrace-infrastructure/tests/qualification_work_claims.rs` following this repo's standard `#[sqlx::test]` fixture pattern (check `tests/common/mod.rs` for the shared workspace/connection-revision/qualification-job builder helpers used by other qualification tests in this same directory, e.g. `qualification_repository.rs`, and reuse them rather than reinventing fixture setup):

```rust
#[sqlx::test(migrator = "vestrace_infrastructure::MIGRATOR")]
async fn claiming_qualification_work_leases_a_requested_job_and_hides_it_from_a_second_claimant(
    pool: sqlx::PgPool,
) {
    // Arrange: seed a workspace, a connection revision, and one
    // qualification_jobs row in state 'requested' (reuse this test file's
    // neighbor's fixture-building helpers for the exact INSERT statements
    // needed to satisfy qualification_jobs' foreign keys).
    // ...

    let claimed_by_first = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT job_id FROM vestrace_claim_qualification_work($1, $2, $3)",
    )
    .bind(workspace_id)
    .bind("worker-a")
    .bind(10i32)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(claimed_by_first, vec![job_id]);

    let claimed_by_second = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT job_id FROM vestrace_claim_qualification_work($1, $2, $3)",
    )
    .bind(workspace_id)
    .bind("worker-b")
    .bind(10i32)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(claimed_by_second.is_empty(), "a live claim must hide the job from a second claimant");

    let finished = sqlx::query_scalar::<_, bool>(
        "SELECT vestrace_finish_qualification_work($1, $2, $3, $4)",
    )
    .bind(workspace_id)
    .bind(job_id)
    .bind("worker-a")
    .bind("completed")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(finished);

    let reclaimed = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT job_id FROM vestrace_claim_qualification_work($1, $2, $3)",
    )
    .bind(workspace_id)
    .bind("worker-c")
    .bind(10i32)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(reclaimed.is_empty(), "a completed claim must not resurface as claimable work");
}
```

Fill in the fixture setup by reading a neighboring qualification test's exact helper calls — do not invent new fixture-building SQL when this repo already has a working pattern for it.

- [ ] **Step 5: Run the test**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test qualification_work_claims -- --test-threads=1` (or `--test qualification_baseline_repository` if extended there instead)
Expected: PASS.

- [ ] **Step 6: Admit the new migration to scope**

Add `"migrations/0217_qualification_job_work_claims.sql"` to `scripts/p05-scope.mjs`'s `changeScopePaths` (sorted position, immediately after `migrations/0216_installation_drain_request.sql`), the same path to `docs/development-evidence/v1-g0-05-preflight.json`'s `change_scope_paths` (same position), bump `tests/p05_scope.test.mjs`'s pinned `changeScopePaths.length` by 1, and add a `scope_amendments` entry to the preflight JSON (see Task 8's own entry for the exact shape to copy). If Task 4 created a new test file rather than extending an existing one, also admit that path in this same step.

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add migrations/0217_qualification_job_work_claims.sql crates/vestrace-infrastructure/src/postgres/pool.rs \
        crates/vestrace-infrastructure/tests/qualification_work_claims.rs \
        scripts/p05-scope.mjs docs/development-evidence/v1-g0-05-preflight.json tests/p05_scope.test.mjs
git commit -m "feat(p06-followup): add a claim/finish lease over qualification_jobs

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 2: Qualification job executor (worker)

**Files:**
- Create: `crates/vestrace-application/src/provider_qualification.rs` (extend — add the new port/types)
- Create: `crates/vestrace-infrastructure/src/postgres/qualification_work_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs` (export the new repository and add a `QualificationWorkerRuntime`-equivalent cycle driver, or fold the cycle logic directly into the worker command — see Step 3)
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Test: `crates/vestrace-infrastructure/tests/qualification_work_claims.rs` (extend from Task 1, or a new file) plus a worker-level test if this repo has an existing pattern for testing `worker.rs`'s poll functions directly (check for existing tests of `poll_embedding_work` first — if none exist, a repository-level test proving `run_next_probe` gets driven correctly is sufficient, per Task 1's own test).

**Interfaces:**
- Consumes: `migrations/0217`'s `vestrace_claim_qualification_work`/`vestrace_finish_qualification_work` (Task 1). `QualificationJobService::run_next_probe` and `PgQualificationProbeRunner::new(store, dispatch, worker_id)` (both pre-existing, `crates/vestrace-application/src/provider_qualification.rs:146-185`, `crates/vestrace-infrastructure/src/postgres/qualification_job_repository.rs:252-285`). `governed.dispatch()` (pre-existing in `worker.rs`, returns `SharedProviderDispatchRepository`).
- Produces: `poll_qualification_work(runtime: Option<&QualificationWorkerRuntime>, contexts: &[RequestContext]) -> PollOutcome`, slotted into `worker.rs`'s existing poll chain the same way `poll_embedding_work` is (both call sites: the `once` branch and the main loop).

- [ ] **Step 1: Add the work-claim port and types to the application layer**

In `crates/vestrace-application/src/provider_qualification.rs`, add after the existing `QualificationJobRepository` trait (after its closing `}`):

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationWorkClaim {
    pub job_id: QualificationJobId,
    pub owner: String,
    pub claim_deadline: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualificationWorkOutcome {
    Completed,
    RetryableFailure,
    DefiniteFailure,
}

#[async_trait]
pub trait QualificationWorkRepository: Send + Sync {
    async fn claim(
        &self,
        context: &RequestContext,
        owner: &str,
        limit: u32,
    ) -> Result<Vec<QualificationWorkClaim>, ApplicationError>;

    async fn finish(
        &self,
        context: &RequestContext,
        claim: &QualificationWorkClaim,
        outcome: QualificationWorkOutcome,
    ) -> Result<(), ApplicationError>;
}
```

This mirrors `crates/vestrace-application/src/embedding/work.rs`'s `EmbeddingWorkClaim`/`EmbeddingWorkOutcome`/`EmbeddingWorkRepository` exactly, minus the `kind` field (qualification has only one kind of work). Check `crates/vestrace-application/src/lib.rs`'s re-export list and add `QualificationWorkClaim, QualificationWorkOutcome, QualificationWorkRepository` to it, in alphabetical position, the same way every other new application type in this plan's sibling P06 tasks had to be re-exported (see the note in Global Constraints about `verify-dirty-baseline.mjs` flagging any newly-dirty file).

- [ ] **Step 2: Implement the Postgres repository**

Create `crates/vestrace-infrastructure/src/postgres/qualification_work_repository.rs`:

```rust
//! PostgreSQL lease/claim over qualification_jobs, mirroring
//! embedding_work_repository.rs's claim/finish shape for one work kind.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use vestrace_application::{
    ApplicationError, QualificationWorkClaim, QualificationWorkOutcome, QualificationWorkRepository,
    RequestContext,
};
use vestrace_domain::QualificationJobId;

use super::{PgInstallationMutationPermit, PgScopedTransaction, PgStore, PermitMode};

const CLAIM_CONFLICT: &str = "qualification work claim conflict";

pub struct PgQualificationWorkRepository {
    permit: PgInstallationMutationPermit,
}

impl PgQualificationWorkRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            permit: PgInstallationMutationPermit::new(store),
        }
    }
}

#[derive(FromRow)]
struct ClaimRow {
    job_id: uuid::Uuid,
    claim_owner: String,
    claim_deadline: DateTime<Utc>,
}

fn claim(row: ClaimRow) -> Result<QualificationWorkClaim, ApplicationError> {
    Ok(QualificationWorkClaim {
        job_id: QualificationJobId::from_uuid(row.job_id),
        owner: row.claim_owner,
        claim_deadline: row.claim_deadline,
    })
}

fn storage(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl QualificationWorkRepository for PgQualificationWorkRepository {
    async fn claim(
        &self,
        context: &RequestContext,
        owner: &str,
        limit: u32,
    ) -> Result<Vec<QualificationWorkClaim>, ApplicationError> {
        if owner.trim().is_empty() || owner.len() > 128 || limit == 0 || limit > 128 {
            return Err(ApplicationError::Conflict(CLAIM_CONFLICT.into()));
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        let rows = sqlx::query_as::<_, ClaimRow>(
            "SELECT * FROM vestrace_claim_qualification_work($1,$2,$3)",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(owner)
        .bind(i32::try_from(limit).map_err(|_| ApplicationError::Conflict(CLAIM_CONFLICT.into()))?)
        .fetch_all(transaction.connection())
        .await
        .map_err(storage)?;
        permit.commit().await?;
        rows.into_iter().map(claim).collect()
    }

    async fn finish(
        &self,
        context: &RequestContext,
        claim: &QualificationWorkClaim,
        outcome: QualificationWorkOutcome,
    ) -> Result<(), ApplicationError> {
        if claim.owner.trim().is_empty() {
            return Err(ApplicationError::Conflict(CLAIM_CONFLICT.into()));
        }
        let mut permit = self.permit.acquire(PermitMode::Shared, context).await?;
        let transaction = permit
            .unit_of_work_mut()
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))?;
        let finished: bool =
            sqlx::query_scalar("SELECT vestrace_finish_qualification_work($1,$2,$3,$4)")
                .bind(context.workspace_id.as_uuid())
                .bind(claim.job_id.as_uuid())
                .bind(&claim.owner)
                .bind(match outcome {
                    QualificationWorkOutcome::Completed => "completed",
                    QualificationWorkOutcome::RetryableFailure => "retryable_failure",
                    QualificationWorkOutcome::DefiniteFailure => "definite_failure",
                })
                .fetch_one(transaction.connection())
                .await
                .map_err(storage)?;
        if !finished {
            return Err(ApplicationError::Conflict(CLAIM_CONFLICT.into()));
        }
        permit.commit().await
    }
}
```

Check the exact import path for `PgInstallationMutationPermit`/`PermitMode`/`PgScopedTransaction` by reading the top of `crates/vestrace-infrastructure/src/postgres/embedding_work_repository.rs` — use the identical `use` paths, adjusted for this file's own module location. Register the new module in `crates/vestrace-infrastructure/src/postgres/mod.rs` (`mod qualification_work_repository;` and re-export `PgQualificationWorkRepository` alongside the other `Pg*Repository` exports, alphabetically).

- [ ] **Step 3: Add the poll function to the worker**

In `crates/vestrace-cli/src/commands/worker.rs`, add a function in the same style as `poll_embedding_work` (worker.rs:361-411), placed immediately after it:

```rust
async fn poll_qualification_work(
    store: &PgStore,
    governed: &vestrace_infrastructure::GovernedProviderRuntime,
    worker_id: WorkerId,
    contexts: &[RequestContext],
) -> PollOutcome {
    use vestrace_application::{QualificationWorkOutcome, QualificationWorkRepository};
    use vestrace_infrastructure::postgres::PgQualificationWorkRepository;

    let repository = PgQualificationWorkRepository::new(store.clone());
    let runner = vestrace_infrastructure::postgres::PgQualificationProbeRunner::new(
        store.clone(),
        governed.dispatch(),
        worker_id,
    );
    let service = vestrace_application::QualificationJobService::new(
        vestrace_infrastructure::postgres::PgQualificationJobRepository::new(store.clone()),
    );

    let mut outcome = PollOutcome::default();
    for context in contexts {
        let claims = match repository.claim(context, worker_id.as_str(), 10).await {
            Ok(claims) => claims,
            Err(error) => {
                tracing::warn!(
                    workspace = %context.workspace_id,
                    %error,
                    "qualification work claim failed"
                );
                outcome.failed = true;
                continue;
            }
        };
        for claim in claims {
            outcome.did_work = true;
            let ordinal = match next_qualification_ordinal(store, context, claim.job_id).await {
                Ok(Some(ordinal)) => ordinal,
                Ok(None) => {
                    // Every ordinal already has a result; nothing to run this
                    // tick. Release the claim as completed so it does not sit
                    // leased until its deadline for no reason.
                    let _ = repository
                        .finish(context, &claim, QualificationWorkOutcome::Completed)
                        .await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(job_id = %claim.job_id.as_uuid(), %error, "qualification ordinal lookup failed");
                    outcome.failed = true;
                    let _ = repository
                        .finish(context, &claim, QualificationWorkOutcome::RetryableFailure)
                        .await;
                    continue;
                }
            };
            let result = service.run_next_probe(context, claim.job_id, ordinal, &runner).await;
            // `work_outcome` starts optimistic and is downgraded below if
            // `finalize_success` fails. It must NOT be finalized to
            // `Completed` before that call is known to have succeeded:
            // `finalize_success` is the only path that moves
            // `qualification_jobs.state` to `'succeeded'` (see
            // `migrations/0185_qualification_job_lifecycle.sql`'s
            // `vestrace_finalize_qualification_job`). If it errors and the
            // claim is still released as `Completed`, the job is stranded
            // forever at `state='running'`: `next_qualification_ordinal`
            // finds no more ordinals to probe, so it never becomes
            // reclaimable work again, and `finalize_success` never gets a
            // retry.
            let mut work_outcome = match &result {
                Ok(state) if state.is_terminal() => QualificationWorkOutcome::Completed,
                Ok(_) => QualificationWorkOutcome::Completed, // one ordinal done; releases for the next tick
                Err(_) => QualificationWorkOutcome::RetryableFailure,
            };
            if let Err(error) = &result {
                tracing::warn!(job_id = %claim.job_id.as_uuid(), ordinal, %error, "qualification probe failed");
                outcome.failed = true;
            }
            if let Ok(state) = &result {
                if *state == vestrace_domain::QualificationJobState::Succeeded {
                    // The three ids below are freshly minted, not looked up —
                    // matching this codebase's universal convention that a
                    // caller publishing a new immutable revision mints its
                    // own id (the same pattern create_model_revision,
                    // create_connection, etc. all use). Confirmed against
                    // `migrations/0185_qualification_job_lifecycle.sql:769`
                    // (`vestrace_finalize_qualification_job`), which validates
                    // only that these three arguments are non-null and
                    // workspace-consistent before using them to publish new
                    // rows — it does not require them to reference anything
                    // pre-existing.
                    let finalization = vestrace_application::QualificationFinalization {
                        job_id: claim.job_id,
                        connection_qualification_revision_id:
                            vestrace_domain::ConnectionQualificationRevisionId::new(),
                        chat_model_qualification_revision_id:
                            vestrace_domain::ModelQualificationRevisionId::new(),
                        embedding_model_qualification_revision_id:
                            vestrace_domain::ModelQualificationRevisionId::new(),
                    };
                    if let Err(error) = service.finalize_success(context, finalization).await {
                        tracing::warn!(job_id = %claim.job_id.as_uuid(), %error, "qualification finalize_success failed");
                        outcome.failed = true;
                        // Downgrade: the probe succeeded but the job never
                        // reached a terminal `state`. Releasing this as
                        // `Completed` would strand it — force a retry.
                        work_outcome = QualificationWorkOutcome::RetryableFailure;
                    }
                }
            }
            let _ = repository.finish(context, &claim, work_outcome).await;
        }
    }
    outcome
}
```

Confirm `ConnectionQualificationRevisionId::new()`/`ModelQualificationRevisionId::new()` are the real constructor names in `crates/vestrace-domain/src/` (every other id newtype in this codebase exposes `::new()` backed by `Uuid::now_v7()` — this is very likely correct as written, but a one-line check before compiling saves a wasted build).

`next_qualification_ordinal` is a small new helper (co-locate it in worker.rs, near `poll_qualification_work`):

```rust
const Q1_ORDINALS: [&str; 12] = [
    "00", "10", "15", "20", "30", "35", "40", "50", "60", "70", "80", "90",
];

async fn next_qualification_ordinal(
    store: &PgStore,
    context: &RequestContext,
    job_id: vestrace_domain::QualificationJobId,
) -> Result<Option<&'static str>, vestrace_application::ApplicationError> {
    let mut scoped = store
        .begin_scoped(context)
        .await
        .map_err(|error| vestrace_application::ApplicationError::Storage(error.to_string()))?;
    let completed: Vec<String> = sqlx::query_scalar(
        "SELECT probe_ordinal FROM qualification_probe_results WHERE qualification_job_id = $1",
    )
    .bind(job_id.as_uuid())
    .fetch_all(scoped.connection())
    .await
    .map_err(|error| vestrace_application::ApplicationError::Storage(error.to_string()))?;
    scoped
        .commit()
        .await
        .map_err(|error| vestrace_application::ApplicationError::Storage(error.to_string()))?;
    Ok(Q1_ORDINALS
        .iter()
        .find(|ordinal| !completed.iter().any(|done| done == *ordinal))
        .copied())
}
```

This mirrors the `begin_scoped`/`fetch_all`/`scoped.commit()` pattern already used by `list_safe_models`/`list_safe_connections` (`crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs`, `connection_revision_repository.rs`). Confirm `PgStore::begin_scoped` and the resulting scoped-transaction type's `.connection()`/`.commit()` methods are directly reachable from `worker.rs` (they are used elsewhere in this same file via `PgQualificationWorkRepository`'s internals from Step 2, so the import path is already established in this task).

- [ ] **Step 4: Slot the poller into both call sites**

In `crates/vestrace-cli/src/commands/worker.rs`, add `poll_qualification_work(&store, &governed, worker_id, &run_contexts).await` to both the `once` branch (worker.rs:278-285) and the main loop (worker.rs:294-328), ORed into the same `did_work`/`outcome.merge(...)` expression `poll_embedding_work` already participates in — follow the exact same call pattern shown in the research already gathered for this task (`outcome.merge(poll_embedding_work(...).await);` becomes a sibling line for qualification).

- [ ] **Step 5: Write and run a test**

Using the fixture pattern from Task 1's test, extend it (or add a new test in the same file) proving: a `requested` qualification job, driven through `poll_qualification_work` repeatedly (call the function directly in the test, not through the full `worker.rs::run` loop), reaches `succeeded` after enough calls, using a fake/counting `QualificationQ1Adapter` the same way `provider_dispatch_is_atomic.rs`'s existing `qualification_runner_composes_the_original_q1_dispatch_once` test does (reuse that test's `CountingQ1Adapter` pattern rather than inventing a new fake).

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test qualification_work_claims -- --test-threads=1`
Expected: PASS.

- [ ] **Step 6: Format check and admit new files to scope**

Run: `cargo fmt -p vestrace-application -p vestrace-infrastructure -p vestrace-cli -- --check`, fix any drift.

Add `crates/vestrace-infrastructure/src/postgres/qualification_work_repository.rs` to `scripts/p05-scope.mjs`/preflight (sorted, alongside the other `postgres/*.rs` entries) — `provider_qualification.rs`, `mod.rs`, and `worker.rs` are pre-existing files; if any of them were not already in scope from earlier P05/P06 work, add them too (check with `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` after this task's edits — any "new dirty path outside scope" finding names exactly which files still need adding). Bump the pinned scope count.

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-application/src/provider_qualification.rs crates/vestrace-application/src/lib.rs \
        crates/vestrace-infrastructure/src/postgres/qualification_work_repository.rs \
        crates/vestrace-infrastructure/src/postgres/mod.rs \
        crates/vestrace-cli/src/commands/worker.rs \
        crates/vestrace-infrastructure/tests/qualification_work_claims.rs \
        scripts/p05-scope.mjs docs/development-evidence/v1-g0-05-preflight.json tests/p05_scope.test.mjs
git commit -m "feat(p06-followup): drive qualification jobs to completion from the worker

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 3: Connector and Provider auto-materialization

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs:359-375` (`insert_stable_connection`)
- Modify: `crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs` (`insert_or_verify_stable_model`)
- Test: extend existing suites (`crates/vestrace-infrastructure/tests/connection_revision_lifecycle.rs`, or wherever `create_governed`/connection creation is already tested end-to-end against a real Postgres instance; same for models via `crates/vestrace-infrastructure/tests/model_binding_snapshot.rs` or a sibling)

**Interfaces:**
- Consumes: `CreateConnectionRevision.connection` (existing, has `.id`, `.connector_id`, `.workspace_id`, `.name`), `CreateModelRevision.model`/`.command` (existing, has `.provider_id`, `.workspace_id`, `.model_name`), `ConnectionKind` (existing enum, `LMStudioLocal`/`OpenAiChatCompletionsV1`).
- Produces: nothing new consumed by later tasks in this plan — this is a self-contained backend fix.

- [ ] **Step 1: Write the failing test for connections**

In `crates/vestrace-infrastructure/tests/connection_revision_lifecycle.rs` (or wherever `PgConnectionRevisionRepository::create_governed` is already exercised against a real database — read the file first to match its existing fixture style), add:

```rust
#[sqlx::test(migrator = "vestrace_infrastructure::MIGRATOR")]
async fn creating_a_connection_on_a_fresh_workspace_materializes_its_connector(pool: sqlx::PgPool) {
    // Arrange: a fresh workspace/principal with zero rows in `connectors`.
    // (reuse this file's existing fixture helper for workspace/principal setup)

    let connector_id = uuid::Uuid::now_v7();
    let command = /* build a CreateConnectionRevision the same way this file's
                      existing "a connection is created" test does, but with
                      connection.connector_id = ConnectorId::from_uuid(connector_id)
                      against a workspace where `connectors` has no matching row */;

    repository.create_governed(context.clone(), command).await.unwrap();

    let provider_type: String = sqlx::query_scalar(
        "SELECT provider_type FROM connectors WHERE id = $1",
    )
    .bind(connector_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(provider_type, "local"); // for a LMStudioLocal-kind connection
}
```

Fill in the exact `CreateConnectionRevision` construction and repository/context setup by copying this test file's own existing, already-working test for a successful connection creation — do not invent a different fixture shape.

- [ ] **Step 2: Run it to see it fail**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test connection_revision_lifecycle creating_a_connection_on_a_fresh_workspace_materializes_its_connector -- --test-threads=1`
Expected: FAIL — either the whole creation fails with a `connectors_connector_id_fkey` violation (if this test workspace is otherwise fresh), or the `provider_type` read returns no row.

- [ ] **Step 3: Implement the connector auto-materialization**

In `crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs`, modify `insert_stable_connection` (currently at line 359, per the exact code already read: a plain `INSERT INTO connections` with no preceding statement):

```rust
async fn insert_stable_connection(
    transaction: &mut PgScopedTransaction,
    command: &CreateConnectionRevision,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(command.connection.connector_id.as_uuid())
    .bind(command.connection.workspace_id.as_uuid())
    .bind(&command.connection.name)
    .bind(connector_provider_type(command.kind))
    .execute(transaction.connection())
    .await
    .map_err(storage_error)?;

    sqlx::query(
        "INSERT INTO connections \
         (id, connector_id, workspace_id, principal_id, name, status, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(command.connection.id.as_uuid())
    .bind(command.connection.connector_id.as_uuid())
    .bind(command.connection.workspace_id.as_uuid())
    .bind(command.connection.principal_id.as_uuid())
    .bind(&command.connection.name)
    .bind(connection_status(command.connection.status))
    .bind(command.connection.created_at)
    .execute(transaction.connection())
    .await
    .map_err(storage_error)?;
    Ok(())
}

fn connector_provider_type(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::LMStudioLocal => "local",
        ConnectionKind::OpenAiChatCompletionsV1 => "remote",
    }
}
```

Check whether `command.kind` is the correct field name on `CreateConnectionRevision` for the connection's `ConnectionKind` (it was described in the P06 spec's research as a top-level field: `CreateConnectionRevision { connection, revision_id, execution_guard_id, kind, logical_base_url, ... }` — confirm against the current struct definition in `crates/vestrace-application/src/` before using it, since this task's diff must reference the real field name, not a guessed one). `storage_error` should already be defined in this file (used by the surrounding functions); reuse it, do not redefine it.

- [ ] **Step 4: Run the connection test again**

Expected: PASS.

- [ ] **Step 5: Repeat Steps 1-4 for the provider/model side**

In `crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs`, modify `insert_or_verify_stable_model` (the function already reads, per earlier research: `INSERT INTO models (id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken, created_at) VALUES (...) ON CONFLICT (id) DO NOTHING`):

```rust
async fn insert_or_verify_stable_model(
    transaction: &mut PgScopedTransaction,
    command: &CreateModelRevision,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO providers (id, workspace_id, name, locality) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(command.model.provider_id.as_uuid())
    .bind(command.model.workspace_id.as_uuid())
    .bind(&command.model.model_name)
    .bind("governed")
    .execute(transaction.connection())
    .await
    .map_err(storage_error)?;

    sqlx::query(
        "INSERT INTO models \
         (id, provider_id, workspace_id, model_name, context_window, \
          input_cost_per_mtoken, output_cost_per_mtoken, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(command.model.id.as_uuid())
    .bind(command.model.provider_id.as_uuid())
    .bind(command.model.workspace_id.as_uuid())
    .bind(&command.model.model_name)
    .bind(command.model.context_window as i32)
    .bind(command.model.input_cost_per_mtoken)
    .bind(command.model.output_cost_per_mtoken)
    .bind(command.model.created_at)
    .execute(transaction.connection())
    .await
    .map_err(storage_error)?;
    Ok(())
}
```

`"governed"` is the fixed, decorative `locality` value — confirmed by grep (`grep -rn "providers\." crates/vestrace-infrastructure/src/postgres/*.rs` finds no query anywhere that selects `providers.locality`; the only `.locality` reads in the codebase are on an unrelated struct, `RoutingCandidate`, populated from a legacy HTTP request body in `crates/vestrace-http/src/api/routing.rs`, never from this table). Write an equivalent test to Step 1, proving a fresh-workspace model-revision creation materializes its provider row with `locality = 'governed'`.

- [ ] **Step 6: Run both suites, format check**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test connection_revision_lifecycle -- --test-threads=1` and the equivalent for the model-revision test file.
Run: `cargo fmt -p vestrace-infrastructure -- --check`.
Expected: PASS, no drift.

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs \
        crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs \
        crates/vestrace-infrastructure/tests/connection_revision_lifecycle.rs
git commit -m "feat(p06-followup): auto-materialize connector/provider compatibility rows

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

(No new files created in this task — nothing to admit to scope beyond files already registered from P06, unless `verify-dirty-baseline.mjs` reports otherwise after this task's edits, per Global Constraints.)

---

### Task 4: Expose the real no-auth binding revision id

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs:109-205` (`list_safe_connections` / `decode_safe_connection_projection`)
- Modify: `crates/vestrace-application/src/connections.rs:31-37` (`GovernedConnectionProjection`)
- Modify: `crates/vestrace-http/src/api/connections.rs:43-62` (`ConnectionResponse`)
- Test: extend the existing test module in `crates/vestrace-http/src/api/connections.rs` or add an infra-level test for `list_safe_connections`

**Interfaces:**
- Consumes: `no_auth_binding_revisions` table (existing, `migrations/0176_connection_revisions_and_no_auth_bindings.sql:581-598`, keyed by `(workspace_id, connection_id, connection_revision_id)`).
- Produces: `GovernedConnectionProjection.no_auth_binding_revision_id: Option<Uuid>` and `ConnectionResponse.no_auth_binding_revision_id: Option<Uuid>`. Task 5 (console) consumes this field by exact name.

- [ ] **Step 1: Add the field to both Rust types**

In `crates/vestrace-application/src/connections.rs`:

```rust
pub struct GovernedConnectionProjection {
    pub id: ConnectionId,
    pub revision_id: Option<ConnectionRevisionId>,
    pub state: String,
    pub qualification_state: String,
    pub blockers: Vec<String>,
    pub no_auth_binding_revision_id: Option<uuid::Uuid>,
}
```

In `crates/vestrace-http/src/api/connections.rs`:

```rust
#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub id: uuid::Uuid,
    pub revision_id: Option<uuid::Uuid>,
    pub state: String,
    pub qualification_state: String,
    pub blockers: Vec<String>,
    pub no_auth_binding_revision_id: Option<uuid::Uuid>,
}

impl From<GovernedConnectionProjection> for ConnectionResponse {
    fn from(projection: GovernedConnectionProjection) -> Self {
        Self {
            id: projection.id.as_uuid(),
            revision_id: projection.revision_id.map(|id| id.as_uuid()),
            state: projection.state,
            qualification_state: projection.qualification_state,
            blockers: projection.blockers,
            no_auth_binding_revision_id: projection.no_auth_binding_revision_id,
        }
    }
}
```

- [ ] **Step 2: Add the join to the query**

In `crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs`, modify `list_safe_connections`'s SQL (currently the exact text already read: a `SELECT` with two `LEFT JOIN`s on `connection_revision_heads` and `connection_qualification_heads`/`connection_qualification_revisions`):

```sql
SELECT connection.id,
       head.current_revision_id AS connection_revision_id,
       head.state AS connection_state,
       qualification.valid_until AS qualification_valid_until,
       no_auth.id AS no_auth_binding_revision_id
  FROM connections AS connection
  LEFT JOIN connection_revision_heads AS head
    ON head.workspace_id=connection.workspace_id
   AND head.connection_id=connection.id
  LEFT JOIN connection_qualification_heads AS qualification_head
    ON qualification_head.workspace_id=connection.workspace_id
   AND qualification_head.connection_revision_id=head.current_revision_id
  LEFT JOIN connection_qualification_revisions AS qualification
    ON qualification.workspace_id=connection.workspace_id
   AND qualification.id=qualification_head.current_qualification_revision_id
  LEFT JOIN no_auth_binding_revisions AS no_auth
    ON no_auth.workspace_id=connection.workspace_id
   AND no_auth.connection_id=connection.id
   AND no_auth.connection_revision_id=head.current_revision_id
 WHERE connection.workspace_id=$1
 ORDER BY connection.created_at, connection.id
```

(Joined on `connection_id` AND `connection_revision_id = head.current_revision_id`, per the table's own real key shape, not just `connection_id` alone — a connection with more than one revision must only surface the no-auth binding for its *current* revision.)

Update `decode_safe_connection_projection` to read the new column and populate the new struct field:

```rust
let no_auth_binding_revision_id: Option<uuid::Uuid> = row
    .try_get("no_auth_binding_revision_id")
    .map_err(storage_error)?;
```

and add `no_auth_binding_revision_id,` to the `Ok(GovernedConnectionProjection { ... })` construction at the end of the function.

- [ ] **Step 3: Write the failing test, then make it pass**

Extend `crates/vestrace-http/src/api/connections.rs`'s existing test module (it already has a `SpyRevisions`/similar test double and real-database-backed tests per earlier P06 work) or add an infra-level test asserting: a Connection created with `auth_mode: none` produces a `list_safe_connections` row whose `no_auth_binding_revision_id` matches the real row in `no_auth_binding_revisions` for that connection's current revision — read directly via SQL in the test's own assertion, the same way Task 3's tests do.

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test connection_revision_lifecycle -- --test-threads=1` (or wherever this test lands)
Expected: PASS.

Run: `cargo fmt -p vestrace-application -p vestrace-http -p vestrace-infrastructure -- --check`
Expected: no drift.

- [ ] **Step 4: Commit**

```bash
git add crates/vestrace-application/src/connections.rs crates/vestrace-http/src/api/connections.rs \
        crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs
git commit -m "feat(p06-followup): expose the real no-auth binding revision id

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 5: Console — real no-auth binding id in the Connections page

**Files:**
- Modify: `apps/console/src/sdk/client.ts`
- Modify: `apps/console/src/routes/ConnectionsPage.tsx`

**Interfaces:**
- Consumes: `ConnectionResponse.no_auth_binding_revision_id` (Task 4, backend).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add the field to the SDK type**

In `apps/console/src/sdk/client.ts`, modify `GovernedConnectionItem`:

```typescript
export interface GovernedConnectionItem {
  id: string;
  revision_id: string | null;
  state: string;
  qualification_state: string | null;
  blockers: string[];
  no_auth_binding_revision_id: string | null;
}
```

- [ ] **Step 2: Use the real id in ConnectionsPage.tsx**

Replace, in `handleTest` (currently `target: { branch: 'no_auth', binding_revision_id: crypto.randomUUID() }`):

```typescript
  const handleTest = async (connectionId: string) => {
    const connection = items.find((c) => c.id === connectionId);
    if (!connection?.revision_id) {
      notify('warning', 'This connection has no published revision yet.');
      return;
    }
    if (!connection.no_auth_binding_revision_id) {
      notify('warning', 'This connection has no no-auth binding yet — only the no-auth branch is supported here.');
      return;
    }
    if (!chatRevisionInput.trim() || !embeddingRevisionInput.trim()) {
      notify('warning', 'Paste a chat and an embedding Model revision id to qualify against.');
      return;
    }
    try {
      await vestraceClient.requestConnectionQualification(
        connectionId,
        {
          job_id: crypto.randomUUID(),
          target_binding_id: crypto.randomUUID(),
          connection_id: connectionId,
          connection_revision_id: connection.revision_id,
          target: { branch: 'no_auth', binding_revision_id: connection.no_auth_binding_revision_id },
          chat_model_revision_id: chatRevisionInput.trim(),
          embedding_model_revision_id: embeddingRevisionInput.trim(),
        },
        { requestId: crypto.randomUUID() },
      );
      setTestingId(connectionId);
      notify('info', `Qualification requested for ${connectionId}.`);
    } catch (err: unknown) {
      const described = describeError(err, 'connection qualification');
      notify('error', `${described.title}: ${described.detail}`);
    }
  };
```

(The credentialed branch's `target: { branch: 'credential', ... }` shape is not built here — it remains out of scope per this plan's Non-goals; this task only fixes the no-auth path this repo can actually exercise.)

- [ ] **Step 3: Type-check**

Run: `cd apps/console && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/console/src/sdk/client.ts apps/console/src/routes/ConnectionsPage.tsx
git commit -m "feat(p06-followup): qualify connections using the real no-auth binding id

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 6: Console — embedding-kind Model publication

**Files:**
- Modify: `apps/console/src/routes/ModelsPage.tsx`

**Interfaces:**
- Consumes: `ModelKind` (existing SDK type, `'chat' | 'embedding'`).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add a Kind field to the form's state and payload**

In `ModelsPage.tsx`, add state (near the existing `wireModelId`/`selectedConnectionId` declarations):

```typescript
  const [modelKind, setModelKind] = useState<'chat' | 'embedding'>('chat');
```

Change the `createModelRevision` call's `kind: 'chat'` (currently hardcoded) to `kind: modelKind`.

- [ ] **Step 2: Add the selector to the form JSX**

Add, immediately after the `Wire Model Id *` field and before the `Connection *` field:

```tsx
          <div>
            <label htmlFor="model-kind" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Kind *
            </label>
            <select
              id="model-kind"
              value={modelKind}
              onChange={(e) => setModelKind(e.target.value as 'chat' | 'embedding')}
              className="field-control"
              style={{ width: '100%' }}
            >
              <option value="chat">Chat</option>
              <option value="embedding">Embedding</option>
            </select>
          </div>
```

Reset it alongside the other fields on successful submit (`setModelKind('chat')`, next to `setModelName('')`/`setWireModelId('')`).

- [ ] **Step 3: Type-check**

Run: `cd apps/console && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/console/src/routes/ModelsPage.tsx
git commit -m "feat(p06-followup): support publishing embedding-kind Model revisions

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 7: Evidence — browser and restart proof that qualification reaches a real terminal state

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/p06-followup-no-auth-run-test.txt`
- Create: `docs/development-evidence/v1-g0-06-followup-real-execution.md`

**Interfaces:**
- Consumes: everything from Tasks 1-6, running together.

This is evidence-gathering, not code-writing — the same nature as P06's own Task 8. **This task does not attempt to prove a real AG-UI completion** — gap 4 (composing `GovernedRunStepInputAuthority`) is deferred out of this plan (spec §3.4); `POST /ag-ui/run` is expected to still refuse exactly as it did in P06's own evidence run. This task's job is to prove the three gaps this plan actually closes are closed, live. Two things to keep in mind, both already learned the hard way in this program:

1. **Rebuild all images before testing** (`docker compose build vestrace-server vestrace-worker console`) and confirm the running containers actually picked up the new build — a prior evidence run in this program mis-diagnosed working code as broken because a browser had a stale cached bundle from before a rebuild. If anything looks wrong, verify with a cache-bypassing `fetch(..., {cache: 'no-store'})` against the served JS before concluding the code itself is wrong.
2. **The qualification worker involves real wall-clock time** — up to 12 real network probes per job (the fixed ordinal sequence), against a real LM Studio instance. Budget for this; do not assume a fast poll means something is broken.

- [ ] **Step 1: Bring up the stack and verify each fix independently**

Rebuild and start (`docker compose down -v` for a clean start is acceptable here since this is disposable local state, not evidence, then `docker compose build` + `up -d`). Confirm each fix directly before the full walkthrough:
- Create a Connection through the console form on a workspace with zero `connectors` rows — should now succeed (Task 3).
- Register a chat Model and an embedding Model through the console form, both bound to that Connection — should now succeed (Task 3, Task 6).
- Check `no_auth_binding_revision_id` is present in `GET /v1/connections`'s response for that connection (Task 4).
- Confirm the worker process picks up a requested qualification job (`docker compose logs vestrace-worker | grep -i qualification`, or query `qualification_job_work_claims` directly) within a few seconds of it being requested (Task 2).

- [ ] **Step 2: Full Playwright walkthrough to a real qualified state**

Through the browser: create the Connection, publish chat + embedding Models, "Test Connection" with the real no-auth binding id and both real Model revision ids, and wait for the job to reach `qualified` (record how long this actually took — real network probes against LM Studio). Set the workspace chat default and confirm the Settings "Models" tab reflects it. Then attempt an AG-UI message through the chat widget and record the *current, expected* result: a refusal (either the original `governed_run_input_required`-successor 503, or whatever `POST /ag-ui/run` currently returns) — this is not a failure of this task, it is the honest, documented boundary of what this plan closes. Save screenshots at each milestone (connection qualified, both models registered, default set, the AG-UI refusal) to `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/`.

Write the full transcript, including every command run and its real output (never characterize a failure as a skip, per this program's standing evidence discipline), to `docs/development-evidence/v1-g0-05-gate/p06-followup-no-auth-run-test.txt`.

- [ ] **Step 3: Restart cycle**

`docker compose down` (volume preserved, no `-v`) + `up -d`. Confirm: the qualified Connection/Model state, the workspace default, and any `qualification_job_work_claims`/`qualification_probe_results` rows all survive. Append this to the same evidence file.

- [ ] **Step 4: Write the closing summary**

Create `docs/development-evidence/v1-g0-06-followup-real-execution.md`, following the exact structure and honesty standard of `docs/development-evidence/v1-g0-06-real-execution.md` (P06's own closing doc): state plainly that this package closes gaps 1-3 (connector/provider auto-materialization, qualification execution) with real, live evidence, that gap 4 (AG-UI real execution) remains exactly as blocked as P06 left it and was deliberately not attempted here (cite the spec's §3.4 deferral, not a new failure), and cite the transcript and screenshots by exact sha256. If anything in Tasks 1-6 does not fully close the loop once tested live (a realistic possibility given Task 2's `finalize_success` step was flagged as needing implementation-time resolution), record exactly what still blocks it with the same rigor P06's own Task 8 used — do not round a partial result up to a full pass, and do not round this package's real, narrower scope up to "AG-UI now works."

- [ ] **Step 5: Admit the new evidence files to scope**

Add `docs/development-evidence/v1-g0-06-followup-real-execution.md`, `docs/development-evidence/v1-g0-05-gate/p06-followup-no-auth-run-test.txt`, and every screenshot under `p06-followup-screenshots/` to `scripts/p05-scope.mjs`/preflight (sorted), bump the pinned count, and add a `scope_amendments` entry following this program's established shape:

```json
{
  "authorized_at_utc": "<today, ISO-8601, one second after the previous latest entry>",
  "authorized_by": "user standing authorization",
  "reason": "P06 follow-up Task 7 closes live evidence for connector/provider auto-materialization and qualification-job execution reaching a real terminal state; gap 4 (AG-UI real execution) remains deliberately out of scope, per docs/superpowers/plans/2026-09-18-vestrace-v1-p06-followup-real-execution.md.",
  "paths": [
    "docs/superpowers/plans/2026-09-18-vestrace-v1-p06-followup-real-execution.md",
    "docs/development-evidence/v1-g0-06-followup-real-execution.md",
    "docs/development-evidence/v1-g0-05-gate/p06-followup-no-auth-run-test.txt"
  ]
}
```

(List every screenshot path individually alongside these three, matching this program's own established per-file scope-listing convention — do not list a directory.)

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS.

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`
Expected: silent (exit 0).

- [ ] **Step 6: Commit**

```bash
git add docs/development-evidence/v1-g0-06-followup-real-execution.md \
        docs/development-evidence/v1-g0-05-gate/p06-followup-no-auth-run-test.txt \
        docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/ \
        scripts/p05-scope.mjs docs/development-evidence/v1-g0-05-preflight.json tests/p05_scope.test.mjs
git commit -m "docs(p06-followup): record whether the no-auth branch reaches a real completion

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Push

Per this repository's standing rule, push after every completed task using the temp-index technique (never the real index/HEAD):

```bash
export GIT_INDEX_FILE=/tmp/vestrace-temp-index-$$
rm -f "$GIT_INDEX_FILE"
git add -A
SHA=$(git commit-tree $(git write-tree) -p $(git rev-parse origin/main) -m "...")
git push origin "$SHA:refs/heads/main"
rm -f "$GIT_INDEX_FILE"
unset GIT_INDEX_FILE
```

Always fetch and use the current `origin/main` tip as `-p`, not a remembered SHA.
