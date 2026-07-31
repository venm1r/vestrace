# Vestrace Harness v0.2 Implementation Roadmap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement each detailed plan task-by-task.

**Goal:** Extend the approved Vestrace v0.1 memory-first platform into a universal, durable and policy-governed AI-agent harness without weakening memory, provenance or security boundaries.

**Design source:** `docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md`  
**Rig decision:** `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`  
**Prerequisite roadmap:** `docs/superpowers/plans/2026-07-31-vestrace-roadmap.md`

## Global Constraints

- Rust Edition 2024 and the repository Rust version floor remain authoritative.
- PostgreSQL remains the source of truth for durable run state, journals, policies, budgets, jobs and metadata.
- Binary artifact content is accessed only through `ArtifactStorePort`.
- Vestrace owns all domain, application, persistence and public contracts.
- Rig types are confined to `vestrace-rig-adapter` and cannot enter public or persisted contracts.
- The native OpenAI-compatible provider adapter is a production implementation and the baseline Rig-free path.
- Model-generated actions are proposals; Policy Engine and execution adapters enforce authority.
- External side effects require idempotency, approval binding where applicable, verification and audit.
- Delegation depth defaults to one.
- `Commit` autonomy is disabled by default.
- Every plan follows TDD, ends in independently testable software and uses one branch with task-level commits.

---

## Why this roadmap is split

The Harness design covers durable orchestration, model execution, tools, sandboxes, policies, credentials, artifacts, channels, extensions and evaluation. These have different safety and recovery failure modes. Each plan below establishes narrow contracts and an exit gate before downstream plans consume them.

## Dependency summary

```text
Vestrace v0.1 plans
        ↓
H1 Durable Run Core
        ↓
H2 Policy, Approval and Budget Core
        ├───────────────┐
        ↓               │
H0-RIG Spike            │
        ↓               │
H3 Model Runtime        │
        ↓               │
H4 Tool Runtime and Sandbox
        ↓
H5 Planning and Delegation
        ↓
H6 Context and Artifact Runtime
        ↓
H7 Conversations, Channels and Triggers
        ↓
H8 Connections and Credential Broker
        ↓
H9 Agent Packages and Extension Registry
        ↓
H10 Observability and Evaluation
        ↓
H11 Universal Vertical Slice and Product Surface
```

`H0-RIG` blocks only the production choice of the bounded model↔tool loop. It does not block v0.1 Foundation, Memory Core, Retrieval, Interfaces, provider registry, the native OpenAI-compatible adapter, or the initial durable Run and Policy contracts.

## H1. Durable Run Core

Builds:

- `AgentRun`, `RunStep`, `RunEvent` and immutable plan-reference identifiers;
- authoritative run and step state machines;
- append-only execution journal;
- checkpoints, resume cursor and context references;
- optimistic concurrency, leases and heartbeats;
- pause, resume, cancel and waiting states;
- parent/subrun lineage fields without delegation behavior;
- PostgreSQL work items and transactional creation with run state changes;
- logical replay and restart integration tests.

**Exit gate:** a multi-step Run survives server and worker restart, resumes from its durable checkpoint, prevents concurrent workers from advancing the same version and produces the same logical state through journal replay.

## H2. Policy, Approval and Budget Core

Builds:

- centralized `PolicyEnginePort` and typed authorization requests;
- decisions `Permit`, `Deny`, `PrepareOnly`, `RequireApproval` and obligations;
- capability ceilings and data classifications;
- payload-bound approval grants;
- hierarchical budgets, reservations and reconciliation;
- workspace quotas and run/subrun allocations;
- policy and budget journals;
- partial outcome on resource exhaustion.

**Exit gate:** a worker cannot execute a protected action without a matching policy decision and approval; parallel steps cannot reserve more than the remaining hard budget; and budget exhaustion produces a durable partial result instead of uncontrolled retries.

## H0-RIG. Rig Agent Runtime Spike

**Type:** blocking architecture spike, not production runtime.  
**Decision source:** `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`

### Inputs

The spike consumes these stable Vestrace-owned contracts from H1 and H2:

- `ModelLoopPort` draft boundary;
- canonical model, tool and run event DTOs;
- `PolicyEnginePort` test implementation;
- budget reservation port;
- checkpoint envelope with engine metadata and journal cursor;
- mock `ToolExecutionPort` supporting deterministic side effects and `Unknown` outcomes.

### Spike structure

Use an isolated experimental crate that is excluded from production default features:

```text
crates/vestrace-rig-spike/
├── src/lib.rs
├── src/adapter.rs
├── src/checkpoint.rs
├── src/hooks.rs
└── tests/
    ├── policy_interception.rs
    ├── pause_resume.rs
    ├── side_effect_idempotency.rs
    ├── unknown_outcome.rs
    ├── streaming_equivalence.rs
    └── type_boundary.rs
```

The crate may use a pinned Rig version. No production crate may depend on the spike.

### Required experiments

1. Pause after model tool proposal and before execution.
2. Deny a tool through Vestrace policy and return a structured result to the loop.
3. Execute a permitted tool only through `ToolExecutionPort`.
4. Crash after an external side effect but before loop acknowledgement; recover without duplicating it.
5. Preserve and reconcile `Unknown` external completion.
6. Restore from a versioned checkpoint and canonical journal cursor.
7. Compare streaming and non-streaming canonical events.
8. Normalize usage, errors and tool calls into Vestrace DTOs.
9. Compile domain/application crates without any Rig dependency.
10. Replace the pinned Rig version in a trial branch and measure adapter-only change scope.

### Decision output

Create a follow-up ADR with exactly one outcome:

- `Accepted` — implement `RigModelLoopAdapter`;
- `AcceptedWithRestrictions` — use specified Rig components only;
- `Rejected` — implement `NativeVestraceModelLoop`.

The ADR must list evidence from every required experiment. Ambiguous results fail the gate.

**Exit gate:** all experiments have reproducible tests and the follow-up ADR selects the production model-loop strategy. Until this gate passes, H3 may implement provider adapters and invocation records but may not commit to a production loop engine.

## H3. Model Runtime and Provider Adapters

Builds:

- Vestrace-owned `ModelProviderPort`, `ModelLoopPort` and invocation DTOs;
- production native OpenAI-compatible adapter;
- Rig provider adapter behind a feature flag;
- model definition bindings and capability negotiation;
- structured-output validation and canonical streaming events;
- usage, pricing and budget reconciliation;
- explainable router integration and controlled fallback;
- production model-loop implementation selected by the H0-RIG ADR;
- shared provider conformance suite;
- mandatory Rig-free CI path.

**Exit gate:** the same logical invocation succeeds through native and Rig provider adapters, produces equivalent canonical events, respects policy and budget decisions, and the baseline vertical test passes with Rig disabled.

## H4. Tool Runtime and Sandbox

Builds:

- versioned `ToolDefinition` and execution bindings;
- risk and side-effect classifications;
- `ToolExecutionPort` and adapters for native, HTTP, MCP, human and subagent tools;
- prepare/preview/approve/commit/verify lifecycle;
- idempotency keys, retry classification and reconciliation;
- `Unknown` terminal state;
- `SandboxProviderPort`, ephemeral workspaces and Docker implementation;
- network allowlists, resource limits and controlled artifact mounts;
- tool discovery and prompt-injection trust boundaries.

**Exit gate:** a destructive tool cannot run without exact approval binding; a simulated lost response does not duplicate a side effect; and sandboxed code cannot access undelegated artifacts, network destinations or credentials.

## H5. Planning and Delegation

Builds:

- Direct, Guided and Workflow execution modes;
- immutable `ExecutionPlan` revisions and `PlanValidator`;
- step dependency scheduling and bounded replanning;
- Coordinator and one-level `SubRun` delegation;
- `DelegationRequest`, delegation token and resource allocation;
- isolated subagent context;
- typed `HandoffArtifact`;
- trust, validation and cross-check policies for subrun results;
- cycle, depth and parallelism limits.

**Exit gate:** a Coordinator creates a validated plan, delegates two isolated subruns with narrower capabilities and budgets, resumes after a restart and rejects a handoff that fails its output or evidence contract.

## H6. Context and Artifact Runtime

Builds:

- Identity, Memory, Run State, Working Context, Evidence and Artifact layers;
- `ModelContextEnvelope` and context snapshots;
- token-aware context allocation and deterministic truncation order;
- structured scratchpad without hidden chain-of-thought retention;
- `Artifact`, immutable revisions and provenance graph;
- `ArtifactStorePort`, local content-addressed store and S3-compatible adapter;
- quarantine, validation and derived representations;
- controlled sandbox mounts, deliverable export, retention and garbage collection;
- run consolidation into memory candidates.

**Exit gate:** a large input document is quarantined, snapshotted, represented by relevant excerpts, processed in a sandbox and exported as a validated deliverable with hash and complete provenance; only evaluated outcomes become memory candidates.

## H7. Conversations, Channels and Triggers

Builds:

- `Conversation`, `InteractionEvent` and run routing;
- typed human requests for clarification, review and approval;
- channel-independent outcomes;
- HTTP, WebSocket/SSE and CLI adapters;
- durable event cursors and reconnect behavior;
- notification port and user detail levels;
- manual, API, schedule and run-continuation triggers;
- `Observe`, `Suggest`, `Prepare`, `Execute` autonomy levels;
- run proposals, cooldown, deduplication and causal-loop prevention.

**Exit gate:** a Run begins through HTTP, pauses for clarification, resumes from a CLI response, streams missed events after reconnect and can be started by a schedule without exceeding its trigger ceiling.

## H8. Connections and Credential Broker

Builds:

- `ConnectorDefinition` and user/workspace-bound `Connection`;
- OAuth and API-key authorization flows;
- `SecretReference` and secret backend port;
- centralized Credential Broker and operation-bound credential leases;
- reauthorization, rotation and revocation;
- delegated connection grants for subruns;
- credential proxy for sandboxed tools;
- secret leak detection, redaction and usage audit.

**Exit gate:** a tool acts through a short-lived lease without exposing the secret to the model or sandbox, a subrun receives only delegated operations, and revocation prevents subsequent calls while preserving audit records.

## H9. Agent Packages and Extension Registry

Builds:

- `AgentProfile`, `SkillDefinition`, `WorkflowDefinition` and immutable `AgentRuntimeSnapshot`;
- explicit overlays and compatibility validation;
- `AgentPackage` and local package registry;
- `ExtensionManifest`, extension identities and revision pinning;
- external process, HTTP and MCP extension protocols;
- capability negotiation, health and circuit breaker;
- conformance suites and permission-diff activation;
- Universal Assistant, Research and Workspace Automation reference packages.

**Exit gate:** a signed or digest-pinned package installs inactive, passes compatibility and permission checks, activates explicitly and creates a Run whose snapshot remains unchanged when the package is later updated.

## H10. Observability and Evaluation

Builds:

- OpenTelemetry-compatible logs, metrics and traces;
- execution journal projections and security audit trail;
- privacy-aware capture modes;
- integrity checkpoints for audit records;
- evaluation datasets and `EvaluationRun`;
- deterministic, reference, rubric and independent-model grading;
- risk-oriented verification and correction limits;
- regression gates, shadow mode and safe replay;
- user-facing Run explanation and operational dashboards.

**Exit gate:** a component revision cannot activate after a quality or policy regression; a production-like Run can be safely replayed without repeating writes; and the user can inspect sources, tools, checks, budget and unresolved warnings without viewing hidden reasoning.

## H11. Universal Vertical Slice and Product Surface

Builds:

- the complete universal task path from interaction to durable outcome;
- Universal Assistant, Research and Workspace Automation package integration;
- versioned `/v1` API, event stream, MCP adapter and public schemas;
- Rust and TypeScript SDK ergonomic layers;
- staged artifact upload/download and signed webhooks;
- minimal web console for tasks, plans, approvals, artifacts, budgets, profiles, connections and triggers;
- Personal, Team and Embedded deployment profiles;
- Docker Compose acceptance environment;
- upgrade, backup, recovery and operator documentation.

### Mandatory acceptance scenario

```text
User task
→ durable AgentRun
→ validated plan
→ memory-aware context
→ policy-constrained model routing
→ isolated research subruns
→ evidence and verification
→ sandboxed document generation
→ artifact validation and preview
→ approved export
→ memory candidates
→ restart and resume proof
```

**Exit gate:** every v0.2 readiness criterion in the Harness design passes, including no duplicate side effects, strict delegated capabilities, exact approval binding, hard budget enforcement, verified `Succeeded`, full artifact provenance and equivalent HTTP/CLI/MCP application behavior.

## Branch policy

Use one branch per detailed plan. Suggested names:

```text
feat/harness-run-core
feat/harness-policy-budget
spike/rig-agent-runtime
feat/harness-model-runtime
feat/harness-tool-sandbox
feat/harness-planning-delegation
feat/harness-context-artifacts
feat/harness-channels-triggers
feat/harness-credentials
feat/harness-packages-extensions
feat/harness-observability-evals
feat/harness-vertical-slice
```

The spike branch produces tests and ADR evidence, not production dependencies.

## Verification baseline

Every production plan keeps these commands green:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features --features provider-openai-compatible
```

Plans with PostgreSQL run migration and RLS suites. Plans with public contracts run schema and compatibility tests. Plans with sandbox behavior run isolation tests in the supported container environment.

## Scope exclusions for v0.2

- managed public SaaS;
- `Commit` autonomy by default;
- unrestricted agent-created extensions, skills or policies;
- deep unbounded agent hierarchies;
- mandatory Kubernetes, Kafka or NATS;
- public marketplace;
- automatic online modification of active profiles or router policies;
- storage of hidden chain-of-thought;
- direct credentials in models, tools or sandboxes;
- Rig as a domain, persistence or public-contract dependency.

## Completion definition

The roadmap is complete only when H1–H11 pass their exit gates, H0-RIG has a conclusive follow-up ADR, and the mandatory vertical slice works both with the selected production model loop and with the native OpenAI-compatible provider path while Rig provider support remains optional.
