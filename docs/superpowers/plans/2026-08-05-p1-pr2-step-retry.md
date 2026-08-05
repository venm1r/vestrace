# P1 / PR2 Step Retry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic, crash-safe retry semantics for one run step, including durable attempt identity, timed retry waiting, retry-limit exhaustion, checkpoint format v2, and exact recovery.

**Architecture:** Keep retry truth entirely in the append-only run event stream. The pure domain decision/reducer layer emits and applies attempt-aware multi-event transitions; the existing command service and PostgreSQL committer persist each batch atomically. Checkpoints remain derived acceleration data and advance to format version 2 because `RunState` serialization changes.

**Tech Stack:** Rust 1.85, Serde/serde_json, SQLx 0.8, PostgreSQL 17, async-trait, SHA-256 checkpoints, GitHub Actions.

## Global Constraints

- Base all work on `agent/p1-state-engine-vertical`; implement on `agent/p1-step-retry`.
- Preserve readability of existing persisted `StepStarted`, `StepCompleted`, and `StepFailed` JSON events.
- A retry keeps the same `step_id`; attempt identity is `(workspace_id, run_id, step_id, attempt)`.
- `max_attempts` includes the first attempt and must be at least `1`.
- The reducer never reads a clock; `command.issued_at` is compared with stored `resume_at`.
- Failure/schedule and resume/start are committed as atomic two-event batches.
- Do not add a scheduler, worker loop, due-work query, HTTP endpoint, run-wide retry, automatic backoff, command idempotency, subagents, compensation, capabilities, CAS, memory, or UI.
- Preserve workspace RLS, projection rebuild, stream-owned optimistic concurrency, and sanitized storage errors.
- Use strict RED → GREEN TDD and commit each independently reviewable slice.

---

### Task 1: Introduce attempt-aware domain contracts

**Files:**
- Modify: `crates/vestrace-domain/src/run/state.rs`
- Modify: `crates/vestrace-domain/src/run/command.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/error.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Modify: `crates/vestrace-domain/tests/run_state_engine.rs`

**Interfaces:**
- Produces `StepFailureDisposition`, `StepFailure`, `RunStatus::WaitingForRetry`, `RunWait::Retry`, and `StallReason::RetryLimitExceeded`.
- Changes `RunCommand::StartStep` to include `max_attempts`.
- Changes `RunCommand::FailStep` to include `disposition: StepFailureDisposition`.
- Adds `RunCommand::ResumeStepRetry { step_id }`.
- Adds attempt-aware events without changing legacy event variants.

- [ ] **Step 1: Write compile RED tests for public contracts**

Update imports and add a test that constructs:

```rust
let resume_at = fixture.at + chrono::Duration::seconds(30);
let start = RunCommand::StartStep {
    step_id,
    kind: "tool".to_owned(),
    label: Some("Retryable tool".to_owned()),
    max_attempts: 3,
};
let fail = RunCommand::FailStep {
    step_id,
    code: "provider_timeout".to_owned(),
    message: "provider timed out".to_owned(),
    disposition: StepFailureDisposition::Retry { resume_at },
};
let resume = RunCommand::ResumeStepRetry { step_id };
```

Assert JSON round trips for every new command, event, wait, and stall-reason variant.

- [ ] **Step 2: Run the domain target and verify RED**

Run:

```bash
cargo test -p vestrace-domain --test run_state_engine -- --nocapture
```

Expected: compile failure for missing retry types, fields, commands, events, and status.

- [ ] **Step 3: Add state and command types**

Implement:

```rust
pub enum StepFailureDisposition {
    Final,
    Retry { resume_at: Timestamp },
}

pub struct StepFailure {
    pub code: String,
    pub message: String,
    pub failed_at: Timestamp,
}
```

Extend `RunStepState` with `attempt: u32` and `max_attempts: u32`. Add the full retry context to `RunWait::Retry`. Add `WaitingForRetry` to `RunStatus`, include it in `is_waiting`, and add typed retry-limit exhaustion to `StallReason`.

- [ ] **Step 4: Add attempt-aware events and stable names**

Add:

```rust
StepAttemptStarted { step_id, kind, label, attempt, max_attempts }
StepAttemptCompleted { step_id, attempt, output_references }
StepAttemptFailed { step_id, attempt, max_attempts, code, message, retryable }
StepRetryScheduled { step_id, kind, label, failed_attempt, next_attempt, max_attempts, resume_at, code, message }
StepRetryResumed { step_id, attempt }
```

Map them to exactly:

```text
step.attempt_started
step.attempt_completed
step.attempt_failed
step.retry_scheduled
step.retry_resumed
```

All new event versions are `1`; legacy event names and payloads remain unchanged.

- [ ] **Step 5: Add typed retry-not-ready error**

Add:

```rust
RunDecisionError::RetryNotReady { resume_at: Timestamp }
```

Use a stable error message that does not include transient database details.

- [ ] **Step 6: Run formatting and compile tests**

Run:

```bash
cargo fmt --all --check
cargo test -p vestrace-domain --test run_state_engine -- --nocapture
```

Expected: tests compile; behavioral retry tests remain for Task 2.

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-domain
git commit -m "feat(domain): add step retry contracts"
```

---

### Task 2: Implement deterministic retry decisions and reduction

**Files:**
- Modify: `crates/vestrace-domain/src/run/decision.rs`
- Modify: `crates/vestrace-domain/src/run/reducer.rs`
- Modify: `crates/vestrace-domain/tests/run_state_engine.rs`

**Interfaces:**
- `StartStep` emits one `StepAttemptStarted(attempt = 1)` event.
- `FailStep(Final)` emits one attempt failure.
- `FailStep(Retry)` emits failure + schedule or failure + stall.
- `ResumeStepRetry` emits resume + next-attempt start.
- Legacy `StepStarted` replay normalizes state to attempt `1`, max `1`.

- [ ] **Step 1: Write behavioral RED tests**

Add focused tests for:

```rust
assert_eq!(decide(Some(&running), &start_zero).unwrap_err(),
    RunDecisionError::InvalidCommand("max attempts must be at least one".to_owned()));
```

Also assert:

- first attempt is `1` with configured limit;
- completion/failure commands operating on legacy active state emit new attempt-aware events;
- final failure clears the step and leaves the run `Running`;
- retryable failure emits exactly two events with consecutive sequences after envelope construction;
- `resume_at < issued_at` is rejected;
- exhaustion emits `StepAttemptFailed` then `Stalled(RetryLimitExceeded)`;
- early resume returns `RetryNotReady`;
- due resume emits `StepRetryResumed` then `StepAttemptStarted` for the same step and attempt + 1;
- generic `Resume` cannot resume retry waiting;
- wrong step IDs and attempt mismatches fail explicitly;
- replay across two retries is deterministic.

- [ ] **Step 2: Run tests and verify behavioral RED**

Run:

```bash
cargo test -p vestrace-domain --test run_state_engine -- --nocapture
```

Expected: failures because decision and reducer do not implement the new variants.

- [ ] **Step 3: Implement decision helpers**

Add checked helpers:

```rust
fn require_positive_attempt_limit(max_attempts: u32) -> Result<(), RunDecisionError>;
fn next_attempt(attempt: u32) -> Result<u32, RunDecisionError>;
fn require_retry_due(issued_at: Timestamp, resume_at: Timestamp) -> Result<(), RunDecisionError>;
```

`FailStep(Retry)` must clone the active step context before emitting failure. When the limit is available, emit failure then schedule; when exhausted, emit failure then `Stalled`.

- [ ] **Step 4: Implement reducer transitions**

Rules:

```text
StepAttemptFailed: clear active step, remain Running
StepRetryScheduled: require no active step, enter WaitingForRetry with full context
StepRetryResumed: require matching retry wait, clear wait, return Running
StepAttemptStarted: require Running/no wait/no active step, create active attempt
```

Legacy `StepStarted` creates `attempt = 1`, `max_attempts = 1`. Legacy complete/fail continue to apply. New complete/fail require exact step and attempt identity.

- [ ] **Step 5: Prove every committed prefix is reducible**

Add tests that apply only the first event of each two-event batch and assert a valid intermediate state:

- after failure only: `Running`, no active step, no wait;
- after retry-resumed only: `Running`, no active step, no wait.

Then apply the second event and assert the final intended state.

- [ ] **Step 6: Run domain matrix**

Run:

```bash
cargo test -p vestrace-domain --all-targets --all-features
cargo clippy -p vestrace-domain --all-targets --all-features -- -D warnings
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-domain
git commit -m "feat(domain): implement deterministic step retries"
```

---

### Task 3: Advance checkpoint format and projection status

**Files:**
- Create: `migrations/0113_step_retry_checkpoint_format.sql`
- Modify: `crates/vestrace-application/src/runs/recovery.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run_recovery_store.rs`
- Modify: `tests/run_stream_recovery_migrations.rs`
- Modify: `tests/run_recovery_service.rs`
- Modify: `tests/run_recovery_store.rs`

**Interfaces:**
- `RUN_CHECKPOINT_FORMAT_VERSION` becomes `2`.
- `run_checkpoints` accepts only format `2` after migration.
- PostgreSQL projection mapping supports `waiting_for_retry`.

- [ ] **Step 1: Write migration and mapping RED tests**

Add assertions that:

```rust
let version: i16 = sqlx::query_scalar(
    "SELECT format_version FROM run_checkpoints LIMIT 1"
).fetch_one(&pool).await.unwrap();
assert_eq!(version, 2);
```

Before inserting, verify migration leaves `run_events` and `run_streams` counts unchanged and removes all old checkpoints. Assert format `1` insertion fails with SQLSTATE `23514`, while format `2` succeeds.

Add repository/recovery tests that round-trip `RunStatus::WaitingForRetry` as `waiting_for_retry`.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test --test run_stream_recovery_migrations -- --nocapture
cargo test --test run_recovery_store -- --nocapture
cargo test --test run_recovery_service -- --nocapture
```

Expected: failures for format version `1` and unsupported status mapping.

- [ ] **Step 3: Implement migration 0113**

Use:

```sql
TRUNCATE TABLE run_checkpoints;
ALTER TABLE run_checkpoints
    DROP CONSTRAINT run_checkpoints_format_version_supported;
ALTER TABLE run_checkpoints
    ADD CONSTRAINT run_checkpoints_format_version_supported
    CHECK (format_version = 2);
ALTER TABLE run_checkpoints
    ALTER COLUMN format_version SET DEFAULT 2;
```

Do not modify streams or events.

- [ ] **Step 4: Update application/infrastructure constants and mappings**

Set `RUN_CHECKPOINT_FORMAT_VERSION: u16 = 2`. Replace hard-coded `format_version: 1` fixtures with the exported constant. Add `waiting_for_retry` to both `status_as_str/status_name` and `parse_status` mappings.

- [ ] **Step 5: Run recovery and migration matrix**

Run:

```bash
cargo test --test run_stream_recovery_migrations -- --nocapture
cargo test --test run_recovery_service -- --nocapture
cargo test --test run_recovery_store -- --nocapture
cargo test --test run_repository -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add migrations/0113_step_retry_checkpoint_format.sql crates/vestrace-application crates/vestrace-infrastructure tests
git commit -m "feat(recovery): advance retry checkpoints to format v2"
```

---

### Task 4: Prove atomic multi-event command commits

**Files:**
- Modify: `tests/run_command_service.rs`
- Modify: `tests/run_command_committer.rs`
- Create: `tests/run_step_retry_postgres.rs`

**Interfaces:**
- Existing `RunCommandService` and `PgRunCommandCommitter` require no new port methods.
- A two-event transition advances the stream and projection by exactly two versions in one transaction.

- [ ] **Step 1: Write application RED tests for event numbering**

Build a retry-waiting stream and assert `RunCommandService` produces:

```text
failure sequence = expected + 1
schedule sequence = expected + 2
projection.version = expected + 2
```

Do the same for resume/start. Assert both events share actor, causation ID, correlation ID, and command timestamp.

- [ ] **Step 2: Write PostgreSQL RED tests**

The new `run_step_retry_postgres` target must assert:

- failure + schedule commits two events and `waiting_for_retry` projection atomically;
- resume + start commits two events and recreates attempt `2`;
- injected duplicate event ID or invalid projection causes full rollback: no partial event, unchanged stream head, unchanged projection;
- two concurrent due resumes yield one success and one optimistic conflict;
- waiting retry survives projection deletion and is rebuilt exactly.

- [ ] **Step 3: Run tests and verify RED or existing support**

Run:

```bash
cargo test --test run_command_service -- --nocapture
cargo test --test run_step_retry_postgres -- --nocapture
```

Expected: application tests may pass after Task 2; PostgreSQL tests initially fail on any missed status/event/checkpoint integration. Do not alter committer code unless the failing test proves an atomicity defect.

- [ ] **Step 4: Apply only evidence-driven production fixes**

If necessary, update event encoding/decoding or projection status mapping. Preserve the existing single transaction:

```text
lock run_streams
→ validate expected head
→ insert entire event batch
→ advance stream head to final sequence
→ upsert final projection
→ commit
```

- [ ] **Step 5: Run concurrency and rollback matrix**

Run:

```bash
cargo test --test run_step_retry_postgres -- --nocapture
cargo test --test run_command_committer -- --nocapture
cargo test --test run_projection_independence -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add tests crates/vestrace-application crates/vestrace-infrastructure
git commit -m "test(p1): prove atomic step retry commits"
```

---

### Task 5: Complete retry recovery and end-to-end acceptance

**Files:**
- Modify: `tests/run_state_engine_recovery.rs`
- Modify: `tests/run_recovery_service.rs`
- Create: `docs/superpowers/reports/2026-08-05-p1-pr2-exit-gate.md`
- Modify: `docs/superpowers/specs/2026-08-05-p1-pr2-step-retry-design.md` only to record implemented clarifications found during review
- Modify: `docs/superpowers/plans/2026-08-05-p1-pr2-step-retry.md` checkbox state only after evidence exists

**Interfaces:**
- Produces final P1/PR2 evidence without exposing scheduler or worker functionality.

- [ ] **Step 1: Write end-to-end retry acceptance**

Execute:

```text
Create → MarkReady → Start
→ StartStep(max_attempts = 3), attempt 1
→ FailStep(Retry, resume_at = T), two-event commit
→ checkpoint while WaitingForRetry
→ reject ResumeStepRetry at T - 1µs
→ ResumeStepRetry at T, two-event commit
→ fail attempt 2 with retry at T2
→ restart services/reconstruct from PostgreSQL
→ resume attempt 3 at T2
→ complete attempt 3
→ complete run
→ delete projection
→ restore from checkpoint plus tail
→ rebuild identical projection
```

Assert one logical `step_id`, attempts `1, 2, 3`, exact stored due times, final event count/version, and deterministic replay equality.

- [ ] **Step 2: Add retry-limit terminal acceptance**

With `max_attempts = 2`, fail attempt 2 using retry disposition and assert one atomic batch:

```text
StepAttemptFailed(attempt = 2, retryable = true)
Stalled(RetryLimitExceeded { attempts = 2 })
```

No retry wait or third attempt may exist.

- [ ] **Step 3: Run focused and full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
bash ./scripts/foundation-doc-truth.sh
bash ./scripts/foundation-boundary-truth.sh
```

Then rely on PR CI for standalone migration, console build, and clean Docker Compose smoke/RLS acceptance.

- [ ] **Step 4: Record exit-gate evidence**

The report must list the exact final commit and CI run, and verify:

1. first attempt is `1`;
2. the same `step_id` is retained across retries;
3. retry due time survives restart/replay;
4. early resume is rejected;
5. failure/schedule and resume/start are atomic;
6. limit exhaustion stalls atomically;
7. concurrent resume produces one success;
8. checkpoint format v2 is enforced;
9. legacy step events remain replayable;
10. projection deletion/rebuild remains exact;
11. all P0/P1-PR1 gates remain green;
12. scheduler/worker/HTTP/idempotency remain deferred.

- [ ] **Step 5: Update PR description and keep it draft**

Replace design-only status with delivered scope, RED→GREEN evidence, final CI run, and deferred boundaries. Do not mark ready and do not merge.

- [ ] **Step 6: Commit**

```bash
git add tests docs/superpowers
git commit -m "docs(p1): record step retry exit gate"
```
