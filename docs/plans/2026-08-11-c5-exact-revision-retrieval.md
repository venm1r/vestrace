# C5 Exact-revision Retrieval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the current text retrieval path disclose only a workspace-authorized, real active `MemoryRevision` and carry its exact content/provenance into context construction.

**Architecture:** Keep the existing `RetrievalService` and `TextRetriever` ports. The application service rejects a request whose workspace differs from the trusted `RequestContext`; the PostgreSQL adapter sets the RLS workspace and joins `memories.active_revision_id` to `memory_revisions` before producing a candidate. Candidates carry hydrated revision content, and `ContextPackBuilder` renders that content while retaining the exact revision ID. This is the C5 boundary slice only; full capability grants, classification/share policy, temporal/as-of retrieval, conflict projection, and universal G2 authorization remain pending.

**Tech Stack:** Rust 2024, Tokio/async-trait, Serde/Schemars, SQLx/PostgreSQL, existing Vestrace retrieval domain/application ports.

## Global Constraints

- `RET-001`: workspace/security filters apply before downstream ranking or context disclosure.
- `RET-002`: current retrieval does not return superseded, expired, rejected, or deleted memories as current truth.
- `RET-005`: every context item preserves the exact source revision reference.
- `CAP-001`: this slice enforces trusted request-workspace binding; the effective capability/policy engine remains a later governance gate.
- Do not generate placeholder revision IDs or silently fall back to a different revision.
- Preserve all unrelated dirty files, deleted documentation, and generated graphify evidence.
- Do not modify the frozen historical `0008_search_documents` migration; runtime proof requiring PostgreSQL is reported separately if `DATABASE_URL`/Docker is unavailable.

---

### Task 1: Add failing authorization and hydration-contract tests

**Files:**
- Modify: `crates/vestrace-application/src/retrieval/service.rs`
- Modify: `crates/vestrace-application/src/retrieval/context_builder.rs`
- Modify: `crates/vestrace-domain/src/retrieval/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/text_retriever.rs`
- Test: focused module tests and existing retrieval fixtures

**Interfaces:**
- Consumes: `RequestContext`, `RetrievalService`, `RetrievalCandidate`, and the PostgreSQL text retriever query.
- Produces: tests requiring request/context workspace identity, non-placeholder revision identity, and candidate content being used for context rendering.

- [x] **Step 1: Write the failing workspace-boundary test**

Add an async service test with a stub text retriever and journal. Build a request for `requested_workspace`, a context for `trusted_workspace`, call `RetrievalService::search`, and assert it returns `ApplicationError::Policy` without invoking the retriever.

- [x] **Step 2: Run the boundary test and confirm RED**

Run:

```powershell
cargo test -p vestrace-application retrieval::service::tests::search_rejects_workspace_mismatch
```

Expected: FAIL because the current service forwards the request to the retriever without comparing its workspace to `RequestContext.workspace_id`.

- [x] **Step 3: Write the failing exact-content context test**

Extend the candidate fixture with a stable `revision_id` and `content`, then build a context pack and assert the first `ContextItem` has that revision ID and renders the candidate content rather than the diagnostic explanation.

- [x] **Step 4: Run the context test and confirm RED**

Run:

```powershell
cargo test -p vestrace-application retrieval::context_builder::tests::context_item_preserves_exact_revision_content
```

Expected: FAIL because `RetrievalCandidate` has no hydrated content field and the builder renders `explanation`.

---

### Task 2: Implement the smallest green application/domain change

**Files:**
- Modify: `crates/vestrace-domain/src/retrieval/mod.rs`
- Modify: `crates/vestrace-application/src/retrieval/service.rs`
- Modify: `crates/vestrace-application/src/retrieval/context_builder.rs`
- Modify: retrieval fixture constructors in `crates/vestrace-application/src/retrieval/{context_builder,fusion,rerank}.rs` and `tests/v01_acceptance.rs`

**Interfaces:**
- Consumes: failing tests from Task 1.
- Produces: `RetrievalCandidate.content: String`; `RetrievalService::search` rejects mismatched workspace requests; context items use candidate content and exact revision ID.

- [x] **Step 1: Add hydrated content to `RetrievalCandidate` and update fixtures**

Use an explicit `content: String` field. Existing synthetic fixtures must set `content` to their prior explanation or an empty string; no fixture may invent a revision ID for a database result.

- [x] **Step 2: Add the trusted workspace check**

Before normalization or channel calls, return `ApplicationError::Policy("retrieval workspace does not match request context")` when `request.workspace_id != context.workspace_id`.

- [x] **Step 3: Render hydrated content in `ContextPackBuilder`**

Use `candidate.content` when non-empty, retaining the explanation in `inclusion_explanation`; keep the exact `candidate.revision_id` on every `ContextItem`.

- [x] **Step 4: Run focused application/domain tests**

Run:

```powershell
cargo test -p vestrace-domain retrieval
cargo test -p vestrace-application retrieval
```

Expected: PASS, with the mismatch request rejected before the stub channel and context output retaining revision/content identity.

---

### Task 3: Hydrate exact active revisions in PostgreSQL

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/text_retriever.rs`
- Create: `tests/migrations.rs` focused schema assertion if needed
- Create: `docs/documentation-gap-delta-2026-08-11-c5-exact-revision.md`

**Interfaces:**
- Consumes: `RequestContext`, `NormalizedRetrievalRequest`, `memories`, `memory_revisions`, and `search_documents`.
- Produces: text candidates with `memory_id`, the real `memory_revisions.id`, and hydrated revision content; the query explicitly scopes workspace and requires an active revision.

- [x] **Step 1: Change the SQL projection**

Join `memory_revisions` through `memories.active_revision_id`, select `mr.id AS revision_id` and `mr.content`, retain current-status/kind filters, and bind `context.workspace_id` explicitly in addition to RLS.

- [x] **Step 2: Map SQL rows without placeholder IDs**

Parse UUID `revision_id` and content from each row. Map SQL/range failures to `ApplicationError::Storage`; never call `MemoryRevisionId::new()` in the PostgreSQL adapter.

- [x] **Step 3: Add source-based documentation delta**

Record the implemented boundary and focused evidence, while marking capability grants, classification/share checks, temporal/as-of semantics, unresolved conflict projection, and database runtime qualification as pending or blocked.

- [x] **Step 4: Run compile and static tests**

Run:

```powershell
cargo check --workspace
cargo test --workspace --no-run
cargo test -p vestrace-application retrieval
```

Expected: PASS. Run the migration/DB-backed retrieval test only when `DATABASE_URL` and a working PostgreSQL service are available.

---

### Task 4: Verify the bounded gate and report remaining scope

**Files:**
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/requirement-coverage-v0.2.md` only if the delta needs a source index correction; otherwise leave the user change untouched.

**Interfaces:**
- Consumes: exact diff, focused tests, workspace compile output, and DB availability evidence.
- Produces: linked C5 delta without claiming full RET/CAP qualification.

- [x] **Step 1: Run scoped formatting and diff checks**

Run `rustfmt --edition 2024 --config-path rustfmt.toml --check` on changed Rust files and `git diff --check`.

- [x] **Step 2: Run fresh workspace verification**

Run `cargo check --workspace`, `cargo test --workspace --no-run`, and the focused retrieval tests. Record any pre-existing warnings separately.

- [x] **Step 3: Inspect status and document blockers**

Run `git status --short`; preserve unrelated dirty changes and state that PostgreSQL runtime qualification remains blocked if Docker/API access or `DATABASE_URL` is unavailable.
