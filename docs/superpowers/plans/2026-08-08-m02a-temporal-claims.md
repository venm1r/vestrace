# M0.2-A Temporal Claims Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce Vestrace's canonical Persistent Cognition foundation — immutable Evidence, Claims with claim-specific provenance, stable KnowledgeSlots, bitemporal SlotResolutions, historical queries, and provenance explanation — while preserving the v0.1 Memory API as a compatibility surface.

**Architecture:** Add a new `cognition` bounded context beside the existing legacy `memory` module. PostgreSQL remains authoritative; M0.2-A stores relational canonical cognition and uses explicit valid-time plus system-time intervals. Resolution is deliberately manual/internal in this slice: M0.2-A establishes correct semantics and atomic transitions; automatic reconciliation belongs to M0.2-B. The `MemoryUnitOfWork` introduced by P0 remains the **single authoritative transaction boundary** and is extended with cognition operations; M0.2-A does not create a parallel cognition-specific unit-of-work system.

**Tech Stack:** Rust 2024, Serde/Schemars, Tokio, async-trait, SQLx 0.8, PostgreSQL `TIMESTAMPTZ`/JSONB/RLS, existing scoped transaction infrastructure, UUIDv7 IDs.

## Global Constraints

- P0 Memory Write Integrity must be complete before this plan starts.
- Vestrace remains a cognition-centered modular monolith.
- PostgreSQL is the authoritative cognition store.
- Existing migrations are immutable; add only forward migrations beginning after current migration `0114`.
- `MemoryUnitOfWork` / `PgMemoryUnitOfWorkManager` remain the only authoritative write transaction mechanism through M0.2-A.
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
- Slot versioning uses `u32` to remain compatible with the existing `DomainError::RevisionConflict { expected: u32, current: u32 }` contract.
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

migrations/0115_persistent_cognition_foundation.sql

tests/cognition_temporal.rs
tests/cognition_provenance.rs
```

Modify:

```text
crates/vestrace-domain/src/id.rs
crates/vestrace-domain/src/lib.rs
crates/vestrace-application/src/lib.rs
crates/vestrace-application/src/memory/unit_of_work.rs
crates/vestrace-infrastructure/src/postgres/memory_unit_of_work.rs
crates/vestrace-infrastructure/src/postgres/mod.rs
crates/vestrace-cli/src/commands/server.rs
docs/domain-model.md
docs/database-schema.md
docs/architecture.md
```

The legacy `memory/` domain module remains intact in this slice except for an explicit compatibility link near the end of the plan. The P0 memory unit of work is extended rather than replaced.

---

### Task 1: Add cognition identifiers and temporal value objects

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/cognition/mod.rs`
- Create: `crates/vestrace-domain/src/cognition/temporal.rs`
- Create: `crates/vestrace-domain/src/cognition/value.rs`
- Create: `crates/vestrace-domain/src/cognition/scope.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

- [ ] **Step 1: Write failing tests for half-open time intervals**

Cover equality rejection, inclusion of `from`, exclusion of `until`, adjacent non-overlap, and unbounded start/end.

```rust
#[test]
fn interval_rejects_equal_bounds() {
    let t = crate::now();
    assert!(TemporalInterval::new(Some(t), Some(t)).is_err());
}

#[test]
fn adjacent_intervals_do_not_overlap() {
    let a = TemporalInterval::new(Some(ts("2026-08-01T00:00:00Z")), Some(ts("2026-08-05T00:00:00Z"))).unwrap();
    let b = TemporalInterval::new(Some(ts("2026-08-05T00:00:00Z")), None).unwrap();
    assert!(!a.overlaps(&b));
}
```

- [ ] **Step 2: Add UUIDv7 domain IDs**

```rust
domain_id!(EvidenceId);
domain_id!(ClaimId);
domain_id!(ClaimEvidenceId);
domain_id!(KnowledgeSlotId);
domain_id!(SlotResolutionId);
```

- [ ] **Step 3: Implement `TemporalInterval`**

```rust
pub struct TemporalInterval {
    pub from: Option<Timestamp>,
    pub until: Option<Timestamp>,
}
```

Constructor requires `until > from` whenever both are present. Implement deterministic `contains()` and `overlaps()` with `[from, until)` semantics.

- [ ] **Step 4: Add validated semantic keys**

Create transparent newtypes `ScopeKey`, `SubjectKey`, `PredicateKey`, `ValueSchemaRef`. Constructors trim, reject empty values, reject values over 255 bytes, and accept only ASCII alphanumeric plus `.`, `_`, `-`, `:`, `/`.

- [ ] **Step 5: Add scope and slot policy types**

```rust
pub enum ScopeKind {
    Workspace, Team, Project, Repository, Deployment,
    User, Agent, Conversation, Execution, Custom,
}

pub struct KnowledgeScope {
    pub kind: ScopeKind,
    pub key: ScopeKey,
}

pub enum SlotCardinality { Single, Multiple }
pub enum ResolutionPolicy { Manual }
pub enum TemporalPolicy { Atemporal, ValidTime }
```

`ResolutionPolicy::Manual` is intentionally the only M0.2-A policy.

- [ ] **Step 6: Add schema-qualified `ClaimValue`**

```rust
pub struct ClaimValue {
    pub schema: ValueSchemaRef,
    pub value: serde_json::Value,
}
```

M0.2-A validates presence/equality of schema identity but does not implement arbitrary JSON Schema evaluation.

- [ ] **Step 7: Run tests and commit**

```bash
cargo test -p vestrace-domain cognition
git add crates/vestrace-domain
git commit -m "feat(cognition): add temporal and semantic value types"
```

---

### Task 2: Add Evidence, Claim, KnowledgeSlot, ClaimEvidence, and SlotResolution

**Files:**
- Create: `crates/vestrace-domain/src/cognition/evidence.rs`
- Create: `crates/vestrace-domain/src/cognition/slot.rs`
- Create: `crates/vestrace-domain/src/cognition/claim.rs`
- Create: `crates/vestrace-domain/src/cognition/resolution.rs`
- Modify: `crates/vestrace-domain/src/cognition/mod.rs`

- [ ] **Step 1: Write constructor/invariant tests first**

Cover: blank Claim text rejected; invalid evidence content hash rejected; invalid locator rejected; resolution closing twice rejected; resolution cannot close at/before `recorded_from`; slot revision conflict reports the existing `u32` `DomainError::RevisionConflict`.

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

When present, `content_hash` is lowercase 64-character SHA-256 hex.

- [ ] **Step 3: Implement KnowledgeSlot with `u32` optimistic version**

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
    pub version: u32,
    pub created_at: Timestamp,
}
```

Provide `bump_version(expected: u32) -> Result<Self, DomainError>`. Mismatch maps to the existing `DomainError::RevisionConflict`.

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

Blank `canonical_text` is invalid.

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

`TextRange` requires `end > start`; JSON Pointer requires a non-empty string beginning with `/`. Claim derivation remains independent from evidence role.

- [ ] **Step 6: Implement SlotResolution**

```rust
pub enum ResolutionDisposition { Accepted, Contested }

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

`Manual.note` must be non-empty. `close(at)` requires an open resolution and `at > recorded_from`.

- [ ] **Step 7: Run tests and commit**

```bash
cargo test -p vestrace-domain cognition
git add crates/vestrace-domain/src/cognition
git commit -m "feat(cognition): add evidence claims slots and resolutions"
```

---

### Task 3: Add PostgreSQL canonical cognition schema

**Files:**
- Create: `migrations/0115_persistent_cognition_foundation.sql`
- Create: `tests/cognition_provenance.rs`
- Create: `tests/cognition_temporal.rs`

- [ ] **Step 1: Write failing DB tests before migration implementation**

Prove: cross-workspace references fail; Claim must reference a same-workspace Slot; resolution Claim must belong to its Slot; invalid valid/system intervals fail; Single current Accepted overlap fails; adjacent intervals succeed; Multiple slots allow overlapping accepted values; all new tables obey RLS.

- [ ] **Step 2: Add composite identities for workspace-safe foreign keys**

Add forward-only unique constraints to existing authoritative tables as needed:

```sql
ALTER TABLE events
    ADD CONSTRAINT uq_events_id_workspace UNIQUE (id, workspace_id);
ALTER TABLE memories
    ADD CONSTRAINT uq_memories_id_workspace UNIQUE (id, workspace_id);
ALTER TABLE memory_revisions
    ADD CONSTRAINT uq_memory_revisions_id_workspace_memory
    UNIQUE (id, workspace_id, memory_id);
```

- [ ] **Step 3: Create `evidence_records`**

Required columns: `id`, `workspace_id`, `source_event_id`, `observed_at`, `recorded_at`, `content_hash`. Use composite `(source_event_id, workspace_id) -> events(id, workspace_id)` and `ON DELETE RESTRICT`.

- [ ] **Step 4: Create `knowledge_slots`**

Required columns:

```text
id UUID PK
workspace_id UUID
scope_kind TEXT
scope_key TEXT
subject_key TEXT
predicate_key TEXT
cardinality single|multiple
value_schema TEXT
resolution_policy manual
temporal_policy atemporal|valid_time
version INT NOT NULL CHECK (version >= 0)
created_at TIMESTAMPTZ
```

Add unique semantic identity:

```sql
UNIQUE (workspace_id, scope_kind, scope_key, subject_key, predicate_key)
```

and `UNIQUE (id, workspace_id)`.

- [ ] **Step 5: Create `claims`**

Include `value JSONB`, `value_schema`, `canonical_text`, asserted valid bounds, `observed_at`, `confidence`, `importance`, optional `derivation_id`, `recorded_at`. Add composite Slot FK and `UNIQUE (id, workspace_id, slot_id)`. Add a trigger rejecting `claims.value_schema != knowledge_slots.value_schema`.

- [ ] **Step 6: Create `claim_evidence`**

Use composite workspace-safe FKs to Claims and Evidence. Store `role`, `support_strength`, optional locator JSONB, and timestamp. Add `UNIQUE (claim_id, evidence_id, role)`.

- [ ] **Step 7: Create `slot_resolutions`**

Required columns:

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

Use `(claim_id, workspace_id, slot_id) -> claims(id, workspace_id, slot_id)` and strict interval checks.

- [ ] **Step 8: Enforce Single-slot current Accepted non-overlap**

Use a deferred constraint trigger. It applies only when `disposition='accepted'`, `recorded_until IS NULL`, and slot cardinality is `single`. Reject overlapping `[valid_from, valid_until)` intervals using PostgreSQL range overlap semantics. Historical rows with closed system time may overlap because they describe different system-time views.

- [ ] **Step 9: Create `memory_claim_links` compatibility table**

Use composite FKs to `memories`, `memory_revisions`, and `claims`. This table correlates legacy cognitive objects with Claims; it does not create dual canonical truth.

- [ ] **Step 10: Enable and force RLS on all new cognition tables**

Use the existing workspace policy helper. Every new authoritative cognition table has `ENABLE ROW LEVEL SECURITY`, `FORCE ROW LEVEL SECURITY`, and matching `USING/WITH CHECK` predicates.

- [ ] **Step 11: Run DB tests and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test cognition_provenance --test cognition_temporal -- --test-threads=1

git add migrations/0115_persistent_cognition_foundation.sql tests/cognition_provenance.rs tests/cognition_temporal.rs
git commit -m "feat(storage): add persistent cognition foundation"
```

---

### Task 4: Define cognition commands, queries, and extend the single Unit of Work

**Files:**
- Create: `crates/vestrace-application/src/cognition/mod.rs`
- Create: `crates/vestrace-application/src/cognition/commands.rs`
- Create: `crates/vestrace-application/src/cognition/queries.rs`
- Create: `crates/vestrace-application/src/cognition/ports.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Modify: `crates/vestrace-application/src/memory/unit_of_work.rs`

- [ ] **Step 1: Define write commands**

Create `RecordEvidenceCommand`, `EnsureKnowledgeSlotCommand`, `ProposeClaimCommand`, `ClaimEvidenceInput`, and `LegacyMemoryRef`. `ProposeClaimCommand` requires at least one evidence input at application validation time.

- [ ] **Step 2: Define manual atomic resolution transition**

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
    pub expected_version: u32,
    pub close_resolution_ids: Vec<SlotResolutionId>,
    pub new_resolutions: Vec<NewResolutionInput>,
    pub idempotency_key: String,
}
```

This is internal/manual M0.2-A state application, not the M0.2-B `MemoryMutationSet` protocol.

- [ ] **Step 3: Define read queries/models**

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

pub struct KnowledgeTimelineQuery { pub slot_id: KnowledgeSlotId }
pub struct ExplainKnowledgeQuery { pub resolution_id: SlotResolutionId }
```

Use `ResolvedKnowledge`, `KnowledgeQueryResult::{Known, Contested, Unknown}`, and `KnowledgeExplanation`.

- [ ] **Step 4: Define read-only `CognitionRepository`**

It exposes Slot lookup, bitemporal point query, current query, timeline, and explain. It does not own authoritative writes.

- [ ] **Step 5: Extend P0 `MemoryUnitOfWork` with cognition write methods**

Add methods for:

```text
insert_evidence
find_slot_by_semantic_key_for_update
find_slot_for_update
insert_slot
update_slot(expected_version: u32, new_version: u32)
insert_claim
insert_claim_evidence
find_resolution_for_update
close_resolution
insert_resolution
link_memory_claim
```

Retain the existing memory/event/outbox/idempotency methods. Do **not** define `CognitionUnitOfWork`, `CognitionUnitOfWorkManager`, or any second transactional abstraction.

- [ ] **Step 6: Add compile-time fake tests and run**

```bash
cargo test -p vestrace-application
git add crates/vestrace-application
git commit -m "feat(cognition): define temporal cognition use cases"
```

---

### Task 5: Implement PostgreSQL reads and extend `PgMemoryUnitOfWork`

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/cognition_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/memory_unit_of_work.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Test: `crates/vestrace-infrastructure/tests/postgres.rs`

- [ ] **Step 1: Implement shared cognition row parsers**

Keep one parser per canonical cognition row type and share enum/string mappings between read repository and UoW code. Do not duplicate interpretation logic.

- [ ] **Step 2: Implement point-in-time query semantics**

For `valid_at` + `known_at`, select resolutions where:

```sql
recorded_from <= $known_at
AND (recorded_until IS NULL OR recorded_until > $known_at)
AND (valid_from IS NULL OR valid_from <= $valid_at)
AND (valid_until IS NULL OR valid_until > $valid_at)
```

Join Claims and order deterministically: Accepted before Contested, Claim confidence descending, Claim recorded time descending, ID ascending.

- [ ] **Step 3: Implement Current, Timeline, Explain**

Current uses caller-supplied `at` for both valid/system time. Timeline returns all historical and current resolutions ordered by system time. Explain returns Resolution → Claim → ClaimEvidence → Evidence while preserving derivation, role, locator, strength, and timestamps.

- [ ] **Step 4: Extend `PgMemoryUnitOfWork` using the same `PgScopedTransaction`**

All new cognition writes execute against `self.tx.connection()`. Slot/resolution reads used for writes use `FOR UPDATE`.

Slot optimistic update uses INT/u32 semantics:

```sql
UPDATE knowledge_slots
SET version = $new_version
WHERE id = $id
  AND workspace_id = $workspace_id
  AND version = $expected_version
```

Require exactly one affected row; stale versions map to `DomainError::RevisionConflict` with `u32` values.

- [ ] **Step 5: Add transaction rollback/optimistic conflict tests**

Prove that a failed resolution insert rolls back prior resolution closes and slot version changes. Prove a stale expected version makes no writes.

- [ ] **Step 6: Run infrastructure tests and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test -p vestrace-infrastructure cognition -- --test-threads=1

git add crates/vestrace-infrastructure
git commit -m "feat(cognition): add postgres cognition persistence"
```

---

### Task 6: Implement CognitionService write behavior

**Files:**
- Create: `crates/vestrace-application/src/cognition/service.rs`
- Modify: `crates/vestrace-application/src/cognition/mod.rs`
- Test: `crates/vestrace-application/tests/cognition_service.rs`

**Interfaces:**
- Depends on `CognitionRepository` for reads and the existing `MemoryUnitOfWorkManager` for authoritative writes.

- [ ] **Step 1: Test that Evidence creation cannot create knowledge**

`record_evidence` inserts Evidence + outbox + idempotency only; no Claim or SlotResolution.

- [ ] **Step 2: Implement `record_evidence` atomically**

```text
BEGIN scoped UoW
idempotency check
construct Evidence(Event source)
insert Evidence
outbox cognition.evidence.recorded
idempotency response
COMMIT
```

- [ ] **Step 3: Test/implement `ensure_slot` semantic identity**

Same semantic key + same definition is idempotent. Same key + changed schema/cardinality/policies returns `ApplicationError::Conflict`; M0.2-A never silently mutates slot definition.

- [ ] **Step 4: Test Claim proposal requires Evidence**

Empty evidence vector returns `DomainError::InvalidArgument` before writes.

- [ ] **Step 5: Implement `propose_claim`**

Within one UoW: idempotency check → lock/read Slot → validate ClaimValue schema equals Slot schema → insert Claim → insert all ClaimEvidence links → optional legacy memory link → outbox `cognition.claim.proposed` → idempotency → commit. Do not create SlotResolution.

- [ ] **Step 6: Test and implement atomic `apply_slot_resolution`**

Within one UoW:

1. idempotency check;
2. load Slot `FOR UPDATE`;
3. validate `expected_version: u32`;
4. lock each resolution to close and ensure same workspace/slot/open state;
5. capture one system timestamp;
6. close old resolutions;
7. insert all new resolutions with the same `recorded_from`;
8. bump slot version exactly once;
9. outbox `cognition.slot.resolved` with old/new versions and resolution IDs;
10. idempotency result;
11. commit.

Deferred DB constraints validate Single-slot non-overlap at commit.

- [ ] **Step 7: Run tests and commit**

```bash
cargo test -p vestrace-application --test cognition_service
git add crates/vestrace-application
git commit -m "feat(cognition): implement evidence claim and resolution writes"
```

---

### Task 7: Implement temporal query classification and explanation

**Files:**
- Modify: `crates/vestrace-application/src/cognition/queries.rs`
- Modify: `crates/vestrace-application/src/cognition/service.rs`
- Test: `crates/vestrace-application/tests/cognition_queries.rs`

- [ ] **Step 1: Classify read results deterministically**

```text
no rows -> Unknown
one or more Accepted -> Known(accepted rows)
no Accepted + one or more Contested -> Contested(contested rows)
```

- [ ] **Step 2: Implement `get_current(at)`**

Caller supplies `at`; do not hide `NOW()` in repository logic.

- [ ] **Step 3: Implement full `get_at(valid_at, known_at)`**

Preserve the two axes explicitly in API and tests.

- [ ] **Step 4: Implement Timeline and Explain**

Timeline includes closed system-time states. Explain returns Resolution, Claim, derivation, ClaimEvidence roles/strength/locator, Evidence source/timestamps/hash.

- [ ] **Step 5: Run tests and commit**

```bash
cargo test -p vestrace-application --test cognition_queries
git add crates/vestrace-application/src/cognition crates/vestrace-application/tests/cognition_queries.rs
git commit -m "feat(cognition): add bitemporal knowledge queries"
```

---

### Task 8: Preserve legacy Memory compatibility without dual truth

**Files:**
- Modify: `crates/vestrace-application/src/cognition/commands.rs`
- Modify: `crates/vestrace-application/src/cognition/service.rs`
- Modify: `crates/vestrace-application/src/memory/unit_of_work.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/memory_unit_of_work.rs`
- Test: `tests/cognition_provenance.rs`

- [ ] **Step 1: Add optional `LegacyMemoryRef` to Claim proposal**

```rust
pub struct LegacyMemoryRef {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
}
```

- [ ] **Step 2: Validate link through composite FKs**

The link requires same workspace, revision belongs to memory, and Claim belongs to workspace.

- [ ] **Step 3: Prove no automatic legacy Memory is created**

A Claim without `legacy_memory` must create zero `memories`/`memory_revisions` rows.

- [ ] **Step 4: Prove a positive link**

Create one existing MemoryRevision + one Claim and assert exactly one `memory_claim_links` row.

- [ ] **Step 5: Run tests and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test cognition_provenance -- --test-threads=1

git add crates/vestrace-application crates/vestrace-infrastructure tests/cognition_provenance.rs
git commit -m "feat(cognition): link legacy memory to canonical claims"
```

---

### Task 9: Wire cognition services without freezing a public API prematurely

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Modify: `crates/vestrace-cli/src/commands/server.rs`

- [ ] **Step 1: Export `PgCognitionRepository` only**

Continue using the existing `PgMemoryUnitOfWorkManager`; do not export a second manager.

- [ ] **Step 2: Construct CognitionService using the same store/UoW manager as legacy MemoryService**

This guarantees both contexts share one scoped transaction infrastructure.

- [ ] **Step 3: Keep current memory HTTP/MCP contracts unchanged**

Do not add unstable public cognition endpoints in M0.2-A unless an explicit API spec is approved later. Acceptance tests may call application services directly.

- [ ] **Step 4: Run workspace check and commit**

```bash
cargo check --workspace --all-targets
git add crates/vestrace-cli crates/vestrace-application/src/lib.rs crates/vestrace-infrastructure/src/postgres/mod.rs
git commit -m "refactor(wiring): add persistent cognition services"
```

---

### Task 10: Prove SQLite -> PostgreSQL bitemporal evolution end-to-end

**Files:**
- Modify: `tests/cognition_temporal.rs`
- Modify: `tests/cognition_provenance.rs`

- [ ] **Step 1: Use deterministic timestamps**

```text
T1 = 2026-08-01T00:00:00Z
T3 = 2026-08-03T00:00:00Z
T5 = 2026-08-05T00:00:00Z
T6 = 2026-08-06T00:00:00Z
T7 = 2026-08-07T00:00:00Z
T8 = 2026-08-08T00:00:00Z
```

- [ ] **Step 2: Seed SQLite knowledge**

Create Event/Evidence E1, Slot `project:vestrace / project:vestrace / database.primary`, Claim C1 = `"SQLite"`, asserted valid `[T1, ∞)`, and Accepted R1 with valid `[T1, ∞)` and system `[T1, ∞)`.

- [ ] **Step 3: Assert pre-correction system view**

```text
ValidAt(T6), KnownAt(T7) -> SQLite
```

- [ ] **Step 4: Record migration evidence learned at T8**

Create Event/Evidence E2 and Claim C2 = `"PostgreSQL"`, asserted valid `[T5, ∞)`, Evidence recorded at T8.

- [ ] **Step 5: Apply one atomic T8 correction**

Using `expected_version: u32`, close R1 at T8, then insert:

```text
R1b -> C1 SQLite      valid [T1,T5), system [T8,∞)
R2  -> C2 PostgreSQL  valid [T5,∞),  system [T8,∞)
```

Bump Slot version once.

- [ ] **Step 6: Assert canonical queries**

```text
Current(at=T8)                -> PostgreSQL
ValidAt(T3), KnownAt(T8)      -> SQLite
ValidAt(T6), KnownAt(T8)      -> PostgreSQL
ValidAt(T6), KnownAt(T7)      -> SQLite
Timeline                      -> R1 + R1b + R2
```

- [ ] **Step 7: Assert explanation**

`Explain(R2)` traces exactly:

```text
R2 -> Claim C2 -> ClaimEvidence -> Evidence E2 -> source Event
```

and preserves derivation ID, evidence role, support strength, observed/recorded timestamps.

- [ ] **Step 8: Re-run historical query after correction**

After all T8 changes, `ValidAt(T6), KnownAt(T7)` must still return SQLite. This guards against destructive overwrite implementations.

- [ ] **Step 9: Run acceptance tests and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test cognition_temporal --test cognition_provenance -- --test-threads=1

git add tests/cognition_temporal.rs tests/cognition_provenance.rs
git commit -m "test(cognition): prove bitemporal knowledge evolution"
```

---

### Task 11: Synchronize documentation and run the M0.2-A exit gate

**Files:**
- Modify: `docs/domain-model.md`
- Modify: `docs/database-schema.md`
- Modify: `docs/architecture.md`

- [ ] **Step 1: Document the canonical chain**

```text
Evidence -> Claim -> KnowledgeSlot -> SlotResolution -> Knowledge Query
Evidence != Claim
Claim != accepted Knowledge
```

- [ ] **Step 2: Document bitemporal semantics**

Define valid time vs system time and `[from, until)` behavior.

- [ ] **Step 3: Document compatibility**

`Memory`/`MemoryRevision` remain cognitive-object/compatibility constructs. `memory_claim_links` correlates legacy records with Claims but does not make legacy Memory canonical truth for atomic knowledge.

- [ ] **Step 4: Document the M0.2-A limit**

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

- [ ] **Step 6: Run full DB verification**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --workspace --all-targets -- --test-threads=1
```

- [ ] **Step 7: Verify one transaction system by search**

```bash
rg "CognitionUnitOfWork|CognitionUnitOfWorkManager|PgCognitionUnitOfWork" crates
```

Expected: no matches.

```bash
rg "MemoryUnitOfWork|PgMemoryUnitOfWorkManager" crates/vestrace-application crates/vestrace-infrastructure crates/vestrace-cli
```

Expected: memory and cognition writes both use the same transaction infrastructure.

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
[ ] every Claim has at least one ClaimEvidence link through application writes
[ ] Claim derivation is independent from EvidenceRole
[ ] KnowledgeSlot semantic identity is unique per workspace
[ ] KnowledgeSlot version is u32 and uses existing RevisionConflict semantics
[ ] all memory+cognition authoritative writes use one MemoryUnitOfWork system
[ ] Slot writes use optimistic versioning
[ ] valid-time intervals use [from, until)
[ ] system-time history is preserved after corrections
[ ] Single slot current Accepted intervals cannot overlap
[ ] Current returns current accepted value
[ ] ValidAt returns modeled-world historical state
[ ] KnownAt reconstructs prior Vestrace belief state
[ ] ValidAt+KnownAt works after later correction
[ ] Timeline includes closed and current system-time resolutions
[ ] Explain traces Resolution -> Claim -> ClaimEvidence -> Evidence -> Event
[ ] cross-workspace cognition references fail at database level
[ ] RLS isolates all new cognition tables
[ ] legacy Memory API remains functional
[ ] no automatic reconciliation is introduced in this slice
[ ] SQLite -> PostgreSQL acceptance scenario passes end-to-end
[ ] cargo fmt/clippy/test pass
[ ] full PostgreSQL integration suite passes
```
