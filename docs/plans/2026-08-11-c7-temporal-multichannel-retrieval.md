# C7 Temporal and Multi-channel Retrieval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make retrieval apply an explicit temporal perspective before ranking and support optional exact/vector/structured channels with deterministic fusion and honest degradation.

**Architecture:** Preserve the existing `RetrievalService`, retriever ports, PostgreSQL workspace boundary, and C6 `ContextPackBuilder`. Extend candidates and context items with the revision validity/status metadata needed to annotate historical results. The PostgreSQL text adapter uses active-revision selection for `Current`, validity-window selection for `AsOf`, and revision-history joins for `Timeline`/`AllHistory`; the application service runs configured channels independently, fuses successful results with RRF, and fails closed when no safe channel succeeds.

**Tech Stack:** Rust 2024, Tokio, SQLx PostgreSQL, Axum, Serde/Schemars, existing Vestrace retrieval/domain types.

## Global Constraints

- `RET-002`: temporal perspective is applied before final current-truth assembly; superseded/expired knowledge is never presented as current without explicit status metadata.
- `RET-009`: multi-channel fusion is rank-based and deterministic; a failed derived channel produces a named degraded warning when a safe channel remains.
- `RET-010`: if every configured channel fails, retrieval returns `ApplicationError::Unavailable` instead of an empty successful result.
- `RET-012`: no runtime retrieval cache exists in the inspected implementation; this slice must not claim cache invalidation, and candidate generation metadata is preserved for the future cache boundary.
- `RET-015`: retrieval results and ContextPacks retain perspective, degradation, and source temporal metadata.
- `Current` selects the active revision and active lifecycle status only.
- `AsOf(t)` implements the explicit validity-window interpretation (`VALID_AS_OF`); `KNOWN_AS_OF` and `RECONSTRUCTED_AS_OF` remain separate future modes and must not be silently conflated.
- `Timeline` returns matching revisions in historical order and `AllHistory` may include non-current lifecycle states with explicit status metadata.
- Workspace and request-context checks remain before retrieval, ranking, fusion, or model-facing assembly.
- Preserve all unrelated dirty files, deleted documentation, generated graphify evidence, and the uncommitted C1-C6 work.
- PostgreSQL runtime integration is evidence-gated by `DATABASE_URL`/Docker availability and cannot be replaced by compile-only claims.

---

### Task 1: Add RED tests for temporal normalization, metadata, and channel degradation

**Files:**
- Modify: `crates/vestrace-application/src/retrieval/request.rs`
- Modify: `crates/vestrace-application/src/retrieval/service.rs`
- Modify: `crates/vestrace-domain/src/retrieval/mod.rs`
- Modify: `crates/vestrace-http/src/api/retrieval.rs`

**Interfaces:**
- Consumes: existing `TimePerspective`, retriever ports, and C6 candidate/context contracts.
- Produces: failing tests that define explicit temporal defaults, temporal metadata retention, optional-channel degradation, fail-closed behavior, and HTTP perspective parsing.

- [x] **Step 1: Add temporal request tests**

Add tests asserting `Current` normalizes to `MemoryStatus::Active`, while `AsOf`, `Timeline`, and `AllHistory` do not silently collapse to `Current`. Add a builder test for `with_time_perspective(TimePerspective::AsOf(at))` retaining the exact timestamp.

- [x] **Step 2: Run temporal tests and confirm RED**

Run:

```powershell
cargo test -p vestrace-application retrieval::request
```

Expected: new assertions fail because the request contract does not yet expose historical metadata/default behavior required by C7.

- [x] **Step 3: Add channel service tests**

Create in-memory retriever stubs for text, vector, exact, and structured channels. Assert that successful channels are fused, one failed optional channel sets `degraded = true` and names that channel, and all failed channels return `ApplicationError::Unavailable`.

- [x] **Step 4: Run channel tests and confirm RED**

Run:

```powershell
cargo test -p vestrace-application retrieval::service
```

Expected: compile or assertion failure because `RetrievalService` currently owns only the text channel and treats complete failure as a successful empty result.

- [x] **Step 5: Add API perspective parsing tests**

Assert that `current`, `timeline`, and `all_history` parse to their domain variants, `as_of` requires a timestamp, and unknown values return a client error.

- [x] **Step 6: Run the API tests and confirm RED**

Run:

```powershell
cargo test -p vestrace-http api::retrieval
```

Expected: FAIL because the HTTP request currently has no temporal perspective fields or parser.

---

### Task 2: Implement temporal candidate/context metadata and request/API wiring

**Files:**
- Modify: `crates/vestrace-domain/src/retrieval/mod.rs`
- Modify: `crates/vestrace-application/src/retrieval/request.rs`
- Modify: `crates/vestrace-application/src/retrieval/context_builder.rs`
- Modify: `crates/vestrace-http/src/api/retrieval.rs`
- Modify: `tests/retrieval_e2e.rs`
- Modify: `tests/v01_acceptance.rs`
- Modify: candidate fixtures in `crates/vestrace-application/src/retrieval/{fusion,rerank}.rs`

**Interfaces:**
- Consumes: Task 1 RED tests and C6 exact-revision candidate contract.
- Produces: `RetrievalCandidate` temporal/status fields, `ContextItem` temporal/status annotations, `ContextPack.temporal_perspective`, and HTTP-to-domain perspective conversion.

- [x] **Step 1: Add temporal metadata fields**

Extend `RetrievalCandidate` with `memory_status`, `revision_number`, `valid_from`, `valid_until`, `revision_created_at`, and `source_generation`. Extend `ContextItem` with the selected revision’s status and validity interval. Extend `ContextPack` with `temporal_perspective` and update constructor call sites with `TimePerspective::Current` where no retrieval request is available.

- [x] **Step 2: Implement request defaults and perspective validation**

Keep `Current` forced to active status. For non-current perspectives, preserve caller-provided lifecycle filters and add an explicit `with_allowed_statuses` builder; when the untouched default `[Active]` is used for `Timeline` or `AllHistory`, expand it to all statuses so historical queries do not silently return only current rows. Keep `AsOf` as validity-based semantics and document the mode boundary in code comments.

- [x] **Step 3: Wire HTTP temporal fields**

Add optional `time_perspective` and `as_of` fields to `RetrievalSearchRequest`. Parse `current`, `as_of`, `timeline`, and `all_history`; require `as_of` for the `as_of` mode and pass the resulting `TimePerspective` into `RetrievalRequest`.

- [x] **Step 4: Pass metadata through the C6 builder**

Keep `ContextPackBuilder::build` as a current-compatible wrapper and add a perspective-aware build path used by `RetrievalService`. Copy candidate status/validity into each `ContextItem` without changing C6 section order, provenance, representation, or budget behavior.

- [x] **Step 5: Run the focused green tests**

Run:

```powershell
cargo test -p vestrace-application retrieval::request
cargo test -p vestrace-application retrieval::context_builder
cargo test -p vestrace-http api::retrieval
```

Expected: PASS, with existing C5/C6 tests updated only for the additive metadata contract.

---

### Task 3: Implement PostgreSQL temporal selection and generation-aware hydration

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/text_retriever.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/memory_repository.rs`
- Create: `migrations/0122_retrieval_temporal_indexes.sql`
- Modify: `tests/migrations.rs`

**Interfaces:**
- Consumes: Task 2 normalized perspective and candidate metadata contract.
- Produces: workspace-scoped SQL that applies temporal predicates before ranking and hydrates exact revision/status/validity/generation metadata.

- [x] **Step 1: Add a migration/index test expectation**

Extend migration source checks to require the temporal revision index used by historical retrieval and the existing workspace-safe search/revision columns.

- [x] **Step 2: Run the migration test and confirm RED**

Run:

```powershell
cargo test --test migrations migrations_expose_temporal_and_state_revision_columns -- --nocapture
```

Expected: the environment may stop before test execution when `DATABASE_URL` is absent; if the test runs, it fails until the new migration is present. Record either outcome separately from Rust unit tests.

- [x] **Step 3: Add temporal SQL branches**

Use active-revision join for `Current`; a workspace-scoped lateral revision selection with `valid_from <= as_of` and `as_of < valid_until` for `AsOf`; and all matching revision joins ordered by revision time/number for `Timeline` and `AllHistory`. Keep status/kind/workspace filters before FTS rank ordering.

- [x] **Step 4: Hydrate metadata and preserve the generation**

Select `m.status`, `m.state_revision`, `mr.revision_number`, `mr.valid_from`, `mr.valid_until`, and `mr.created_at`; map them into `RetrievalCandidate`. Extend memory revision save/find queries to persist and read the temporal columns already present in migration `0116`.

- [x] **Step 5: Run compile and focused infrastructure checks**

Run:

```powershell
cargo check -p vestrace-infrastructure
cargo test --test migrations migrations_expose_temporal_and_state_revision_columns -- --nocapture
```

Expected: compile passes; migration execution is either PASS or explicitly environment-blocked.

---

### Task 4: Wire optional channels, deterministic degradation, and fusion evidence

**Files:**
- Modify: `crates/vestrace-application/src/retrieval/service.rs`
- Modify: `crates/vestrace-application/src/retrieval/ports.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Modify: `crates/vestrace-http/src/api/retrieval.rs`
- Modify: service construction call sites in `crates/vestrace-cli`, `crates/vestrace-http`, and `crates/vestrace-mcp`

**Interfaces:**
- Consumes: Task 1 channel tests and Task 2 temporal result/context metadata.
- Produces: `RetrievalService::with_channels`, optional channel execution, named `degraded_channels`, deterministic RRF fusion, and fail-closed no-safe-channel behavior.

- [x] **Step 1: Add optional channel fields and constructor**

Keep `RetrievalService::new(text, journal)` source-compatible for current text-only wiring. Add `with_channels(text, vector, exact, structured, journal)` with `Option<Shared...Retriever>` fields.

- [x] **Step 2: Execute channels independently**

Run each configured channel, append successful result sets, and preserve channel names. A failed optional channel adds a warning and `degraded_channels` entry; a failed text channel is treated the same if another safe channel succeeds.

- [x] **Step 3: Enforce no-safe-channel failure**

If every configured channel returns an error, return `ApplicationError::Unavailable("all retrieval channels failed")`; do not journal or return an empty successful result. If a configured channel succeeds with zero candidates, it still counts as an available deterministic path.

- [x] **Step 4: Propagate degradation and perspective**

Add `degraded_channels` to `RetrievalResult`, copy it into `ContextPack`, and use the perspective-aware builder. Extend the HTTP response with perspective/degraded-channel metadata while preserving existing response fields.

- [x] **Step 5: Run service and HTTP tests**

Run:

```powershell
cargo test -p vestrace-application retrieval::service
cargo test -p vestrace-application retrieval::fusion
cargo test -p vestrace-http api::retrieval
```

Expected: PASS, including one-channel failure degradation and all-channel fail-closed behavior.

---

### Task 5: Record the C7 delta and execute the bounded gate

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-11-c7-temporal-multichannel.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/plans/2026-08-11-c7-temporal-multichannel-retrieval.md`

**Interfaces:**
- Consumes: Task 2-4 implementation and verification outputs.
- Produces: source-based C7 status that distinguishes implemented temporal/multi-channel behavior from the unimplemented cache, conflict, classification, mount, and profile gates.

- [x] **Step 1: Run scoped formatting and full local verification**

Run scoped rustfmt, `cargo check --workspace`, `cargo test --workspace --lib`, `cargo test --workspace --no-run`, `cargo test --test v01_acceptance`, `npm run typecheck --prefix apps/console`, and `git diff --check`.

- [x] **Step 2: Write and link the C7 documentation delta**

Record the exact temporal modes implemented, channel/degradation behavior, passing evidence, the PostgreSQL runtime gate, and explicit limits: RET-003 conflicts, RET-007 classification/model destination, RET-012 actual cache invalidation, RET-014 mounts, and C8 qualification.

- [x] **Step 3: Inspect dirty status and finish without commit**

Mark this plan complete, preserve unrelated files, and report no commit/reset/rebase was performed. Do not claim v0.2 qualification.
