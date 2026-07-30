# Vestrace Retrieval and Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add real OpenAI-compatible extraction and embeddings, hybrid candidate retrieval, explainable fusion and reranking, temporal/conflict handling, graph expansion, token-bounded context packs, retrieval journaling and graceful degraded modes.

**Architecture:** Retrieval is a staged application pipeline behind independent ports for exact, text, vector, structured, execution and graph channels. Candidate channels return ranked lists and explanations; Reciprocal Rank Fusion creates a common baseline before deterministic domain reranking. External models are optional enhancements: the system must continue through text and structured paths when generation, embeddings or reranking providers fail.

**Tech Stack:** Existing Foundation and Memory Core, Rust, Tokio, reqwest, Serde/Schemars, SQLx, PostgreSQL FTS, pg_trgm, pgvector, tracing, wiremock for provider contract tests.

## Global Constraints

- Complete Foundation and Memory Core plans first.
- Hard workspace, security, status and time filters run before semantic ranking.
- Current retrieval never presents superseded, expired, rejected or deleted memory as current fact.
- RRF combines ranked channels; raw FTS and vector scores are never added directly.
- A model reranker may reorder or reject supplied candidates but cannot invent facts.
- Every context item retains provenance and inclusion explanation.
- Context packs must not exceed their declared token budget.
- Retrieval works without an available embedding endpoint or model reranker.
- Embeddings, search documents, summaries and context-pack caches are rebuildable.
- OpenAI-compatible API is the only real AI provider protocol in v0.1.

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
  retrieval/fusion.rs
  retrieval/temporal.rs
  retrieval/conflicts.rs
  retrieval/rerank.rs
  retrieval/context_builder.rs
  retrieval/explain.rs

crates/vestrace-infrastructure/src/
  providers/openai_compatible.rs
  providers/dto.rs
  postgres/search_document_repository.rs
  postgres/text_retriever.rs
  postgres/vector_retriever.rs
  postgres/structured_retriever.rs
  postgres/execution_retriever.rs
  postgres/graph_repository.rs
  postgres/retrieval_journal.rs

migrations/
  0008_search_documents.sql
  0009_embedding_spaces.sql
  0010_retrieval_journal_and_context_packs.sql

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
- Produces `RetrievalIntent`, `TimePerspective`, `RetrievalCandidate`, `CandidateOrigin`, `CandidateExplanation`, `ContextPack`, `ContextSection`, `ContextItem`, `EmbeddingVector`.

- [ ] **Step 1: Write failing domain tests**

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

- [ ] **Step 3: Implement candidate scores as components**

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

Keep the final score derivation explicit and serializable for explanations.

- [ ] **Step 4: Implement context item representations**

Each `ContextItem` includes the authoritative object ID, selected representation level (`Full`, `Summary`, `Atomic`, `Reference`), rendered text, accounted tokens, source IDs, conflict state and inclusion explanation.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain retrieval
git add crates/vestrace-domain
git commit -m "feat(retrieval): add retrieval and context domain types"
```

---

### Task 2: Define provider ports and an OpenAI-compatible HTTP client

**Files:**
- Create: `crates/vestrace-application/src/providers/mod.rs`
- Create: `crates/vestrace-application/src/providers/ports.rs`
- Create: `crates/vestrace-infrastructure/src/providers/mod.rs`
- Create: `crates/vestrace-infrastructure/src/providers/dto.rs`
- Create: `crates/vestrace-infrastructure/src/providers/openai_compatible.rs`
- Create: `tests/provider_contract.rs`
- Modify: workspace dependencies to add `reqwest`, `secrecy`, `wiremock`

**Interfaces:**
- Produces ports `TextGenerationProvider`, `EmbeddingProvider`, `ProviderHealth`.
- Produces `OpenAiCompatibleClient` supporting `/chat/completions`, `/embeddings`, timeouts and cancellation.
- Every call accepts a stable `operation_key` and returns usage/latency metadata.

- [ ] **Step 1: Write failing wire-level generation test**

Use wiremock to expect:

```json
{
  "model": "test-model",
  "messages": [{"role":"user","content":"extract"}],
  "response_format": {"type":"json_schema","json_schema":{"name":"memory_candidates"}}
}
```

Return a fixed OpenAI-compatible response and assert parsed content, token usage and model name.

- [ ] **Step 2: Define exact provider contracts**

```rust
#[async_trait::async_trait]
pub trait TextGenerationProvider: Send + Sync {
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError>;
}

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, request: EmbeddingRequest)
        -> Result<EmbeddingResponse, ProviderError>;
}
```

`ProviderError` distinguishes timeout, rate limit, unavailable, invalid response, policy and unknown completion state.

- [ ] **Step 3: Implement client request and response DTOs**

Accept OpenAI-compatible deviations only through narrowly scoped serde aliases. Reject missing embedding vectors, non-finite values and token usage with negative values.

- [ ] **Step 4: Implement retry boundaries**

The HTTP client performs no hidden unbounded retries. It may retry a connection failure only when no request body was accepted; all other retry decisions belong to the job handler.

- [ ] **Step 5: Run and commit**

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
- Produces `ExtractionPolicyVersion`, `PromptTemplateVersion`, `ExtractionResult` and strict JSON schema for candidate memories.
- Candidate output cannot directly activate memory; it passes through `MemoryWritePolicy`.

- [ ] **Step 1: Write failing valid-response test**

Feed a deterministic provider response with one preference and one constraint. Assert both map to typed `StructuredMemory`, source the original event and carry the configured extractor/prompt versions.

- [ ] **Step 2: Write failing invalid-response tests**

Cover unknown memory kind, invalid confidence, missing content and output that attempts to include a secret value. Each must return a permanent schema/policy error without partial writes.

- [ ] **Step 3: Define extraction schema**

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

- [ ] **Step 4: Implement extraction and persistence boundary**

The job handler records `ModelExecution` metadata through a temporary execution port defined here; the formal model registry will replace its backing implementation in Plan 5 without changing extraction signatures.

- [ ] **Step 5: Run and commit**

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
- Produces rebuildable table `memory_search_documents` with revision ID, normalized content, `tsvector`, trigram text and update generation.
- Produces `TextRetriever::search(context, request, limit) -> Vec<RankedCandidate>`.

- [ ] **Step 1: Write failing FTS and typo tests**

Insert a Russian decision containing `PostgreSQL` and `графовая проекция`. Assert exact-term FTS finds it and trigram fallback finds a configured typo without crossing workspace boundaries.

- [ ] **Step 2: Create search-document schema**

Use a generated or explicitly maintained weighted `tsvector` built from content, structured subject/predicate and labels. Add GIN indexes for FTS and trigram search.

- [ ] **Step 3: Implement transactional projection job**

Handle `memory.revision.created` outbox-derived jobs. Upsert only when `(revision_id, content_hash, projection_version)` differs.

- [ ] **Step 4: Implement text search with hard filters**

The SQL must filter workspace, allowed statuses, time perspective, kinds and scopes before applying rank limits.

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
- Create: `crates/vestrace-infrastructure/src/postgres/vector_retriever.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_repository.rs`
- Create: `tests/vector_retrieval.rs`

**Interfaces:**
- Produces `embedding_spaces` and `memory_embeddings`.
- An embedding space fixes provider/model, dimension, distance metric, normalization and version.
- Stores `embedding vector` without a table-wide dimension typmod; creates a validated partial expression index per active embedding space using a cast to `vector(N)`.

- [ ] **Step 1: Write failing dimension and space-isolation tests**

Test that a 3-dimensional result cannot be inserted into a 4-dimensional space and that search in space A never compares vectors from space B.

- [ ] **Step 2: Create embedding schema**

Store dimension and content hash. Unique key:

```text
memory_revision_id + embedding_space_id + content_hash
```

- [ ] **Step 3: Implement safe index creation**

Create an administrative repository method:

```rust
async fn ensure_vector_index(&self, space: &EmbeddingSpace) -> Result<(), InfrastructureError>;
```

Validate dimension against an allowed integer range, derive an identifier from the UUID, quote it safely, and create a partial HNSW index on `(embedding::vector(N)) WHERE embedding_space_id = '<uuid>'`.

- [ ] **Step 4: Implement embedding job handler**

On provider failure, classify retryability. Existing embeddings remain usable. Never delete a previous valid space during a failed rebuild.

- [ ] **Step 5: Implement vector search**

Filter by embedding space and cast both stored vector and query to the same dimension. Return rank, distance and origin explanation.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test vector_retrieval
git add migrations/0009_embedding_spaces.sql crates tests/vector_retrieval.rs
git commit -m "feat(retrieval): add embedding spaces and vector search"
```

---

### Task 6: Define retrieval application ports and request normalization

**Files:**
- Create: `crates/vestrace-application/src/retrieval/mod.rs`
- Create: `crates/vestrace-application/src/retrieval/request.rs`
- Create: `crates/vestrace-application/src/retrieval/ports.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Test: inline unit tests

**Interfaces:**
- Produces `RetrievalRequest`, `NormalizedRetrievalRequest`, `RetrievalFilters`, `ScopeFilter`, `RetrievalPolicy`.
- Produces ports `ExactRetriever`, `TextRetriever`, `VectorRetriever`, `StructuredRetriever`, `ExecutionRetriever`, `GraphRepository`, `RetrievalJournal`.

- [ ] **Step 1: Write failing normalization tests**

Assert empty queries are rejected except explicit `Timeline` object queries. Assert `current` defaults to active statuses only. Assert requested workspace cannot differ from `RequestContext.workspace_id`.

- [ ] **Step 2: Implement exact request types**

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

- [ ] **Step 3: Implement deterministic intent defaults**

Map each intent to preferred memory kinds, channel limits, graph edge allowlist, recency weight and provenance requirements. Keep policy data versioned and serializable.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application retrieval::request
git add crates/vestrace-application
git commit -m "feat(retrieval): define retrieval request pipeline"
```

---

### Task 7: Add structured, execution and exact retrieval channels

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/structured_retriever.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/execution_retriever.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/exact_retriever.rs`
- Create: `tests/hybrid_retrieval.rs`

**Interfaces:**
- Exact retrieval resolves UUIDs, stable names and structured subject/predicate/value triples.
- Structured retrieval filters typed JSON payloads without relying on semantic similarity.
- Execution retrieval returns existing event/tool/model/step outcomes available at this stage.

- [ ] **Step 1: Write failing exact-precedence test**

Given an exact structured fact and a semantically similar but different fact, assert exact lookup ranks first before fusion adjustments.

- [ ] **Step 2: Add JSONB expression indexes**

Add a new forward migration `0010_structured_search_indexes.sql` if `0010` is not already used; otherwise use the next sequence number. Index commonly queried `subject`, `predicate`, task state and decision choice fields by memory kind.

- [ ] **Step 3: Implement channels**

Every result returns `channel_rank` and an explanation. Do not return unsupported statuses and apply RLS-scoped transaction context.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test hybrid_retrieval
git add migrations crates/vestrace-infrastructure tests/hybrid_retrieval.rs
git commit -m "feat(retrieval): add exact and structured channels"
```

---

### Task 8: Implement RRF fusion and deterministic reranking

**Files:**
- Create: `crates/vestrace-application/src/retrieval/fusion.rs`
- Create: `crates/vestrace-application/src/retrieval/rerank.rs`
- Test: unit tests beside modules

**Interfaces:**
- Produces `reciprocal_rank_fusion(channel_results, k) -> Vec<RetrievalCandidate>`.
- Produces `DeterministicReranker::rerank(request, candidates) -> Vec<RetrievalCandidate>`.

- [ ] **Step 1: Write failing RRF test**

```rust
#[test]
fn candidate_present_in_two_channels_outranks_single_channel_candidate() {
    let fused = reciprocal_rank_fusion(fixture_channels(), 60.0);
    assert_eq!(fused[0].memory_id, shared_candidate_id());
}
```

- [ ] **Step 2: Implement RRF exactly**

For each channel rank starting at 1:

```text
score += channel_weight / (k + rank)
```

Deduplicate by authoritative object/revision identity before adding contributions.

- [ ] **Step 3: Implement deterministic score components**

Apply scope, kind, importance, confidence, recency, validity and provenance weights from the versioned retrieval policy. Apply redundancy and unresolved-conflict penalties explicitly.

- [ ] **Step 4: Add diversity selection**

Limit near-duplicates, one-session domination and excessive derived summaries while preserving hard constraints and critical decisions.

- [ ] **Step 5: Run and commit**

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
- Produces `TemporalResolver`, `ConflictResolver`, and PostgreSQL implementation of `GraphRepository`.
- Graph expansion supports depth 1–2, edge allowlists, neighbor limits and score decay.

- [ ] **Step 1: Write failing current/timeline test**

Insert Python decision, then Rust decision superseding it. `Current` must return Rust only. `Timeline` must return both in order with the supersedes relation.

- [ ] **Step 2: Implement as-of selection**

Select the revision that was valid and known at the requested time. Keep `occurred_at`, validity time and recorded time distinct.

- [ ] **Step 3: Implement conflict rules**

Resolution priority: status/supersedes, temporal validity, source quality, human confirmation, confidence. If unresolved, return one explicit conflict context object rather than silently choosing.

- [ ] **Step 4: Implement graph expansion SQL**

Use recursive CTE with maximum depth parameter capped at `2`, workspace filter in every recursive step, relation allowlist and per-origin neighbor limit.

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
- Produces `ContextPackBuilder::build(request, ranked_candidates) -> ContextPack`.
- Produces ordered sections and four representation levels.

- [ ] **Step 1: Write failing budget test**

Build candidates whose full representations exceed 1,000 tokens. Assert resulting `accounted_tokens <= 1_000`, all hard constraints remain, and lower-priority details are compressed or omitted.

- [ ] **Step 2: Implement safe fallback token counting**

When no model-specific tokenizer exists, count UTF-8 bytes as a conservative upper bound. This may underfill context but must not exceed the declared budget.

- [ ] **Step 3: Implement section priorities**

Order: hard constraints, current facts, critical decisions, open tasks, procedures, supporting details, recent events, history. Intent-specific policies may reallocate section quotas without violating mandatory-item rules.

- [ ] **Step 4: Implement compression ladder**

Try `Full`, then stored `Summary`, then typed atomic rendering, then stable reference. Every compressed item retains the same source IDs.

- [ ] **Step 5: Implement explanations**

Store channel origins, filters, score components, conflict state, included representation, omitted duplicate IDs and reason for inclusion.

- [ ] **Step 6: Run and commit**

```bash
cargo test --test context_pack
git add crates/vestrace-application tests/context_pack.rs
git commit -m "feat(context): build token-bounded context packs"
```

---

### Task 11: Persist retrieval runs, context packs and degraded state

**Files:**
- Create: `migrations/0011_retrieval_journal_and_context_packs.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/retrieval_journal.rs`
- Create: `tests/degraded_retrieval.rs`
- Create: `tests/retrieval_e2e.rs`

**Interfaces:**
- Produces tables `retrieval_runs`, `retrieval_candidates`, `context_packs`, `context_pack_items`.
- Produces `RetrievalService::search` and `RetrievalService::build_context`.
- Results include `degraded: bool` and structured warnings.

- [ ] **Step 1: Create retrieval journal schema**

Store policy version, request hash, channels attempted, durations, candidate ranks, final selection and warning codes. Do not duplicate full sensitive memory content in the journal.

- [ ] **Step 2: Implement orchestration service**

Run independent candidate channels concurrently after hard-filter normalization. If vector search fails, record warning `embedding_unavailable`, continue with remaining channels and set `degraded = true`.

- [ ] **Step 3: Write degraded-mode test**

Use a failing `EmbeddingProvider`; assert FTS/structured results are returned, vector origin is absent, warning is present and context build succeeds.

- [ ] **Step 4: Write end-to-end retrieval test**

Scenario:

```text
record architecture events
→ extraction candidates
→ activate decisions and constraints
→ generate search documents and embeddings
→ revise old decision with supersedes
→ current retrieval returns new decision
→ timeline returns both
→ context pack stays under budget and includes provenance
```

- [ ] **Step 5: Run and commit**

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
- Test: `tests/retrieval_rebuild.rs`

**Interfaces:**
- Supports:

```bash
vestrace rebuild search-documents --workspace <uuid>
vestrace rebuild embeddings --workspace <uuid> --space <uuid>
vestrace rebuild context-cache --workspace <uuid>
```

- Rebuild is resumable and idempotent through jobs.

- [ ] **Step 1: Write failing rebuild test**

Create memories, generate derivatives, delete all search documents and embeddings, run rebuild handlers, then assert the same required logical results are found.

- [ ] **Step 2: Implement paged rebuild scheduling**

Schedule jobs by stable revision ID cursor. Never load an entire workspace into memory.

- [ ] **Step 3: Implement progress reporting**

Return operation ID and persist scheduled/completed/failed counts without storing memory content.

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

Verify the acceptance scenario with embeddings available and unavailable. Confirm:

- exact, FTS, trigram, vector and structured channels are individually tested;
- RRF operates on rank positions, not raw mixed score values;
- hard workspace/status/time filters precede ranking;
- superseded knowledge is absent from current context;
- unresolved conflicts are explicit;
- graph depth never exceeds two in v0.1;
- context packs never exceed the accounted budget;
- every item includes provenance and explanation;
- deleting all derived retrieval data and rebuilding preserves required logical results.
