# P1 State Engine Recovery Vertical Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make run events independently durable, add validated checkpoints, restore state from checkpoint plus tail events, and rebuild a deleted projection identically.

**Architecture:** Introduce `run_streams` as the durable stream identity and optimistic concurrency boundary. Keep `run_events` authoritative, store immutable `run_checkpoints` as derived snapshots, and make `agent_runs` a replaceable projection. Application recovery logic remains SQL-free and uses one PostgreSQL recovery port.

**Tech Stack:** Rust 1.85, async-trait, Serde/serde_json, SHA-256 via `sha2`, SQLx 0.8, PostgreSQL 17, Axum workspace integration tests, GitHub Actions.

## Global Constraints

- Base the draft PR on `agent/p0-restore-main` and integrate R1.1–R1.3 from `agent/r1-projection-integration`.
- `run_events` remain canonical and append-only.
- Checkpoints and `agent_runs` are derived and may never override events.
- Every transition continues through `RunCommandService` and the pure domain reducer.
- Preserve workspace RLS and sanitized storage errors.
- Do not implement HTTP command routing, persisted idempotency, P1 PR2+, capabilities, CAS, workers, memory, or UI.
- Use TDD: each production boundary is introduced by a focused failing test.

---

### Task 1: Decouple stream authority from the projection

**Files:**
- Create: `migrations/0112_run_streams_and_checkpoints.sql`
- Create: `tests/run_stream_recovery_migrations.rs`
- Modify: `tests/support/mod.rs` only if a reusable seed/helper is required

**Interfaces:**
- Produces PostgreSQL tables `run_streams` and `run_checkpoints`.
- Produces the invariant that deleting `agent_runs` never deletes `run_events` or `run_checkpoints`.
- Later tasks rely on `(workspace_id, run_id)` as the stable stream key.

- [ ] **Step 1: Write migration RED tests**

Add SQLx tests that assert:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn projection_deletion_preserves_authoritative_stream(pool: sqlx::PgPool) {
    // seed workspace, principal, agent_runs, run_events, run_streams, checkpoint
    sqlx::query("DELETE FROM agent_runs WHERE workspace_id = $1 AND id = $2")
        .bind(workspace_id)
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(count(&pool, "run_streams").await, 1);
    assert_eq!(count(&pool, "run_events").await, 1);
    assert_eq!(count(&pool, "run_checkpoints").await, 1);
}
```

Also assert:

- `run_events_workspace_run_fkey` references `run_streams`;
- checkpoint format/hash constraints reject invalid rows;
- runtime role cannot update/delete events or checkpoints;
- RLS hides foreign-workspace stream/checkpoint rows.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test --test run_stream_recovery_migrations -- --nocapture
```

Expected: FAIL because `run_streams`, `run_checkpoints`, and the replacement foreign keys do not exist.

- [ ] **Step 3: Implement migration 0112**

Create `run_streams`, backfill from `agent_runs`, and choose the head as:

```sql
COALESCE(MAX(run_events.sequence), 0)
```

Legacy projection-only rows therefore receive stream version `0`; their non-event-sourced projection version is not promoted to canonical stream history.

Then:

- drop `run_events_workspace_run_fkey` to `agent_runs`;
- add the composite FK to `run_streams`;
- add a non-cascading derived-child FK from `agent_runs` to `run_streams`;
- create `run_checkpoints` with format version `1` and lowercase 64-character hash constraint;
- enable/force RLS for both new tables;
- create workspace policies matching existing `vestrace.workspace_id` session context;
- grant runtime `SELECT/INSERT/UPDATE` on `run_streams`, `SELECT/INSERT` on checkpoints, and no runtime `DELETE`.

- [ ] **Step 4: Run migration and existing database tests**

Run:

```bash
cargo test --test run_stream_recovery_migrations -- --nocapture
cargo test --test run_event_store_migrations -- --nocapture
cargo test --test migrations -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add migrations/0112_run_streams_and_checkpoints.sql tests/run_stream_recovery_migrations.rs tests/support/mod.rs
git commit -m "feat: decouple run streams from projections"
```

---

### Task 2: Add typed checkpoint and recovery application logic

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/vestrace-application/Cargo.toml`
- Create: `crates/vestrace-application/src/runs/projection.rs`
- Create: `crates/vestrace-application/src/runs/recovery.rs`
- Modify: `crates/vestrace-application/src/runs/mod.rs`
- Modify: `crates/vestrace-application/src/runs/ports.rs`
- Modify: `crates/vestrace-application/src/runs/command_service.rs`
- Create: `crates/vestrace-application/tests/run_recovery_service.rs`

**Interfaces:**
- Produces:

```rust
pub const RUN_CHECKPOINT_FORMAT_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunCheckpoint {
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub sequence: RunVersion,
    pub format_version: u16,
    pub state_hash: String,
    pub state: RunState,
    pub created_at: Timestamp,
}

pub fn project_run(state: &RunState) -> AgentRun;
pub fn hash_run_state(state: &RunState) -> Result<String, ApplicationError>;
```

- Produces `RunRecoveryStore`, `RunRecoveryOperations`, and `RunRecoveryService`.

- [ ] **Step 1: Write application RED tests**

Use an in-memory `RunRecoveryStore` double and assert:

```rust
let checkpoint = service.create_checkpoint(&context, run_id).await.unwrap();
assert_eq!(checkpoint.sequence, RunVersion::new(3).unwrap());
assert_eq!(checkpoint.state_hash, hash_run_state(&checkpoint.state).unwrap());
```

Add tests for:

- restore without a checkpoint equals full `replay`;
- restore with checkpoint plus tail equals full `replay`;
- altered checkpoint state fails local hash validation;
- checkpoint state differing from authoritative prefix fails validation;
- stream head greater than restored final version fails;
- `project_run` output is identical for command and recovery paths.

- [ ] **Step 2: Run application tests and verify RED**

Run:

```bash
cargo test -p vestrace-application --test run_recovery_service -- --nocapture
```

Expected: compile failure for absent recovery types, traits, service, and projection mapper.

- [ ] **Step 3: Add SHA-256 and shared projection mapping**

Add:

```toml
sha2 = "0.10"
```

at workspace level and `sha2.workspace = true` in application.

Implement deterministic lowercase hex without adding a second encoding dependency:

```rust
pub fn hash_run_state(state: &RunState) -> Result<String, ApplicationError> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(state)
        .map_err(|_| ApplicationError::Internal("run state serialization failed".to_owned()))?;
    Ok(Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect())
}
```

Move the existing private `project_run` body into `projection.rs` and call it from `RunCommandService`.

- [ ] **Step 4: Implement recovery ports and service**

Define:

```rust
#[async_trait]
pub trait RunRecoveryStore: Send + Sync {
    async fn load_stream_head(... ) -> Result<Option<RunVersion>, ApplicationError>;
    async fn load_events_through(... ) -> Result<Vec<RunEventEnvelope>, ApplicationError>;
    async fn load_events_after(... ) -> Result<Vec<RunEventEnvelope>, ApplicationError>;
    async fn load_latest_checkpoint(... ) -> Result<Option<RunCheckpoint>, ApplicationError>;
    async fn save_checkpoint(... ) -> Result<(), ApplicationError>;
    async fn replace_projection(... ) -> Result<(), ApplicationError>;
}
```

Define `RunRecoveryOperations` with `create_checkpoint`, `validate_checkpoint`, `restore`, and `rebuild_projection`.

Local checkpoint validation must check format, IDs, version, and hash before any tail event is applied. Authoritative validation must replay the prefix and compare exact `RunState` equality.

- [ ] **Step 5: Run application tests**

Run:

```bash
cargo test -p vestrace-application --test run_recovery_service -- --nocapture
cargo test -p vestrace-application --all-targets --all-features
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/vestrace-application
git commit -m "feat: add typed run recovery service"
```

---

### Task 3: Implement PostgreSQL recovery storage

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/run_recovery_store.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/run_recovery_store.rs`

**Interfaces:**
- Produces `PgRunRecoveryStore::new(PgStore) -> PgRunRecoveryStore`.
- Implements every `RunRecoveryStore` method with scoped transactions.

- [ ] **Step 1: Write PostgreSQL adapter RED tests**

Add tests for:

- typed checkpoint save/load round-trip;
- identical duplicate checkpoint is idempotent;
- same sequence with different hash/state returns `ApplicationError::Conflict`;
- `load_events_through` and `load_events_after` return contiguous ordered ranges;
- projection replacement inserts a missing row;
- projection replacement rejects a stale expected stream version;
- foreign workspace cannot observe or replace the projection.

- [ ] **Step 2: Run adapter tests and verify RED**

Run:

```bash
cargo test --test run_recovery_store -- --nocapture
```

Expected: compile failure for absent `PgRunRecoveryStore`.

- [ ] **Step 3: Implement row mapping and checkpoint persistence**

Use a stored row containing `serde_json::Value` and deserialize into `RunState`. Reject unsupported formats, invalid IDs/version, and hash mismatch as sanitized storage errors.

Use `INSERT ... ON CONFLICT DO NOTHING`; when zero rows are inserted, load the existing row and return success only when all immutable fields match.

- [ ] **Step 4: Implement guarded projection replacement**

In one scoped transaction:

```text
SELECT current_version FROM run_streams ... FOR UPDATE
→ compare with expected_stream_version
→ INSERT agent_runs ...
   ON CONFLICT (workspace_id, id) DO UPDATE ...
→ commit
```

The upsert must write all derived projection fields and preserve the state-derived timestamps exactly.

- [ ] **Step 5: Run adapter and RLS tests**

Run:

```bash
cargo test --test run_recovery_store -- --nocapture
cargo test --test run_event_store_rls -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vestrace-infrastructure/src/postgres tests/run_recovery_store.rs
git commit -m "feat: persist run checkpoints and rebuild projections"
```

---

### Task 4: Move optimistic concurrency to `run_streams`

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/run_event_store.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run_command_committer.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run_repository.rs`
- Modify: `tests/run_event_store_append.rs`
- Modify: `tests/run_command_committer.rs`
- Create: `tests/run_projection_independence.rs`

**Interfaces:**
- `PgRunEventStore::append` and `PgRunCommandCommitter::commit` use `run_streams.current_version` as the optimistic head.
- Command commits upsert the projection and work when it is absent.
- Legacy `RunRepository::create` creates a version-0 stream and projection atomically, without fabricating events.

- [ ] **Step 1: Write concurrency/projection RED tests**

Add tests that:

- delete `agent_runs` after a valid event stream exists, then commit the next typed transition and assert projection recreation;
- run two command commits with the same expected stream version and assert exactly one success;
- assert the winner advances both event head and `run_streams.current_version` atomically;
- assert a failed event insert leaves stream head and projection unchanged;
- assert legacy repository creation creates `run_streams.current_version = 0` and no events.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test --test run_projection_independence -- --nocapture
cargo test --test run_command_committer -- --nocapture
cargo test --test run_event_store_append -- --nocapture
```

Expected: failures caused by locking/foreign-key dependence on `agent_runs`.

- [ ] **Step 3: Refactor event append**

For expected version zero, insert the stream row before events. For an existing stream, lock `run_streams` and compare `current_version`. Insert the entire event batch, then update the stream head in the same transaction.

Reject a stream with events whose maximum sequence disagrees with `current_version` as storage corruption.

- [ ] **Step 4: Refactor canonical command commit**

Remove projection-row ownership from concurrency. Validate existing events against the locked stream head, append the new batch, advance the stream head, and upsert the supplied event-derived projection.

A missing projection is not an error. A projection whose stored version/state disagrees with the event replay may be replaced only by the newly derived state after the authoritative stream itself validates.

- [ ] **Step 5: Update legacy repository create**

Within one transaction:

```text
insert run_streams(current_version = 0)
→ insert agent_runs projection
→ commit
```

Duplicate stream/projection identity maps to conflict/storage behavior consistent with current repository contracts.

- [ ] **Step 6: Run all affected tests**

Run:

```bash
cargo test --test run_projection_independence -- --nocapture
cargo test --test run_command_committer -- --nocapture
cargo test --test run_event_store_append -- --nocapture
cargo test --test run_event_store -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-infrastructure tests
git commit -m "refactor: make run streams own optimistic concurrency"
```

---

### Task 5: Prove checkpoint replay and projection rebuild end to end

**Files:**
- Create: `tests/run_state_engine_recovery.rs`
- Modify: `docs/architecture.md`
- Modify: `docs/superpowers/reports/2026-08-05-p0-exit-gate.md` only if wording must distinguish completed P0 evidence from the new stacked P1 work; do not rewrite P0 results
- Modify: `.github/workflows/ci.yml` only when a separate recovery acceptance command is needed
- Create: `docs/superpowers/reports/2026-08-05-p1-pr1-exit-gate.md`

**Interfaces:**
- Produces the mandatory P1/PR1 acceptance evidence.

- [ ] **Step 1: Write the end-to-end RED acceptance**

The test must:

```text
Create run through RunCommandService
→ MarkReady
→ Start
→ create checkpoint at version 3
→ StartStep
→ CompleteStep
→ Complete run
→ save expected final RunState and AgentRun
→ DELETE agent_runs only
→ assert events/checkpoint/stream still exist
→ validate checkpoint
→ restore checkpoint plus tail
→ rebuild projection
→ load rebuilt projection
→ assert exact equality with expected state/projection
```

Also add corruption cases:

- modify checkpoint JSON through the privileged test connection and expect hash failure;
- create a sequence gap or invalid event type in an isolated test database and expect explicit replay/storage failure rather than projection fallback.

- [ ] **Step 2: Run the acceptance and verify RED**

Run:

```bash
cargo test --test run_state_engine_recovery -- --nocapture
```

Expected: FAIL until all recovery wiring is complete.

- [ ] **Step 3: Complete integration wiring**

Export constructors and shared aliases needed by the test. Do not add an HTTP or CLI recovery surface in this slice.

- [ ] **Step 4: Run the complete verification matrix**

Run:

```bash
cargo fmt --all --check
bash ./scripts/foundation-doc-truth.sh
bash ./scripts/foundation-boundary-truth.sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
npm --prefix console ci
npm --prefix console run typecheck
npm --prefix console run build
docker compose config --quiet
docker compose up --build --detach --wait --wait-timeout 180
./scripts/foundation-smoke.sh
bash ./scripts/foundation-run-smoke.sh
timeout 90s ./scripts/foundation-runtime-rls.sh
docker compose down --volumes --remove-orphans
```

Expected: all commands succeed.

- [ ] **Step 5: Update architecture truthfully**

Document:

- `run_streams` as stream identity/concurrency metadata;
- `run_events` as authority;
- checkpoints and `agent_runs` as derived;
- P1/PR1 maturity only, without claiming pause/retry/subagent/compensation readiness.

- [ ] **Step 6: Write the P1/PR1 exit-gate report**

Record exact branch/head, migration, test scenarios, GitHub Actions run ID, and deferred scope. Do not mark P1 overall complete; only P1 / PR1 Typed Core is complete.

- [ ] **Step 7: Commit**

```bash
git add tests/run_state_engine_recovery.rs docs .github/workflows/ci.yml
git commit -m "test: prove state engine checkpoint recovery vertical"
```

---

## Plan self-review

- Spec coverage: stream authority, checkpoints, validation, replay, rebuild, optimistic concurrency, RLS, corruption detection, and acceptance each have a dedicated task.
- Scope: HTTP idempotency and later P1 phases are explicitly excluded.
- Type consistency: `RunCheckpoint`, `RunRecoveryStore`, `RunRecoveryOperations`, `RunRecoveryService`, `PgRunRecoveryStore`, `project_run`, and `hash_run_state` use one spelling throughout.
- No placeholders: every task has concrete files, interfaces, RED command, implementation boundary, GREEN command, and commit.
