# Vestrace v0.1 Implementation Roadmap

**Design source:** `docs/superpowers/specs/2026-07-31-vestrace-v0.1-design.md`  
**Repository:** `venm1r/vestrace`  
**Delivery model:** five sequential, independently reviewable implementation plans.

## Goal

Deliver Vestrace v0.1 as a local-first Rust service that records agent events, maintains auditable long-term memory, performs hybrid retrieval, builds context packs, exposes MCP and HTTP interfaces, and maintains a versioned registry of models and cognitive assets.

## Why the implementation is split

The approved design contains several large subsystems with different failure modes. A single plan would create long-lived branches, weak review boundaries, and unclear rollback points. Each plan below ends with running, testable software and stabilizes contracts consumed by the next plan.

## Plan sequence

### 1. Foundation

**Plan:** `docs/superpowers/plans/2026-07-31-vestrace-foundation.md`

Builds:

- Rust workspace and crate boundaries;
- shared identifiers, time and error types;
- CLI command skeleton;
- configuration loading;
- PostgreSQL connectivity and forward-only migrations;
- workspaces, principals and transaction context;
- RLS baseline;
- structured tracing, health checks and CI;
- development Docker Compose.

**Exit gate:** a fresh checkout can run formatting, linting and tests; migrate an empty PostgreSQL database; start `vestrace server`; pass liveness/readiness checks; and prove workspace isolation through RLS integration tests.

### 2. Memory Core

**Plan:** `docs/superpowers/plans/2026-07-31-vestrace-memory-core.md`

Builds:

- append-only events;
- Memory and MemoryRevision aggregates;
- scopes, sources, derivations, conflicts and typed edges;
- manual/assisted/automatic write policies;
- remember, revise, supersede, expire, soft-delete and hard-purge application services;
- PostgreSQL jobs, leases, retries, dead-letter state and transactional outbox;
- worker process and deterministic memory extraction test doubles.

**Exit gate:** an integration test records an event, creates a candidate memory, activates it, revises it with optimistic concurrency, preserves provenance, retries a leased background job safely, and removes all content and derivatives through an authorized purge seam.

### 3. Retrieval and Context

**Plan:** `docs/superpowers/plans/2026-07-31-vestrace-retrieval-context.md`

Builds:

- OpenAI-compatible generation and embedding provider ports;
- candidate extraction worker;
- search documents, PostgreSQL FTS, `pg_trgm`, pgvector and structured retrieval;
- Reciprocal Rank Fusion;
- temporal filtering, conflict handling, graph expansion and deterministic reranking;
- token-aware context packs;
- retrieval explanations, journaling and degraded-mode behavior.

**Exit gate:** an end-to-end test ingests conflicting historical decisions, retrieves only the active decision for `current`, returns the full sequence for `timeline`, produces a bounded context pack with provenance, and continues through FTS/structured search when embeddings are unavailable.

### 4. Interfaces and Security

**Plan:** `docs/superpowers/plans/2026-07-31-vestrace-interfaces-security.md`

Builds:

- versioned REST API;
- MCP stdio and Streamable HTTP adapters;
- tools and read-only resources from the approved contract;
- local trusted and bearer-token authentication;
- roles, capabilities, scoped policies and delegated authority;
- sensitivity classification, provider locality checks and redaction;
- administrative operations and operation-status APIs;
- HTTP/MCP contract and security-negative tests.

**Exit gate:** the same application command produces equivalent HTTP and MCP results; an authenticated principal cannot cross workspace boundaries or elevate permissions; restricted content cannot be sent to an unauthorized remote provider; and all write contracts enforce idempotency and expected revisions.

### 5. Cognitive Runtime Foundation

**Plan:** `docs/superpowers/plans/2026-07-31-vestrace-cognitive-runtime-foundation.md`

Builds:

- provider and model registry with versioned capabilities, limits and prices;
- explainable routing policies and fallback chains;
- model execution records and quality evaluations;
- versioned agents, skills and workflow graphs;
- external execution and step history;
- conversion of execution outcomes into memory candidates;
- diagnostics, `doctor`, rebuild commands, final Compose wiring and v0.1 acceptance tests.

**Exit gate:** Vestrace can register several models, choose the cheapest candidate that satisfies a quality threshold and privacy policy, explain the decision, persist actual outcome metrics, store versioned cognitive assets, record an external workflow run, derive memory from its outcome, and pass the complete v0.1 acceptance suite.

## Dependency graph

```text
Foundation
    ↓
Memory Core
    ↓
Retrieval and Context
    ↓
Interfaces and Security
    ↓
Cognitive Runtime Foundation
```

Later plans may add new implementations behind established ports, but must not bypass contracts finalized in earlier plans without an explicit design amendment.

## Normative cross-plan contracts

These contracts are authoritative when an individual task uses shorter wording.

### Shared error placement

- `crates/vestrace-application/src/error.rs` defines `ApplicationError` and maps domain, conflict, policy, unavailable, storage and internal failures without importing SQLx.
- `crates/vestrace-infrastructure/src/error.rs` defines `InfrastructureError` and may wrap SQLx, migration, HTTP-client and configuration failures.
- Interface adapters map both to stable public error codes; they never serialize internal messages directly.

Foundation implementers add these two files with the first task that references each type, even when a task file list abbreviates them.

### Scope types

Memory Core defines:

```rust
pub enum MemoryScopeKind {
    Workspace,
    Project,
    User,
    Agent,
    Team,
    Workflow,
    Execution,
    Session,
    Task,
    Global,
}

pub struct MemoryScope {
    pub kind: MemoryScopeKind,
    pub id: uuid::Uuid,
}
```

`Global` is workspace-global and its ID is the workspace UUID. `MemoryScopeInput` in extraction/API DTOs maps to this domain type and is not a second scope model.

### Purge authorization seam

Memory Core must not depend on security types introduced in Plan 4. Its purge service depends on:

```rust
#[async_trait::async_trait]
pub trait PurgeAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        context: &RequestContext,
        memory_id: MemoryId,
        reason: &str,
    ) -> Result<PurgeAuthorization, ApplicationError>;
}

pub struct PurgeAuthorization {
    pub authorization_reference: String,
    pub expires_at: Timestamp,
}
```

Plan 2 uses a deterministic test adapter. Plan 4 implements the production adapter by validating `data.purge` capability and a payload-bound `ApprovalRecord`, then passes the approval ID as `authorization_reference`. The purge audit table stores that opaque reference without a foreign key to a subsystem that does not yet exist in Plan 2.

### Integration-test package

Root-level `tests/` belong to the non-publishable root package `vestrace-integration-tests`. A command using a root test is:

```bash
cargo test --test <test_name>
```

A command using a crate-local integration test is:

```bash
cargo test -p <crate_name> --test <test_name>
```

### Migration sequence

```text
0001 extensions
0002 identity and workspaces
0003 RLS baseline
0004 sessions and events
0005 memories and revisions
0006 provenance, scopes and relations
0007 idempotency, jobs, outbox and purge audit
0008 search documents
0009 embedding spaces
0010 structured search indexes
0011 retrieval journal and context packs
0012 tokens, policies and approvals
0013 audit and redaction
0014 provider and model registry
0015 routing, model executions and evaluations
0016 agents, skills and workflows
0017 external execution history
0018 diagnostics and metric rollups
```

Each migration is created once and never edited after its task commit.

### Routing quality blend

Plan 5 uses the exact v0.1 blend:

```text
observed_weight = min(observation_count / 20.0, 1.0)
baseline_weight = 1.0 - observed_weight
expected_quality =
    observed_weight × smoothed_task_type_quality
  + baseline_weight × configured_baseline_quality
```

When observation count is zero, expected quality equals configured baseline quality.

## Branch and review policy

Use one branch per plan:

```text
feat/foundation
feat/memory-core
feat/retrieval-context
feat/interfaces-security
feat/cognitive-runtime-foundation
```

Each task ends with its own commit. Each plan ends with a draft pull request and requirements review against both the plan and approved design specification.

## Verification policy

Every plan keeps these commands green before merge:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Plans that introduce PostgreSQL also run listed integration and migration commands. Plans that introduce interfaces add contract suites. The final plan runs Docker Compose smoke and the full v0.1 acceptance scenario.

## Scope control

The following remain outside every v0.1 plan unless the design specification is amended:

- Vestrace-owned workflow execution;
- autonomous task decomposition;
- Neo4j runtime dependency;
- native Anthropic adapter;
- Python components;
- LightRAG, Redis, Kafka, RabbitMQ and Qdrant;
- web administration UI;
- OIDC and organization management;
- autonomous online learning;
- distributed clustering.

## Completion definition

The roadmap is complete only when all five plan exit gates pass and the final acceptance scenario demonstrates:

```text
event
→ provable memory
→ hybrid retrieval
→ bounded context pack
→ model routing and recorded execution outcome
→ improved subsequent task context
```
