# Memory Write Integrity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every authoritative v0.1 memory write atomic, policy-aware, workspace-scoped, and idempotent before introducing the Persistent Cognition data model.

**Architecture:** Keep the existing public `MemoryUseCases` contract, `Memory`, `MemoryRevision`, HTTP routes, and MCP tools intact. Replace the current multi-repository autocommit write path with an application-level `MemoryUnitOfWork` opened by `MemoryUnitOfWorkManager`; the PostgreSQL implementation wraps the existing `PgStore::begin_scoped()` transaction and executes event, memory, revision, provenance, relation, outbox, and idempotency writes through one `PgConnection`. Reads used only for `find_memory` remain on the pool-backed `MemoryRepository`.

**Tech Stack:** Rust 2024, Tokio, async-trait, SQLx 0.8, PostgreSQL, existing RLS session variables, existing outbox/idempotency schema, existing domain `MemoryWritePolicy`.

## Global Constraints

- Vestrace remains a cognition-centered modular monolith; do not introduce microservices or a new event bus.
- PostgreSQL is authoritative.
- Existing migrations remain immutable; this plan requires no schema migration.
- `MemoryUseCases` public behavior and current HTTP/MCP request shapes remain compatible.
- All authoritative memory writes run inside a workspace/principal-scoped PostgreSQL transaction.
- `MemoryWritePolicy` is enforced: `AutoActivate -> Active`, `RequireApproval -> Candidate`, `DiscardCandidate -> Rejected`.
- Provenance is persisted before any memory may become `Active`.
- `RememberMemoryCommand.evidence_role` is honored instead of being silently replaced with `DirectSource`.
- Idempotency lookup and idempotency record insertion are part of the same transaction as the authoritative write.
- Outbox insertion is part of the same transaction as the authoritative write.
- Failed commands leave no partial memory, revision, source, relation, outbox, or idempotency state.
- Keep hard purge on its existing explicit administrative path; do not broaden this plan into purge redesign.

---

## File Structure

Create:

```text
crates/vestrace-application/src/memory/unit_of_work.rs
crates/vestrace-infrastructure/src/postgres/memory_unit_of_work.rs
crates/vestrace-application/tests/memory_write_integrity.rs
```

Modify:

```text
crates/vestrace-application/src/memory/mod.rs
crates/vestrace-application/src/memory/ports.rs
crates/vestrace-application/src/memory/services.rs
crates/vestrace-infrastructure/src/postgres/mod.rs
crates/vestrace-infrastructure/src/postgres/memory_repository.rs
crates/vestrace-cli/src/commands/server.rs
crates/vestrace-cli/src/commands/mcp.rs
tests/memory_lifecycle.rs
docs/domain-model.md
docs/architecture.md
```

The new unit-of-work file owns only transaction-scoped memory write interfaces. Do not move unrelated repositories or run-state transaction code into it.

---

### Task 1: Define the memory transaction boundary

**Files:**
- Create: `crates/vestrace-application/src/memory/unit_of_work.rs`
- Modify: `crates/vestrace-application/src/memory/mod.rs`
- Modify: `crates/vestrace-application/src/memory/ports.rs`
- Test: `crates/vestrace-application/tests/memory_write_integrity.rs`

**Interfaces:**
- Consumes: existing `RequestContext`, `IdempotencyRecord`, `OutboxMessage`, `Event`, `Memory`, `MemoryRevision`, `MemorySource`, `KnowledgeRelation`.
- Produces: `MemoryUnitOfWork`, `MemoryUnitOfWorkManager`; read-only `MemoryRepository` remains available for `find_memory`.

- [ ] **Step 1: Write the compile-time fake unit-of-work test**

Create `crates/vestrace-application/tests/memory_write_integrity.rs` with a minimal fake that proves the intended interface can hold all objects needed by a single memory transaction:

```rust
use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, IdempotencyRecord, MemoryUnitOfWork, MemoryUnitOfWorkManager,
    OutboxMessage, RequestContext,
};
use vestrace_domain::{
    Event, KnowledgeRelation, Memory, MemoryRevision, MemorySource,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
};

struct FakeUnitOfWork;

#[async_trait]
impl MemoryUnitOfWork for FakeUnitOfWork {
    async fn find_idempotency(
        &mut self,
        _workspace_id: WorkspaceId,
        _key: &str,
    ) -> Result<Option<IdempotencyRecord>, ApplicationError> {
        Ok(None)
    }

    async fn insert_event(&mut self, _event: &Event) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_memory(&mut self, _memory: &Memory) -> Result<(), ApplicationError> { Ok(()) }
    async fn update_memory(&mut self, _memory: &Memory) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_revision(&mut self, _revision: &MemoryRevision) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_source(&mut self, _source: &MemorySource) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_relation(&mut self, _relation: &KnowledgeRelation) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_outbox(&mut self, _message: &OutboxMessage) -> Result<(), ApplicationError> { Ok(()) }
    async fn insert_idempotency(&mut self, _record: &IdempotencyRecord) -> Result<(), ApplicationError> { Ok(()) }

    async fn load_memory_for_update(
        &mut self,
        _id: MemoryId,
    ) -> Result<Option<Memory>, ApplicationError> {
        Ok(None)
    }

    async fn find_revision(
        &mut self,
        _id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError> {
        Ok(None)
    }

    async fn commit(self: Box<Self>) -> Result<(), ApplicationError> { Ok(()) }
    async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> { Ok(()) }
}

struct FakeManager;

#[async_trait]
impl MemoryUnitOfWorkManager for FakeManager {
    async fn begin(
        &self,
        _context: &RequestContext,
    ) -> Result<Box<dyn MemoryUnitOfWork>, ApplicationError> {
        Ok(Box::new(FakeUnitOfWork))
    }
}
```

- [ ] **Step 2: Run the test and verify it fails because the interfaces do not exist**

Run:

```bash
cargo test -p vestrace-application --test memory_write_integrity
```

Expected: compile failure for unresolved imports `MemoryUnitOfWork` and `MemoryUnitOfWorkManager`.

- [ ] **Step 3: Add the exact application interfaces**

Create `crates/vestrace-application/src/memory/unit_of_work.rs`:

```rust
use async_trait::async_trait;
use vestrace_domain::{
    Event, KnowledgeRelation, Memory, MemoryRevision, MemorySource,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
};

use crate::{ApplicationError, IdempotencyRecord, OutboxMessage, RequestContext};

#[async_trait]
pub trait MemoryUnitOfWork: Send {
    async fn find_idempotency(
        &mut self,
        workspace_id: WorkspaceId,
        key: &str,
    ) -> Result<Option<IdempotencyRecord>, ApplicationError>;

    async fn insert_event(&mut self, event: &Event) -> Result<(), ApplicationError>;
    async fn insert_memory(&mut self, memory: &Memory) -> Result<(), ApplicationError>;
    async fn update_memory(&mut self, memory: &Memory) -> Result<(), ApplicationError>;
    async fn insert_revision(&mut self, revision: &MemoryRevision) -> Result<(), ApplicationError>;
    async fn insert_source(&mut self, source: &MemorySource) -> Result<(), ApplicationError>;
    async fn insert_relation(&mut self, relation: &KnowledgeRelation) -> Result<(), ApplicationError>;
    async fn insert_outbox(&mut self, message: &OutboxMessage) -> Result<(), ApplicationError>;
    async fn insert_idempotency(&mut self, record: &IdempotencyRecord) -> Result<(), ApplicationError>;

    async fn load_memory_for_update(
        &mut self,
        id: MemoryId,
    ) -> Result<Option<Memory>, ApplicationError>;

    async fn find_revision(
        &mut self,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError>;

    async fn commit(self: Box<Self>) -> Result<(), ApplicationError>;
    async fn rollback(self: Box<Self>) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait MemoryUnitOfWorkManager: Send + Sync {
    async fn begin(
        &self,
        context: &RequestContext,
    ) -> Result<Box<dyn MemoryUnitOfWork>, ApplicationError>;
}
```

Export these types from `memory/mod.rs` and the crate root using the existing re-export style.

Change `MemoryRepository` in `memory/ports.rs` to the read-only contract:

```rust
#[async_trait]
pub trait MemoryRepository: Send + Sync {
    async fn find_memory_by_id(&self, id: MemoryId) -> Result<Option<Memory>, ApplicationError>;
    async fn find_revision_by_id(
        &self,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError>;
}
```

Do not alter `EventRepository`, `ProvenanceRepository`, `RelationRepository`, `OutboxRepository`, or `IdempotencyRepository`; they may still be used by non-memory code.

- [ ] **Step 4: Run application tests**

Run:

```bash
cargo test -p vestrace-application
```

Expected: PASS after updating any compile-only fakes affected by the read-only `MemoryRepository` split.

- [ ] **Step 5: Commit**

```bash
git add crates/vestrace-application

git commit -m "refactor(memory): define transactional write boundary"
```

---

### Task 2: Implement `PgMemoryUnitOfWork` on the existing scoped transaction

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/memory_unit_of_work.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/memory_repository.rs`
- Test: `crates/vestrace-infrastructure/tests/postgres.rs`

**Interfaces:**
- Consumes: `MemoryUnitOfWork`, `MemoryUnitOfWorkManager`, `PgStore::begin_scoped`, `PgScopedTransaction::connection`.
- Produces: `PgMemoryUnitOfWorkManager` and transaction-scoped implementation of all P0 memory writes.

- [ ] **Step 1: Write an infrastructure test proving all writes share one rollback boundary**

Add a PostgreSQL test that:

1. creates a workspace/principal context;
2. opens `PgMemoryUnitOfWorkManager::begin`;
3. inserts a candidate `Memory` and first `MemoryRevision`;
4. intentionally inserts a `MemorySource` with a nonexistent `event_id`;
5. confirms the operation errors;
6. drops/rolls back the unit of work;
7. verifies both `memories` and `memory_revisions` contain zero rows for those IDs.

Use explicit assertions:

```rust
assert_eq!(memory_count, 0);
assert_eq!(revision_count, 0);
```

- [ ] **Step 2: Run the infrastructure test and verify it fails before the implementation exists**

Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test -p vestrace-infrastructure postgres_memory_unit_of_work_rolls_back_partial_write -- --exact
```

Expected: compile failure for missing `PgMemoryUnitOfWorkManager`.

- [ ] **Step 3: Implement the manager using the existing scoped transaction**

Create:

```rust
#[derive(Clone)]
pub struct PgMemoryUnitOfWorkManager {
    store: PgStore,
}

impl PgMemoryUnitOfWorkManager {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

pub struct PgMemoryUnitOfWork {
    tx: PgScopedTransaction,
}
```

`MemoryUnitOfWorkManager::begin()` must call only:

```rust
let tx = self.store.begin_scoped(context).await.map_err(storage_error)?;
Ok(Box::new(PgMemoryUnitOfWork { tx }))
```

Every SQL statement in this type must execute against:

```rust
self.tx.connection()
```

Never use a stored `PgPool` from this unit-of-work implementation.

- [ ] **Step 4: Implement transaction-scoped idempotency reads/writes**

Use the existing columns exactly:

```sql
SELECT idempotency_key, workspace_id, request_hash, response_payload,
       status, created_at, expires_at
FROM idempotency_keys
WHERE workspace_id = $1 AND idempotency_key = $2
FOR UPDATE
```

Insert with a normal `INSERT`, not `ON CONFLICT DO NOTHING`; if a concurrent transaction wins the unique key race, map PostgreSQL unique violation to `ApplicationError::Conflict`.

- [ ] **Step 5: Implement transaction-scoped memory operations**

`insert_memory` uses a plain `INSERT` and is used for the initial `Candidate` row.

`update_memory` uses:

```sql
UPDATE memories
SET status = $3,
    active_revision_id = $4,
    updated_at = $5
WHERE id = $1 AND workspace_id = $2
```

Require `rows_affected() == 1`; otherwise return `ApplicationError::Storage("memory update affected unexpected row count".into())`.

`load_memory_for_update` uses:

```sql
SELECT id, workspace_id, kind, status, active_revision_id, created_at, updated_at
FROM memories
WHERE id = $1
FOR UPDATE
```

and reuses/extracts the existing row parsing logic from `memory_repository.rs` rather than creating divergent enum parsing.

- [ ] **Step 6: Implement revision, event, source, relation, and outbox writes**

Copy the current SQL column sets exactly from the existing PostgreSQL repositories, but execute every query on `self.tx.connection()`.

Do not delete the existing repositories; this task only makes the memory service stop depending on their autocommit write methods.

- [ ] **Step 7: Implement commit and rollback**

```rust
async fn commit(self: Box<Self>) -> Result<(), ApplicationError> {
    self.tx.commit().await.map_err(storage_error)
}

async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> {
    self.tx.rollback().await.map_err(storage_error)
}
```

- [ ] **Step 8: Remove obsolete write methods from `PgMemoryRepository`**

Delete `save_memory` and `save_revision` from the `MemoryRepository` impl. Keep `find_memory_by_id` and `find_revision_by_id` behavior unchanged.

- [ ] **Step 9: Run infrastructure tests**

Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test -p vestrace-infrastructure
```

Expected: PASS, including the new rollback test.

- [ ] **Step 10: Commit**

```bash
git add crates/vestrace-infrastructure

git commit -m "feat(memory): add postgres memory unit of work"
```

---

### Task 3: Make `remember_memory` transactional and enforce write policy

**Files:**
- Modify: `crates/vestrace-application/src/memory/services.rs`
- Test: `crates/vestrace-application/tests/memory_write_integrity.rs`
- Test: `tests/memory_lifecycle.rs`

**Interfaces:**
- Consumes: `MemoryRepository`, `MemoryUnitOfWorkManager`, `MemoryWritePolicy::evaluate`, `ActivationDecision`.
- Produces: unchanged `MemoryUseCases::remember_memory` external signature with correct policy semantics.

- [ ] **Step 1: Add three failing service tests for policy outcomes**

Using a fake unit of work that records inserted/updated domain values, test:

```rust
Automatic + confidence 0.90 -> MemoryStatus::Active
Manual    + confidence 0.90 -> MemoryStatus::Candidate
Automatic + confidence 0.20 -> MemoryStatus::Rejected
```

Also assert that the saved `MemorySource.role` equals `cmd.evidence_role` for a non-default role such as `EvidenceRole::SupportingContext`.

- [ ] **Step 2: Run the tests and verify current behavior fails**

Run:

```bash
cargo test -p vestrace-application --test memory_write_integrity
```

Expected: FAIL because current `MemoryService` always activates and always creates a direct source.

- [ ] **Step 3: Reduce `MemoryService` dependencies**

Change the service shape to:

```rust
pub struct MemoryService<M, U> {
    memory_repo: M,
    unit_of_work_manager: U,
}
```

with bounds:

```rust
M: MemoryRepository,
U: MemoryUnitOfWorkManager,
```

`find_memory` continues to use `memory_repo`. All mutating use cases open a unit of work.

- [ ] **Step 4: Move idempotency lookup inside the unit of work**

Add a helper taking `&mut dyn MemoryUnitOfWork`:

```rust
async fn check_idempotency_in_uow(
    uow: &mut dyn MemoryUnitOfWork,
    workspace_id: WorkspaceId,
    key: &str,
    request_hash: &str,
) -> Result<Option<serde_json::Value>, ApplicationError>
```

If the key exists with the same hash, deserialize the cached response, rollback the read-only unit of work, and return it. If the hash differs, rollback and return `ApplicationError::Conflict("idempotency key reused with different request".to_owned())`.

- [ ] **Step 5: Implement the exact create ordering**

Inside `remember_memory`:

```text
BEGIN scoped transaction
  -> idempotency check
  -> insert Memory(Candidate, active_revision_id=None)
  -> insert MemoryRevision #1
  -> insert MemorySource using cmd.evidence_role
  -> evaluate cmd.policy
  -> update Memory to Active | Candidate | Rejected
  -> insert outbox row
  -> insert idempotency record with final Memory response
COMMIT
```

For policy application:

```rust
let decision = cmd.policy.evaluate(cmd.kind, cmd.confidence);
let final_memory = match decision {
    ActivationDecision::AutoActivate => memory.activate(rev_id, at)?,
    ActivationDecision::RequireApproval => memory,
    ActivationDecision::DiscardCandidate => memory.reject(at)?,
};
```

Only `AutoActivate` sets `active_revision_id`.

- [ ] **Step 6: Enrich the existing outbox payload without changing the topic**

Keep topic `memory.created`. Add:

```json
{
  "status": "active|candidate|rejected",
  "activation_decision": "auto_activate|require_approval|discard_candidate"
}
```

Do not create new externally consumed topics in P0.

- [ ] **Step 7: Run tests**

```bash
cargo test -p vestrace-domain memory_lifecycle
cargo test -p vestrace-application --test memory_write_integrity
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vestrace-application tests/memory_lifecycle.rs

git commit -m "fix(memory): enforce transactional write policy"
```

---

### Task 4: Make `revise_memory`, `record_event`, and `link_knowledge` atomic

**Files:**
- Modify: `crates/vestrace-application/src/memory/services.rs`
- Test: `crates/vestrace-application/tests/memory_write_integrity.rs`

**Interfaces:**
- Consumes: transactional methods from Tasks 1-2.
- Produces: all existing `MemoryUseCases` writes now share one atomic consistency boundary.

- [ ] **Step 1: Add a failing revision-conflict test**

The fake unit of work returns an active revision with `revision_number = 3`. Call `revise_memory` with `expected_revision = 2` and assert:

```rust
assert!(matches!(
    error,
    ApplicationError::Domain(DomainError::RevisionConflict { expected: 2, current: 3 })
));
```

Assert no revision, source, outbox, or idempotency write was recorded after the conflict.

- [ ] **Step 2: Move revision reads into the transaction**

`revise_memory` must:

```text
BEGIN
  -> idempotency check
  -> load_memory_for_update(memory_id)
  -> validate workspace
  -> find active revision inside the same transaction
  -> compare expected_revision
  -> create revision N+1
  -> insert revision
  -> insert source
  -> update active_revision_id
  -> insert outbox
  -> insert idempotency
COMMIT
```

Do not read the active revision through the pool-backed repository before opening the transaction.

- [ ] **Step 3: Make `record_event` atomic**

Move event insert, outbox insert (`event.recorded`), and idempotency record into one `MemoryUnitOfWork` transaction.

- [ ] **Step 4: Make `link_knowledge` atomic**

Move relation insert, outbox insert (`memory.relation.linked`), and idempotency record into one unit of work.

- [ ] **Step 5: Add rollback tests for event and relation paths**

Configure the fake unit of work to fail on `insert_outbox`. Assert the service returns an error and never calls `commit`. For PostgreSQL integration coverage, Task 6 verifies no rows persist after a real transaction failure.

- [ ] **Step 6: Run application tests**

```bash
cargo test -p vestrace-application
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-application

git commit -m "fix(memory): make all write use cases atomic"
```

---

### Task 5: Rewire server and MCP construction to the unit-of-work manager

**Files:**
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-cli/src/commands/mcp.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`

**Interfaces:**
- Consumes: `PgMemoryRepository`, `PgMemoryUnitOfWorkManager`.
- Produces: runtime construction matching the new `MemoryService<M, U>` constructor.

- [ ] **Step 1: Export the PostgreSQL manager**

From `crates/vestrace-infrastructure/src/postgres/mod.rs`, re-export:

```rust
pub use memory_unit_of_work::PgMemoryUnitOfWorkManager;
```

and re-export from `vestrace-infrastructure` crate root using the existing postgres export pattern.

- [ ] **Step 2: Replace the six-repository memory constructor in `server.rs`**

Use:

```rust
let pool = store.pool().clone();
let memory_service = MemoryService::new(
    PgMemoryRepository::new(pool.clone()),
    PgMemoryUnitOfWorkManager::new(store.clone()),
);
```

Remove now-unused memory-write repository imports from this file.

- [ ] **Step 3: Apply the same constructor change in `mcp.rs`**

Do not change MCP tool names or result shapes.

- [ ] **Step 4: Check the workspace**

Run:

```bash
cargo check --workspace --all-targets
```

Expected: PASS with no unused imports introduced by the refactor.

- [ ] **Step 5: Commit**

```bash
git add crates/vestrace-cli crates/vestrace-infrastructure/src/postgres/mod.rs

git commit -m "refactor(wiring): use transactional memory service"
```

---

### Task 6: Add PostgreSQL acceptance tests for atomicity, policy, and idempotency

**Files:**
- Create: `tests/memory_write_integrity.rs`
- Modify: `tests/support/mod.rs` only if a reusable workspace/principal seed helper is required

**Interfaces:**
- Consumes: real `PgStore`, `PgMemoryRepository`, `PgMemoryUnitOfWorkManager`, `MemoryService`.
- Produces: P0 exit-gate tests that prove the database behavior rather than only application fakes.

- [ ] **Step 1: Add a real automatic-policy create test**

Record a source event, then call `remember_memory` with `MemoryWritePolicy::Automatic` and confidence `0.90`. Assert in SQL:

```text
memories.status = 'active'
memories.active_revision_id IS NOT NULL
memory_revisions count = 1
memory_sources count = 1
outbox topic 'memory.created' count = 1
idempotency_keys count = 1
```

- [ ] **Step 2: Add a manual-policy create test**

Use `MemoryWritePolicy::Manual`. Assert:

```text
status = 'candidate'
active_revision_id IS NULL
revision exists
source exists
```

This proves `RequireApproval` preserves the candidate and its provenance without violating the active-source trigger.

- [ ] **Step 3: Add a discarded-candidate test**

Use `Automatic` with confidence `0.20`. Assert:

```text
status = 'rejected'
active_revision_id IS NULL
revision exists
source exists
```

- [ ] **Step 4: Add an idempotent replay test**

Call the identical command twice with the same idempotency key. Assert the returned `Memory` objects are equal and SQL counts remain:

```text
memories = 1
memory_revisions = 1
memory_sources = 1
outbox(memory.created) = 1
idempotency_keys = 1
```

- [ ] **Step 5: Add an idempotency misuse test**

Reuse the key with changed content and assert `ApplicationError::Conflict`. Verify no second revision/source/outbox row exists.

- [ ] **Step 6: Add a forced rollback test**

Create an integration-only wrapper around `MemoryUnitOfWork` or use a direct real transaction to force a foreign-key failure after memory/revision insertion but before commit. Assert all rows created earlier in that transaction are absent afterwards.

The test must prove the property with SQL counts; a mocked `commit` flag is not sufficient.

- [ ] **Step 7: Run the P0 database suite**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test memory_write_integrity -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 8: Run the existing memory/provenance tests for regression**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test memory_lifecycle --test provenance -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add tests/memory_write_integrity.rs tests/support/mod.rs

git commit -m "test(memory): prove atomic memory write integrity"
```

---

### Task 7: Synchronize architecture documentation and run the P0 exit gate

**Files:**
- Modify: `docs/domain-model.md`
- Modify: `docs/architecture.md`

**Interfaces:**
- Consumes: completed transactional behavior from Tasks 1-6.
- Produces: documentation that no longer describes the old autocommit write path.

- [ ] **Step 1: Update the Memory write-path documentation**

Document this exact ordering:

```text
MemoryService
  -> MemoryUnitOfWorkManager.begin(RequestContext)
  -> scoped PostgreSQL transaction
  -> idempotency check
  -> authoritative writes
  -> outbox + idempotency record
  -> commit
```

Document policy outcomes:

```text
AutoActivate    -> Active
RequireApproval -> Candidate
DiscardCandidate-> Rejected
```

- [ ] **Step 2: Mark the old repository description as obsolete**

Remove any statement that `MemoryService` performs authoritative writes by independently calling `PgMemoryRepository`, `PgProvenanceRepository`, `PgOutboxRepository`, and `PgIdempotencyRepository`.

Keep those repository adapters documented where they are still independently used.

- [ ] **Step 3: Run formatting, linting, and unit tests**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 4: Run database integration tests**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --workspace --all-targets -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 5: Verify the P0 invariants manually with repository search**

Run:

```bash
rg "MemoryService::new|save_memory\(|save_revision\(|save_source\(|insert_outbox|insert_idempotency" crates/vestrace-application crates/vestrace-cli crates/vestrace-infrastructure
```

Expected: the `MemoryService` authoritative path uses `MemoryUnitOfWork`; no independent autocommitted write sequence remains in `services.rs`.

- [ ] **Step 6: Commit the documentation**

```bash
git add docs/domain-model.md docs/architecture.md

git commit -m "docs: document atomic memory write boundary"
```

## P0 Exit Gate

Do not start M0.2-A until all of these are true:

```text
[ ] remember_memory is atomic
[ ] revise_memory is atomic and checks expected revision under lock
[ ] record_event is atomic with outbox/idempotency
[ ] link_knowledge is atomic with outbox/idempotency
[ ] policy input changes persisted status
[ ] evidence_role is preserved
[ ] active memory has persisted revision + provenance before commit
[ ] identical idempotent replay creates no duplicate rows
[ ] failed write leaves no partial authoritative rows
[ ] workspace-scoped transaction context is used for every write
[ ] cargo fmt/clippy/test pass
[ ] PostgreSQL integration suite passes
```
