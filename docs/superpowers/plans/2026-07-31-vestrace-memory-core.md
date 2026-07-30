# Vestrace Memory Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Vestrace's authoritative event, memory, revision, provenance, relation, job and outbox model with transactional writes, optimistic concurrency, workspace isolation and safe lifecycle operations.

**Architecture:** Keep domain aggregates pure and persist them through repository ports. Application services own transaction boundaries and write an outbox record in the same transaction as every authoritative change. The worker leases PostgreSQL jobs and invokes typed handlers; AI extraction remains a port with deterministic test implementations until the retrieval plan adds a real provider.

**Tech Stack:** Existing Foundation workspace, Rust, Tokio, SQLx, PostgreSQL, Serde, Schemars, tracing, proptest.

## Global Constraints

- Complete `2026-07-31-vestrace-foundation.md` first.
- Events are append-only; corrections create new events.
- Memory content changes only by creating a new `MemoryRevision`.
- Active memory must have at least one source.
- Derived memory must have a `Derivation`.
- All external write commands require an idempotency key.
- Versioned updates require `expected_revision`.
- PostgreSQL is authoritative; jobs, outbox and provenance are stored transactionally.
- Hard purge is an explicit administrative operation and removes content and derivatives.
- Cross-workspace references are invalid even when UUIDs exist.
- Migration `0007_idempotency_jobs_outbox.sql` is created once with the complete idempotency, job, outbox and purge-audit schema and is never edited after commit.

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
  memory/extraction.rs
  memory/purge.rs
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
  purge_repository.rs

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

crates/vestrace-application/tests/
  extraction.rs
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
- Produces IDs `SessionId`, `EventId`, `MemoryId`, `MemoryRevisionId`, `MemorySourceId`, `DerivationId`, `RelationId`, `JobId`, `OutboxId`.
- Produces `MemoryKind`, `MemoryStatus`, `StructuredMemory`, `MemoryRevision`, `Memory`, `Confidence`, and `Importance`.

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
        if value.is_finite() && (0.0..=1.0).contains(&value) {
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

Include `AssertionData`, `DecisionData`, `TaskData`, `ProcedureData`, `ObservationData`, `OutcomeData`, and `SummaryData`. Use a tagged enum with explicit schema version; do not use a free-form top-level JSON map.

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

Each method validates an explicit state-transition table and records the transition time.

- [ ] **Step 6: Run tests and commit**

```bash
cargo test -p vestrace-domain memory
git add crates/vestrace-domain
git commit -m "feat(memory): add core memory domain model"
```

---

### Task 2: Define events, provenance, relations and write policies

**Files:**
- Modify: `crates/vestrace-domain/src/event.rs`
- Create: `crates/vestrace-domain/src/provenance.rs`
- Create: `crates/vestrace-domain/src/relation.rs`
- Create: `crates/vestrace-domain/src/policy.rs`
- Test: inline unit and property tests

**Interfaces:**
- Produces `Event`, `ActorRef`, `SubjectRef`, `EventLink`.
- Produces `MemorySource`, `EvidenceRole`, `SourceRef`, `Derivation`, `DerivationMethod`.
- Produces `KnowledgeRef`, `RelationType`, `KnowledgeRelation`.
- Produces `MemoryWritePolicy::{Manual, Assisted, Automatic}` and `ActivationDecision`.

- [ ] **Step 1: Write failing invariant tests**

```rust
#[test]
fn derived_source_requires_derivation() {
    let result = MemorySource::new_derived(revision_id(), SourceRef::Event(event_id()), None);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Implement immutable events**

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

Source references are tagged enums. `EvidenceRole::Derived` requires `DerivationId`. Derivation methods are `Extraction`, `Summarization`, `Inference`, `Consolidation`, `ConflictResolution`, `HumanAuthored`, and `Imported`.

- [ ] **Step 4: Implement typed relations**

Support at minimum `Supports`, `Contradicts`, `Supersedes`, `CausedBy`, `ResultedIn`, `DependsOn`, `ProducedBy`, and `RelatedTo`.

- [ ] **Step 5: Implement write-policy decisions**

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

`Manual` always requires review. `Assisted` activates only when configured thresholds pass and no unresolved conflict exists. `Automatic` uses versioned thresholds and still refuses activation without a source.

- [ ] **Step 6: Run and commit**

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
- Prevents cross-workspace causation links.

- [ ] **Step 1: Write failing database tests**

Test that duplicate event idempotency is rejected, payload round-trips, cross-workspace causation fails, and the application database role cannot update or delete events.

- [ ] **Step 2: Create tables and indexes**

Use `JSONB NOT NULL` for payload and indexes on `(workspace_id, occurred_at DESC)`, `(workspace_id, correlation_id)`, and `(workspace_id, event_type, occurred_at DESC)`.

- [ ] **Step 3: Enforce append-only behavior**

Create a trigger that raises SQLSTATE `55000` for `UPDATE` or `DELETE` by the normal application role. A separate administrative database role used by hard purge is explicitly exempted and is never used by ordinary repositories.

- [ ] **Step 4: Add and test RLS**

Use `vestrace_current_workspace_id()` and force RLS.

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test event_ingestion
```

- [ ] **Step 5: Commit**

```bash
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
- `memories.current_revision_id` must reference a revision of the same memory.
- Revision number is unique per memory.

- [ ] **Step 1: Write failing constraint tests**

Test invalid score range, invalid validity interval, duplicate revision number and current revision pointing to another memory.

- [ ] **Step 2: Create memory tables**

```sql
CHECK (confidence >= 0 AND confidence <= 1),
CHECK (importance >= 0 AND importance <= 1),
CHECK (valid_until IS NULL OR valid_from IS NULL OR valid_until >= valid_from)
```

Store readable content as `TEXT NOT NULL` and typed structured content as `JSONB NOT NULL`.

- [ ] **Step 3: Enforce current-revision ownership**

Use a deferred constraint trigger so memory, first revision and current pointer can be inserted atomically.

- [ ] **Step 4: Add RLS and indexes, then run tests**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test memory_lifecycle
```

- [ ] **Step 5: Commit**

```bash
git add migrations/0005_memories_and_revisions.sql tests/memory_lifecycle.rs
git commit -m "feat(storage): add memory revision schema"
```

---

### Task 5: Add provenance, scopes, conflicts and relation schema

**Files:**
- Create: `migrations/0006_provenance_scopes_relations.sql`
- Create: `tests/provenance.rs`

**Interfaces:**
- Produces `memory_sources`, `derivations`, `derivation_inputs`, `memory_scopes`, `memory_conflicts`, and `knowledge_relations`.
- `global` means workspace-global and uses the workspace UUID as `scope_id`.

- [ ] **Step 1: Write failing provenance coverage tests**

Insert an active memory without a source and assert commit failure. Insert a derived source without derivation and assert failure.

- [ ] **Step 2: Create sources and derivations**

Use explicit `source_kind` plus `source_id` and a validation trigger that checks the authoritative table and workspace.

- [ ] **Step 3: Create scopes, conflicts and relations**

Use unique `(memory_id, scope_kind, scope_id)`. A conflict records left/right revision IDs, status, resolution revision, method and timestamps.

- [ ] **Step 4: Add deferred active-source invariant**

At commit, every `Active` memory must have at least one source linked to its current revision.

- [ ] **Step 5: Run and commit**

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
- Produces commands `RecordEvent`, `RememberMemory`, `ReviseMemory`, `ChangeMemoryStatus`, `LinkKnowledge`, and `HardPurgeMemory`.
- Produces transaction-scoped repository ports and `OutboxMessage`.

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

```rust
async fn insert_event(&mut self, event: &Event) -> Result<(), ApplicationError>;
async fn find_event_by_idempotency_key(&mut self, workspace: WorkspaceId, key: &str) -> Result<Option<Event>, ApplicationError>;
async fn load_memory_for_update(&mut self, id: MemoryId) -> Result<MemoryRecord, ApplicationError>;
async fn insert_memory_with_revision(&mut self, memory: &Memory, revision: &MemoryRevision) -> Result<(), ApplicationError>;
async fn append_revision(&mut self, memory: &Memory, revision: &MemoryRevision) -> Result<(), ApplicationError>;
```

- [ ] **Step 3: Add compile-only fakes for service tests**

Provide in-memory fakes under `#[cfg(test)]`; no SQLx type appears in application or domain traits.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application
git add crates/vestrace-application
git commit -m "feat(application): define memory commands and ports"
```

---

### Task 7: Implement transactional event and memory services

**Files:**
- Create: `crates/vestrace-application/src/memory/services.rs`
- Modify: `crates/vestrace-application/src/event/record.rs`
- Test: unit tests beside services

**Interfaces:**
- Produces `RecordEventService`, `RememberMemoryService`, `ReviseMemoryService`, `ChangeMemoryStatusService`, and `LinkKnowledgeService`.
- Every service accepts `&RequestContext` and one command.
- Idempotent replay returns the original resource ID without a second mutation.

- [ ] **Step 1: Write failing idempotency test**

Call `RecordEventService::execute` twice with the same command and assert one insert and the same `EventId` twice.

- [ ] **Step 2: Implement event transaction flow**

```text
begin scoped transaction
→ verify or create idempotency record
→ insert event
→ store result
→ append outbox topic event.recorded
→ commit
```

- [ ] **Step 3: Write failing revision-conflict test**

Load revision `4`, execute `expected_revision = 3`, expect `RevisionConflict { expected: 3, current: 4 }`, and assert no writes.

- [ ] **Step 4: Implement remember, revise, lifecycle and linking services**

Remember creates revision `1`. Revise locks the row, checks revision, appends `current + 1`, updates current pointer, and emits `memory.revision.created`. Cross-workspace links fail before write and at the database boundary.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application memory
git add crates/vestrace-application
git commit -m "feat(application): implement transactional memory services"
```

---

### Task 8: Add the complete idempotency, jobs, outbox and purge-audit schema

**Files:**
- Create: `migrations/0007_idempotency_jobs_outbox.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/idempotency_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/event_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/memory_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/provenance_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/relation_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Test: existing event, lifecycle and provenance suites

**Interfaces:**
- `0007` creates `idempotency_records`, `jobs`, `outbox_messages`, and `purge_audit` in one immutable migration.
- Repositories implement the ports from Task 6 using one scoped SQLx transaction.

- [ ] **Step 1: Create the complete migration once**

`idempotency_records` stores command, key, request hash, result type, result JSON and timestamps. `jobs` stores type, payload, priority, status, scheduling, attempts, max attempts, lease owner/time, last error and idempotency key. `outbox_messages` stores topic, aggregate reference, payload, dispatch state and key. `purge_audit` stores object IDs, actor, reason, approval ID and time without content.

- [ ] **Step 2: Write idempotency integration tests**

Two identical commands return one event. Reusing the key with a different request hash returns `idempotency_conflict`.

- [ ] **Step 3: Implement repository SQL**

Use static `query!`/`query_as!` where practical. Map database constraint failures to stable application errors. Do not expose `PgTransaction` outside infrastructure.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test event_ingestion --test memory_lifecycle --test provenance
git add migrations/0007_idempotency_jobs_outbox.sql crates/vestrace-infrastructure tests
git commit -m "feat(storage): add memory repositories and durable work schema"
```

After this commit, `0007_idempotency_jobs_outbox.sql` must never be edited.

---

### Task 9: Implement PostgreSQL leases, retries and transactional outbox

**Files:**
- Create: `crates/vestrace-domain/src/job.rs`
- Create: `crates/vestrace-application/src/jobs/mod.rs`
- Create: `crates/vestrace-application/src/jobs/ports.rs`
- Create: `crates/vestrace-application/src/jobs/worker.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/job_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/outbox_repository.rs`
- Create: `tests/jobs.rs`

**Interfaces:**
- Produces `JobType`, `JobStatus`, `JobLease`, `JobHandler`, and `Worker`.
- `lease_next` uses `FOR UPDATE SKIP LOCKED` against the schema created in Task 8.
- Outbox dispatch creates jobs with deterministic keys and marks messages dispatched atomically.

- [ ] **Step 1: Write failing two-worker lease test**

Insert one queued job. Concurrently lease from two workers. Assert exactly one receives the job.

- [ ] **Step 2: Implement lease SQL**

```sql
SELECT id
FROM jobs
WHERE status IN ('queued', 'retry_scheduled')
  AND available_at <= now()
ORDER BY priority DESC, available_at ASC
FOR UPDATE SKIP LOCKED
LIMIT 1;
```

Update status and lease fields before commit.

- [ ] **Step 3: Implement retry classification**

```rust
pub enum JobFailure {
    Retryable { code: String, message: String },
    Permanent { code: String, message: String },
}
```

Retryable failures use exponential backoff with jitter; exhausted jobs become `dead_letter`.

- [ ] **Step 4: Implement outbox dispatch**

Claim messages with `SKIP LOCKED`, insert jobs using deterministic idempotency keys, and mark messages dispatched in the same transaction.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test jobs
git add crates tests/jobs.rs
git commit -m "feat(worker): add PostgreSQL jobs and outbox"
```

---

### Task 10: Wire the worker process and deterministic extraction seam

**Files:**
- Create: `crates/vestrace-application/src/memory/extraction.rs`
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Create: `crates/vestrace-application/tests/extraction.rs`
- Modify: `tests/jobs.rs`

**Interfaces:**
- Produces `MemoryExtractor`:

```rust
#[async_trait::async_trait]
pub trait MemoryExtractor: Send + Sync {
    async fn extract(&self, event: &Event) -> Result<Vec<ExtractedMemoryCandidate>, ExtractionError>;
}
```

- Produces `ExtractEventJobHandler` using the port and `RememberMemoryService`.
- This plan provides only `DeterministicExtractor` for tests and smoke scenarios.

- [ ] **Step 1: Write a failing extraction job test**

A recorded event emits an outbox message, dispatcher creates `memory.extract`, handler creates one candidate, and provenance points to the source event.

- [ ] **Step 2: Implement extraction identity**

The handler idempotency key is:

```text
source_event_id + extractor_version + policy_version
```

- [ ] **Step 3: Implement graceful worker loop**

On shutdown, stop leasing, finish or release the current lease, and log IDs without source content.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application --test extraction
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test jobs
git add crates/vestrace-application crates/vestrace-cli tests
git commit -m "feat(worker): process memory extraction jobs"
```

---

### Task 11: Implement administrative hard purge

**Files:**
- Create: `crates/vestrace-application/src/memory/purge.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/purge_repository.rs`
- Create: `tests/purge.rs`

**Interfaces:**
- Produces `HardPurgeMemoryService::execute(context, command, approval)`.
- Requires explicit `ApprovalRecordId` and a `data.purge` capability assertion supplied by the later policy adapter.
- Deletes content, revisions, sources, conflicts, relations, pending jobs, outbox entries and known derivative rows.
- Writes one minimal row to the existing `purge_audit` table.

- [ ] **Step 1: Write a failing purge test**

Create memory with two revisions, provenance and queued derivative job. Purge it. Assert all content tables have zero matching rows and `purge_audit` contains only identifiers, actor, approval, reason and timestamp.

- [ ] **Step 2: Implement one privileged purge transaction**

Use explicit deletion order through the administrative database role. Do not edit migration `0007`, do not rely on broad workspace cascades, and do not delete the minimal audit row.

- [ ] **Step 3: Verify normal role cannot purge**

Add a negative integration test expecting insufficient privilege when the normal application role attempts the privileged repository operation.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test purge
git add crates/vestrace-application crates/vestrace-infrastructure tests/purge.rs
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

Then run one acceptance scenario proving:

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

Confirm events remain append-only, active memories cannot commit without sources, cross-workspace references fail, duplicate idempotency keys are deterministic, two workers cannot lease the same job, and no task edits `0007_idempotency_jobs_outbox.sql` after its creation commit.
