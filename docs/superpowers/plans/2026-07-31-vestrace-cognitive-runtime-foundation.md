# Vestrace Cognitive Runtime Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Vestrace v0.1 with a versioned provider/model registry, explainable quality-first routing, model execution evaluations, versioned agents/skills/workflows, external execution history, execution-derived memory, diagnostics, metrics, final deployment wiring and the complete acceptance suite.

**Architecture:** Cognitive assets are authoritative versioned aggregates separate from memory. The model router performs deterministic hard filtering followed by policy-specific ranking and always stores its candidates and explanation. Vestrace records external workflow execution but does not schedule or execute workflow graphs in v0.1. Execution outcomes feed the existing memory extraction path through append-only events and jobs.

**Tech Stack:** All preceding plans, Rust, Tokio, SQLx, PostgreSQL, Axum, rmcp, Serde/Schemars, tracing, Prometheus-compatible metrics, Docker Compose.

## Global Constraints

- Complete Foundation, Memory Core, Retrieval/Context and Interfaces/Security first.
- Vestrace does not execute workflow graphs in v0.1.
- OpenAI-compatible endpoints remain the only real provider protocol.
- Providers, models, prices, policies, agents, skills and workflows are versioned.
- The default router first enforces a minimum quality threshold, then minimizes expected cost while considering reliability, latency, privacy and locality.
- Every routing decision stores all considered candidates and rejection reasons.
- Raw execution/evaluation records are authoritative; performance memories are derived.
- Cognitive assets are not represented as ordinary Memory records.
- Models and tools act under delegated authority and provider data-transfer policy.
- Neo4j remains absent from runtime; all graph projections are rebuildable PostgreSQL-backed interfaces.
- Final acceptance must pass without a paid or public AI provider by using deterministic test providers.

---

## Locked file structure additions

```text
crates/vestrace-domain/src/
  models/mod.rs
  models/capability.rs
  models/cost.rs
  models/routing.rs
  cognitive/mod.rs
  cognitive/agent.rs
  cognitive/skill.rs
  cognitive/workflow.rs
  execution/mod.rs
  execution/status.rs
  evaluation.rs

crates/vestrace-application/src/
  models/mod.rs
  models/commands.rs
  models/ports.rs
  models/router.rs
  models/execution.rs
  models/evaluation.rs
  cognitive/mod.rs
  cognitive/commands.rs
  cognitive/ports.rs
  cognitive/services.rs
  execution/mod.rs
  execution/commands.rs
  execution/ports.rs
  execution/services.rs
  diagnostics/mod.rs
  diagnostics/doctor.rs

crates/vestrace-infrastructure/src/postgres/
  provider_repository.rs
  model_repository.rs
  routing_repository.rs
  model_execution_repository.rs
  evaluation_repository.rs
  agent_repository.rs
  skill_repository.rs
  workflow_repository.rs
  execution_repository.rs
  diagnostics_repository.rs

crates/vestrace-http/src/routes/
  models.rs
  agents.rs
  skills.rs
  workflows.rs
  executions.rs
  evaluations.rs

crates/vestrace-mcp/src/tools/
  models.rs
  cognitive.rs
  executions.rs

crates/vestrace-cli/src/commands/
  doctor.rs
  rebuild.rs
  schema.rs

migrations/
  0014_provider_and_model_registry.sql
  0015_routing_executions_and_evaluations.sql
  0016_agents_skills_workflows.sql
  0017_external_execution_history.sql
  0018_diagnostics_and_metric_rollups.sql

tests/
  model_registry.rs
  model_routing.rs
  routing_privacy.rs
  model_evaluations.rs
  cognitive_assets.rs
  workflow_graph.rs
  execution_history.rs
  execution_to_memory.rs
  diagnostics.rs
  compose_smoke.rs
  v01_acceptance.rs
```

---

### Task 1: Add provider, model, capability and pricing domain types

**Files:**
- Create: `crates/vestrace-domain/src/models/mod.rs`
- Create: `crates/vestrace-domain/src/models/capability.rs`
- Create: `crates/vestrace-domain/src/models/cost.rs`
- Modify: `crates/vestrace-domain/src/id.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Adds IDs `ProviderId`, `ProviderRevisionId`, `ModelId`, `ModelRevisionId`, `CostProfileId`, `RoutingPolicyId`, `RoutingDecisionId`, `ModelExecutionId`, `QualityEvaluationId`.
- Produces `ProviderProfile`, `ProviderRevision`, `ModelProfile`, `ModelRevision`, `ModelCapability`, `ModelSpecialization`, `ModelCostProfile`, `ProviderLocality`, `LifecycleStatus`.

- [ ] **Step 1: Write failing model invariants tests**

```rust
#[test]
fn remote_model_requires_provider_endpoint_reference() {
    let result = ProviderRevision::new_remote("cloud", None, secret_ref());
    assert!(result.is_err());
}

#[test]
fn cost_profile_rejects_negative_prices() {
    assert!(ModelCostProfile::new(-0.01, 0.02, "USD", now()).is_err());
}
```

- [ ] **Step 2: Implement typed capabilities**

Support stable names:

```text
generation.text
generation.code
reasoning.basic
reasoning.advanced
tools.calling
output.structured
input.images
input.audio
context.long
embedding
reranking
```

Unknown names may be preserved in import metadata but cannot satisfy a required typed capability until recognized.

- [ ] **Step 3: Implement versioned provider and model revisions**

Stable profile IDs identify a logical provider/model. Mutable endpoint metadata, limits, capabilities and specializations live in immutable numbered revisions with one current pointer.

- [ ] **Step 4: Implement pricing**

Store input-token, output-token and per-request costs with currency and validity interval. Local profiles use `local_compute_weight` and zero API price; do not pretend this is monetary cost unless configured.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain models
git add crates/vestrace-domain
git commit -m "feat(models): add versioned model registry domain"
```

---

### Task 2: Add provider and model registry persistence

**Files:**
- Create: `migrations/0014_provider_and_model_registry.sql`
- Create: `crates/vestrace-application/src/models/mod.rs`
- Create: `crates/vestrace-application/src/models/commands.rs`
- Create: `crates/vestrace-application/src/models/ports.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/provider_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_repository.rs`
- Create: `tests/model_registry.rs`

**Interfaces:**
- Produces provider/model create, revise, list, get and lifecycle services.
- Stores secret references only.
- Preserves exact revision used by every later routing decision.

- [ ] **Step 1: Write failing revision-history test**

Create a model at revision 1, change context limit at expected revision 1, assert revision 2 is current and revision 1 remains readable.

- [ ] **Step 2: Create registry tables**

Tables: `providers`, `provider_revisions`, `models`, `model_revisions`, `model_capabilities`, `model_specializations`, `model_cost_profiles`, `provider_health_checks`.

- [ ] **Step 3: Add uniqueness and temporal constraints**

Ensure provider model name is unique within a provider revision's active namespace. Prevent overlapping valid cost intervals for the same model revision and currency through an exclusion constraint.

- [ ] **Step 4: Implement application services and repositories**

All writes require idempotency and expected revision where applicable. Validate endpoint schemes and secret-reference prefixes (`env:`, `file:`, `vault:`, `keyring:`).

- [ ] **Step 5: Add RLS and capabilities**

Management requires `model.manage` or `provider.manage`; reading requires `model.read`.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test model_registry
git add migrations/0014_provider_and_model_registry.sql crates tests/model_registry.rs
git commit -m "feat(models): persist providers models and pricing"
```

---

### Task 3: Define routing requirements, policies and deterministic scoring

**Files:**
- Create: `crates/vestrace-domain/src/models/routing.rs`
- Create: `crates/vestrace-application/src/models/router.rs`
- Create: `tests/model_routing.rs`

**Interfaces:**
- Produces `TaskRequirements`, `RoutingStrategy`, `RoutingPolicy`, `RoutingCandidate`, `RoutingDecision`.
- Produces `ModelRouter::route(context, task, policy) -> RoutingDecision`.

- [ ] **Step 1: Write failing quality-then-cost test**

Given:

```text
cheap-general quality 0.78 cost 0.01
local-coder quality 0.86 cost 0.03
cloud-reasoner quality 0.94 cost 0.20
minimum quality 0.85
```

Balanced must select `local-coder`, reject `cheap-general` below threshold and retain `cloud-reasoner` as a more expensive eligible fallback.

- [ ] **Step 2: Implement hard filters**

Filter in this order:

```text
lifecycle/health
required capabilities
context limit
input modality
privacy and locality
task budget
```

Each rejected candidate receives one or more stable reason codes.

- [ ] **Step 3: Define v0.1 quality estimate**

```text
expected_quality =
  observed_weight × smoothed_task_type_quality
  + baseline_weight × configured_baseline_quality
```

Use `observed_weight = min(observation_count / 20, 1)`. If no observations exist, use configured baseline only. Smoothed success reliability is `(successes + 1) / (attempts + 2)`.

- [ ] **Step 4: Define expected cost and latency**

Estimate cost from token estimates and the cost profile valid at routing time. Use recent task-type p50 latency when available, otherwise configured baseline latency.

- [ ] **Step 5: Implement strategy behavior**

`Fixed`, `BestQuality`, `LowestCost`, `LowestLatency`, `Balanced`, `LocalOnly`, `WithinBudget`, `QualityAboveThreshold`, `CustomWeighted`. Tie-breaking is deterministic: higher reliability, lower latency, local preference, then stable model UUID ordering.

- [ ] **Step 6: Run and commit**

```bash
cargo test --test model_routing
git add crates tests/model_routing.rs
git commit -m "feat(routing): select models with explainable policies"
```

---

### Task 4: Persist routing decisions, model executions and evaluations

**Files:**
- Create: `migrations/0015_routing_executions_and_evaluations.sql`
- Create: `crates/vestrace-application/src/models/execution.rs`
- Create: `crates/vestrace-application/src/models/evaluation.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/routing_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_execution_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/evaluation_repository.rs`
- Create: `tests/model_evaluations.rs`

**Interfaces:**
- Produces tables `routing_policies`, `routing_decisions`, `routing_candidates`, `model_executions`, `quality_evaluations`, `model_task_type_rollups`.
- Replaces the temporary model-execution backing from Retrieval Plan without changing extraction provider signatures.

- [ ] **Step 1: Write failing provenance test**

Route and execute a model, revise its profile afterward, and assert the old decision still points to the exact model/provider/cost/policy revisions used at decision time.

- [ ] **Step 2: Create routing and execution schema**

Store normalized task requirements, candidate components, rejection reasons, selected model, fallback order, predicted quality/cost/latency and timestamps.

- [ ] **Step 3: Handle unknown completion state**

If an external call may have completed but response persistence failed, record `completion_state = 'unknown'`. Automatic retry requires an operation-specific policy; it must never be treated as a clean pre-call failure.

- [ ] **Step 4: Implement evaluation types**

Support deterministic test result, JSON-schema validation, human score, LLM judge metadata and downstream outcome. Deterministic checks take precedence for code/structured tasks when aggregating quality.

- [ ] **Step 5: Implement rollup job**

Aggregate by `(model_revision_id, task_type)` with sample count, smoothed quality, success rate, latency percentiles and average cost. Raw records remain authoritative.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test model_evaluations
git add migrations/0015_routing_executions_and_evaluations.sql crates tests/model_evaluations.rs
git commit -m "feat(models): record routing executions and quality"
```

---

### Task 5: Enforce privacy-aware routing and provider fallback

**Files:**
- Create: `tests/routing_privacy.rs`
- Modify: `crates/vestrace-application/src/models/router.rs`
- Modify: provider invocation orchestration

**Interfaces:**
- Router consumes `ProviderDataPolicy` decisions from Interfaces/Security.
- Produces ordered fallback model revisions that all satisfy hard policy constraints.

- [ ] **Step 1: Write failing Restricted-data test**

Provide one high-quality remote model and one lower-quality local model. With Restricted data and no remote-transfer approval, route to local model even when remote has higher quality.

- [ ] **Step 2: Write failing fallback test**

Mark selected model unavailable after routing; invocation uses the next eligible fallback and records `routing.fallback.used` without considering candidates previously rejected by privacy or capability filters.

- [ ] **Step 3: Integrate health freshness**

Treat stale health as unknown according to policy. `LocalOnly` and privacy hard constraints never soften because health data is missing.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test routing_privacy --test model_routing
git add crates tests/routing_privacy.rs
git commit -m "feat(routing): enforce privacy and fallback constraints"
```

---

### Task 6: Add agent and skill domain aggregates

**Files:**
- Create: `crates/vestrace-domain/src/cognitive/mod.rs`
- Create: `crates/vestrace-domain/src/cognitive/agent.rs`
- Create: `crates/vestrace-domain/src/cognitive/skill.rs`
- Modify: `crates/vestrace-domain/src/id.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Adds `AgentId`, `AgentRevisionId`, `SkillId`, `SkillRevisionId`.
- Produces `AgentDefinition`, `AgentRevision`, `SkillDefinition`, `SkillRevision`, `SkillImplementation`.

- [ ] **Step 1: Write failing asset invariant tests**

Agent revision with a nonexistent/duplicate skill reference is rejected by application validation. A skill with invalid input or output JSON Schema is rejected. An agent cannot grant itself capabilities absent from its owner policy ceiling.

- [ ] **Step 2: Implement agent revisions**

Store role, instructions, model requirements, skill/tool references, memory scopes, context policy, budget policy and requested capability set.

- [ ] **Step 3: Implement skill revisions**

Support `Prompt`, `McpTool`, `Http`, `Composite`, `Human`. Each stores typed implementation configuration, required capabilities, dependencies, applicability conditions, examples and input/output schemas.

- [ ] **Step 4: Validate JSON Schemas at write time**

Compile schemas with the selected Rust JSON Schema validator. Reject unsupported or invalid schemas before persistence.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain cognitive
git add crates/vestrace-domain
git commit -m "feat(cognitive): add agents and skills domain"
```

---

### Task 7: Add workflow graph domain and validation

**Files:**
- Create: `crates/vestrace-domain/src/cognitive/workflow.rs`
- Create: `tests/workflow_graph.rs`

**Interfaces:**
- Adds `WorkflowId`, `WorkflowRevisionId`, `WorkflowNodeId`, `WorkflowTransitionId`.
- Produces `WorkflowDefinition`, `WorkflowRevision`, `WorkflowNode`, `WorkflowTransition`, `WorkflowNodeKind`.
- Validates storage graph only; does not execute it.

- [ ] **Step 1: Write failing graph tests**

Reject:

```text
missing start node
transition to missing node
unreachable required node
duplicate node ID
parallel node without join where policy requires one
sub-workflow reference across workspace
```

Allow cycles only when an explicit loop policy with maximum iterations is present.

- [ ] **Step 2: Implement node kinds**

`Agent`, `Skill`, `Tool`, `Decision`, `Parallel`, `Join`, `HumanApproval`, `SubWorkflow`, `End`.

- [ ] **Step 3: Implement graph validation report**

Return all validation errors with node/transition references rather than stopping at the first error.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test workflow_graph
git add crates/vestrace-domain tests/workflow_graph.rs
git commit -m "feat(cognitive): validate stored workflow graphs"
```

---

### Task 8: Persist and version cognitive assets

**Files:**
- Create: `migrations/0016_agents_skills_workflows.sql`
- Create: `crates/vestrace-application/src/cognitive/mod.rs`
- Create: `crates/vestrace-application/src/cognitive/commands.rs`
- Create: `crates/vestrace-application/src/cognitive/ports.rs`
- Create: `crates/vestrace-application/src/cognitive/services.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/agent_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/skill_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/workflow_repository.rs`
- Create: `tests/cognitive_assets.rs`

**Interfaces:**
- Produces create, revise, get and list services for agents, skills and workflows.
- Every revision is immutable and optimistic-concurrency protected.
- Asset relations reference stable IDs and exact revisions where execution reproducibility requires it.

- [ ] **Step 1: Write failing optimistic concurrency tests**

Two updates from revision 2: one creates revision 3, the other returns revision conflict. Verify revision 2 remains unchanged.

- [ ] **Step 2: Create tables**

Tables: `agents`, `agent_revisions`, `agent_skills`, `skills`, `skill_revisions`, `skill_dependencies`, `workflows`, `workflow_revisions`, `workflow_nodes`, `workflow_transitions`.

- [ ] **Step 3: Add graph and reference constraints**

Use deferred validation for current revision pointers and application graph validation before persistence. Force workspace RLS.

- [ ] **Step 4: Implement authorization**

Read requires corresponding `.read`; management requires `.manage`. Agent-facing MCP remains read-only for definitions.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test cognitive_assets --test workflow_graph
git add migrations/0016_agents_skills_workflows.sql crates tests
git commit -m "feat(cognitive): persist versioned cognitive assets"
```

---

### Task 9: Add external workflow and step execution history

**Files:**
- Create: `crates/vestrace-domain/src/execution/mod.rs`
- Create: `crates/vestrace-domain/src/execution/status.rs`
- Create: `migrations/0017_external_execution_history.sql`
- Create: `crates/vestrace-application/src/execution/mod.rs`
- Create: `crates/vestrace-application/src/execution/commands.rs`
- Create: `crates/vestrace-application/src/execution/ports.rs`
- Create: `crates/vestrace-application/src/execution/services.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/execution_repository.rs`
- Create: `tests/execution_history.rs`

**Interfaces:**
- Adds `WorkflowExecutionId`, `StepExecutionId`, `ToolInvocationId`, `ExecutionArtifactId`, `ExecutionOutcomeId`.
- Produces commands `StartExecution`, `RecordStep`, `CompleteExecution`.
- Vestrace validates and records external state transitions but does not schedule the next node.

- [ ] **Step 1: Write failing transition tests**

Allowed:

```text
queued → running → waiting → running → succeeded|failed|cancelled
```

Reject completed-to-running, step referencing a node outside the execution's workflow revision, and duplicate attempt number.

- [ ] **Step 2: Create execution tables**

Store exact workflow, agent, skill, model and tool revisions used; input/output artifact references; errors; attempts; timestamps; correlation and causation IDs.

- [ ] **Step 3: Implement idempotent services**

External orchestrators may resend start/step/complete commands. Same key and hash returns original result; changed payload conflicts.

- [ ] **Step 4: Add provenance links**

Every step/model/tool/outcome creates corresponding event references so memory sources can cite them.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test execution_history
git add crates migrations/0017_external_execution_history.sql tests/execution_history.rs
git commit -m "feat(execution): record external workflow history"
```

---

### Task 10: Feed execution outcomes into memory extraction

**Files:**
- Create: `tests/execution_to_memory.rs`
- Modify: `crates/vestrace-application/src/execution/services.rs`
- Modify: extraction job routing

**Interfaces:**
- Completing a step or workflow appends an `execution.outcome.recorded` event and outbox message.
- Extraction can create `Observation`, `Outcome`, `Procedure`, `Decision` or `Task` candidates with direct execution provenance.

- [ ] **Step 1: Write failing outcome-to-memory test**

Record a failed migration step caused by missing pgvector, then a successful retry. Deterministic extractor produces:

```text
Observation: pgvector extension was missing
Procedure: install vector extension before vector migrations
Outcome: retry succeeded after extension installation
```

Assert sources point to exact step attempts and model/tool records.

- [ ] **Step 2: Implement event/outbox emission**

Do it in the same transaction as execution completion. Derivation method is `Extraction` or `Consolidation`, never `HumanAuthored` unless explicitly supplied by a user.

- [ ] **Step 3: Add conflict/dedup behavior**

Repeated identical outcome events add supporting sources to an existing candidate when structural identity matches; they do not create unlimited duplicates.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test execution_to_memory
git add crates tests/execution_to_memory.rs
git commit -m "feat(memory): derive knowledge from execution outcomes"
```

---

### Task 11: Expose model, cognitive asset and execution contracts

**Files:**
- Create: `crates/vestrace-http/src/routes/models.rs`
- Create: `crates/vestrace-http/src/routes/agents.rs`
- Create: `crates/vestrace-http/src/routes/skills.rs`
- Create: `crates/vestrace-http/src/routes/workflows.rs`
- Create: `crates/vestrace-http/src/routes/executions.rs`
- Create: `crates/vestrace-http/src/routes/evaluations.rs`
- Create: `crates/vestrace-mcp/src/tools/models.rs`
- Create: `crates/vestrace-mcp/src/tools/cognitive.rs`
- Create: `crates/vestrace-mcp/src/tools/executions.rs`
- Modify: MCP resources and schema snapshots
- Create: contract tests beside existing suites

**Interfaces:**
- HTTP exposes model registry, routing, evaluations, cognitive asset management and execution logging.
- MCP exposes:

```text
vestrace_models_list
vestrace_model_route
vestrace_model_evaluation_record
vestrace_agent_get
vestrace_skill_get
vestrace_workflow_get
vestrace_workflow_context
vestrace_execution_start
vestrace_step_record
vestrace_execution_complete
```

- MCP does not expose provider management, asset permission management or hard purge.

- [ ] **Step 1: Write failing HTTP/MCP equivalence tests**

For model route and execution start, compare normalized results and error codes through both adapters.

- [ ] **Step 2: Implement routes/tools through shared services**

No routing score calculation or graph validation belongs in interface crates.

- [ ] **Step 3: Register remaining MCP resources**

Add models, agents, skills, workflows and executions now that backing subsystems exist.

- [ ] **Step 4: Regenerate schemas and run tests**

```bash
cargo test --test http_mcp_equivalence --test mcp_contract --test http_contract
cargo run -p vestrace-cli -- schema http > schemas/openapi-v1.json
cargo run -p vestrace-cli -- schema mcp > schemas/mcp-tools-v1.json
```

- [ ] **Step 5: Commit**

```bash
git add crates schemas tests
git commit -m "feat(api): expose models assets and execution history"
```

---

### Task 12: Add diagnostics, doctor and rebuild verification

**Files:**
- Create: `migrations/0018_diagnostics_and_metric_rollups.sql`
- Create: `crates/vestrace-application/src/diagnostics/mod.rs`
- Create: `crates/vestrace-application/src/diagnostics/doctor.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/diagnostics_repository.rs`
- Modify: `crates/vestrace-cli/src/commands/doctor.rs`
- Modify: `crates/vestrace-cli/src/commands/rebuild.rs`
- Create: `tests/diagnostics.rs`

**Interfaces:**
- `vestrace doctor` checks migrations, extensions, broken references, current revision consistency, active memory source coverage, outbox lag, expired leases, dead-letter jobs, missing search documents/embeddings and stale model health.
- Exit code `0` means no error-severity findings; warning-only findings are printed but keep exit `0`; errors exit non-zero.

- [ ] **Step 1: Write failing corrupted-reference test**

Using a privileged test connection, create a controlled inconsistency. `DoctorService` must report a stable finding code and affected object ID.

- [ ] **Step 2: Implement typed findings**

```rust
pub struct DiagnosticFinding {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub object: Option<KnowledgeRef>,
    pub message: String,
    pub remediation: String,
}
```

No finding contains memory content or secrets.

- [ ] **Step 3: Implement rebuild verification**

After rebuild commands, doctor confirms derivative coverage and reports counts by embedding/search projection version.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test diagnostics
git add migrations/0018_diagnostics_and_metric_rollups.sql crates tests/diagnostics.rs
git commit -m "feat(ops): add doctor and consistency diagnostics"
```

---

### Task 13: Add metrics and final Compose topology

**Files:**
- Modify: `docker-compose.yml`
- Modify: `Dockerfile`
- Create: `crates/vestrace-http/src/metrics.rs`
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Create: `tests/compose_smoke.rs`
- Modify: `README.md`

**Interfaces:**
- Compose services: `postgres`, `vestrace-server`, `vestrace-worker`.
- Exposes Prometheus-compatible `/metrics` with no sensitive labels.
- Worker and server support graceful shutdown.

- [ ] **Step 1: Add metric definitions**

System metrics: HTTP/MCP latency and errors, queue length, oldest job age, retry/dead-letter count, outbox lag, retrieval duration/candidate count/context size.

AI metrics: model latency, token usage, estimated cost, provider errors, fallback rate, schema failures and quality scores.

Memory metrics: events, candidates, activations, revisions, conflicts, expirations and retrieval use.

Never label metrics with user content, query text, memory IDs or unbounded model response values.

- [ ] **Step 2: Add server/worker graceful shutdown tests**

Server stops accepting new work and drains bounded requests. Worker stops leasing jobs, finishes or releases the current lease, then exits.

- [ ] **Step 3: Finalize Compose**

Server and worker use the same image with different subcommands. Add health checks, persistent PostgreSQL volume and no bundled model endpoint requirement.

- [ ] **Step 4: Run smoke test**

```bash
docker compose up --build -d
curl --fail http://127.0.0.1:8080/health/ready
curl --fail http://127.0.0.1:8080/metrics
docker compose exec vestrace-server vestrace doctor
docker compose down -v
```

- [ ] **Step 5: Commit**

```bash
git add Dockerfile docker-compose.yml crates tests/compose_smoke.rs README.md
git commit -m "chore: finalize Vestrace runtime topology"
```

---

### Task 14: Implement the full v0.1 acceptance suite

**Files:**
- Create: `tests/v01_acceptance.rs`
- Create: `tests/fixtures/openai-compatible/`
- Modify: `.github/workflows/ci.yml`
- Create: `docs/acceptance/v0.1.md`

**Interfaces:**
- Provides one deterministic full-cycle acceptance test and a human-readable acceptance procedure.

- [ ] **Step 1: Build deterministic provider fixtures**

Fixtures cover extraction, embeddings, model routing execution response and evaluation. No external network or API key is required.

- [ ] **Step 2: Implement full-cycle test**

The test must prove:

```text
create workspace and principal
→ register local and remote model profiles
→ create agent, skill and workflow definitions
→ record user and architecture events
→ extract and activate memories with provenance
→ generate search documents and embeddings
→ retrieve current decisions and build bounded context
→ route a workflow step using Balanced quality-first-then-cost
→ record model and external step execution
→ evaluate result
→ derive new outcome/procedure memory
→ retrieve that memory in a new session
→ revise stale knowledge
→ verify old knowledge is absent from current context
```

- [ ] **Step 3: Add security assertions to the same scenario**

Verify foreign workspace denial, Restricted remote-provider rejection, self-elevation denial and hard purge approval requirement.

- [ ] **Step 4: Add failure/degraded assertions**

Disable embeddings and reranker; FTS/structured context still succeeds with warnings. Mark selected model unavailable; valid fallback is used and explained.

- [ ] **Step 5: Add CI job and acceptance document**

The `main` workflow runs image build, Compose smoke, acceptance test, dependency audit and container vulnerability scan.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test v01_acceptance -- --nocapture
docker compose up --build -d
cargo test --test compose_smoke -- --nocapture
docker compose down -v
git add tests docs/acceptance .github/workflows/ci.yml
git commit -m "test: verify complete Vestrace v0.1 lifecycle"
```

---

## Cognitive Runtime Foundation completion gate

Run all verification with fresh output:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --workspace --all-features
cargo run -p vestrace-cli -- schema http | diff -u schemas/openapi-v1.json -
cargo run -p vestrace-cli -- schema mcp | diff -u schemas/mcp-tools-v1.json -
docker compose up --build -d
curl --fail http://127.0.0.1:8080/health/ready
curl --fail http://127.0.0.1:8080/metrics
docker compose exec vestrace-server vestrace doctor
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test v01_acceptance -- --nocapture
docker compose down -v
```

Review requirements line by line against the approved design and roadmap. Confirm:

- routing is deterministic, explainable and preserves exact revisions;
- Balanced filters by minimum quality before minimizing cost;
- privacy constraints are hard filters and survive fallback;
- model observations update rollups without overwriting raw records;
- agents, skills and workflows are immutable-revision assets, not memories;
- workflow graphs are stored and validated but never executed by Vestrace;
- external execution events can produce provenance-backed memories;
- all advertised HTTP and MCP contracts have real backing services;
- diagnostics detect intentional corruption and derivative gaps;
- the complete cycle `event → memory → retrieval → context → routing/execution outcome → improved next context` passes without an external provider.
