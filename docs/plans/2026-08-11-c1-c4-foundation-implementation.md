# C1-C4 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status (2026-08-12 audit):** all tasks closed against source. Steps are checked
on the basis of the delivered end state, not on a re-observation of the original
RED runs; the RED-before-GREEN sequence and the original evidence are recorded in
[`../documentation-gap-delta-2026-08-11-c1-c4.md`](../documentation-gap-delta-2026-08-11-c1-c4.md).
Re-verified on 2026-08-12: `Event` exposes `occurred_at: Option<Timestamp>` /
`recorded_at`; `migrations/0120_generic_event_temporal_fields.sql` exists;
`PgMemoryRepository` binds and reads `state_revision`; `Conflict::resolve` and
`accept_ambiguity` are reachable only from `ReconciliationProposed` with a
retained `reconciliation_ref`; `CognitiveMutation::validate_expected_state` is
the single stale-write check. `cargo test -p vestrace-domain` — 165 passed;
`cargo test --test v01_acceptance v01_degraded_embeddings_and_reranker_disabled`
— passed; `cargo test --workspace --no-run` — passed; `git diff --check` — clean.

**Exception:** the Task 2 Step 4 migration assertion
(`cargo test --test migrations …`) still has no host-run evidence — it was
blocked on an unset `DATABASE_URL` in the original slice and was not re-run here.
The column existence is asserted only through the migration file and the
repository mapping, not through a live PostgreSQL run.

**Goal:** Complete the in-progress C1-C4 implementation slice so temporal facts, immutable memory revisions, claim/conflict state, and reconciliation records have compiling runtime paths and mutation-sensitive evidence.

**Architecture:** Preserve the existing Run-first execution boundary and MemoryService. Add generic Event temporal fields at the domain and PostgreSQL repository boundary, keep Memory revisions append-only with explicit aggregate state revisions, and use the existing Claim/Conflict domain types as the source of truth for reconciliation state transitions. Do not introduce a second event store, retrieval authorization system, or any later G/H/E/T phase.

**Tech Stack:** Rust 2024, Serde/Schemars, SQLx/PostgreSQL forward migrations, Cargo tests, existing Vestrace conformance registry and CLI.

## Global Constraints

- `occurred_at` is optional source-world time; `recorded_at` is authoritative Vestrace registration time.
- Unknown occurrence time must remain absent and must not block recording when `recorded_at` is known.
- Semantic Memory changes create a new `MemoryRevision`; old revisions remain queryable.
- `valid_until`, when present with `valid_from`, must not precede `valid_from`.
- Open conflicts remain explicit; no universal latest-timestamp-wins resolution is allowed.
- Reconciliation records preserve inputs, basis, policy or human decision, outcome, actor, and creation time.
- Preserve all unrelated dirty files, generated graphify evidence, and the user’s existing documentation deletions.

---

### Task 1: Make the current C1-C4 delta compile and prove the intended API

**Files:**
- Modify: `tests/v01_acceptance.rs`
- Test: `crates/vestrace-domain/src/event.rs`
- Test: `crates/vestrace-domain/src/claim/claim.rs`
- Test: `crates/vestrace-domain/src/claim/conflict.rs`
- Test: `crates/vestrace-domain/src/claim/mutation.rs`

**Interfaces:**
- Consumes: existing `Event`, `MemoryRevision`, `Claim`, `Conflict`, and `ReconciliationRecord` APIs.
- Produces: focused tests that fail for missing temporal fields and unsafe conflict/reconciliation transitions, plus a synchronized acceptance fixture.

- [x] **Step 1: Add the failing temporal Event test**

```rust
#[test]
fn event_keeps_unknown_occurrence_time_and_records_registration_time() {
    let recorded_at = now();
    let event = Event::new(
        EventId::new(),
        WorkspaceId::new(),
        None,
        "observation",
        ActorRef::System("test".into()),
        serde_json::json!({}),
        recorded_at,
    )
    .unwrap();

    assert_eq!(event.occurred_at, None);
    assert_eq!(event.recorded_at, recorded_at);
}
```

- [x] **Step 2: Add failing claim/conflict tests**

```rust
#[test]
fn conflict_cannot_be_resolved_without_a_reconciliation_reference() {
    let conflict = Conflict::new(
        ConflictId::new(),
        WorkspaceId::new(),
        ConflictKind::SemanticContradiction,
        vec![ClaimId::new(), ClaimId::new()],
        PrincipalId::new(),
        now(),
    );

    assert!(conflict.resolve().is_err());
}
```

Add companion assertions that a valid reconciliation reference permits resolution, a `Claim` keeps its revision history through lifecycle changes, and `ReconciliationRecord` rejects missing authoritative basis for deterministic reconciliation.

- [x] **Step 3: Synchronize the existing acceptance fixture with the current retrieval contract**

Change the stale fixture to pass `revision_id: memory_rev.id` and `conflict_ids: Vec::new()`; do not revert the production retrieval API back to an optional revision.

- [x] **Step 4: Run the focused tests and confirm RED**

Run:

```powershell
cargo test -p vestrace-domain event_keeps_unknown_occurrence_time_and_records_registration_time
cargo test -p vestrace-domain conflict_cannot_be_resolved_without_a_reconciliation_reference
cargo test --test v01_acceptance v01_degraded_embeddings_and_reranker_disabled
```

Expected: the new temporal/conflict tests fail because the required production invariants are not yet implemented; the acceptance test no longer fails at type checking after its fixture is synchronized.

---

### Task 2: Implement generic Event temporal semantics and Memory revision persistence

**Files:**
- Modify: `crates/vestrace-domain/src/event.rs`
- Modify: `crates/vestrace-domain/src/memory/revision.rs`
- Modify: `crates/vestrace-application/src/memory/commands.rs`
- Modify: `crates/vestrace-application/src/memory/services.rs`
- Modify: `crates/vestrace-http/src/api/memory.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/event_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/memory_repository.rs`
- Create: `migrations/0120_generic_event_temporal_fields.sql`
- Test: `tests/migrations.rs`

**Interfaces:**
- Consumes: existing `Event::new`, `MemoryService`, `MemoryRepository`, and SQLx migration loading.
- Produces: `Event { occurred_at: Option<Timestamp>, recorded_at: Timestamp }` and persisted aggregate-level `Memory.state_revision` with forward-compatible backfill.

- [x] **Step 1: Implement the minimum Event temporal fields**

Add `occurred_at: Option<Timestamp>` and `recorded_at: Timestamp` to `Event`. Keep `Event::new` backward-compatible by accepting the existing timestamp as `recorded_at` and defaulting `occurred_at` to `None`; add a constructor or builder for known occurrence time without synthesizing a timestamp when it is unknown.

- [x] **Step 2: Extend the event migration and repository mapping**

Create migration `0120_generic_event_temporal_fields.sql` that adds nullable `occurred_at` and non-null `recorded_at`, backfills `recorded_at = created_at`, and preserves `created_at` for compatibility. Update insert/select mapping and the HTTP response projection so recorded time is not confused with source occurrence time.

- [x] **Step 3: Implement Memory aggregate state-revision persistence**

Bind `Memory.state_revision` in the existing `memories.state_revision` column on insert/update and parse it on reads. Keep `MemoryRevision` append-only and use `Memory.state_revision` as the aggregate-level optimistic-concurrency version; do not add a second revision counter to `MemoryRevision`.

- [x] **Step 4: Run focused green verification**

Run:

```powershell
cargo test -p vestrace-domain event_keeps_unknown_occurrence_time_and_records_registration_time
cargo test -p vestrace-domain memory
cargo test --test migrations migrations_create_required_memory_tables
```

Expected: PASS, with migration tests proving the new columns exist and the domain test proving unknown occurrence time remains `None`.

---

### Task 3: Harden Claim/Conflict and Mutation/Reconciliation semantics

**Files:**
- Modify: `crates/vestrace-domain/src/claim/conflict.rs`
- Modify: `crates/vestrace-domain/src/claim/mutation.rs`
- Modify: `crates/vestrace-domain/src/claim/claim.rs`
- Test: `crates/vestrace-domain/src/claim/claim.rs`
- Test: `crates/vestrace-domain/src/claim/conflict.rs`
- Test: `crates/vestrace-domain/src/claim/mutation.rs`

**Interfaces:**
- Consumes: `ConflictStatus`, `ReconciliationClass`, `ReconciliationOutcome`, and typed `EvidenceRef` values.
- Produces: explicit conflict resolution gating and evidence-preserving reconciliation validation without changing later retrieval or governance APIs.

- [x] **Step 1: Make conflict resolution require a proposed reconciliation**

Allow `Open -> ReconciliationProposed` only through `propose_reconciliation`. Permit `resolve` and `accept_ambiguity` only from `ReconciliationProposed`, with the existing `reconciliation_ref` retained. Keep `obsolete` as an explicit history-preserving terminal transition.

- [x] **Step 2: Validate reconciliation records at construction boundaries**

Reject empty deterministic basis evidence, blank policy versions, and blank human decisions with `DomainError::InvalidArgument`. Preserve all input evidence and outcome fields in successful records. Do not infer a winner from timestamps.

- [x] **Step 3: Verify stale mutation protection**

Keep `CognitiveMutation::validate_expected_state` as the single stale-write check and add tests for matching and mismatching expected revisions. Ensure `defer_to_human` and `accept_ambiguity` remain class-restricted and do not mutate the original conflict retroactively.

- [x] **Step 4: Run focused green verification**

Run:

```powershell
cargo test -p vestrace-domain claim
cargo test -p vestrace-domain conformance
```

Expected: PASS, including mutation-sensitive failures for illegal direct resolution and incomplete reconciliation evidence.

---

### Task 4: Verify the bounded gate and report remaining scope honestly

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-11-c1-c4.md`
- Modify: `docs/documentation-status-v0.2.md`

**Interfaces:**
- Consumes: focused test output, migration output, and the exact implementation diff.
- Produces: a source-based gap delta that distinguishes implemented C1-C4 foundations from still-unimplemented C5+ runtime behavior.

- [x] **Step 1: Run formatting and full compile/test gates**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace --no-run
cargo test -p vestrace-domain
npm run typecheck --prefix apps/console
```

- [x] **Step 2: Run the conformance CLI without overstating qualification**

Run `cargo run -p vestrace-cli -- conformance list --profile memory` and `cargo run -p vestrace-cli -- conformance check --profile memory --json`; record whether results are executable evidence or only static registry coverage.

- [x] **Step 3: Record the source-based gap delta**

Create a dated delta instead of rewriting the large baseline documents. Mark only the requirements proven by current source and tests as implemented/candidate. Keep C5-C8, G/H/E/T phases explicitly pending and preserve the rule that documentation alone does not qualify a profile. Link the delta from `documentation-status-v0.2.md` and state that the observed checkout is dirty/uncommitted.

- [x] **Step 4: Inspect the exact diff and finish with evidence**

Run `git diff --check`, `git status --short`, and a scoped `git diff --stat` for files touched by this plan. Leave unrelated user changes and generated graphify artifacts untouched.
