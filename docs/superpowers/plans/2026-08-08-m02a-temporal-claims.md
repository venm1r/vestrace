# M0.2-A Temporal Claims Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce Vestrace's canonical Persistent Cognition foundation — immutable Evidence, Claims with claim-specific provenance, stable KnowledgeSlots, bitemporal SlotResolutions, historical queries, and provenance explanation — while preserving the v0.1 Memory API as a compatibility surface.

**Architecture:** Add a new `cognition` bounded context beside the existing legacy `memory` module. PostgreSQL remains authoritative; M0.2-A stores relational canonical cognition and uses explicit valid-time plus system-time intervals. Resolution is deliberately manual/internal in this slice: M0.2-A establishes correct semantics and atomic transitions; automatic reconciliation belongs to M0.2-B.

**Tech Stack:** Rust 2024, Serde/Schemars, Tokio, async-trait, SQLx 0.8, PostgreSQL `TIMESTAMPTZ`/JSONB/RLS, existing scoped transaction infrastructure, UUIDv7 IDs.

## Global Constraints

- P0 Memory Write Integrity must be complete before this plan starts.
- Vestrace remains a cognition-centered modular monolith.
- PostgreSQL is the authoritative cognition store.
- Existing migrations are immutable; add only forward migrations beginning after current migration `0114`.
- Evidence is immutable.
- Claims are append-oriented assertions and do not become knowledge automatically.
- A KnowledgeSlot provides semantic identity for competing values.
- Accepted knowledge is represented by SlotResolution, not by mutating Claim status.
- Valid time and system time are independent.
- All temporal intervals use half-open semantics `[from, until)`; if both bounds exist, `until > from`.
- `recorded_from` is required for a SlotResolution; `recorded_until`, when set, must be strictly greater.
- A single-cardinality slot cannot have overlapping current-system-time `Accepted` valid intervals.
- Claim provenance is attached through `ClaimEvidence`, not only through legacy `MemorySource`.
- Evidence role and Claim derivation are independent.
- The first required Evidence source is an existing Vestrace `Event`; other source kinds are deferred.
- Claim values are schema-qualified JSON values, not untyped top-level maps.
- Query semantics must support Current, ValidAt, KnownAt, ValidAt+KnownAt, Timeline, and Explain.
- No automatic LLM conflict resolution is introduced in M0.2-A.
- Existing `Memory`, `MemoryRevision`, `RetrievalIntent`, and `ContextPack` remain available.
- No dedicated graph database, new vector database, Kafka, or microservice split.

---

## File Structure

Create:

```text
crates/vestrace-domain/src/cognition/mod.rs
crates/vestrace-domain/src/cognition/temporal.rs
crates/vestrace-domain/src/cognition/value.rs
crates/vestrace-domain/src/cognition/scope.rs
crates/vestrace-domain/src/cognition/evidence.rs
crates/vestrace-domain/src/cognition/slot.rs
crates/vestrace-domain/src/cognition/claim.rs
crates/vestrace-domain/src/cognition/resolution.rs

crates/vestrace-application/src/cognition/mod.rs
crates/vestrace-application/src/cognition/commands.rs
crates/vestrace-application/src/cognition/queries.rs
crates/vestrace-application/src/cognition/ports.rs
crates/vestrace-application/src/cognition/service.rs

crates/vestrace-infrastructure/src/postgres/cognition_repository.rs
crates/vestrace-infrastructure/src/postgres/cognition_unit_of_work.rs

migrations/0115_persistent_cognition_foundation.sql

tests/cognition_temporal.rs
tests/cognition_provenance.rs
```

Modify:

```text
crates/vestrace-domain/src/id.rs
crates/vestrace-domain/src/lib.rs
crates/vestrace-application/src/lib.rs
crates/vestrace-infrastructure/src/postgres/mod.rs
crates/vestrace-cli/src/commands/server.rs
docs/domain-model.md
docs/database-schema.md
docs/architecture.md
```

The legacy `memory/` module remains intact in this slice except for an explicit compatibility link added near the end of the plan.

---

### Task 1: Add cognition identifiers and temporal value objects

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/cognition/mod.rs`
- Create: `crates/vestrace-domain/src/cognition/temporal.rs`
- Create: `crates/vestrace-domain/src/cognition/value.rs`
- Create: `crates/vestrace-domain/src/cognition/scope.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Consumes: existing `Timestamp`, `DomainError`, UUIDv7 `domain_id!` macro.
- Produces: `EvidenceId`, `ClaimId`, `ClaimEvidenceId`, `KnowledgeSlotId`, `SlotResolutionId`, `TemporalInterval`, semantic-key newtypes, scope/cardinality/schema types.

- [ ] **Step 1: Write failing unit tests for half-open temporal semantics**

In `cognition/temporal.rs`, start with tests:

```rust
#[test]
fn interval_rejects_equal_bounds() {
    let t = crate::now();
    assert!(TemporalInterval::new(Some(t), Some(t)).is_err());
}

#[test]
fn half_open_interval_excludes_until() {
    let from = ts("2026-08-01T00:00:00Z");
    let until = ts("2026-08-05T00:00:00Z");
    let interval = TemporalInterval::new(Some(from), Some(until)).unwrap();
    assert!(interval.contains(from));
    assert!(!interval.contains(until));
}

#[test]
fn adjacent_intervals_do_not_overlap() {
    let a = TemporalInterval::new(
        Some(ts("2026-08-01T00:00:00Z")),
        Some(ts("2026-08-05T00:00:00Z")),
    ).unwrap();
    let b = TemporalInterval::new(
        Some(ts("2026-08-05T00:00:00Z")),
        None,
    ).unwrap();
    assert!(!a.overlaps(&b));
}
```

The local `ts()` test helper parses RFC3339 into the crate's `Timestamp` type.

- [ ] **Step 2: Add the new UUIDv7 IDs**

Append to `id.rs`:

```rust
domain_id!(EvidenceId);
domain_id!(ClaimId);
domain_id!(ClaimEvidenceId);
domain_id!(KnowledgeSlotId);
domain_id!(SlotResolutionId);
```

- [ ] **Step 3: Implement `TemporalInterval`**

Use:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalInterval {
    pub from: Option<Timestamp>,
    pub until: Option<Timestamp>,
}
```

Constructor invariant:

```rust
if let (Some(from), Some(until)) = (from, until) {
    if until <= from {
        return Err(DomainError::InvalidArgument(
            "temporal interval requires until > from".into(),
        ));
    }
}
```

Implement `contains()` and `overlaps()` using half-open semantics and supporting unbounded ends.

- [ ] **Step 4: Implement validated semantic-key types**

Create newtypes:

```text
ScopeKey
SubjectKey
PredicateKey
ValueSchemaRef
```

Each constructor must:

- trim surrounding whitespace;
- reject empty strings;
- reject strings longer than 255 bytes;
- accept ASCII alphanumeric plus `.`, `_`, `-`, `:`, `/`;
- reject whitespace and other characters.

Expose `as_str()`; serialize transparently.

- [ ] **Step 5: Implement scope and slot policy enums**

```rust
pub enum ScopeKind {
    Workspace,
    Team,
    Project,
    Repository,
    Deployment,
    User,
    Agent,
    Conversation,
    Execution,
    Custom,
}

pub struct KnowledgeScope {
    pub kind: ScopeKind,
    pub key: ScopeKey,
}

pub enum SlotCardinality { Single, Multiple }
pub enum ResolutionPolicy { Manual }
pub enum TemporalPolicy { Atemporal, ValidTime }
```

`ResolutionPolicy::Manual` is intentionally the only M0.2-A variant; M0.2-B extends it.

- [ ] **Step 6: Implement schema-qualified `ClaimValue`**

```rust
pub struct ClaimValue {
    pub schema: ValueSchemaRef,
    pub value: serde_json::Value,
}
```

Do not try to validate arbitrary workspace JSON Schema in M0.2-A; enforce only that every value carries an explicit schema reference.

- [ ] **Step 7: Export the cognition module and run domain tests**

Run:

```bash
cargo test -p vestrace-domain cognition
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vestrace-domain

git commit -m "feat(cognition): add temporal and semantic value types"
```

---

### Task 2: Add Evidence, Claim, KnowledgeSlot, and SlotResolution domain types

**Files:**
- Create: `crates/vestrace-domain/src/cognition/evidence.rs`
- Create: `crates/vestrace-domain/src/cognition/slot.rs`
- Create: `crates/vestrace-domain/src/cognition/claim.rs`
- Create: `crates/vestrace-domain/src/cognition/resolution.rs`
- Modify: `crates/vestrace-domain/src/cognition/mod.rs`

**Interfaces:**
- Consumes: Task 1 IDs/value objects, existing `Confidence`, `Importance`, `EvidenceRole`, `DerivationId`, `EventId`.
- Produces: canonical M0.2-A cognition domain types.

- [ ] **Step 1: Write failing constructor/invariant tests**

Cover at minimum:

```text
Evidence records an Event source without becoming Knowledge.
Claim rejects blank canonical_text.
Claim value schema must equal the slot schema at application/storage validation time.
KnowledgeSlot starts at version 0.
SlotResolution rejects recorded_until <= recorded_from.
Closing a resolution twice is rejected.
Accepted and Contested are distinct dispositions.
```

- [ ] **Step 2: Implement immutable Evidence**

```rust
pub enum EvidenceSourceRef {
    Event(EventId),
}

pub struct Evidence {
    pub id: EvidenceId,
    pub workspace_id: WorkspaceId,
    pub source: EvidenceSourceRef,
    pub observed_at: Option<Timestamp>,
    pub recorded_at: Timestamp,
    pub content_hash: Option<String>,
}
```

`Evidence::new_event(...)` creates the initial variant. `content_hash`, when present, must be a lowercase 64-character hexadecimal SHA-256 string; otherwise return `DomainError::InvalidArgument`.

- [ ] **Step 3: Implement KnowledgeSlot**

```rust
pub struct KnowledgeSlot {
    pub id: KnowledgeSlotId,
    pub workspace_id: WorkspaceId,
    pub scope: KnowledgeScope,
    pub subject: SubjectKey,
    pub predicate: PredicateKey,
    pub cardinality: SlotCardinality,
    pub value_schema: ValueSchemaRef,
    pub resolution_policy: ResolutionPolicy,
    pub temporal_policy: TemporalPolicy,
    pub version: u64,
    pub created_at: Timestamp,
}
```

Provide `bump_version(expected: u64) -> Result<Self, DomainError>` that returns `RevisionConflict` if `self.version != expected` and otherwise increments with checked addition.

- [ ] **Step 4: Implement Claim**

```rust
pub struct Claim {
    pub id: ClaimId,
    pub workspace_id: WorkspaceId,
    pub slot_id: KnowledgeSlotId,
    pub value: ClaimValue,
    pub canonical_text: String,
    pub asserted_validity: TemporalInterval,
    pub observed_at: Option<Timestamp>,
    pub confidence: Confidence,
    pub importance: Importance,
    pub derivation_id: Option<DerivationId>,
    pub recorded_at: Timestamp,
}
```

Constructor rejects blank `canonical_text`.

- [ ] **Step 5: Implement claim-specific provenance**

```rust
pub enum EvidenceLocator {
    JsonPointer(String),
    TextRange { start: u32, end: u32 },
}

pub struct ClaimEvidence {
    pub id: ClaimEvidenceId,
    pub workspace_id: WorkspaceId,
    pub claim_id: ClaimId,
    pub evidence_id: EvidenceId,
    pub role: EvidenceRole,
    pub support_strength: Confidence,
    pub locator: Option<EvidenceLocator>,
    pub created_at: Timestamp,
}
```

`TextRange` requires `end > start`; `JsonPointer` requires a non-empty string starting with `/`.

Do not infer derivation from `EvidenceRole`; Claim carries `derivation_id` independently.

- [ ] **Step 6: Implement SlotResolution**

```rust
pub enum ResolutionDisposition {
    Accepted,
    Contested,
}

pub enum ResolutionReason {
    Manual { note: String },
}

pub struct SlotResolution {
    pub id: SlotResolutionId,
    pub workspace_id: WorkspaceId,
    pub slot_id: KnowledgeSlotId,
    pub claim_id: ClaimId,
    pub disposition: ResolutionDisposition,
    pub validity: TemporalInterval,
    pub recorded_from: Timestamp,
    pub recorded_until: Option<Timestamp>,
    pub reason: ResolutionReason,
}
```

`ResolutionReason::Manual.note` must be non-empty after trim.

Provide:

```rust
pub fn close(mut self, at: Timestamp) -> Result<Self, DomainError>
```

which requires `recorded_until.is_none()` and `at > recorded_from`.

- [ ] **Step 7: Run tests**

```bash
cargo test -p vestrace-domain cognition
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vestrace-domain/src/cognition

git commit -m "feat(cognition): add evidence claims slots and resolutions"
```

---

### Task 3: Add PostgreSQL canonical cognition schema

**Files:**
- Create: `migrations/0115_persistent_cognition_foundation.sql`
- Create: `tests/cognition_provenance.rs`
- Create: `tests/cognition_temporal.rs`

**Interfaces:**
- Consumes: existing `workspaces`, `events`, `memories`, `memory_revisions`, RLS helper `vestrace_current_workspace_id()`.
- Produces: canonical tables `evidence_records`, `knowledge_slots`, `claims`, `claim_evidence`, `slot_resolutions`, and compatibility table `memory_claim_links`.

- [ ] **Step 1: Write database tests that fail before migration 0115 exists**

Tests must assert:

```text
cross-workspace Event -> Evidence reference fails
cross-workspace Claim -> Slot reference fails
cross-workspace ClaimEvidence links fail
resolution claim must belong to the same slot
invalid valid interval fails
invalid recorded interval fails
current overlapping Accepted resolutions fail for a Single slot
adjacent Accepted intervals succeed
Multiple slot allows overlapping accepted values
RLS hides every new cognition table across workspaces
```

- [ ] **Step 2: Add composite identity constraints needed for workspace-safe FKs**

In migration `0115`, add:

```sql
ALTER TABLE events
    ADD CONSTRAINT uq_events_id_workspace UNIQUE (id, workspace_id);

ALTER TABLE memories
    ADD CONSTRAINT uq_memories_id_workspace UNIQUE (id, workspace_id);

ALTER TABLE memory_revisions
    ADD CONSTRAINT uq_memory_revisions_id_workspace_memory
    UNIQUE (id, workspace_id, memory_id);
```

These are forward-only additions; do not edit prior migrations.

- [ ] **Step 3: Create `evidence_records`**

```sql
CREATE TABLE evidence_records (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_event_id UUID NOT NULL,
    observed_at TIMESTAMPTZ,
    recorded_at TIMESTAMPTZ NOT NULL,
    content_hash TEXT,
    CONSTRAINT uq_evidence_id_workspace UNIQUE (id, workspace_id),
    CONSTRAINT fk_evidence_event_workspace
        FOREIGN KEY (source_event_id, workspace_id)
        REFERENCES events(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT chk_evidence_sha256
        CHECK (content_hash IS NULL OR content_hash ~ '^[0-9a-f]{64}$')
);
```

Add indexes:

```sql
CREATE INDEX idx_evidence_workspace_recorded
    ON evidence_records (workspace_id, recorded_at DESC);
CREATE INDEX idx_evidence_workspace_event
    ON evidence_records (workspace_id, source_event_id);
```

- [ ] **Step 4: Create `knowledge_slots`**

Use columns:

```text
id UUID PK
workspace_id UUID
scope_kind TEXT
scope_key TEXT
subject_key TEXT
predicate_key TEXT
cardinality TEXT CHECK single|multiple
value_schema TEXT
resolution_policy TEXT CHECK manual
 temporal_policy TEXT CHECK atemporal|valid_time
version BIGINT CHECK version >= 0
created_at TIMESTAMPTZ
```

Add:

```sql
UNIQUE (id, workspace_id)
UNIQUE (workspace_id, scope_kind, scope_key, subject_key, predicate_key)
```

- [ ] **Step 5: Create `claims`**

Use:

```sql
CREATE TABLE claims (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    slot_id UUID NOT NULL,
    value JSONB NOT NULL,
    value_schema TEXT NOT NULL,
    canonical_text TEXT NOT NULL CHECK (btrim(canonical_text) <> ''),
    asserted_valid_from TIMESTAMPTZ,
    asserted_valid_until TIMESTAMPTZ,
    observed_at TIMESTAMPTZ,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    importance REAL NOT NULL CHECK (importance >= 0.0 AND importance <= 1.0),
    derivation_id UUID REFERENCES derivations(id) ON DELETE SET NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT uq_claim_id_workspace UNIQUE (id, workspace_id),
    CONSTRAINT uq_claim_id_workspace_slot UNIQUE (id, workspace_id, slot_id),
    CONSTRAINT fk_claim_slot_workspace
        FOREIGN KEY (slot_id, workspace_id)
        REFERENCES knowledge_slots(id, workspace_id)
        ON DELETE CASCADE,
    CONSTRAINT chk_claim_validity
        CHECK (
            asserted_valid_until IS NULL OR
            asserted_valid_from IS NULL OR
            asserted_valid_until > asserted_valid_from
        )
);
```

Add a trigger that rejects a Claim whose `value_schema` differs from its KnowledgeSlot's `value_schema`.

- [ ] **Step 6: Create `claim_evidence`**

Use composite workspace-safe foreign keys to `claims(id, workspace_id)` and `evidence_records(id, workspace_id)`. Add:

```text
role TEXT NOT NULL
support_strength REAL CHECK 0..1
locator JSONB
created_at TIMESTAMPTZ
UNIQUE (claim_id, evidence_id, role)
```

- [ ] **Step 7: Create `slot_resolutions`**

Use:

```text
id UUID PK
workspace_id UUID
slot_id UUID
claim_id UUID
disposition accepted|contested
valid_from TIMESTAMPTZ NULL
valid_until TIMESTAMPTZ NULL
recorded_from TIMESTAMPTZ NOT NULL
recorded_until TIMESTAMPTZ NULL
reason JSONB NOT NULL
```

The FK from resolution to Claim must be:

```sql
FOREIGN KEY (claim_id, workspace_id, slot_id)
REFERENCES claims(id, workspace_id, slot_id)
```

Add checks:

```sql
CHECK (valid_until IS NULL OR valid_from IS NULL OR valid_until > valid_from)
CHECK (recorded_until IS NULL OR recorded_until > recorded_from)
```

Indexes:

```sql
CREATE INDEX idx_resolutions_current_slot
    ON slot_resolutions (workspace_id, slot_id, disposition)
    WHERE recorded_until IS NULL;

CREATE INDEX idx_resolutions_system_time
    ON slot_resolutions (workspace_id, slot_id, recorded_from, recorded_until);

CREATE INDEX idx_resolutions_valid_time
    ON slot_resolutions (workspace_id, slot_id, valid_from, valid_until);
```

- [ ] **Step 8: Enforce single-cardinality current accepted non-overlap**

Create a deferred constraint trigger. For a row where:

```text
disposition = accepted
recorded_until IS NULL
slot.cardinality = single
```

reject another current `Accepted` resolution for the same slot when:

```sql
tstzrange(other.valid_from, other.valid_until, '[)')
&& tstzrange(new.valid_from, new.valid_until, '[)')
```

Raise SQLSTATE `23P01` so tests can treat it as an exclusion/conflict invariant.

Historical rows with `recorded_until IS NOT NULL` are allowed to overlap because they represent different system-time views.

- [ ] **Step 9: Create the legacy compatibility link**

```sql
CREATE TABLE memory_claim_links (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    memory_id UUID NOT NULL,
    revision_id UUID NOT NULL,
    claim_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, memory_id, revision_id, claim_id),
    FOREIGN KEY (memory_id, workspace_id)
        REFERENCES memories(id, workspace_id) ON DELETE CASCADE,
    FOREIGN KEY (revision_id, workspace_id, memory_id)
        REFERENCES memory_revisions(id, workspace_id, memory_id) ON DELETE CASCADE,
    FOREIGN KEY (claim_id, workspace_id)
        REFERENCES claims(id, workspace_id) ON DELETE CASCADE
);
```

- [ ] **Step 10: Enable and force workspace RLS on all new tables**

For each new cognition table:

```sql
ALTER TABLE <table> ENABLE ROW LEVEL SECURITY;
ALTER TABLE <table> FORCE ROW LEVEL SECURITY;
CREATE POLICY <table>_workspace_isolation ON <table>
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
```

- [ ] **Step 11: Run migration/database tests**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test cognition_provenance --test cognition_temporal -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 12: Commit**

```bash
git add migrations/0115_persistent_cognition_foundation.sql tests/cognition_provenance.rs tests/cognition_temporal.rs

git commit -m "feat(storage): add persistent cognition foundation"
```

---

### Task 4: Define cognition commands, queries, and repository ports

**Files:**
- Create: `crates/vestrace-application/src/cognition/mod.rs`
- Create: `crates/vestrace-application/src/cognition/commands.rs`
- Create: `crates/vestrace-application/src/cognition/queries.rs`
- Create: `crates/vestrace-application/src/cognition/ports.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Consumes: domain cognition types from Tasks 1-2.
- Produces: stable application interfaces used by PostgreSQL adapters and service tests.

- [ ] **Step 1: Define write commands**

Use exact command types:

```rust
pub struct RecordEvidenceCommand {
    pub evidence_id: EvidenceId,
    pub source_event_id: EventId,
    pub observed_at: Option<Timestamp>,
    pub content_hash: Option<String>,
    pub idempotency_key: String,
}

pub struct EnsureKnowledgeSlotCommand {
    pub slot_id: KnowledgeSlotId,
    pub scope: KnowledgeScope,
    pub subject: SubjectKey,
    pub predicate: PredicateKey,
    pub cardinality: SlotCardinality,
    pub value_schema: ValueSchemaRef,
    pub temporal_policy: TemporalPolicy,
    pub idempotency_key: String,
}

pub struct ClaimEvidenceInput {
    pub claim_evidence_id: ClaimEvidenceId,
    pub evidence_id: EvidenceId,
    pub role: EvidenceRole,
    pub support_strength: Confidence,
    pub locator: Option<EvidenceLocator>,
}

pub struct ProposeClaimCommand {
    pub claim_id: ClaimId,
    pub slot_id: KnowledgeSlotId,
    pub value: ClaimValue,
    pub canonical_text: String,
    pub asserted_validity: TemporalInterval,
    pub observed_at: Option<Timestamp>,
    pub confidence: Confidence,
    pub importance: Importance,
    pub derivation_id: Option<DerivationId>,
    pub evidence: Vec<ClaimEvidenceInput>,
    pub idempotency_key: String,
}
```

Require at least one `ClaimEvidenceInput` in application validation.

- [ ] **Step 2: Define atomic resolution transition input**

```rust
pub struct NewResolutionInput {
    pub resolution_id: SlotResolutionId,
    pub claim_id: ClaimId,
    pub disposition: ResolutionDisposition,
    pub validity: TemporalInterval,
    pub reason: ResolutionReason,
}

pub struct ApplySlotResolutionCommand {
    pub slot_id: KnowledgeSlotId,
    pub expected_version: u64,
    pub close_resolution_ids: Vec<SlotResolutionId>,
    pub new_resolutions: Vec<NewResolutionInput>,
    pub idempotency_key: String,
}
```

This is an internal/manual atomic state transition, not the future M0.2-B `MemoryMutationSet` proposal protocol.

- [ ] **Step 3: Define read models and queries**

```rust
pub struct KnowledgePointQuery {
    pub slot_id: KnowledgeSlotId,
    pub valid_at: Timestamp,
    pub known_at: Timestamp,
}

pub struct CurrentKnowledgeQuery {
    pub slot_id: KnowledgeSlotId,
    pub at: Timestamp,
}

pub struct KnowledgeTimelineQuery {
    pub slot_id: KnowledgeSlotId,
}

pub struct ExplainKnowledgeQuery {
    pub resolution_id: SlotResolutionId,
}
```

Read model:

```rust
pub struct ResolvedKnowledge {
    pub resolution: SlotResolution,
    pub claim: Claim,
}

pub enum KnowledgeQueryResult {
    Known(Vec<ResolvedKnowledge>),
    Contested(Vec<ResolvedKnowledge>),
    Unknown,
}

pub struct KnowledgeExplanation {
    pub resolution: SlotResolution,
    pub claim: Claim,
    pub evidence: Vec<(ClaimEvidence, Evidence)>,
}
```

For a single-cardinality slot, `Known` normally contains one item; use `Vec` so the same API supports `Multiple` slots.

- [ ] **Step 4: Define read and write ports**

`CognitionRepository` reads:

```rust
async fn find_slot(&self, ctx: &RequestContext, id: KnowledgeSlotId) -> Result<Option<KnowledgeSlot>, ApplicationError>;
async fn query_point(&self, ctx: &RequestContext, query: &KnowledgePointQuery) -> Result<Vec<ResolvedKnowledge>, ApplicationError>;
async fn query_current(&self, ctx: &RequestContext, query: &CurrentKnowledgeQuery) -> Result<Vec<ResolvedKnowledge>, ApplicationError>;
async fn timeline(&self, ctx: &RequestContext, query: &KnowledgeTimelineQuery) -> Result<Vec<SlotResolution>, ApplicationError>;
async fn explain(&self, ctx: &RequestContext, query: &ExplainKnowledgeQuery) -> Result<Option<KnowledgeExplanation>, ApplicationError>;
```

Define a transaction-scoped `CognitionUnitOfWork`/manager for cognition writes. If P0 introduced `MemoryUnitOfWork`, preserve its behavior and either extend it behind a compatibility alias or rename it in one mechanical commit before adding cognition methods; do not maintain two independent authoritative transaction systems.

Required write methods:

```text
find_idempotency
insert_idempotency
insert_outbox
insert_evidence
insert_slot / find_slot_for_update / update_slot
insert_claim
insert_claim_evidence
find_resolution_for_update
close_resolution
insert_resolution
link_memory_claim
commit / rollback
```

- [ ] **Step 5: Add compile-only fake implementations in application tests**

Verify all command/query types and ports can be implemented without depending on SQLx.

- [ ] **Step 6: Run application tests and commit**

```bash
cargo test -p vestrace-application

git add crates/vestrace-application

git commit -m "feat(cognition): define temporal cognition use cases"
```

---

### Task 5: Implement PostgreSQL cognition repository and transactional write adapter

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/cognition_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/cognition_unit_of_work.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Test: `crates/vestrace-infrastructure/tests/postgres.rs`

**Interfaces:**
- Consumes: Task 4 ports, `PgStore::begin_scoped`, migration 0115 tables.
- Produces: `PgCognitionRepository`, `PgCognitionUnitOfWorkManager`.

- [ ] **Step 1: Implement row parsers centrally**

Keep one parser per domain row type:

```text
parse_evidence_row
parse_slot_row
parse_claim_row
parse_claim_evidence_row
parse_resolution_row
```

Do not duplicate string-to-enum mappings between reader and unit-of-work modules; expose them as `pub(super)` helpers from `cognition_repository.rs`.

- [ ] **Step 2: Implement point-in-time query semantics**

For `KnowledgePointQuery(valid_at, known_at)`, select resolutions satisfying:

```sql
recorded_from <= $known_at
AND (recorded_until IS NULL OR recorded_until > $known_at)
AND (valid_from IS NULL OR valid_from <= $valid_at)
AND (valid_until IS NULL OR valid_until > $valid_at)
```

Join Claims and order deterministically by:

```text
disposition accepted before contested
confidence DESC
claim.recorded_at DESC
claim.id ASC
```

- [ ] **Step 3: Implement current query semantics**

`CurrentKnowledgeQuery { at }` is equivalent to:

```text
valid_at = at
known_at = at
```

Do not use `NOW()` inside the repository when the caller supplied `at`; deterministic tests depend on the explicit timestamp.

- [ ] **Step 4: Implement timeline**

Return every resolution for the slot, including historical system-time rows, ordered by:

```text
recorded_from ASC,
valid_from NULLS FIRST,
id ASC
```

- [ ] **Step 5: Implement explain**

Load one resolution, its Claim, all `ClaimEvidence`, and each referenced Evidence. Preserve evidence-role and locator fields. Return `None` if the resolution is not visible in the current workspace.

- [ ] **Step 6: Implement the cognition unit of work**

Begin with `PgStore::begin_scoped(ctx)`. All writes execute on the same `PgConnection`.

`find_slot_for_update` and `find_resolution_for_update` must use `FOR UPDATE`.

`update_slot` performs optimistic version update:

```sql
UPDATE knowledge_slots
SET version = $new_version
WHERE id = $id AND workspace_id = $workspace_id AND version = $expected_version
```

Require exactly one affected row.

- [ ] **Step 7: Run infrastructure tests**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test -p vestrace-infrastructure cognition -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/vestrace-infrastructure

git commit -m "feat(cognition): add postgres cognition adapters"
```

---

### Task 6: Implement cognition write service with atomic slot transitions

**Files:**
- Create: `crates/vestrace-application/src/cognition/service.rs`
- Modify: `crates/vestrace-application/src/cognition/mod.rs`
- Test: `crates/vestrace-application/tests/cognition_service.rs`

**Interfaces:**
- Consumes: commands and `CognitionUnitOfWorkManager` from Task 4.
- Produces: `CognitionService` write methods and deterministic manual resolution behavior.

- [ ] **Step 1: Test Evidence creation does not create knowledge**

Call `record_evidence`; assert fake UoW records an Evidence row and outbox/idempotency rows but no Claim or SlotResolution.

- [ ] **Step 2: Implement `record_evidence`**

Ordering:

```text
BEGIN
idempotency check
construct Evidence(source Event)
insert Evidence
insert outbox topic cognition.evidence.recorded
insert idempotency response
COMMIT
```

- [ ] **Step 3: Test slot idempotency and semantic identity**

`ensure_slot` called twice with the same semantic key and identical definition returns the same logical definition without creating a second row. If the same semantic key is reused with a different cardinality/schema/policy, return `ApplicationError::Conflict`.

- [ ] **Step 4: Implement `ensure_slot`**

Use an in-transaction lookup by semantic key. Do not silently mutate an existing slot definition in M0.2-A.

- [ ] **Step 5: Test Claim proposal requires Evidence**

Call `propose_claim` with `evidence = []`; expect `DomainError::InvalidArgument("claim requires at least one evidence reference".into())` and no writes.

- [ ] **Step 6: Implement `propose_claim`**

Inside one transaction:

```text
idempotency check
lock/read slot
validate claim.value.schema == slot.value_schema
insert Claim
verify/insert each ClaimEvidence link
insert outbox cognition.claim.proposed
insert idempotency response
commit
```

Do not insert a SlotResolution.

- [ ] **Step 7: Test optimistic resolution application**

Start slot version `0`. Apply command with `expected_version=0`; expect version `1`. Repeat a different command with stale `expected_version=0`; expect `RevisionConflict` and no partial resolution changes.

- [ ] **Step 8: Implement `apply_slot_resolution`**

Inside one transaction:

1. idempotency check;
2. load slot `FOR UPDATE`;
3. verify `expected_version`;
4. load every `close_resolution_id FOR UPDATE` and require matching workspace + slot + `recorded_until IS NULL`;
5. close each at one captured `recorded_at` timestamp;
6. construct every new SlotResolution with the same `recorded_from` timestamp;
7. insert new resolutions;
8. bump slot version exactly once;
9. insert outbox `cognition.slot.resolved` containing old/new version and resolution IDs;
10. insert idempotency result;
11. commit.

Database deferred constraints perform final single-cardinality overlap validation at commit.

- [ ] **Step 9: Run service tests and commit**

```bash
cargo test -p vestrace-application --test cognition_service

git add crates/vestrace-application

git commit -m "feat(cognition): implement evidence claim and resolution writes"
```

---

### Task 7: Implement temporal query service and explanation

**Files:**
- Create or extend: `crates/vestrace-application/src/cognition/queries.rs`
- Modify: `crates/vestrace-application/src/cognition/service.rs`
- Test: `crates/vestrace-application/tests/cognition_queries.rs`

**Interfaces:**
- Consumes: `CognitionRepository`.
- Produces: Current/ValidAt/KnownAt/Timeline/Explain application behavior.

- [ ] **Step 1: Implement deterministic classification of repository rows**

Given resolved rows:

```text
no rows -> Unknown
one or more Accepted rows -> Known(accepted rows)
no Accepted but one or more Contested rows -> Contested(contested rows)
```

For a `Single` slot, repository/database invariants guarantee there is at most one current accepted value at a point in valid/system time.

- [ ] **Step 2: Add `get_current`**

Accept caller-supplied `at: Timestamp`; do not hide wall-clock access inside the repository.

- [ ] **Step 3: Add `get_at`**

Expose the full bitemporal point query:

```text
valid_at = modeled-world time
known_at = Vestrace system-time view
```

- [ ] **Step 4: Add `timeline`**

Return all SlotResolutions, including closed historical system-time rows.

- [ ] **Step 5: Add `explain`**

Return `KnowledgeExplanation` preserving:

```text
SlotResolution
Claim
Claim derivation_id
Claim confidence/importance
ClaimEvidence role/support_strength/locator
Evidence source Event reference
Evidence observed_at/recorded_at/content_hash
```

- [ ] **Step 6: Run query tests**

```bash
cargo test -p vestrace-application --test cognition_queries
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-application/src/cognition crates/vestrace-application/tests/cognition_queries.rs

git commit -m "feat(cognition): add bitemporal knowledge queries"
```

---

### Task 8: Add legacy Memory compatibility links without dual truth

**Files:**
- Modify: `crates/vestrace-application/src/cognition/commands.rs`
- Modify: `crates/vestrace-application/src/cognition/ports.rs`
- Modify: `crates/vestrace-application/src/cognition/service.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/cognition_unit_of_work.rs`
- Test: `crates/vestrace-application/tests/cognition_service.rs`
- Test: `tests/cognition_provenance.rs`

**Interfaces:**
- Consumes: `memory_claim_links` from migration 0115.
- Produces: explicit optional correlation between old `MemoryRevision` objects and new Claims.

- [ ] **Step 1: Extend `ProposeClaimCommand` with an optional compatibility reference**

```rust
pub struct LegacyMemoryRef {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
}

pub legacy_memory: Option<LegacyMemoryRef>
```

- [ ] **Step 2: Validate the compatibility link transactionally**

When provided, `link_memory_claim` must rely on the composite database FKs to require:

```text
same workspace
revision belongs to memory
claim belongs to workspace
```

- [ ] **Step 3: Do not auto-create legacy Memory from every Claim**

Add a test proving a Claim without `legacy_memory` creates no `memories` or `memory_revisions` rows. This prevents dual source-of-truth behavior.

- [ ] **Step 4: Add a positive link test**

Create a legacy MemoryRevision and a Claim, link them, and verify exactly one `memory_claim_links` row.

- [ ] **Step 5: Run tests and commit**

```bash
cargo test -p vestrace-application --test cognition_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test cognition_provenance -- --test-threads=1

git add crates/vestrace-application crates/vestrace-infrastructure tests/cognition_provenance.rs

git commit -m "feat(cognition): link legacy memory to canonical claims"
```

---

### Task 9: Wire cognition services into the runtime without exposing unstable public API

**Files:**
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Consumes: `PgCognitionRepository`, `PgCognitionUnitOfWorkManager`, `CognitionService`.
- Produces: constructed internal service ready for acceptance tests and subsequent HTTP/MCP API work.

- [ ] **Step 1: Export the new adapters**

Re-export `PgCognitionRepository` and `PgCognitionUnitOfWorkManager` from the postgres module/crate root.

- [ ] **Step 2: Construct the CognitionService in server wiring**

Use the same `PgStore`/pool as State Engine and legacy Memory Service.

Do not add new HTTP routes in M0.2-A unless needed by an existing acceptance harness; the canonical acceptance test may call application services directly.

- [ ] **Step 3: Keep legacy memory wiring unchanged**

Existing `/v1/memories`, MCP `search_memories`, and `get_memory` remain compatibility surfaces until a later explicit API spec.

- [ ] **Step 4: Run workspace check and commit**

```bash
cargo check --workspace --all-targets

git add crates/vestrace-cli crates/vestrace-application/src/lib.rs crates/vestrace-infrastructure/src/postgres/mod.rs

git commit -m "refactor(wiring): add persistent cognition services"
```

---

### Task 10: Implement the canonical SQLite -> PostgreSQL bitemporal acceptance scenario

**Files:**
- Modify: `tests/cognition_temporal.rs`
- Modify: `tests/cognition_provenance.rs`

**Interfaces:**
- Consumes: complete M0.2-A domain/application/PostgreSQL vertical.
- Produces: executable proof of Persistent Cognition temporal semantics.

- [ ] **Step 1: Seed deterministic timestamps**

Use exact UTC times:

```text
T1 = 2026-08-01T00:00:00Z
T5 = 2026-08-05T00:00:00Z
T7 = 2026-08-07T00:00:00Z
T8 = 2026-08-08T00:00:00Z
```

- [ ] **Step 2: Record initial SQLite Evidence and Claim**

Create Event/Evidence E1 and Claim C1:

```text
slot: project:vestrace / project:vestrace / database.primary
value_schema: vestrace.scalar.string.v1
value: "SQLite"
asserted validity: [T1, infinity)
```

Apply accepted Resolution R1:

```text
valid [T1, infinity)
system [T1, infinity)
```

- [ ] **Step 3: Assert the pre-correction view**

```text
ValidAt(T7), KnownAt(T7) -> SQLite
```

- [ ] **Step 4: Record PostgreSQL migration Evidence learned at T8**

Create E2/C2:

```text
value: "PostgreSQL"
asserted validity: [T5, infinity)
Evidence recorded at T8
```

- [ ] **Step 5: Apply one atomic correction at system time T8**

The `ApplySlotResolutionCommand` must:

```text
close R1 at T8
insert R1b for C1: SQLite valid [T1, T5), system [T8, infinity)
insert R2  for C2: PostgreSQL valid [T5, infinity), system [T8, infinity)
bump slot version once
```

- [ ] **Step 6: Assert all canonical queries**

The test must assert exactly:

```text
Current(at=T8)                     -> PostgreSQL
ValidAt(T3), KnownAt(T8)           -> SQLite
ValidAt(T6), KnownAt(T8)           -> PostgreSQL
ValidAt(T6), KnownAt(T7)           -> SQLite
Timeline                           -> R1 + R1b + R2 with system intervals
```

where `T3 = 2026-08-03T00:00:00Z` and `T6 = 2026-08-06T00:00:00Z`.

- [ ] **Step 7: Assert explanation**

`Explain(R2)` must return:

```text
R2
-> Claim C2
-> ClaimEvidence link
-> Evidence E2
-> source Event ID
```

and preserve `derivation_id`, evidence role, support strength, and recorded/observed timestamps.

- [ ] **Step 8: Assert historical system knowledge survives correction**

After the T8 correction, query `ValidAt(T6), KnownAt(T7)` again and assert it still returns SQLite. This guards against destructive update implementations that accidentally erase the previous belief state.

- [ ] **Step 9: Run the acceptance tests**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test cognition_temporal --test cognition_provenance -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add tests/cognition_temporal.rs tests/cognition_provenance.rs

git commit -m "test(cognition): prove bitemporal knowledge evolution"
```

---

### Task 11: Synchronize documentation and run the M0.2-A exit gate

**Files:**
- Modify: `docs/domain-model.md`
- Modify: `docs/database-schema.md`
- Modify: `docs/architecture.md`

**Interfaces:**
- Consumes: completed M0.2-A implementation.
- Produces: documentation matching the canonical cognition model and explicit boundary with M0.2-B.

- [ ] **Step 1: Document the canonical chain**

Add:

```text
Evidence -> Claim -> KnowledgeSlot -> SlotResolution -> Knowledge Query
```

State explicitly:

```text
Evidence != Claim
Claim != accepted Knowledge
```

- [ ] **Step 2: Document the two time axes**

Define:

```text
valid time  = when the state applies in the modeled world
system time = when Vestrace held that resolution as its knowledge view
```

Document `[from, until)` semantics.

- [ ] **Step 3: Document compatibility status**

Mark `Memory`/`MemoryRevision` as retained cognitive-object/compatibility constructs. `memory_claim_links` correlates legacy records with Claims but does not make the legacy Memory row canonical truth for atomic knowledge.

- [ ] **Step 4: Document the deliberate M0.2-A limitation**

State plainly:

```text
M0.2-A resolution is manual/internal and atomic.
Automatic contradiction detection and reconciliation are M0.2-B.
```

- [ ] **Step 5: Run static verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 6: Run full database verification**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --workspace --all-targets -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 7: Verify migration compatibility**

Run the existing CLI/database readiness path and confirm migration `0115` is applied and checksum-compatible. Do not edit `0115` after it has shipped to a shared environment; corrections after that point require `0116+`.

- [ ] **Step 8: Commit documentation**

```bash
git add docs/domain-model.md docs/database-schema.md docs/architecture.md

git commit -m "docs: document temporal cognition foundation"
```

## M0.2-A Exit Gate

Do not start M0.2-B until all conditions are true:

```text
[ ] Evidence is immutable and workspace-safe
[ ] Claim creation never auto-accepts knowledge
[ ] every Claim has at least one ClaimEvidence link
[ ] Claim derivation is independent from evidence role
[ ] KnowledgeSlot semantic identity is unique per workspace
[ ] slot writes use optimistic versioning
[ ] valid-time intervals use [from, until)
[ ] system-time history is preserved after corrections
[ ] Single slot current Accepted intervals cannot overlap
[ ] Current query returns the current accepted value
[ ] ValidAt query returns modeled-world historical state
[ ] KnownAt reconstructs the prior Vestrace belief state
[ ] ValidAt+KnownAt works after a later correction
[ ] Timeline returns closed and current system-time resolutions
[ ] Explain traces Resolution -> Claim -> ClaimEvidence -> Evidence -> Event
[ ] cross-workspace cognition references fail at database level
[ ] RLS isolates all new cognition tables
[ ] legacy Memory API remains functional
[ ] no automatic reconciliation is smuggled into this slice
[ ] SQLite -> PostgreSQL acceptance scenario passes end-to-end
[ ] cargo fmt/clippy/test pass
[ ] full PostgreSQL integration suite passes
```
