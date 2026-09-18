# C6 ContextPack 2.0 Assembly Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Assemble bounded ContextPacks with typed sections, exact revision provenance, and a deterministic `Full → Summary → Atomic → Reference` representation ladder.

**Architecture:** Keep `ContextPackBuilder` as the application-owned deterministic assembler. Retrieval candidates carry their authoritative `MemoryKind`; the builder routes candidates to the documented section order instead of treating every result as a constraint. Each item receives a typed `MemoryRevisionRef`, and the builder selects the richest representation that fits the remaining hard budget without invoking a model or inventing summaries from another source.

**Tech Stack:** Rust 2024, Serde/Schemars, existing Vestrace domain retrieval types, unit tests.

## Global Constraints

- `RET-004`: `ContextPack.used_tokens` never exceeds the declared token budget.
- `RET-005`: every `ContextItem` preserves exact source/revision provenance.
- `RET-006`: every included item records its section, representation, and inclusion reason.
- `RET-013`: `Full`, `Summary`, `Atomic`, and `Reference` retain the original source/revision reference.
- Hard section order remains: constraints, current facts, decisions, tasks, procedures, supporting, recent events, history.
- The compression ladder is deterministic and provider-free; no model-generated summary or semantic authority change is introduced.
- Preserve all unrelated dirty files, deleted documentation, and generated graphify evidence.
- Temporal/as-of selection, conflict warnings, classification/model-destination policy, and cross-workspace mounts remain outside this C6 slice.

---

### Task 1: Add failing C6 builder tests

**Files:**
- Modify: `crates/vestrace-application/src/retrieval/context_builder.rs`
- Modify: `crates/vestrace-domain/src/retrieval/mod.rs`
- Modify: retrieval candidate fixtures in `crates/vestrace-application/src/retrieval/{fusion,rerank}.rs`, `tests/v01_acceptance.rs`, and infrastructure mapping

**Interfaces:**
- Consumes: exact-revision C5 candidate contract.
- Produces: tests requiring `MemoryKind` section routing, typed revision provenance, and all four representation levels.

- [x] **Step 1: Write the failing section/provenance test**

Add candidates of kind `Constraint`, `Decision`, and `Task` in non-section order. Build a pack and assert section labels are `constraints`, `decisions`, `tasks`; assert each item has `EvidenceRef::MemoryRevisionRef` for the candidate memory/revision.

- [x] **Step 2: Run the test and confirm RED**

Run:

```powershell
cargo test -p vestrace-application retrieval::context_builder::tests::routes_candidates_and_preserves_revision_provenance
```

Expected: compile or assertion failure because candidates have no typed kind and the current builder puts every item in `constraints` with an empty provenance list.

- [x] **Step 3: Write the failing representation-ladder test**

Use a small budget and a candidate containing multiple sentences. Assert the builder emits `Summary`, `Atomic`, and `Reference` for progressively constrained candidates, and that each item remains within the budget.

- [x] **Step 4: Run the representation test and confirm RED**

Run:

```powershell
cargo test -p vestrace-application retrieval::context_builder::tests::uses_deterministic_representation_ladder
```

Expected: FAIL because the current builder emits `Full` for fitting content and `Reference` for an oversized item without Summary/Atomic stages.

---

### Task 2: Implement typed section routing and provenance

**Files:**
- Modify: `crates/vestrace-domain/src/retrieval/mod.rs`
- Modify: `crates/vestrace-application/src/retrieval/context_builder.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/text_retriever.rs`
- Modify: candidate fixtures in `crates/vestrace-application/src/retrieval/{context_builder,fusion,rerank}.rs` and `tests/v01_acceptance.rs`

**Interfaces:**
- Consumes: Task 1 failing tests.
- Produces: `RetrievalCandidate.kind: MemoryKind`; SQL candidates hydrate `m.kind`; `ContextPackBuilder` routes by kind and populates typed revision provenance.

- [x] **Step 1: Add `MemoryKind` to the candidate contract**

Add a required `kind: MemoryKind` field. Parse `m.kind` in PostgreSQL using the same explicit enum mapping as the memory repository; update synthetic fixtures with their actual kind.

- [x] **Step 2: Implement section mapping**

Map `Constraint → constraints`, `Fact/Preference → current_facts`, `Decision → decisions`, `Task → tasks`, `Procedure → procedures`, `Observation → recent_events`, and `Outcome/Summary → supporting`. Keep the documented order and preserve candidate order within each section.

- [x] **Step 3: Populate exact revision provenance**

For every emitted `ContextItem`, add `EvidenceRef::MemoryRevisionRef { memory_id, revision_id }` unless a future candidate-specific provenance extension exists. Include section and representation in `inclusion_explanation`.

- [x] **Step 4: Run focused green tests**

Run:

```powershell
cargo test -p vestrace-application retrieval::context_builder
cargo test -p vestrace-domain retrieval
```

Expected: PASS, with section order, exact provenance, and C5 workspace behavior preserved.

---

### Task 3: Implement the deterministic representation ladder

**Files:**
- Modify: `crates/vestrace-application/src/retrieval/context_builder.rs`

**Interfaces:**
- Consumes: typed candidates and hard remaining token budget.
- Produces: `Full`, `Summary`, `Atomic`, or `Reference` `ContextItem` values, each with `accounted_tokens <= remaining_budget`.

- [x] **Step 1: Make token counting conservative for non-empty short text**

Use a ceiling byte upper bound so non-empty text never accounts as zero tokens: `max(1, ceil(bytes / 4))`.

- [x] **Step 2: Add deterministic text stages**

Use the full content first, then the first sentence as Summary, then the first clause/line as Atomic, and finally a stable `memory=<id> revision=<id>` Reference. Truncate only at a UTF-8 boundary and account exactly for the selected text.

- [x] **Step 3: Verify all ladder levels and budget behavior**

Run:

```powershell
cargo test -p vestrace-application retrieval::context_builder
```

Expected: PASS with no item exceeding the budget and no representation losing its source/revision ID.

---

### Task 4: Record the C6 delta and verify the bounded gate

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-11-c6-context-pack.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/plans/2026-08-11-c6-context-pack-assembly.md`

**Interfaces:**
- Consumes: focused test output, workspace compile output, and exact diff.
- Produces: source-based C6 status without claiming temporal, conflict, classification, or profile qualification.

- [x] **Step 1: Run scoped formatting and workspace checks**

Run scoped rustfmt, `cargo check --workspace`, `cargo test --workspace --lib`, `cargo test --workspace --no-run`, and `git diff --check`.

- [x] **Step 2: Write the source-based delta**

Record implemented section/provenance/representation behavior and keep RET-001/002/003/007/014 and C7/G2 limits explicit.

- [x] **Step 3: Inspect dirty status and finish without commit**

Preserve the existing dirty worktree and report any PostgreSQL runtime gate separately; do not commit or reset unrelated work.
