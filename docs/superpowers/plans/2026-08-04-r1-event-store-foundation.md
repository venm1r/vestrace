# R1.2 PostgreSQL Run Event Store Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist R1.1 run event envelopes in an append-only PostgreSQL stream with ordered loading, workspace RLS, and optimistic concurrency.

**Architecture:** Evolve the existing `run_events` table in place, expose a dedicated application `RunEventStore` port, and implement `PgRunEventStore` using scoped transactions. The P0 `RunRepository` and HTTP create/list/get path remain unchanged until R1.3.

**Tech Stack:** Rust 1.85, edition 2024, Tokio, SQLx 0.8, PostgreSQL, Serde, existing workspace-scoped RLS helpers.

## Global Constraints

- Work only on `agent/r1-event-store-foundation`, stacked on `agent/r1-domain-kernel`.
- Do not change current HTTP behavior.
- Do not update `agent_runs` from events in this slice.
- Every production behavior must be preceded by a failing test.
- All database access must use explicit workspace filters and scoped transactions.
- Do not add a payload GIN index, new provider dependencies, jobs, outbox, checkpoints, or rebuild behavior.

---

### Task 1: Migration contract and legacy backfill

**Files:**
- Create: `migrations/0111_run_event_store_foundation.sql`
- Modify: `tests/migrations.rs`

**Interfaces:**
- Consumes: existing `run_events` and `agent_runs` tables from migrations 0019–0020.
- Produces: envelope columns, integrity constraints, composite workspace/run ownership, and ordered-read index.

- [ ] **Step 1: Write failing migration tests**

Add SQLx tests that assert:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn run_events_expose_the_r1_envelope_columns(pool: sqlx::PgPool) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'run_events'
           AND column_name IN ('event_version','actor','causation_id','correlation_id','occurred_at')
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(columns, vec!["actor", "causation_id", "correlation_id", "event_version", "occurred_at"]);
}
```

Also seed a legacy-shaped row before executing the migration body in a transaction-specific helper and assert deterministic backfill values. Add constraint tests for zero sequence, blank event type, zero event version, and workspace/run mismatch.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test --test migrations run_events_ -- --nocapture
```

Expected: tests fail because the new columns and constraints do not exist.

- [ ] **Step 3: Implement migration 0111**

The migration must:

```sql
ALTER TABLE run_events
    ADD COLUMN IF NOT EXISTS event_version SMALLINT,
    ADD COLUMN IF NOT EXISTS actor JSONB,
    ADD COLUMN IF NOT EXISTS causation_id UUID,
    ADD COLUMN IF NOT EXISTS correlation_id UUID,
    ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ;

UPDATE run_events
SET event_version = COALESCE(event_version, 1),
    actor = COALESCE(actor, jsonb_build_object('system', jsonb_build_object('component', 'legacy_migration'))),
    causation_id = COALESCE(causation_id, id),
    correlation_id = COALESCE(correlation_id, run_id),
    occurred_at = COALESCE(occurred_at, created_at);
```

Then set all five columns `NOT NULL`, add named checks, add a unique `(workspace_id, id)` constraint to `agent_runs` if absent, replace the old event sequence unique constraint with `(workspace_id, run_id, sequence)`, and add the composite foreign key `(workspace_id, run_id) REFERENCES agent_runs(workspace_id, id) ON DELETE CASCADE`.

- [ ] **Step 4: Run focused migration tests and verify GREEN**

Run the same focused test command. Expected: all `run_events_` migration tests pass.

- [ ] **Step 5: Commit**

```bash
git add migrations/0111_run_event_store_foundation.sql tests/migrations.rs
git commit -m "feat(r1): evolve run event storage schema"
```

---

### Task 2: Application event-store port

**Files:**
- Modify: `crates/vestrace-application/src/runs/ports.rs`
- Modify: `crates/vestrace-application/src/runs/mod.rs` only if a focused module is introduced
- Create: `crates/vestrace-application/tests/run_event_store_contract.rs`

**Interfaces:**
- Consumes: `RequestContext`, `AgentRunId`, `RunVersion`, `RunEventEnvelope`, `ApplicationError`.
- Produces: `RunEventStore` and `SharedRunEventStore`.

- [ ] **Step 1: Write a compile-contract test**

Define a minimal in-memory implementation in the integration test and assert the exact async trait methods compile:

```rust
#[async_trait]
impl RunEventStore for MemoryStore {
    async fn load_stream(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError> {
        Ok(Vec::new())
    }

    async fn append(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        _expected_version: RunVersion,
        events: &[RunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError> {
        events.last().map(|event| event.sequence).ok_or_else(|| ApplicationError::Conflict("empty event batch".into()))
    }
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p vestrace-application --test run_event_store_contract
```

Expected: compile failure because `RunEventStore` is absent.

- [ ] **Step 3: Add the minimal trait and shared alias**

Add the exact methods from the design and:

```rust
pub type SharedRunEventStore = Arc<dyn RunEventStore>;
```

- [ ] **Step 4: Verify GREEN and commit**

Run the focused test, then:

```bash
git add crates/vestrace-application/src/runs/ports.rs crates/vestrace-application/tests/run_event_store_contract.rs
git commit -m "feat(r1): define run event store port"
```

---

### Task 3: PostgreSQL row codec and ordered loading

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/run_event_store.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/run_event_store.rs`

**Interfaces:**
- Consumes: `RunEventStore`, `PgStore::begin_scoped`, and R1.1 event envelope types.
- Produces: `PgRunEventStore::new(PgStore)` and ordered `load_stream`.

- [ ] **Step 1: Write failing round-trip tests**

Use `#[sqlx::test(migrations = "../../migrations")]` or the repository's established test path. Seed workspace, principal, and owning `agent_runs` row. Insert two fully populated event rows out of retrieval order, call `load_stream`, and assert:

- sequence order is 1 then 2;
- actor, payload, event type/version, causation/correlation, occurred/recorded timestamps round-trip exactly;
- an empty stream returns `Vec::new()`.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p vestrace-infrastructure --test run_event_store load_stream -- --nocapture
```

Expected: compile failure because `PgRunEventStore` is absent.

- [ ] **Step 3: Implement row mapping and load_stream**

Create a private `RunEventRow` using `sqlx::FromRow` with JSON values for actor and payload. Convert numeric sequence with `RunVersion::new`, deserialize JSON with `serde_json::from_value`, and map failures to `ApplicationError::Storage`.

Use:

```sql
SELECT id, workspace_id, run_id, sequence, event_type, event_version,
       actor, causation_id, correlation_id, payload, occurred_at,
       created_at AS recorded_at
FROM run_events
WHERE workspace_id = $1 AND run_id = $2
ORDER BY sequence ASC
```

Execute through `PgStore::begin_scoped`, commit only after all rows decode, and export `PgRunEventStore` from `postgres::mod`.

- [ ] **Step 4: Verify GREEN and commit**

Run the focused test, then commit the adapter and tests.

---

### Task 4: Batch validation and optimistic append

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/run_event_store.rs`
- Modify: `crates/vestrace-infrastructure/tests/run_event_store.rs`

**Interfaces:**
- Consumes: `RunEventEnvelope` batches and `expected_version`.
- Produces: atomic append returning final `RunVersion`.

- [ ] **Step 1: Write failing behavior tests**

Add independent tests for:

- first append at expected version zero;
- second contiguous append;
- stale expected version returns `ApplicationError::Conflict`;
- empty batch is rejected;
- mixed workspace/run identity is rejected;
- first event sequence must equal expected version plus one;
- a gap inside a batch is rejected;
- failed batch writes zero rows.

- [ ] **Step 2: Verify RED**

Run the focused append tests. Expected: failures because append is not implemented.

- [ ] **Step 3: Implement in-memory batch validation**

Before opening or before mutating the transaction, verify:

```rust
if events.is_empty() { return Err(ApplicationError::Conflict("empty event batch".into())); }
```

Every event must match the context workspace and requested run, and sequences must be contiguous from `expected_version.next()`.

- [ ] **Step 4: Implement serialized optimistic append**

Within a scoped transaction:

```sql
SELECT id
FROM agent_runs
WHERE workspace_id = $1 AND id = $2
FOR UPDATE
```

Then:

```sql
SELECT COALESCE(MAX(sequence), 0)
FROM run_events
WHERE workspace_id = $1 AND run_id = $2
```

Compare actual and expected versions. Insert each event with all envelope fields and `created_at = recorded_at`. Commit and return the last sequence.

- [ ] **Step 5: Verify GREEN and commit**

Run all event-store tests and commit.

---

### Task 5: Concurrency and RLS boundaries

**Files:**
- Modify: `crates/vestrace-infrastructure/tests/run_event_store.rs`
- Modify: `tests/rls.rs`
- Modify: test support only where required to grant restricted runtime permissions.

**Interfaces:**
- Consumes: completed `PgRunEventStore` and restricted-role helpers.
- Produces: proof that only one concurrent writer claims a version and another workspace cannot read or append.

- [ ] **Step 1: Write concurrent append test**

Start two tasks against the same run and expected version. Both batches propose the same next sequence with different event IDs. Release them through a barrier. Assert exactly one succeeds, one returns conflict, and the stream has one new event.

- [ ] **Step 2: Verify RED or expose missing serialization**

Run the concurrency test. It must fail before the owning-run row lock is correct.

- [ ] **Step 3: Add restricted-role RLS tests**

Under the runtime role:

- workspace B sees zero events from workspace A;
- workspace B cannot insert an event claiming workspace A;
- runtime role cannot update or delete an event;
- workspace A can select and insert its own event when the owning run exists.

- [ ] **Step 4: Make the minimal permission or transaction corrections**

Do not add application-level bypasses. Correct migration grants or repository transaction behavior only.

- [ ] **Step 5: Verify GREEN and commit**

Run event-store and RLS tests, then commit.

---

### Task 6: Full regression verification and PR evidence

**Files:**
- Modify: `docs/superpowers/specs/2026-08-04-r1-event-store-foundation-design.md` only if implementation decisions changed.
- Modify: draft PR body.

**Interfaces:**
- Consumes: all tasks above.
- Produces: verified stacked draft PR with exact scope and deferred work.

- [ ] **Step 1: Run formatting and lint**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- [ ] **Step 2: Run the complete Rust suite**

```bash
cargo test --workspace --all-targets --all-features
```

- [ ] **Step 3: Run repository acceptance checks**

Use the existing CI matrix to verify console build, Compose readiness, foundation smoke, create/list/get behavior, workspace isolation, and runtime RLS.

- [ ] **Step 4: Compare scope against the design**

Confirm no HTTP, projection, command-idempotency, checkpoint, rebuild, job, provider, or agent-loop changes were introduced.

- [ ] **Step 5: Update draft PR body**

Record RED evidence, final verification commands, schema decisions, and explicitly deferred R1.3 work.
