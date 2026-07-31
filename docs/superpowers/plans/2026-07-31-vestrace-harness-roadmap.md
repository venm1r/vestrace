# Vestrace Harness v0.2 Implementation Roadmap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement each detailed plan task-by-task.

**Goal:** Extend the approved Vestrace v0.1 memory-first platform into a universal, durable and policy-governed AI-agent harness without weakening memory, provenance or security boundaries.

**Design source:** `docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md`  
**Rig decision:** `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`  
**A2A decision:** `docs/superpowers/specs/adr/0003-a2a-interoperability-boundary.md`  
**Prerequisite roadmap:** `docs/superpowers/plans/2026-07-31-vestrace-roadmap.md`

## Global Constraints

- Rust Edition 2024 and the repository Rust version floor remain authoritative.
- PostgreSQL remains the source of truth for durable run state, journals, policies, budgets, jobs and metadata.
- Binary artifact content is accessed only through `ArtifactStorePort`.
- Vestrace owns all domain, application, persistence and public contracts.
- Rig types are confined to `vestrace-rig-adapter` and cannot enter public or persisted contracts.
- A2A SDK types are confined to `vestrace-a2a-adapter` and explicit A2A wire endpoints.
- An A2A Task is never an authoritative `AgentRun` or internal `SubRun`.
- Agent Cards, remote messages, metadata and artifacts are external untrusted input.
- The native OpenAI-compatible provider adapter is a production implementation and the baseline Rig-free path.
- Model-generated actions are proposals; Policy Engine and execution adapters enforce authority.
- External side effects require idempotency, approval binding where applicable, verification and audit.
- Remote-agent dispatch and cancellation are protected operations with durable reconciliation.
- Delegation depth defaults to one.
- `Commit` autonomy is disabled by default.
- Every plan follows TDD, ends in independently testable software and uses one branch with task-level commits.

---

## Why this roadmap is split

The Harness design covers durable orchestration, model execution, tools, sandboxes, policies, credentials, artifacts, channels, extensions, external-agent interoperability and evaluation. These have different safety and recovery failure modes. Each plan below establishes narrow contracts and an exit gate before downstream plans consume them.

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
H9A A2A Interoperability Gateway
        ↓
H10 Observability and Evaluation
        ↓
H11 Universal Vertical Slice and Product Surface
```

`H0-RIG` blocks only the production choice of the bounded model↔tool loop. It does not block v0.1 Foundation, Memory Core, Retrieval, Interfaces, provider registry, the native OpenAI-compatible adapter, or the initial durable Run and Policy contracts.

`H9A` consumes contracts established across H5–H9. Those plans must account for remote agents, but they do not depend on the `a2a-rs` SDK or implement A2A transport behavior themselves.

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
- `ToolExecutionPort` and adapters for native, HTTP, MCP and human tools;
- prepare/preview/approve/commit/verify lifecycle;
- idempotency keys, retry classification and reconciliation;
- `Unknown` terminal state;
- `SandboxProviderPort`, ephemeral workspaces and Docker implementation;
- network allowlists, resource limits and controlled artifact mounts;
- tool discovery and prompt-injection trust boundaries.

External autonomous agents are not modeled as tools. Internal SubRuns and remote A2A delegation are owned by H5 and H9A.

**Exit gate:** a destructive tool cannot run without exact approval binding; a simulated lost response does not duplicate a side effect; and sandboxed code cannot access undelegated artifacts, network destinations or credentials.

## H5. Planning and Delegation

Builds:

- Direct, Guided and Workflow execution modes;
- immutable `ExecutionPlan` revisions and `PlanValidator`;
- step dependency scheduling and bounded replanning;
- Coordinator and one-level internal `SubRun` delegation;
- separate external `RemoteAgentInvocation` delegation boundary;
- `DelegationRequest`, delegation token and resource allocation;
- remote-agent eligibility, trust and declared-skill requirements independent of transport SDKs;
- isolated internal and external delegated context;
- typed `HandoffArtifact` shared by validated internal and external delegation results;
- trust, validation and cross-check policies for delegated results;
- cycle, depth, parallelism and remote-dispatch limits.

The Coordinator distinguishes:

```text
Internal delegation
→ Vestrace-owned SubRun

External delegation
→ Vestrace-owned RemoteAgentInvocation
→ remote-owned protocol task
```

**Exit gate:** a Coordinator creates a validated plan, delegates two isolated internal subruns and one deterministic remote-agent fixture with narrower capabilities and budgets, resumes after a restart and rejects a handoff that fails its output or evidence contract. The fixture uses Vestrace-owned ports, not an A2A transport implementation.

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
- external-agent context envelopes with explicit data classifications and omitted-reason records;
- external artifact candidate ingestion for text, structured data, raw bytes and policy-controlled URLs;
- SSRF, scheme, DNS, size, malware and secret checks before remote artifacts become available;
- run consolidation into memory candidates.

**Exit gate:** a large input document is quarantined, snapshotted, represented by relevant excerpts, processed in a sandbox and exported as a validated deliverable with hash and complete provenance; an untrusted remote artifact follows the same quarantine and validation path; only evaluated outcomes become memory candidates.

## H7. Conversations, Channels and Triggers

Builds:

- `Conversation`, `InteractionEvent` and run routing;
- typed human requests for clarification, review, approval and authentication;
- channel-independent outcomes;
- HTTP, WebSocket/SSE and CLI adapters;
- durable event cursors and reconnect behavior;
- notification port and user detail levels;
- remote-agent continuation contracts for input-required and authentication-required states;
- canonical remote progress events independent of A2A transport frames;
- manual, API, schedule and run-continuation triggers;
- `Observe`, `Suggest`, `Prepare`, `Execute` autonomy levels;
- run proposals, cooldown, deduplication and causal-loop prevention.

Inbound and outbound A2A transport adapters are deferred to H9A; H7 supplies the reusable continuation and streaming semantics.

**Exit gate:** a Run begins through HTTP, pauses for clarification, resumes from a CLI response, streams missed events after reconnect and can be started by a schedule without exceeding its trigger ceiling. A deterministic remote invocation can pause for input and resume through the same channel-independent human-request contract.

## H8. Connections and Credential Broker

Builds:

- `ConnectorDefinition` and user/workspace-bound `Connection`;
- OAuth and API-key authorization flows;
- `SecretReference` and secret backend port;
- centralized Credential Broker and operation-bound credential leases;
- reauthorization, rotation and revocation;
- delegated connection grants for subruns and remote-agent invocations;
- remote-agent connection profiles with allowed identities, transports, operations, resources and classifications;
- request-scoped authentication material for external-agent dispatch;
- credential proxy for sandboxed tools;
- secret leak detection, redaction and usage audit.

Permanent tokens are never stored in Agent Cards, `RemoteAgentInvocation`, A2A messages or durable transport metadata.

**Exit gate:** a tool and a deterministic remote-agent client act through separate short-lived leases without exposing secrets to the model, sandbox or durable invocation state; delegated operations remain bounded; revocation prevents subsequent calls while preserving audit records.

## H9. Agent Packages and Extension Registry

Builds:

- `AgentProfile`, `SkillDefinition`, `WorkflowDefinition` and immutable `AgentRuntimeSnapshot`;
- explicit overlays and compatibility validation;
- `AgentPackage` and local package registry;
- `ExtensionManifest`, extension identities and revision pinning;
- external process, HTTP and MCP extension protocols;
- remote-agent definition registry and immutable Agent Card snapshots;
- local trust classification, allowed skills, transports and data classifications;
- A2A adapter extension category without an SDK dependency in domain/application crates;
- capability negotiation, health and circuit breaker;
- conformance suites and permission-diff activation;
- Universal Assistant, Research and Workspace Automation reference packages.

Agent Card changes produce a new remote-agent revision and cannot silently broaden permissions or data access.

**Exit gate:** a signed or digest-pinned package and a remote-agent definition install inactive, pass compatibility and permission checks, activate explicitly and create Runs whose snapshots remain unchanged when the package or remote Agent Card is later updated.

## H9A. A2A Interoperability Gateway

**Decision source:** `docs/superpowers/specs/adr/0003-a2a-interoperability-boundary.md`

Builds:

- `vestrace-remote-agent-runtime` with Vestrace-owned definitions, invocations, events, errors and reconciliation ports;
- `vestrace-a2a-adapter` using exact-pinned `a2a-rs` crates behind an anti-corruption layer;
- outbound A2A client for JSON-RPC and HTTP+JSON;
- SSE streaming, reconnect and duplicate-event normalization;
- inbound A2A server mapping authenticated protocol calls to CreateRun, ContinueRun, CancelRun and event projections;
- Agent Card discovery, hashing, revisioning and publication;
- mapping between A2A Task states and `RemoteAgentInvocation` without equating their ownership;
- request-scoped Credential Broker integration;
- artifact mapping through H6 quarantine and validation;
- `InputRequired` and `AuthRequired` durable continuation;
- remote task reconciliation after timeout, lost response or worker restart;
- deterministic protocol conformance fixtures and type-boundary checks.

Initial product bindings:

```text
Core A2A v1
JSON-RPC over HTTP
HTTP+JSON / REST
SSE task streaming and subscription
```

Deferred:

```text
gRPC as a required binding
SLIMRPC and collaborative channels
push notification callbacks
automatic Agent Card trust
cross-organization credential delegation
```

### Mandatory acceptance scenario

```text
parent AgentRun
→ protected RemoteAgentInvocation
→ external A2A Task
→ streamed Working status
→ InputRequired
→ durable pause
→ user response
→ continue same external task
→ streamed artifact
→ quarantine and validation
→ verified HandoffArtifact
→ parent Run continuation after worker restart
```

A failure-path test drops the response after the remote server accepts the task. Vestrace reconciles through known external identifiers or remains `Unknown`; it never creates a duplicate remote task automatically.

**Exit gate:** Vestrace can both call a deterministic independent A2A agent and expose a selected Vestrace agent through A2A. JSON-RPC and HTTP+JSON produce equivalent canonical state, SSE resume does not duplicate events, credentials remain request-scoped, untrusted artifacts cannot bypass quarantine, and no `a2a-rs` type appears in Vestrace domain, application or persistence contracts.

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
- user-facing Run explanation and operational dashboards;
- remote-agent metrics for dispatch, reconciliation, input waits, artifact rejection and duplicate-prevention;
- correlation across parent Run, `RemoteAgentInvocation`, external task/context identifiers and transport requests without logging credentials.

**Exit gate:** a component revision cannot activate after a quality or policy regression; a production-like Run can be safely replayed without repeating writes or remote-agent dispatch; and the user can inspect sources, tools, remote delegations, checks, budget and unresolved warnings without viewing hidden reasoning.

## H11. Universal Vertical Slice and Product Surface

Builds:

- the complete universal task path from interaction to durable outcome;
- Universal Assistant, Research and Workspace Automation package integration;
- versioned `/v1` API, event stream, MCP adapter, A2A gateway and public schemas;
- Rust and TypeScript SDK ergonomic layers;
- staged artifact upload/download and signed webhooks;
- minimal web console for tasks, plans, approvals, artifacts, budgets, profiles, connections, remote agents and triggers;
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
→ isolated internal research subruns
→ protected external A2A delegation
→ evidence and verification
→ sandboxed document generation
→ artifact validation and preview
→ approved export
→ memory candidates
→ restart and resume proof
```

The product surface also publishes one reference agent through A2A and verifies that an external deterministic client can create, stream, pause, resume and complete its task without bypassing Vestrace application or policy layers.

**Exit gate:** every v0.2 readiness criterion in the Harness design passes, including no duplicate local or remote side effects, strict delegated capabilities, exact approval binding, hard budget enforcement, verified `Succeeded`, full artifact provenance and equivalent HTTP/CLI/MCP/A2A application behavior where the protocols overlap.

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
feat/harness-a2a-gateway
feat/harness-observability-evals
feat/harness-vertical-slice
```

The Rig spike branch produces tests and ADR evidence, not production dependencies. A2A implementation begins only after H5–H9 contracts are stable.

## Verification baseline

Every production plan keeps these commands green:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features --features provider-openai-compatible
```

Plans with PostgreSQL run migration and RLS suites. Plans with public contracts run schema and compatibility tests. Plans with sandbox behavior run isolation tests in the supported container environment. H9A additionally runs local A2A JSON-RPC, HTTP+JSON and SSE conformance suites with no public network dependency.

## Scope exclusions for v0.2

- managed public SaaS;
- `Commit` autonomy by default;
- unrestricted agent-created extensions, skills or policies;
- deep unbounded agent hierarchies;
- mandatory Kubernetes, Kafka or NATS;
- public marketplace;
- automatic online modification of active profiles or router policies;
- storage of hidden chain-of-thought;
- direct credentials in models, tools, sandboxes or A2A messages;
- Rig as a domain, persistence or public-contract dependency;
- A2A Tasks as authoritative AgentRuns or SubRuns;
- automatic trust in remote Agent Cards or skill claims;
- mandatory A2A gRPC, SLIMRPC, collaborative channels or push callbacks;
- automatic retry of an ambiguous remote-agent dispatch.

## Completion definition

The roadmap is complete only when H1–H11 and H9A pass their exit gates, H0-RIG has a conclusive follow-up ADR, and the mandatory vertical slice works with the selected production model loop, the native OpenAI-compatible provider path and one external A2A delegation while Rig provider support remains optional.
