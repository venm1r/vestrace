# Vestrace Memory Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Vestrace's authoritative event, memory, revision, provenance, relation, job and outbox model with transactional writes, optimistic concurrency, workspace isolation and safe lifecycle operations.

**Architecture:** Keep domain aggregates pure and persist them through repository ports. Application services own transaction boundaries and write an outbox record in the same transaction as every authoritative change. The worker leases PostgreSQL jobs and invokes typed handlers; AI extraction remains a port with deterministic test implementations until the retrieval plan adds a real provider.

**Tech Stack:** Existing Foundation workspace, Rust, Tokio, SQLx, PostgreSQL, Serde, Schemars, tracing, property-based testing with proptest.

## Global Constraints

- Complete `2026-07-31-vestrace-foundation.md` first.
- Events are append-only; corrections create new events.
- Memory content is changed only by creating a new MemoryRevision.
- Active memory must have at least one source.
- Derived memory must have a Derivation.
- All external write commands require an idempotency key.
- Versioned updates require `expected_revision`.
- PostgreSQL is authoritative; jobs, outbox and provenance are stored transactionally.
- Hard purge is an explicit administrative operation and removes content and derivatives.
- Cross-workspace references are invalid even when UUIDs exist.

---

## Locked file structure additions

```text
crates/vestrace-domain/src/
  event.rs
  memory/mod.rs
  memory/kind.rs
  memory/status.rs
  memory/structured.rs
  memory/revision.rs
  provenance.rs
  relation.rs
  job.rs
  policy.rs

crates/vestrace-application/src/
  event/mod.rs
  event/record.rs
  memory/mod.rs
  memory/commands.rs
  memory/services.rs
  memory/ports.rs
  jobs/mod.rs
  jobs/ports.rs
  jobs/worker.rs
  outbox.rs

crates/vestrace-infrastructure/src/postgres/
  event_repository.rs
  memory_repository.rs
  provenance_repository.rs
  relation_repository.rs
  idempotency_repository.rs
  job_repository.rs
  outbox_repository.rs

crates/vestrace-cli/src/commands/worker.rs

migrations/
  0004_sessions_and_events.sql
  0005_memories_and_revisions.sql
  0006_provenance_scopes_relations.sql
  0007_idempotency_jobs_outbox.sql

tests/
  event_ingestion.rs
  memory_lifecycle.rs
  provenance.rs
  jobs.rs
  purge.rs
```

---

### Task 1: Add memory-core identifiers and domain value types

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/event.rs`
- Create: `crates/vestrace-domain/src/memory/kind.rs`
- Create: `crates/vestrace-domain/src/memory/status.rs`
- Create: `crates/vestrace-domain/src/memory/structured.rs`
- Create: `crates/vestrace-domain/src/memory/revision.rs`
- Create: `crates/vestrace-domain/src/memory/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Produces IDs: `SessionId`, `EventId`, `MemoryId`, `MemoryRevisionId`, `MemorySourceId`, `DerivationId`, `RelationId`, `JobId`, `OutboxId`.
- Produces `MemoryKind`, `MemoryStatus`, `StructuredMemory`, `MemoryRevision`, `Memory`.

- [ ] **Step 1: Write failing lifecycle and range tests**

```rust
#[test]
fn confidence_rejects_values_outside_unit_interval() {
    assert!(Confidence::new(-0.01).is_err());
    assert!(Confidence::new(1.01).is_err());
}

#[test]
fn superseded_memory_cannot_be_reactivated() {
    let memory = active_memory_fixture().supersede(now()).unwrap();
    assert!(memory.activate(now()).is_err());
}
```

- [ ] **Step 2: Implement constrained scores**

```rust
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Confidence(f32);

impl Confidence {
    pub fn new(value: f32) -> Result<Self, DomainError> {
        if (0.0..=1.0).contains(&value) && value.is_finite() {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidArgument("confidence must be between 0 and 1".into()))
        }
    }
    pub const fn value(self) -> f32 { self.0 }
}
```

Implement `Importance` identically.

- [ ] **Step 3: Implement memory enums**

```rust
pub enum MemoryKind {
    Fact, Preference, Constraint, Decision, Task,
    Procedure, Observation, Outcome, Summary,
}

pub enum MemoryStatus {
    Candidate, Active, Superseded, Rejected, Expired, Deleted,
}
```

- [ ] **Step 4: Implement typed structured payloads**

Include `AssertionData`, `DecisionData`, `TaskData`, `ProcedureData`, `ObservationData`, `OutcomeData`, and `SummaryData`. Keep unknown future fields compatible through versioned schema wrappers rather than untyped top-level maps.

- [ ] **Step 5: Implement lifecycle methods**

```rust
impl Memory {
    pub fn activate(mut self, at: Timestamp) -> Result<Self, DomainError>;
    pub fn reject(mut self, at: Timestamp) -> Result<Self, DomainError>;
    pub fn supersede(mut self, at: Timestamp) -> Result<Self, DomainError>;
    pub fn expire(mut self, at: Timestamp) -> Result<Self, DomainError>;
    pub fn soft_delete(mut self, at: Timestamp) -> Result<Self, DomainError>;
}
```

Each method validates the allowed state transition and stores the transition timestamp.

- [ ] **Step 6: Run tests and commit**

```bash
cargo test -p vestrace-domain memory
git add crates/vestrace-domain
git commit -m "feat(memory): add core memory domain model"
```

---

### Task 2: Define events, provenance, relations and write policies

**Files:**
- Create: `crates/vestrace-domain/src/provenance.rs`
- Create: `crates/vestrace-domain/src/relation.rs`
- Create: `crates/vestrace-domain/src/policy.rs`
- Modify: `crates/vestrace-domain/src/event.rs`
- Test: inline unit and property tests

**Interfaces:**
- Produces `Event`, `ActorRef`, `SubjectRef`, `EventLink`.
- Produces `MemorySource`, `EvidenceRole`, `SourceRef`, `Derivation`, `DerivationMethod`.
- Produces `KnowledgeRef`, `RelationType`, `KnowledgeRelation`.
- Produces `MemoryWritePolicy::{Manual, Assisted, Automatic}` and `ActivationDecision`.

- [ ] **Step 1: Write failing event and provenance invariant tests**

```rust
#[test]
fn derived_source_requires_derivation() {
    let result = MemorySource::new_derived(revision_id(), SourceRef::Event(event_id()), None);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Implement immutable event value**

```rust
pub struct Event {
    pub id: EventId,
    pub workspace_id: WorkspaceId,
    pub event_type: String,
    pub actor: ActorRef,
    pub subject: Option<SubjectRef>,
    pub session_id: Option<SessionId>,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub payload: serde_json::Value,
    pub occurred_at: Timestamp,
    pub ingested_at: Timestamp,
    pub idempotency_key: String,
}
```

Validate namespaced event types such as `interaction.user_message`; reject empty segments and whitespace.

- [ ] **Step 3: Implement provenance types**

Source references must be tagged enums, not polymorphic string pairs. A `Derived` evidence role must include `DerivationId`.

- [ ] **Step 4: Implement typed relations**

Support at minimum: `Supports`, `Contradicts`, `Supersedes`, `CausedBy`, `ResultedIn`, `DependsOn`, `ProducedBy`, and `RelatedTo`.

- [ ] **Step 5: Implement policy decision rules**

```rust
pub struct CandidateAssessment {
    pub confidence: Confidence,
    pub importance: Importance,
    pub has_primary_source: bool,
    pub conflicts_detected: bool,
}

impl MemoryWritePolicy {
    pub fn decide(&self, assessment: &CandidateAssessment) -> ActivationDecision;
}
```

`Manual` always returns candidate review. `Assisted` activates only when explicit configured thresholds pass and no unresolved conflict exists. `Automatic` uses its own versioned thresholds and still refuses activation without a source.

- [ ] **Step 6: Run property tests and commit**

```bash
cargo test -p vestrace-domain

git add crates/vestrace-domain
git commit -m "feat(memory): add events provenance and write policies"
```

---

### Task 3: Add append-only event and session schema

**Files:**
- Create: `migrations/0004_sessions_and_events.sql`
- Create: `tests/event_ingestion.rs`
- Modify: `tests/support/mod.rs`

**Interfaces:**
- Produces tables `sessions`, `events`, `event_links`.
- Enforces unique `(workspace_id, idempotency_key)` for events.
- Prevents cross-workspace causation links with composite foreign-key strategy or trigger validation.

- [ ] **Step 1: Write failing database tests**

Test that:

1. duplicate event idempotency key is rejected;
2. event payload can be read back exactly;
3. an event in workspace A cannot use a cause from workspace B;
4. application role cannot update or delete an event.

- [ ] **Step 2: Create tables and indexes**

Use `JSONB NOT NULL` for payload, indexes on `(workspace_id, occurred_at DESC)`, `(workspace_id, correlation_id)`, and `(workspace_id, event_type, occurred_at DESC)`.

- [ ] **Step 3: Enforce append-only behavior**

Create a trigger function that raises SQLSTATE `55000` on `UPDATE` or `DELETE` for the application database role. Administrative purge will use a separate privileged function introduced later.

- [ ] **Step 4: Add RLS policies**

Use the Foundation helper function `vestrace_current_workspace_id()` and force RLS.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test event_ingestion
git add migrations/0004_sessions_and_events.sql tests
git commit -m "feat(storage): add append-only event schema"
```

---

### Task 4: Add memory, revision and lifecycle schema

**Files:**
- Create: `migrations/0005_memories_and_revisions.sql`
- Create: `tests/memory_lifecycle.rs`

**Interfaces:**
- Produces tables `memories`, `memory_revisions`, `memory_status_history`.
- `memories.current_revision_id` references a revision belonging to the same memory.
- Unique revision number per memory.
- Stores `kind`, `status`, confidence, importance and validity timestamps.

- [ ] **Step 1: Write failing constraints tests**

Test invalid score range, invalid validity interval, duplicate revision number and current revision pointing to another memory.

- [ ] **Step 2: Create memory tables**

Use check constraints:

```sql
CHECK (confidence >= 0 AND confidence <= 1),
CHECK (importance >= 0 AND importance <= 1),
CHECK (valid_until IS NULL OR valid_from IS NULL OR valid_until >= valid_from)
```

Store structured content as `JSONB NOT NULL` and readable content as `TEXT NOT NULL`.

- [ ] **Step 3: Enforce current-revision ownership**

Use a deferred constraint trigger so a transaction can insert a memory, first revision and current pointer atomically.

- [ ] **Step 4: Add RLS and lifecycle indexes**

Index active current memories by workspace, kind and update time.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test memory_lifecycle
git add migrations/0005_memories_and_revisions.sql tests/memory_lifecycle.rs
git commit -m "feat(storage): add memory revision schema"
```

---

### Task 5: Add provenance, scopes, conflicts and relation schema

**Files:**
- Create: `migrations/0006_provenance_scopes_relations.sql`
- Create: `tests/provenance.rs`

**Interfaces:**
- Produces tables `memory_sources`, `derivations`, `derivation_inputs`, `memory_scopes`, `memory_conflicts`, `knowledge_relations`.
- `global` scope means workspace-global and still carries `workspace_id`.
- Relations use stable typed source and target kinds with validated UUIDs.

- [ ] **Step 1: Write failing provenance coverage test**

Insert an active memory without a source and assert transaction commit fails. Insert a derived source without derivation and assert failure.

- [ ] **Step 2: Create source and derivation tables**

Use explicit source columns (`source_kind`, `source_id`) plus a validation trigger that checks the referenced authoritative table and workspace.

- [ ] **Step 3: Create scope table**

Use `(memory_id, scope_kind, scope_id)` uniqueness. Represent workspace-global scope with `scope_kind = 'global'` and `scope_id = workspace_id`.

- [ ] **Step 4: Create conflict and relation tables**

A conflict records left/right memory revision IDs, status, resolution revision, detection method and timestamps.

- [ ] **Step 5: Add deferred active-source invariant**

At transaction commit, every `Active` memory must have at least one source linked to its current revision.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test provenance
git add migrations/0006_provenance_scopes_relations.sql tests/provenance.rs
git commit -m "feat(storage): add memory provenance and graph schema"
```

---

### Task 6: Define application commands and repository ports

**Files:**
- Create: `crates/vestrace-application/src/event/mod.rs`
- Create: `crates/vestrace-application/src/event/record.rs`
- Create: `crates/vestrace-application/src/memory/mod.rs`
- Create: `crates/vestrace-application/src/memory/commands.rs`
- Create: `crates/vestrace-application/src/memory/ports.rs`
- Create: `crates/vestrace-application/src/outbox.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces commands `RecordEvent`, `RememberMemory`, `ReviseMemory`, `ChangeMemoryStatus`, `LinkKnowledge`, `HardPurgeMemory`.
- Produces repository ports with transaction-scoped methods.
- Produces `OutboxMessage { topic, aggregate_id, payload, idempotency_key }`.

- [ ] **Step 1: Define exact command types**

```rust
pub struct RecordEvent {
    pub workspace_id: WorkspaceId,
    pub actor: ActorRef,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub occurred_at: Timestamp,
    pub session_id: Option<SessionId>,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub idempotency_key: String,
}

pub struct ReviseMemory {
    pub memory_id: MemoryId,
    pub expected_revision: u32,
    pub content: String,
    pub structured: StructuredMemory,
    pub reason: ChangeReason,
    pub source_ids: Vec<MemorySourceId>,
    pub idempotency_key: String,
}
```

- [ ] **Step 2: Define repository ports**

Include exact methods:

```rust
async fn insert_event(&mut self, event: &Event) -> Result<(), ApplicationError>;
async fn find_event_by_idempotency_key(&mut self, workspace: WorkspaceId, key: &str) -> Result<Option<Event>, ApplicationError>;
async fn load_memory_for_update(&mut self, id: MemoryId) -> Result<MemoryRecord, ApplicationError>;
async fn insert_memory_with_revision(&mut self, memory: &Memory, revision: &MemoryRevision) -> Result<(), ApplicationError>;
async fn append_revision(&mut self, memory: &Memory, revision: &MemoryRevision) -> Result<(), ApplicationError>;
```

- [ ] **Step 3: Add compile-only fake repositories**

Provide test fakes in `#[cfg(test)]` modules so application services can be unit tested without SQLx.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application
git add crates/vestrace-application
git commit -m "feat(application): define memory commands and ports"
```

---

### Task 7: Implement event and memory application services transactionally

**Files:**
- Create: `crates/vestrace-application/src/memory/services.rs`
- Modify: `crates/vestrace-application/src/event/record.rs`
- Test: unit tests beside services

**Interfaces:**
- Produces `RecordEventService`, `RememberMemoryService`, `ReviseMemoryService`, `ChangeMemoryStatusService`, `LinkKnowledgeService`.
- Every service accepts `&RequestContext` and one command.
- Idempotent replay returns the original resource identifier without a second mutation.

- [ ] **Step 1: Write failing idempotency test**

Use a fake transaction and repositories. Call `RecordEventService::execute` twice with the same key and assert one insert and the same `EventId` twice.

- [ ] **Step 2: Implement event recording**

Transaction sequence:

```text
begin scoped transaction
→ check idempotency record
→ insert event
→ insert idempotency result
→ append outbox event `event.recorded`
→ commit
```

- [ ] **Step 3: Write failing revision-conflict test**

Load current revision `4`, execute command with `expected_revision = 3`, and expect `ApplicationError::RevisionConflict { expected: 3, current: 4 }` with no writes.

- [ ] **Step 4: Implement remember and revise services**

Remember creates revision `1`. Revise locks the memory row, checks revision, creates `current + 1`, updates current pointer and writes `memory.revision.created` outbox message.

- [ ] **Step 5: Implement lifecycle and linking services**

Validate domain transition before persistence. Prevent cross-workspace links through both application validation and database constraints.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application memory
git add crates/vestrace-application
git commit -m "feat(application): implement transactional memory services"
```

---

### Task 8: Implement PostgreSQL repositories and idempotency storage

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/event_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/memory_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/provenance_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/relation_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/idempotency_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Test: `tests/event_ingestion.rs`, `tests/memory_lifecycle.rs`, `tests/provenance.rs`

**Interfaces:**
- Implements the ports from Task 6 using a shared scoped SQLx transaction.
- Does not expose `PgTransaction` outside infrastructure.

- [ ] **Step 1: Add migration for idempotency records**

Create `migrations/0007_idempotency_jobs_outbox.sql` initially with `idempotency_records`, reserving the same file for job/outbox tables in Task 9.

Store command name, key, request hash, result type, result JSON and timestamps. A reused key with a different request hash must return `idempotency_conflict`.

- [ ] **Step 2: Write integration idempotency test**

Two identical commands return one event. A command with the same key and different payload fails.

- [ ] **Step 3: Implement repository SQL**

Use static `query_as!`/`query!` where practical. Map database constraint errors to stable application error codes.

- [ ] **Step 4: Run integration suite**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test event_ingestion --test memory_lifecycle --test provenance
```

- [ ] **Step 5: Commit**

```bash
git add crates/vestrace-infrastructure migrations/0007_idempotency_jobs_outbox.sql tests
git commit -m "feat(storage): implement memory repositories"
```

---

### Task 9: Implement PostgreSQL jobs, leases and transactional outbox

**Files:**
- Create: `crates/vestrace-domain/src/job.rs`
- Create: `crates/vestrace-application/src/jobs/mod.rs`
- Create: `crates/vestrace-application/src/jobs/ports.rs`
- Create: `crates/vestrace-application/src/jobs/worker.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/job_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/outbox_repository.rs`
- Modify: `migrations/0007_idempotency_jobs_outbox.sql`
- Create: `tests/jobs.rs`

**Interfaces:**
- Produces `JobType`, `JobStatus`, `JobLease`, `JobHandler` and `Worker`.
- `JobRepository::lease_next(worker_id, lease_duration, allowed_types)` uses `FOR UPDATE SKIP LOCKED`.
- Outbox dispatcher converts unprocessed messages into idempotent jobs.

- [ ] **Step 1: Write failing two-worker lease test**

Insert one queued job. Concurrently lease from two repositories. Assert exactly one returns the job.

- [ ] **Step 2: Finish jobs/outbox migration**

Add `jobs` and `outbox_messages` with status checks, priority, `available_at`, attempts, max attempts, lease owner/time, last error and unique idempotency keys.

- [ ] **Step 3: Implement lease SQL**

Use one transaction containing selection and update:

```sql
SELECT id
FROM jobs
WHERE status IN ('queued', 'retry_scheduled')
  AND available_at <= now()
ORDER BY priority DESC, available_at ASC
FOR UPDATE SKIP LOCKED
LIMIT 1;
```

Then update status and lease fields before commit.

- [ ] **Step 4: Implement retry classification**

```rust
pub enum JobFailure {
    Retryable { code: String, message: String },
    Permanent { code: String, message: String },
}
```

Retryable failures schedule exponential backoff with jitter; exhausted jobs become `dead_letter`.

- [ ] **Step 5: Implement outbox dispatcher**

Claim outbox rows with `SKIP LOCKED`, create job records with deterministic keys, then mark outbox rows dispatched in the same transaction.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test jobs
git add crates migrations/0007_idempotency_jobs_outbox.sql tests/jobs.rs
git commit -m "feat(worker): add PostgreSQL jobs and outbox"
```

---

### Task 10: Wire the worker process and deterministic extraction seam

**Files:**
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Create: `crates/vestrace-application/src/memory/extraction.rs`
- Test: `crates/vestrace-application/tests/extraction.rs`
- Test: `tests/jobs.rs`

**Interfaces:**
- Produces `MemoryExtractor` port:

```rust
#[async_trait::async_trait]
pub trait MemoryExtractor: Send + Sync {
    async fn extract(&self, event: &Event) -> Result<Vec<ExtractedMemoryCandidate>, ExtractionError>;
}
```

- Produces `ExtractEventJobHandler` using the port and `RememberMemoryService`.
- Foundation implementation includes only `DeterministicExtractor` for tests and local smoke scenarios.

- [ ] **Step 1: Write failing extraction job test**

A recorded event emits an outbox message, the dispatcher creates `memory.extract`, the handler uses a deterministic extractor and creates one candidate with provenance to the source event.

- [ ] **Step 2: Implement extraction types and handler**

The handler's idempotency key is:

```text
source_event_id + extractor_version + policy_version
```

- [ ] **Step 3: Implement worker loop**

The CLI worker must stop leasing new work on shutdown, finish or release the current lease, and log job IDs without logging source content.

- [ ] **Step 4: Run tests and worker smoke test**

```bash
cargo test -p vestrace-application --test extraction
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test jobs
```

- [ ] **Step 5: Commit**

```bash
git add crates/vestrace-application crates/vestrace-cli tests
git commit -m "feat(worker): process memory extraction jobs"
```

---

### Task 11: Implement administrative hard purge

**Files:**
- Create: `crates/vestrace-application/src/memory/purge.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/purge_repository.rs`
- Create: `tests/purge.rs`
- Modify: `migrations/0007_idempotency_jobs_outbox.sql` only if the plan branch has not yet applied it; otherwise add `0008_purge_functions.sql` and never edit an applied migration

**Interfaces:**
- Produces `HardPurgeMemoryService::execute(context, command, approval)`.
- Requires an explicit `ApprovalRecordId` and `data.purge` capability assertion supplied by the later policy adapter.
- Deletes content, revisions, sources, conflicts, relations, pending jobs, outbox entries and future derivative rows keyed to the memory.
- Leaves one minimal `purge_audit` record without content.

- [ ] **Step 1: Write failing purge test**

Create memory with two revisions, provenance and queued derivative job. Purge it. Assert all content tables have zero matching rows and `purge_audit` contains only IDs, actor, reason and timestamp.

- [ ] **Step 2: Implement purge transaction**

Use one privileged transaction and explicit deletion order. Do not rely on broad cascade from workspace or principal tables.

- [ ] **Step 3: Verify normal application role cannot call purge SQL**

Add a negative integration test expecting `insufficient_privilege`.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test purge
git add crates/vestrace-application crates/vestrace-infrastructure migrations tests/purge.rs
git commit -m "feat(memory): add authorized hard purge"
```

---

## Memory Core completion gate

Run with fresh output:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --workspace --all-features
```

Then run one acceptance test that proves:

```text
record event
→ outbox message
→ leased extraction job
→ deterministic candidate memory
→ activation under policy
→ revision with expected_revision
→ provenance retained
→ superseded revision hidden from current-state repository query
→ authorized hard purge removes content and derivative work
```

Review database constraints, not only application tests. Confirm events remain append-only, active memories cannot commit without sources, cross-workspace references fail, duplicate idempotency keys are deterministic, and two workers cannot lease the same job simultaneously.
