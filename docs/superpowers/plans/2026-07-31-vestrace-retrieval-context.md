# Vestrace Retrieval and Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add real OpenAI-compatible extraction and embeddings, hybrid candidate retrieval, explainable fusion and reranking, temporal/conflict handling, graph expansion, token-bounded context packs, retrieval journaling and graceful degraded modes.

**Architecture:** Retrieval is a staged application pipeline behind independent ports for exact, text, vector, structured, execution and graph channels. Candidate channels return ranked lists and explanations; Reciprocal Rank Fusion creates a common baseline before deterministic domain reranking. External models are optional enhancements: the system continues through text and structured paths when generation, embeddings or reranking providers fail.

**Tech Stack:** Existing Foundation and Memory Core, Rust, Tokio, reqwest, Serde/Schemars, SQLx, PostgreSQL FTS, pg_trgm, pgvector, tracing, wiremock.

## Global Constraints

- Complete Foundation and Memory Core first.
- Hard workspace, security, status and time filters run before semantic ranking.
- Current retrieval never presents superseded, expired, rejected or deleted memory as a current fact.
- RRF combines ranked channels; raw FTS and vector scores are never added directly.
- A model reranker may reorder or reject supplied candidates but cannot invent facts.
- Every context item retains provenance and inclusion explanation.
- Context packs must not exceed their declared token budget.
- Retrieval works without an available embedding endpoint or model reranker.
- Embeddings, search documents, summaries and context-pack caches are rebuildable.
- OpenAI-compatible API is the only real AI provider protocol in v0.1.
- Migration sequence is fixed: `0008_search_documents`, `0009_embedding_spaces`, `0010_structured_search_indexes`, `0011_retrieval_journal_and_context_packs`.

---

## Locked file structure additions

```text
crates/vestrace-domain/src/
  retrieval/mod.rs
  retrieval/intent.rs
  retrieval/candidate.rs
  retrieval/context_pack.rs
  embedding.rs

crates/vestrace-application/src/
  providers/mod.rs
  providers/ports.rs
  extraction/mod.rs
  retrieval/mod.rs
  retrieval/request.rs
  retrieval/ports.rs
  retrieval/embedding.rs
  retrieval/fusion.rs
  retrieval/temporal.rs
  retrieval/conflicts.rs
  retrieval/rerank.rs
  retrieval/context_builder.rs
  retrieval/explain.rs
  retrieval/rebuild.rs

crates/vestrace-infrastructure/src/
  providers/mod.rs
  providers/openai_compatible.rs
  providers/dto.rs
  postgres/search_document_repository.rs
  postgres/text_retriever.rs
  postgres/embedding_repository.rs
  postgres/vector_retriever.rs
  postgres/exact_retriever.rs
  postgres/structured_retriever.rs
  postgres/execution_retriever.rs
  postgres/graph_repository.rs
  postgres/retrieval_journal.rs

migrations/
  0008_search_documents.sql
  0009_embedding_spaces.sql
  0010_structured_search_indexes.sql
  0011_retrieval_journal_and_context_packs.sql

tests/
  provider_contract.rs
  extraction_pipeline.rs
  text_retrieval.rs
  vector_retrieval.rs
  hybrid_retrieval.rs
  temporal_retrieval.rs
  context_pack.rs
  degraded_retrieval.rs
  retrieval_e2e.rs
  retrieval_rebuild.rs
```

---

### Task 1: Define retrieval, embedding and context domain types

**Files:**
- Create: `crates/vestrace-domain/src/retrieval/mod.rs`
- Create: `crates/vestrace-domain/src/retrieval/intent.rs`
- Create: `crates/vestrace-domain/src/retrieval/candidate.rs`
- Create: `crates/vestrace-domain/src/retrieval/context_pack.rs`
- Create: `crates/vestrace-domain/src/embedding.rs`
- Modify: `crates/vestrace-domain/src/id.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Adds IDs `EmbeddingSpaceId`, `RetrievalRunId`, `ContextPackId`.
- Produces `RetrievalIntent`, `TimePerspective`, `RetrievalCandidate`, `CandidateOrigin`, `CandidateExplanation`, `ContextPack`, `ContextSection`, `ContextItem`, and `EmbeddingVector`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn embedding_rejects_wrong_dimension() {
    let result = EmbeddingVector::new(vec![0.0, 1.0], 3);
    assert!(result.is_err());
}

#[test]
fn context_pack_rejects_accounted_tokens_above_budget() {
    let result = ContextPack::new(context_pack_id(), 100, 101, vec![]);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Implement intents and time perspectives**

```rust
pub enum RetrievalIntent {
    SemanticRecall,
    CurrentState,
    DecisionRecall,
    Timeline,
    TaskResume,
    ProcedureLookup,
    UserPreferences,
    ErrorRecovery,
    ModelSelection,
    WorkflowContext,
    Exploration,
}

pub enum TimePerspective {
    Current,
    AsOf(Timestamp),
    Timeline,
    AllHistory,
}
```

- [ ] **Step 3: Implement explainable score components**

```rust
pub struct ScoreComponents {
    pub fused_rank: f32,
    pub scope_match: f32,
    pub kind_match: f32,
    pub importance: f32,
    pub confidence: f32,
    pub recency: f32,
    pub provenance_quality: f32,
    pub redundancy_penalty: f32,
}
```

- [ ] **Step 4: Implement context representations**

Each `ContextItem` includes authoritative object ID, representation level (`Full`, `Summary`, `Atomic`, `Reference`), rendered text, accounted tokens, source IDs, conflict state and inclusion explanation.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain retrieval
git add crates/vestrace-domain
git commit -m "feat(retrieval): add retrieval and context domain types"
```

---

### Task 2: Define provider ports and OpenAI-compatible HTTP client

**Files:**
- Create: `crates/vestrace-application/src/providers/mod.rs`
- Create: `crates/vestrace-application/src/providers/ports.rs`
- Create: `crates/vestrace-infrastructure/src/providers/mod.rs`
- Create: `crates/vestrace-infrastructure/src/providers/dto.rs`
- Create: `crates/vestrace-infrastructure/src/providers/openai_compatible.rs`
- Create: `tests/provider_contract.rs`
- Modify: workspace dependencies for `reqwest`, `secrecy`, and `wiremock`

**Interfaces:**
- Produces `TextGenerationProvider`, `EmbeddingProvider`, and `ProviderHealth` ports.
- Produces `OpenAiCompatibleClient` supporting `/chat/completions`, `/embeddings`, timeouts and cancellation.
- Every call accepts stable `operation_key` and returns usage/latency metadata.

- [ ] **Step 1: Write failing wire-level generation test**

Use wiremock to expect:

```json
{
  "model": "test-model",
  "messages": [{"role":"user","content":"extract"}],
  "response_format": {"type":"json_schema","json_schema":{"name":"memory_candidates"}}
}
```

Return a fixed compatible response and assert content, usage and model name.

- [ ] **Step 2: Define exact provider contracts**

```rust
#[async_trait::async_trait]
pub trait TextGenerationProvider: Send + Sync {
    async fn generate(&self, request: GenerationRequest)
        -> Result<GenerationResponse, ProviderError>;
}

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, request: EmbeddingRequest)
        -> Result<EmbeddingResponse, ProviderError>;
}
```

`ProviderError` distinguishes timeout, rate limit, unavailable, invalid response, policy violation and unknown completion state.

- [ ] **Step 3: Implement DTO validation and retry boundaries**

Reject missing vectors, non-finite values and invalid usage. The HTTP client performs no hidden unbounded retries; job handlers own retry policy.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test provider_contract
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-infrastructure tests/provider_contract.rs
git commit -m "feat(providers): add OpenAI-compatible client"
```

---

### Task 3: Implement schema-constrained memory extraction

**Files:**
- Create: `crates/vestrace-application/src/extraction/mod.rs`
- Modify: `crates/vestrace-application/src/memory/extraction.rs`
- Create: `tests/extraction_pipeline.rs`
- Create: `resources/prompts/memory-extraction-v1.md`

**Interfaces:**
- Produces `OpenAiMemoryExtractor` implementing the existing `MemoryExtractor` port.
- Produces `ExtractionPolicyVersion`, `PromptTemplateVersion`, and strict JSON schema for candidate memories.
- Candidate output passes through `MemoryWritePolicy`; it cannot self-activate.

- [ ] **Step 1: Write valid and invalid response tests**

A valid response with one preference and one constraint maps to typed memories with source event and prompt/extractor versions. Unknown kind, invalid confidence, missing content and secret-bearing output fail without partial writes.

- [ ] **Step 2: Define extraction schema**

```rust
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct ExtractedCandidateDto {
    kind: MemoryKind,
    content: String,
    structured: StructuredMemory,
    confidence: f32,
    importance: f32,
    valid_from: Option<Timestamp>,
    valid_until: Option<Timestamp>,
    scopes: Vec<MemoryScopeInput>,
}
```

- [ ] **Step 3: Implement extraction and execution metadata seam**

Record temporary `ModelExecution` metadata through an application port. Plan 5 replaces its persistence with the formal model registry without changing extraction signatures.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test extraction_pipeline
git add crates resources/prompts tests/extraction_pipeline.rs
git commit -m "feat(extraction): extract typed memory candidates"
```

---

### Task 4: Add search documents and text retrieval

**Files:**
- Create: `migrations/0008_search_documents.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/search_document_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/text_retriever.rs`
- Create: `tests/text_retrieval.rs`

**Interfaces:**
- Produces rebuildable `memory_search_documents` with revision ID, normalized content, `tsvector`, trigram text and projection version.
- Produces `TextRetriever::search(context, request, limit)`.

- [ ] **Step 1: Write failing FTS and typo tests**

Insert a Russian architecture decision. Assert exact term FTS and configured trigram typo lookup find it without crossing workspace boundaries.

- [ ] **Step 2: Create schema and indexes**

Maintain weighted `tsvector` from content, structured subject/predicate and labels. Add GIN indexes for FTS and trigram search.

- [ ] **Step 3: Implement projection job**

Handle `memory.revision.created`. Upsert only when `(revision_id, content_hash, projection_version)` differs.

- [ ] **Step 4: Implement hard-filtered text search**

Filter workspace, allowed statuses, time perspective, kinds and scopes before rank/limit.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test text_retrieval
git add migrations/0008_search_documents.sql crates/vestrace-infrastructure tests/text_retrieval.rs
git commit -m "feat(retrieval): add PostgreSQL text retrieval"
```

---

### Task 5: Add embedding spaces, generation and vector retrieval

**Files:**
- Create: `migrations/0009_embedding_spaces.sql`
- Create: `crates/vestrace-application/src/retrieval/embedding.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/vector_retriever.rs`
- Create: `tests/vector_retrieval.rs`

**Interfaces:**
- Produces `embedding_spaces` and `memory_embeddings`.
- An embedding space fixes provider/model, dimension, distance metric, normalization and version.
- Stores `embedding vector` without table-wide typmod; creates validated partial expression index per active space using `vector(N)` cast.

- [ ] **Step 1: Write dimension and space-isolation tests**

A 3D response cannot enter a 4D space. Search in space A never compares vectors from space B.

- [ ] **Step 2: Create schema**

Unique key:

```text
memory_revision_id + embedding_space_id + content_hash
```

- [ ] **Step 3: Implement safe index creation**

```rust
async fn ensure_vector_index(&self, space: &EmbeddingSpace)
    -> Result<(), InfrastructureError>;
```

Validate dimension, derive a safe index name from UUID, quote identifiers and create partial HNSW index on `(embedding::vector(N)) WHERE embedding_space_id = '<uuid>'`.

- [ ] **Step 4: Implement embedding job and vector search**

Classify provider failures. Preserve previous valid spaces during failed rebuilds. Search filters by space and casts query/stored vectors to the same dimension.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test vector_retrieval
git add migrations/0009_embedding_spaces.sql crates tests/vector_retrieval.rs
git commit -m "feat(retrieval): add embedding spaces and vector search"
```

---

### Task 6: Define retrieval request normalization and channel ports

**Files:**
- Create: `crates/vestrace-application/src/retrieval/mod.rs`
- Create: `crates/vestrace-application/src/retrieval/request.rs`
- Create: `crates/vestrace-application/src/retrieval/ports.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces `RetrievalRequest`, `NormalizedRetrievalRequest`, `RetrievalFilters`, `ScopeFilter`, `RetrievalPolicy`.
- Produces ports `ExactRetriever`, `TextRetriever`, `VectorRetriever`, `StructuredRetriever`, `ExecutionRetriever`, `GraphRepository`, `RetrievalJournal`.

- [ ] **Step 1: Write normalization tests**

Reject empty queries except explicit timeline object queries. `Current` defaults to active only. Requested workspace cannot differ from `RequestContext.workspace_id`.

- [ ] **Step 2: Implement exact request type**

```rust
pub struct RetrievalRequest {
    pub query: String,
    pub intent: RetrievalIntent,
    pub time_perspective: TimePerspective,
    pub filters: RetrievalFilters,
    pub token_budget: Option<u32>,
    pub include_explanation: bool,
}
```

- [ ] **Step 3: Implement versioned intent defaults**

Map intent to preferred kinds, channel limits, graph edge allowlist, recency weight and provenance requirements.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application retrieval::request
git add crates/vestrace-application
git commit -m "feat(retrieval): define retrieval request pipeline"
```

---

### Task 7: Add exact, structured and execution channels

**Files:**
- Create: `migrations/0010_structured_search_indexes.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/exact_retriever.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/structured_retriever.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/execution_retriever.rs`
- Create: `tests/hybrid_retrieval.rs`

**Interfaces:**
- Exact retrieval resolves UUIDs, stable names and subject/predicate/value triples.
- Structured retrieval filters typed JSON payloads.
- Execution retrieval returns available event/tool/model/step outcomes.

- [ ] **Step 1: Write exact-precedence test**

Given an exact structured fact and a semantically similar different fact, exact lookup ranks first before fusion adjustments.

- [ ] **Step 2: Create `0010_structured_search_indexes.sql`**

Add JSONB expression indexes for subject, predicate, task state and decision choice, scoped by memory kind where useful. Commit this migration once and never edit it later.

- [ ] **Step 3: Implement channels**

Every result returns `channel_rank` and explanation. Apply RLS-scoped context and status/time filters.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test hybrid_retrieval
git add migrations/0010_structured_search_indexes.sql crates/vestrace-infrastructure tests/hybrid_retrieval.rs
git commit -m "feat(retrieval): add exact and structured channels"
```

---

### Task 8: Implement RRF fusion and deterministic reranking

**Files:**
- Create: `crates/vestrace-application/src/retrieval/fusion.rs`
- Create: `crates/vestrace-application/src/retrieval/rerank.rs`
- Test: unit tests beside modules

**Interfaces:**
- Produces `reciprocal_rank_fusion(channel_results, k)`.
- Produces `DeterministicReranker::rerank(request, candidates)`.

- [ ] **Step 1: Write failing RRF test**

```rust
#[test]
fn candidate_present_in_two_channels_outranks_single_channel_candidate() {
    let fused = reciprocal_rank_fusion(fixture_channels(), 60.0);
    assert_eq!(fused[0].memory_id, shared_candidate_id());
}
```

- [ ] **Step 2: Implement RRF**

For channel ranks starting at 1:

```text
score += channel_weight / (k + rank)
```

Deduplicate by authoritative object/revision identity before adding contributions.

- [ ] **Step 3: Implement deterministic components and diversity**

Apply scope, kind, importance, confidence, recency, validity, provenance and redundancy components from versioned policy. Limit near-duplicates, one-session domination and excessive derived summaries while preserving hard constraints.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application retrieval::fusion retrieval::rerank
git add crates/vestrace-application
git commit -m "feat(retrieval): add RRF and deterministic reranking"
```

---

### Task 9: Implement temporal resolution, conflicts and graph expansion

**Files:**
- Create: `crates/vestrace-application/src/retrieval/temporal.rs`
- Create: `crates/vestrace-application/src/retrieval/conflicts.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/graph_repository.rs`
- Create: `tests/temporal_retrieval.rs`

**Interfaces:**
- Produces `TemporalResolver`, `ConflictResolver`, and PostgreSQL `GraphRepository`.
- Graph expansion supports depth 1–2, edge allowlists, neighbor limits and score decay.

- [ ] **Step 1: Write current/timeline test**

Insert Python decision, then Rust decision superseding it. `Current` returns Rust only; `Timeline` returns both in order.

- [ ] **Step 2: Implement as-of selection**

Select revision valid and known at requested time while keeping occurred, validity and recorded times distinct.

- [ ] **Step 3: Implement conflict resolution**

Priority: status/supersedes, temporal validity, source quality, human confirmation, confidence. Unresolved conflict becomes explicit context object.

- [ ] **Step 4: Implement recursive graph SQL**

Use recursive CTE capped at depth `2`, workspace filter at every step, relation allowlist and per-origin neighbor limit.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test temporal_retrieval
git add crates tests/temporal_retrieval.rs
git commit -m "feat(retrieval): resolve time conflicts and graph context"
```

---

### Task 10: Build token-bounded context packs

**Files:**
- Create: `crates/vestrace-application/src/retrieval/context_builder.rs`
- Create: `crates/vestrace-application/src/retrieval/explain.rs`
- Create: `tests/context_pack.rs`

**Interfaces:**
- Produces `TokenCounter` port and `ConservativeByteTokenCounter` fallback.
- Produces `ContextPackBuilder::build(request, ranked_candidates)`.

- [ ] **Step 1: Write budget test**

Candidates exceed 1,000 tokens. Result must have `accounted_tokens <= 1_000`, preserve hard constraints and compress/omit lower-priority detail.

- [ ] **Step 2: Implement conservative fallback counting**

When no model tokenizer exists, count UTF-8 bytes as an upper bound. This may underfill but must not exceed budget.

- [ ] **Step 3: Implement section order and compression ladder**

Order: hard constraints, current facts, critical decisions, open tasks, procedures, supporting details, recent events, history. Compression: `Full → Summary → Atomic → Reference`.

- [ ] **Step 4: Implement explanations**

Store origins, filters, score components, conflict state, representation, omitted duplicate IDs and inclusion reason.

- [ ] **Step 5: Run and commit**

```bash
cargo test --test context_pack
git add crates/vestrace-application tests/context_pack.rs
git commit -m "feat(context): build token-bounded context packs"
```

---

### Task 11: Persist retrieval runs and degraded state

**Files:**
- Create: `migrations/0011_retrieval_journal_and_context_packs.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/retrieval_journal.rs`
- Create: `tests/degraded_retrieval.rs`
- Create: `tests/retrieval_e2e.rs`

**Interfaces:**
- Produces `retrieval_runs`, `retrieval_candidates`, `context_packs`, and `context_pack_items`.
- Produces `RetrievalService::search` and `RetrievalService::build_context`.
- Results include `degraded` and structured warnings.

- [ ] **Step 1: Create `0011_retrieval_journal_and_context_packs.sql`**

Store policy version, request hash, channels attempted, durations, ranks, final selection and warning codes. Do not duplicate sensitive content in journal rows.

- [ ] **Step 2: Implement orchestration**

Run independent channels concurrently after normalization. If vector search fails, record `embedding_unavailable`, continue with other channels and set `degraded = true`.

- [ ] **Step 3: Write degraded and end-to-end tests**

The end-to-end scenario records events, extracts/activates decisions, generates derivatives, supersedes old decision, returns correct current/timeline results and builds bounded context with provenance.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test degraded_retrieval --test retrieval_e2e
git add migrations/0011_retrieval_journal_and_context_packs.sql crates tests
git commit -m "feat(retrieval): orchestrate explainable hybrid retrieval"
```

---

### Task 12: Add rebuild commands for derived retrieval data

**Files:**
- Modify: `crates/vestrace-cli/src/commands/rebuild.rs`
- Create: `crates/vestrace-application/src/retrieval/rebuild.rs`
- Create: `tests/retrieval_rebuild.rs`

**Interfaces:**
- Supports:

```bash
vestrace rebuild search-documents --workspace <uuid>
vestrace rebuild embeddings --workspace <uuid> --space <uuid>
vestrace rebuild context-cache --workspace <uuid>
```

- Rebuild is resumable and idempotent through jobs.

- [ ] **Step 1: Write rebuild test**

Create memories and derivatives, delete search documents/embeddings, run rebuild handlers, then assert required logical retrieval results return.

- [ ] **Step 2: Implement paged scheduling**

Schedule jobs by stable revision ID cursor; never load an entire workspace into memory.

- [ ] **Step 3: Implement operation progress**

Persist scheduled/completed/failed counts without memory content.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test retrieval_rebuild
git add crates tests/retrieval_rebuild.rs
git commit -m "feat(retrieval): rebuild derived search data"
```

---

## Retrieval and Context completion gate

Run with fresh output:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --workspace --all-features
```

Verify acceptance with embeddings available and unavailable. Confirm exact, FTS, trigram, vector and structured channels are tested; RRF uses ranks; hard filters precede ranking; superseded knowledge is absent from current context; unresolved conflicts are explicit; graph depth is at most two; context packs stay within budget; every item has provenance/explanation; rebuilding all derived retrieval data preserves required logical results; and migrations `0008` through `0011` are each created once and never edited after commit.
