# Vestrace H3 Model Runtime and Provider Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, run provider tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement Vestrace-owned model invocation contracts, a production native OpenAI-compatible adapter, an optional Rig provider adapter, canonical streaming and structured outputs, policy- and budget-governed routing/fallback, durable invocation records, restart-safe model work and the bounded model-loop implementation selected by ADR-0002.

**Architecture:** Extend the v0.1 provider/model registry, router and `model_executions` records rather than introducing parallel concepts. One logical `ModelExecution` contains one normalized request and one or more ordered `ModelExecutionAttempt` records when controlled fallback is allowed. Every adapter implements the same Vestrace-owned `ModelProviderPort`; provider-specific types remain inside adapter crates. The native OpenAI-compatible adapter is a complete narrow production path and remains usable when Rig is absent. The bounded model loop is a sans-I/O `ModelLoopPort`; ADR-0002 determines its production implementation without changing provider contracts.

**Tech Stack:** Existing Vestrace v0.1, H1 and H2 Rust workspace; Rust Edition 2024; Tokio; Serde; Schemars; SQLx; PostgreSQL 17; Reqwest; Futures; Tokio Util; SHA-256; JSON Schema validation; deterministic loopback HTTP fixtures; optional exact-pinned `rig-core` and `rig-agent` approved by ADR-0002.

## Global Constraints

- Complete all five v0.1 plans, H1 Durable Run Core, H2 Policy/Approval/Budget Core and the H0-RIG spike before Task 11.
- ADR-0001 remains authoritative: `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`.
- ADR-0002 must exist before Task 11 and contain exactly `Accepted`, `AcceptedWithRestrictions` or `Rejected`.
- Vestrace owns every domain type, application port, persisted schema, durable event, public schema and checkpoint envelope.
- No Rig type may appear outside `vestrace-rig-adapter` or in a Vestrace-owned persisted/public contract.
- `vestrace-provider-openai-compatible` is a production adapter, not a reference stub.
- Native support is intentionally limited to text, JSON structured output, tool-call proposals, streaming text/tool deltas and embeddings. Unsupported modalities fail before dispatch.
- Provider adapters never select models, authorize data transfer, issue approvals, reserve budgets or execute tools.
- Every provider dispatch requires a consumed H2 `GuardedAction` for `model.invoke` and the required reservations.
- Provider credentials are resolved only inside adapter infrastructure through a secret reference. Secret material never enters prompts, domain values, durable events, ordinary logs or public DTOs.
- H3 may expose tool definitions and return tool proposals, but H3 never executes a real tool. H4 owns tool authorization and execution.
- Provider responses, stream chunks, tool proposals and refusals are untrusted data and cannot modify policy, capabilities, selection or Run authority.
- `ModelExecution` and every attempt use stable idempotency keys. A fallback is a new attempt under the same logical execution.
- `Unknown` completion is never retried or failed over until reconciliation proves a new dispatch is safe.
- Safety refusal is never bypassed by automatic fallback.
- Authentication failure, policy denial, hard-budget exhaustion and invalid local configuration are non-fallback failures.
- Streaming events are provisional transport observations. Only canonical durable events and final assembled output are authoritative.
- Structured output passes local JSON parsing, the exact stored JSON Schema and any explicitly configured semantic validator.
- Hidden chain-of-thought is neither requested for persistence nor stored.
- H1 Run semantics remain unchanged: each logical Run mutation increments `RunVersion` once and appends one `RunEvent`; provider deltas and attempt telemetry do not change `RunVersion`.
- Existing migrations `0014` and `0015` are never edited. H3 extends them with migrations `0028`–`0031`.
- CI provider tests use loopback fixtures and require no public provider key or internet access.
- The Rig-free path uses `provider-openai-compatible` plus a test-only scripted loop; it does not require a production native loop when ADR-0002 accepts Rig.
- Future implementation branch: `feat/h3-model-runtime-provider-adapters`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  models/mod.rs
  models/runtime.rs
  models/message.rs
  models/output.rs
  models/usage.rs
  models/error.rs
  run/event.rs
  run/work.rs
  run/mod.rs

crates/vestrace-application/src/
  lib.rs
  models/router.rs
  models/ports.rs
  model_runtime/mod.rs
  model_runtime/ports.rs
  model_runtime/commands.rs
  model_runtime/orchestrator.rs
  model_runtime/fallback.rs
  model_runtime/stream.rs
  model_runtime/structured_output.rs
  model_runtime/worker.rs
  model_runtime/loop_port.rs
  model_runtime/test_loop.rs
  model_runtime/native_loop.rs

crates/vestrace-application/tests/
  model_runtime_orchestrator.rs
  model_fallback.rs
  model_stream.rs
  structured_output.rs
  model_worker.rs
  model_loop_conformance.rs

crates/vestrace-provider-openai-compatible/
  Cargo.toml
  src/lib.rs
  src/adapter.rs
  src/auth.rs
  src/config.rs
  src/request.rs
  src/response.rs
  src/stream.rs
  src/embeddings.rs
  src/error.rs

crates/vestrace-rig-adapter/
  Cargo.toml
  src/lib.rs
  src/provider.rs
  src/normalization.rs
  src/loop_adapter.rs
  src/checkpoint.rs

crates/vestrace-model-test-support/
  Cargo.toml
  src/lib.rs
  src/server.rs
  src/scripts.rs
  src/conformance.rs
  src/fixtures.rs

crates/vestrace-infrastructure/src/postgres/
  mod.rs
  model_runtime/mod.rs
  model_runtime/execution_repository.rs
  model_runtime/attempt_repository.rs
  model_runtime/event_repository.rs
  model_runtime/checkpoint_repository.rs
  model_runtime/reconciliation_repository.rs

migrations/
  0028_provider_adapter_bindings_and_runtime_capabilities.sql
  0029_model_execution_attempts_and_events.sql
  0030_model_loop_checkpoints_and_reconciliation.sql
  0031_model_runtime_rls_indexes_and_run_bindings.sql

tests/
  provider_binding_registry.rs
  native_provider_conformance.rs
  rig_provider_conformance.rs
  model_execution_persistence.rs
  model_execution_unknown.rs
  model_runtime_rls.rs
  model_runtime_restart.rs
  model_runtime_rig_free.rs
  h3_acceptance.rs

scripts/
  verify-rig-free-model-runtime.sh
  verify-rig-boundary.sh
```

`model_runtime/native_loop.rs` is created only when ADR-0002 selects a native production loop. `model_runtime/test_loop.rs` is always created behind `model-loop-test` and is excluded from production composition.

---

## Normative contracts

### Existing v0.1 records remain authoritative

H3 does not create a second registry or a `model_invocations` table.

```text
ProviderProfile / ProviderRevision
ModelProfile / ModelRevision
ModelCostProfile
RoutingPolicy / RoutingDecision
ModelExecution
QualityEvaluation
```

H3 adds:

```text
ProviderRevision
└── ProviderAdapterBindingRevision

ModelExecution
├── ModelExecutionAttempt 1
├── ModelExecutionAttempt 2 (optional fallback)
├── ModelExecutionEvent sequence
└── ModelLoopCheckpoint (optional)
```

`ModelExecutionId` remains the logical invocation ID. H3 adds `ProviderAdapterBindingId`, `ProviderAdapterBindingRevisionId`, `ModelExecutionAttemptId`, `ModelExecutionEventId`, `ModelLoopCheckpointId` and `ModelReconciliationId`.

### Canonical domain messages

```rust
pub enum ModelMessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

pub enum ModelContentPart {
    Text { text: String },
    Json { value: serde_json::Value },
    InputReference { reference: RunReference, media_type: String },
}

pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub parts: Vec<ModelContentPart>,
    pub name: Option<String>,
    pub tool_call_id: Option<String>,
}
```

Rules:

- every message has at least one part;
- a text part is at most 4 MiB before later context budgeting;
- `Tool` requires a non-blank `tool_call_id`;
- other roles reject `tool_call_id`;
- `InputReference` requires an authorized reference and a selected model with the matching modality;
- the native adapter initially rejects `InputReference` before network dispatch.

### Output and tool-view contracts

```rust
pub enum ModelOutputContract {
    Text,
    Json {
        schema: serde_json::Value,
        strict: bool,
        semantic_validator: Option<String>,
    },
}

pub struct ModelToolDefinitionView {
    pub stable_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
```

A tool view contains no execution binding, credential reference, policy decision or authorization ticket.

### Invocation requirements and logical request

```rust
pub struct ModelInvocationRequirements {
    pub task_type: String,
    pub required_capabilities: std::collections::BTreeSet<ModelCapability>,
    pub preferred_capabilities: std::collections::BTreeSet<ModelCapability>,
    pub input_modalities: std::collections::BTreeSet<ModelModality>,
    pub minimum_quality_micros: u32,
    pub maximum_latency_ms: Option<u64>,
    pub maximum_cost: Option<ResourceAmount>,
    pub independence: ModelIndependenceRequirement,
    pub fallback_policy: ModelFallbackPolicy,
}

pub struct ModelInvocationRequest {
    pub execution_id: ModelExecutionId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub acting_agent_snapshot_id: AgentRuntimeSnapshotId,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolDefinitionView>,
    pub output_contract: ModelOutputContract,
    pub requirements: ModelInvocationRequirements,
    pub data_classification: DataClassification,
    pub context_snapshot_reference: Option<RunReference>,
    pub request_idempotency_key: String,
    pub requested_at: Timestamp,
}
```

`minimum_quality_micros` is `0..=1_000_000`. Empty task type, messages, idempotency key or JSON schema are invalid.

### Provider binding

```rust
pub struct ProviderAdapterBindingRevision {
    pub id: ProviderAdapterBindingRevisionId,
    pub binding_id: ProviderAdapterBindingId,
    pub revision: u32,
    pub provider_revision_id: ProviderRevisionId,
    pub adapter_kind: ProviderAdapterKind,
    pub adapter_revision: String,
    pub configuration: serde_json::Value,
    pub credential_reference: Option<SecretReference>,
    pub enabled: bool,
    pub content_hash: [u8; 32],
}
```

Initial adapter kinds are `openai-compatible` and `rig`. Configuration contains endpoint, timeout and safe header names, never secret values.

### Usage, results and errors

```rust
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_tokens: u64,
    pub provider_reported: bool,
}

pub struct ModelToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub normalized_arguments: CanonicalArguments,
}

pub struct ProviderInvocationResult {
    pub provider_request_id: Option<String>,
    pub output_text: String,
    pub structured_output: Option<serde_json::Value>,
    pub tool_calls: Vec<ModelToolCall>,
    pub usage: ModelUsage,
    pub finish_reason: ModelFinishReason,
    pub provider_metadata: serde_json::Value,
}

pub enum ProviderErrorKind {
    Transient,
    RateLimited,
    Unavailable,
    Timeout,
    Authentication,
    ContextTooLarge,
    UnsupportedCapability,
    InvalidRequest,
    InvalidResponse,
    InvalidStructuredOutput,
    SafetyRefusal,
    PolicyDenied,
    BudgetExceeded,
    Cancelled,
    UnknownCompletion,
}

pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub stable_code: String,
    pub safe_message: String,
    pub retry_after_ms: Option<u64>,
    pub provider_request_id: Option<String>,
    pub completion_may_have_occurred: bool,
}
```

Fallback matrix:

```text
Transient / RateLimited / Unavailable / Timeout
  fallback only when completion_may_have_occurred = false

InvalidStructuredOutput
  one bounded correction or fallback only when configured

ContextTooLarge
  no blind retry; use an already eligible larger-context fallback
  or return recontextualization-required

SafetyRefusal
  no automatic fallback

Authentication / UnsupportedCapability / InvalidRequest /
PolicyDenied / BudgetExceeded / Cancelled
  no fallback

UnknownCompletion or completion_may_have_occurred = true
  reconciliation required before another dispatch
```

### Canonical stream events

```rust
pub enum ModelStreamEvent {
    Started,
    TextDelta { index: u32, text: String },
    ToolCallDelta {
        index: u32,
        call_id_fragment: String,
        tool_name_fragment: String,
        arguments_fragment: String,
    },
    Usage { usage: ModelUsage },
    Completed { finish_reason: ModelFinishReason },
}
```

`Started` is first and unique; indexes are monotonic per stream; `Completed` is unique and terminal; assembled tool arguments must become valid canonical JSON.

### Application-owned provider request and ports

`ProviderInvocationRequest`, cancellation and provider ports belong to `vestrace-application`, not `vestrace-domain`.

```rust
pub struct ProviderInvocationRequest {
    pub execution_id: ModelExecutionId,
    pub attempt_id: ModelExecutionAttemptId,
    pub model_revision_id: ModelRevisionId,
    pub provider_revision_id: ProviderRevisionId,
    pub binding: ProviderAdapterBindingRevision,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolDefinitionView>,
    pub output_contract: ModelOutputContract,
    pub cancellation: tokio_util::sync::CancellationToken,
    pub deadline: Timestamp,
}

pub type ProviderEventStream = std::pin::Pin<
    Box<dyn futures_core::Stream<Item = Result<ModelStreamEvent, ProviderError>> + Send>
>;

#[async_trait::async_trait]
pub trait ModelProviderPort: Send + Sync {
    fn adapter_kind(&self) -> ProviderAdapterKind;
    async fn invoke(&self, request: ProviderInvocationRequest)
        -> Result<ProviderInvocationResult, ProviderError>;
    async fn stream(&self, request: ProviderInvocationRequest)
        -> Result<ProviderEventStream, ProviderError>;
    async fn embed(&self, request: EmbeddingRequest)
        -> Result<EmbeddingResult, ProviderError>;
}

#[async_trait::async_trait]
pub trait SemanticOutputValidatorPort: Send + Sync {
    async fn validate(
        &self,
        validator_id: &str,
        value: &serde_json::Value,
    ) -> Result<SemanticValidationResult, ApplicationError>;
}
```

### Infrastructure-private credential resolver

```rust
#[async_trait::async_trait]
pub(crate) trait ProviderAuthResolver: Send + Sync {
    async fn resolve(
        &self,
        reference: &SecretReference,
    ) -> Result<ResolvedProviderAuth, ProviderAdapterError>;
}
```

`ResolvedProviderAuth` has redacted `Debug`, is zeroized where supported and never crosses `ModelProviderPort`.

### Durable lifecycle

```rust
pub enum ModelExecutionStatus {
    Created,
    Authorized,
    Routed,
    Reserved,
    Dispatching,
    Streaming,
    Validating,
    Succeeded,
    Failed,
    Unknown,
    Cancelled,
}

pub enum ModelAttemptStatus {
    Prepared,
    Dispatching,
    Streaming,
    Succeeded,
    Failed,
    Unknown,
    Cancelled,
}
```

`Succeeded`, `Failed`, `Unknown` and `Cancelled` stop automatic execution. Explicit reconciliation may resolve `Unknown` to `Succeeded` or `FailedSafeToRetry` without silently dispatching again.

### Sans-I/O model loop

```rust
pub struct ModelLoopStart {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub initial_messages: Vec<ModelMessage>,
    pub advertised_tools: Vec<ModelToolDefinitionView>,
    pub output_contract: ModelOutputContract,
    pub maximum_model_turns: u32,
    pub journal_cursor: ResumeCursor,
}

pub enum ModelLoopInput {
    Start(ModelLoopStart),
    ModelCompleted(ProviderInvocationResult),
    ToolResults(Vec<ModelToolResultView>),
    ReconciliationCompleted(Vec<ModelToolResultView>),
}

pub enum ModelLoopEffect {
    InvokeModel {
        messages: Vec<ModelMessage>,
        tools: Vec<ModelToolDefinitionView>,
        output_contract: ModelOutputContract,
    },
    ToolCallsProposed(Vec<ModelToolCall>),
    WaitingForApproval(Vec<OperationFingerprint>),
    WaitingForReconciliation(Vec<RunReference>),
    Completed(ModelLoopOutput),
    Failed(ModelLoopFailure),
}

pub trait ModelLoopPort: Send {
    fn apply(&mut self, input: ModelLoopInput)
        -> Result<ModelLoopEffect, ApplicationError>;
    fn checkpoint(&self, cursor: ResumeCursor)
        -> Result<ModelLoopEngineCheckpoint, ApplicationError>;
}
```

The loop performs no provider or tool I/O. A tool proposal is never permission to execute.

---

### Task 1: Add model-runtime domain contracts

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/models/message.rs`
- Create: `crates/vestrace-domain/src/models/output.rs`
- Create: `crates/vestrace-domain/src/models/usage.rs`
- Create: `crates/vestrace-domain/src/models/error.rs`
- Create: `crates/vestrace-domain/src/models/runtime.rs`
- Modify: `crates/vestrace-domain/src/models/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Adds the H3 IDs and all domain-owned contracts above.
- Does not define `CancellationToken`, `ModelProviderPort` or adapter request types.

- [ ] **Step 1: Write failing message tests**

```rust
#[test]
fn tool_message_requires_call_id() {
    let result = ModelMessage::new(
        ModelMessageRole::Tool,
        vec![ModelContentPart::Text { text: "done".into() }],
        None,
        None,
    );
    assert!(result.is_err());
}

#[test]
fn user_message_rejects_call_id() {
    let result = ModelMessage::new(
        ModelMessageRole::User,
        vec![ModelContentPart::Text { text: "hello".into() }],
        None,
        Some("call-1".into()),
    );
    assert!(result.is_err());
}
```

Run `cargo test -p vestrace-domain models::message`; expect failure because the types do not exist.

- [ ] **Step 2: Implement messages and output contracts**

Compile JSON Schemas on construction. Reject schemas above 1 MiB, blank semantic-validator IDs, empty parts and names outside `[A-Za-z0-9_.-]` or above 128 bytes.

- [ ] **Step 3: Implement checked usage accounting**

```rust
impl ModelUsage {
    pub fn checked_add(self, other: Self) -> Result<Self, DomainError>;
    pub fn total_tokens(self) -> Result<u64, DomainError>;
}
```

Overflow is an error; accounting never saturates.

- [ ] **Step 4: Implement fallback classification**

```rust
pub fn fallback_disposition(
    error: &ProviderError,
    policy: &ModelFallbackPolicy,
) -> FallbackDisposition;
```

Test every matrix row, especially timeout with `completion_may_have_occurred = true`.

- [ ] **Step 5: Implement invocation and loop value objects**

Require non-empty messages/task/idempotency key, `maximum_model_turns` in `1..=64`, unique tool names and H2 `CanonicalArguments` for tool calls.

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p vestrace-domain models
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(model-runtime): add canonical invocation contracts"
```

---

### Task 2: Add immutable provider bindings and capability negotiation

**Files:**
- Create: `migrations/0028_provider_adapter_bindings_and_runtime_capabilities.sql`
- Modify: `crates/vestrace-domain/src/models/runtime.rs`
- Modify: `crates/vestrace-application/src/models/commands.rs`
- Modify: `crates/vestrace-application/src/models/ports.rs`
- Modify: `crates/vestrace-application/src/models/router.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/provider_repository.rs`
- Create: `tests/provider_binding_registry.rs`

**Interfaces:**
- Produces immutable binding revisions and a current binding pointer per provider revision.
- Produces `RuntimeCapabilityReport` and `negotiate_runtime_capabilities`.

- [ ] **Step 1: Write failing binding tests**

Test creation, immutable revision history, secret rejection, disabled-binding exclusion and tool-capability mismatch. Run the test and expect failure because migration `0028` is absent.

- [ ] **Step 2: Create schema**

Create `provider_adapter_bindings`, `provider_adapter_binding_revisions` and `provider_runtime_capability_reports`; add `current_adapter_binding_revision_id` to `provider_revisions`. Enforce workspace ownership, immutable revisions and content hashes.

- [ ] **Step 3: Implement binding commands**

```text
CreateProviderAdapterBinding
ReviseProviderAdapterBinding(expected_revision)
ActivateProviderAdapterBindingRevision
DisableProviderAdapterBinding
```

All writes use idempotency and optimistic concurrency. Activation verifies that the adapter kind is registered by trusted deployment configuration.

- [ ] **Step 4: Implement intersection-based negotiation**

```rust
pub fn negotiate_runtime_capabilities(
    model: &ModelRevision,
    binding: &ProviderAdapterBindingRevision,
    adapter: &RuntimeCapabilityReport,
) -> Result<EffectiveModelRuntimeCapabilities, CapabilityNegotiationError>;
```

Provider discovery and adapter configuration may reduce, never broaden, model-declared capabilities.

- [ ] **Step 5: Integrate router filters**

Apply adapter availability/capability checks before quality/cost ranking and record `adapter_disabled`, `adapter_unavailable` or `runtime_capability_missing`.

- [ ] **Step 6: Verify and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test provider_binding_registry --test model_routing
git add migrations/0028_provider_adapter_bindings_and_runtime_capabilities.sql crates tests/provider_binding_registry.rs
git commit -m "feat(model-runtime): register provider adapter bindings"
```

---

### Task 3: Define application ports and deterministic test runtime

**Files:**
- Create: `crates/vestrace-application/src/model_runtime/mod.rs`
- Create: `crates/vestrace-application/src/model_runtime/ports.rs`
- Create: `crates/vestrace-application/src/model_runtime/commands.rs`
- Create: `crates/vestrace-application/src/model_runtime/loop_port.rs`
- Create: `crates/vestrace-application/src/model_runtime/test_loop.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-model-test-support/Cargo.toml`
- Create: `crates/vestrace-model-test-support/src/lib.rs`
- Create: `crates/vestrace-model-test-support/src/scripts.rs`
- Create: `crates/vestrace-model-test-support/src/fixtures.rs`

**Interfaces:**
- Produces all H3 application ports and commands.
- Produces `ScriptedProvider`, `ScriptedProviderStream`, `FakeSemanticValidator` and `ScriptedModelLoop`.
- `ScriptedModelLoop` is compiled only with `model-loop-test` or crate-local tests.

- [ ] **Step 1: Write object-safety compile tests**

```rust
fn accepts_provider(_value: std::sync::Arc<dyn ModelProviderPort>) {}
fn accepts_loop(_value: Box<dyn ModelLoopPort>) {}
```

Run application tests and expect failure before ports exist.

- [ ] **Step 2: Define provider registry**

```rust
#[async_trait::async_trait]
pub trait ModelProviderRegistryPort: Send + Sync {
    async fn get(&self, kind: &ProviderAdapterKind)
        -> Result<std::sync::Arc<dyn ModelProviderPort>, ApplicationError>;
    async fn capability_report(&self, kind: &ProviderAdapterKind)
        -> Result<RuntimeCapabilityReport, ApplicationError>;
}
```

Model output cannot register adapters.

- [ ] **Step 3: Define exact repository transitions**

The persistence port provides typed methods for create, authorize, route, create attempt, mark dispatching, append attempt events, complete/fail/unknown attempt, complete logical execution, load and reconcile. Every logical transition uses expected execution revision; attempt event sequence is separate from H1 `RunVersion`.

- [ ] **Step 4: Implement deterministic fakes**

`ScriptedProvider` records canonical requests and supports blocking, streaming, embeddings, timeout before dispatch and lost response after dispatch. `ScriptedModelLoop` emits configured effects and versioned checkpoints without network or provider dependencies.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p vestrace-application model_runtime
cargo test -p vestrace-model-test-support
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-model-test-support
git commit -m "feat(model-runtime): define application runtime ports"
```

---

### Task 4: Implement stream assembly and structured-output validation

**Files:**
- Create: `crates/vestrace-application/src/model_runtime/stream.rs`
- Create: `crates/vestrace-application/src/model_runtime/structured_output.rs`
- Create: `crates/vestrace-application/tests/model_stream.rs`
- Create: `crates/vestrace-application/tests/structured_output.rs`

**Interfaces:**
- Produces `ModelStreamAssembler`, `AssembledProviderOutput`, `StructuredOutputValidator` and `OutputValidationReport`.

- [ ] **Step 1: Write stream-order tests**

Test deterministic text/tool assembly, missing `Started`, duplicate terminal, decreasing indexes, malformed tool JSON and deltas after completion.

- [ ] **Step 2: Implement fail-closed limits**

Limit each delta to 1 MiB, total text to the configured ceiling and tool argument buffers to 4 MiB per call. Protocol violations map to `InvalidResponse`.

- [ ] **Step 3: Write output-validation tests**

Cover JSON parse failure, schema violation with JSON Pointer paths, strict success and semantic-validator rejection.

- [ ] **Step 4: Implement validation order**

```text
assembled output
→ JSON extraction
→ JSON parse
→ stored schema validation
→ optional semantic validation
→ normalized result
```

Provider-declared strictness never skips local validation.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p vestrace-application --test model_stream --test structured_output
git add crates/vestrace-application
git commit -m "feat(model-runtime): validate streams and structured outputs"
```

---

### Task 5: Implement the native OpenAI-compatible adapter

**Files:**
- Create every file under `crates/vestrace-provider-openai-compatible/` listed in the locked structure
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `OpenAiCompatibleProvider: ModelProviderPort`.
- Supports `/chat/completions`, SSE streaming and `/embeddings` against a configured compatible endpoint.

- [ ] **Step 1: Write exact request-mapping tests**

Cover all message roles, strict JSON schema format, tool definitions, timeout/cancellation, omitted empty fields and redacted custom headers. Developer-role mapping is allowed only when the binding capability report declares support; otherwise fail before dispatch rather than silently rewriting role semantics.

- [ ] **Step 2: Implement validated configuration**

```rust
pub struct OpenAiCompatibleConfig {
    pub base_url: url::Url,
    pub request_timeout: std::time::Duration,
    pub connect_timeout: std::time::Duration,
    pub safe_headers: http::HeaderMap,
    pub credential_reference: Option<SecretReference>,
}
```

Reject URL credentials, fragments, query secrets and authentication headers in `safe_headers`. Plain HTTP is allowed only for deployment-policy-approved local/private endpoints.

- [ ] **Step 3: Implement private auth resolution**

Resolve immediately before request construction; apply to an ephemeral request builder; never expose secret material in configuration, errors or debug output.

- [ ] **Step 4: Implement completion and embeddings**

Normalize provider request ID, output, tool calls, usage and finish reason. Reject malformed successful responses. Map errors to stable safe `ProviderError` values.

- [ ] **Step 5: Implement SSE streaming**

Handle comments, blank separators, split UTF-8 frames, `[DONE]`, provider error frames and cancellation. Only canonical events leave the adapter.

- [ ] **Step 6: Publish the baseline capability report**

Report text, streaming, JSON output, tool proposals, embeddings, usage and cancellation. Report multimodal/audio/image/transcription/provider-specific reasoning controls unsupported.

- [ ] **Step 7: Verify and commit**

```bash
cargo test -p vestrace-provider-openai-compatible
cargo clippy -p vestrace-provider-openai-compatible --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/vestrace-provider-openai-compatible
git commit -m "feat(provider): add native OpenAI-compatible adapter"
```

---

### Task 6: Build the shared provider conformance suite

**Files:**
- Create: `crates/vestrace-model-test-support/src/server.rs`
- Create: `crates/vestrace-model-test-support/src/conformance.rs`
- Create: `tests/native_provider_conformance.rs`
- Create: `tests/rig_provider_conformance.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces a loopback scripted server and `ProviderConformanceSuite::run`.

- [ ] **Step 1: Implement the loopback server**

Bind `127.0.0.1:0`; return scripted JSON/SSE, delays, malformed frames, statuses and connection drops. Reject unexpected credentials and store only redacted request captures.

- [ ] **Step 2: Define exact cases**

```text
text completion
streaming equivalence
structured JSON success/failure
single and parallel tool proposals
usage normalization
embeddings
timeout before response
cancellation
rate limit and retry-after
context overflow
malformed success
lost response / unknown completion
secret redaction
provider unavailable
unsupported modality rejected before request
```

- [ ] **Step 3: Run native conformance**

Instantiate the native test factory and assert the complete suite passes.

- [ ] **Step 4: Add the feature-gated Rig test shell**

`rig_provider_conformance.rs` compiles only with `provider-rig`; Task 7 supplies the factory. The shared assertions are not weakened.

- [ ] **Step 5: Verify and commit**

```bash
cargo test --test native_provider_conformance
git add Cargo.toml Cargo.lock crates/vestrace-model-test-support tests
git commit -m "test(provider): add shared adapter conformance suite"
```

---

### Task 7: Implement the optional Rig provider adapter

**Files:**
- Create: `crates/vestrace-rig-adapter/Cargo.toml`
- Create: `crates/vestrace-rig-adapter/src/lib.rs`
- Create: `crates/vestrace-rig-adapter/src/provider.rs`
- Create: `crates/vestrace-rig-adapter/src/normalization.rs`
- Modify: `Cargo.toml`
- Modify: `tests/rig_provider_conformance.rs`
- Modify: `scripts/verify-rig-boundary.sh`

**Interfaces:**
- Produces `RigModelProviderAdapter: ModelProviderPort`.
- This task uses `rig-core`; it does not choose the model loop.

- [ ] **Step 1: Add exact feature-isolated dependencies**

Pin the version approved by H0/ADR-0002. `provider-rig` enables only the adapter and `rig-core`.

- [ ] **Step 2: Extend the boundary script**

Fail if `rig-core` or `rig-agent` appears in domain, application, native provider, HTTP, MCP or PostgreSQL infrastructure dependency trees. Permit only `vestrace-rig-adapter` and the historical spike crate.

- [ ] **Step 3: Implement anti-corruption translation**

Translate canonical requests/results/streams/embeddings at the adapter boundary. Provider-specific metadata remains bounded JSON and cannot broaden capabilities.

- [ ] **Step 4: Normalize every error**

No Rig error, message or tool type appears in a public signature. Add source-boundary tests for forbidden public `rig_` paths.

- [ ] **Step 5: Run the unchanged suite**

```bash
cargo test --features provider-rig --test rig_provider_conformance
bash scripts/verify-rig-boundary.sh
```

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/vestrace-rig-adapter tests/rig_provider_conformance.rs scripts/verify-rig-boundary.sh
git commit -m "feat(provider): add optional Rig adapter"
```

---

### Task 8: Implement governed routing, reservations and fallback

**Files:**
- Create: `crates/vestrace-application/src/model_runtime/orchestrator.rs`
- Create: `crates/vestrace-application/src/model_runtime/fallback.rs`
- Create: `crates/vestrace-application/tests/model_runtime_orchestrator.rs`
- Create: `crates/vestrace-application/tests/model_fallback.rs`
- Modify: `crates/vestrace-application/src/model_runtime/mod.rs`
- Modify: `crates/vestrace-application/src/models/router.rs`

**Interfaces:**
- Produces `ModelRuntimeOrchestrator::invoke` and `ModelFallbackController`.
- Consumes the v0.1 router, H2 guard/budgets, provider registry, validators and price revisions.

- [ ] **Step 1: Write the successful-order test**

Assert:

```text
create execution
→ route
→ fingerprint model.invoke
→ guard prepare and reserve
→ persist policy/routing refs
→ consume guarded action
→ create attempt
→ dispatch
→ validate
→ reconcile actual usage/cost
→ complete attempt and execution
```

The provider call count must remain zero before `consume`.

- [ ] **Step 2: Define fingerprint arguments**

Use only execution ID, model revision, binding revision, context/message/tool/output hashes, estimates and deadline. Never store raw prompts in policy rows.

- [ ] **Step 3: Implement reservation/reconciliation**

Reserve input/output tokens, invocation count and money microunits. Reconcile using the exact price revision selected at routing time. Missing provider usage uses conservative configured accounting with `provider_reported = false`.

- [ ] **Step 4: Test fallback matrix**

Cover pre-response unavailable/rate-limit fallback, ambiguous timeout to `Unknown`, one bounded structured-output correction, safety refusal stop, policy-ineligible fallback exclusion, fallback budget denial and non-reconsideration of privacy/capability rejects.

- [ ] **Step 5: Require attempt-specific authority**

Every fallback gets a new fingerprint, policy evaluation, reservation and ticket. Changing model/binding invalidates the previous authority.

- [ ] **Step 6: Enforce verifier independence**

Apply model revision/family/provider/prompt-lineage exclusions before ranking and persist reasons.

- [ ] **Step 7: Verify and commit**

```bash
cargo test -p vestrace-application --test model_runtime_orchestrator --test model_fallback
git add crates/vestrace-application
git commit -m "feat(model-runtime): govern routing and fallback"
```

---

### Task 9: Persist attempts, events, checkpoints and reconciliation

**Files:**
- Create: migrations `0029` and `0030`
- Create every `model_runtime/*_repository.rs` file listed in the locked structure
- Modify: PostgreSQL module wiring
- Create: `tests/model_execution_persistence.rs`
- Create: `tests/model_execution_unknown.rs`

**Interfaces:**
- Extends `model_executions`; creates attempts, canonical events, loop checkpoints and reconciliation records.

- [ ] **Step 1: Write lifecycle tests**

Verify contiguous attempt/event sequences, exact revision references, append-only events/checkpoints, workspace consistency, no direct `Unknown → dispatch`, and reconciliation without a new request.

- [ ] **Step 2: Create migration `0029` without duplicating v0.1 columns**

Reuse the existing v0.1 `routing_decision_id`, status/outcome/usage fields where semantically identical. Add only H3-specific columns:

```text
run_id, step_id, acting_agent_snapshot_id,
request_fingerprint, request_hash, output_contract_hash,
context_snapshot_reference JSONB,
policy_decision_id, policy_snapshot_id, budget_reservation_id,
runtime_status, runtime_revision, final_attempt_id,
normalized_result JSONB, normalized_failure JSONB, runtime_finished_at
```

Create `model_execution_attempts` and `model_execution_events`. Attempts store exact model/provider/binding/cost/guard references; events store canonical payloads, never raw SSE.

- [ ] **Step 3: Create migration `0030`**

Create append-only `model_loop_checkpoints` with engine/version/schema/opaque bytes/hash/cursor/sensitivity and `model_reconciliations` with evidence and decision.

- [ ] **Step 4: Implement atomic transitions**

Use optimistic `runtime_revision`. Attempt transition plus its canonical event commit in one scoped transaction. Ambiguous commit returns `operation_unknown`; repositories do not auto-retry.

- [ ] **Step 5: Implement reconciliation**

```rust
async fn record_reconciliation(
    execution_id: ModelExecutionId,
    expected_revision: u64,
    outcome: ReconciliationOutcome,
) -> Result<ModelExecutionRecord, ApplicationError>;
```

`Succeeded` requires verified result/evidence; `FailedSafeToRetry` permits only a later explicit command; `StillUnknown` preserves the fence.

- [ ] **Step 6: Verify and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test model_execution_persistence --test model_execution_unknown
git add migrations/0029_model_execution_attempts_and_events.sql migrations/0030_model_loop_checkpoints_and_reconciliation.sql crates tests
git commit -m "feat(model-runtime): persist attempts events and checkpoints"
```

---

### Task 10: Integrate restart-safe model work with H1

**Files:**
- Create: migration `0031`
- Modify: H1 Run work/event contracts
- Create: `crates/vestrace-application/src/model_runtime/worker.rs`
- Create: `crates/vestrace-application/tests/model_worker.rs`
- Modify: H1 worker registry
- Create: `tests/model_runtime_rls.rs`
- Create: `tests/model_runtime_restart.rs`

**Interfaces:**
- Adds `WorkItemKind::InvokeModel`, matching payload and Run events for start/completion/failure/unknown.
- Produces `ModelRuntimeWorkHandler`.

- [ ] **Step 1: Extend Run contracts**

```rust
ModelExecutionStarted { execution_id: ModelExecutionId },
ModelExecutionCompleted { execution_id: ModelExecutionId, result_reference: RunReference },
ModelExecutionFailed { execution_id: ModelExecutionId, failure_code: String },
ModelExecutionUnknown { execution_id: ModelExecutionId, reconciliation_id: ModelReconciliationId },
```

Each is the sole event of one H1 logical mutation; attempt/stream events remain in H3 tables.

- [ ] **Step 2: Test restart before dispatch**

Crash after `ModelExecutionStarted` but before ticket consumption; restart and assert no duplicate attempt/reservation.

- [ ] **Step 3: Test lost response**

Drop the connection after the server accepted the request. On restart, send no second request; after deadline mark the attempt `Unknown`, transition the Run to durable dependency/reconciliation waiting, and emit `ModelExecutionUnknown`.

- [ ] **Step 4: Implement worker order**

```text
lease work
→ acquire Run lease
→ load expected versions
→ create/restore guarded attempt
→ mark dispatching before network
→ invoke with deadline/cancellation
→ persist canonical result
→ reconcile actual budget
→ emit one H1 result mutation
→ complete work and release lease
```

Lease generation fences stale workers.

- [ ] **Step 5: Create migration `0031`**

Force RLS, validate Run/step/model/provider/binding workspace ownership, extend work-kind checks and add indexes for active attempts/events/executions/reconciliations.

- [ ] **Step 6: Verify and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test -p vestrace-application --test model_worker
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test model_runtime_rls --test model_runtime_restart
git add migrations/0031_model_runtime_rls_indexes_and_run_bindings.sql crates tests
git commit -m "feat(model-runtime): add restart-safe model worker"
```

---

### Task 11: Implement the ADR-0002-selected production model loop

**Always:**
- Create: `crates/vestrace-application/tests/model_loop_conformance.rs`
- Modify: loop port/module/factory wiring

**When `Accepted` or `AcceptedWithRestrictions`:**
- Create/modify: `vestrace-rig-adapter/src/loop_adapter.rs`, `checkpoint.rs`, `lib.rs`, `Cargo.toml`

**When `Rejected`:**
- Create: `crates/vestrace-application/src/model_runtime/native_loop.rs`

**Interfaces:**
- Produces one ADR-selected default `ModelLoopPort` factory.
- Any compiled implementation passes the same conformance suite.

- [ ] **Step 1: Add an executable ADR gate**

Parse exactly one ADR outcome. Missing/ambiguous content fails. The production default factory must match the ADR. Test-only `model-loop-test` never satisfies the production gate.

- [ ] **Step 2: Write shared conformance**

Test initial model request, tool proposal without execution, direct completion, turn limit, fail-closed invalid tool resolution, versioned checkpoint/hash/cursor, safe incompatible-checkpoint handling, durable approval/reconciliation waits and absence of hidden-CoT fields.

- [ ] **Step 3A: Implement accepted Rig loop**

Use only H0-approved APIs/restrictions. Hand-drive the state machine; never use direct Rig tool execution, conversation memory, vector stores or workflow runtime. Store Rig state only as opaque engine checkpoint bytes.

- [ ] **Step 3B: Implement rejected native loop**

Implement:

```text
NeedModel
→ AwaitingModel
→ ToolCallsProposed | Completed
→ AwaitingToolResults | WaitingForApproval | WaitingForReconciliation
→ NeedModel | Completed | Failed
```

It performs no I/O and serializes versioned Vestrace state.

- [ ] **Step 4: Make feature combinations CI-safe**

Available implementations may compile together under `all-features`; the ADR-selected default factory remains singular. Deployment configuration cannot select an implementation forbidden by ADR. `model-loop-test` is accepted only by tests and cannot be enabled in production binary profiles.

- [ ] **Step 5: Verify and commit**

Run the selected loop test plus `cargo test --workspace --all-features`. Commit only files applicable to the ADR outcome and the shared tests.

---

### Task 12: Add Rig-free CI and H3 acceptance

**Files:**
- Create: `scripts/verify-rig-free-model-runtime.sh`
- Modify: `scripts/verify-rig-boundary.sh`
- Modify: `.github/workflows/ci.yml`
- Create: `tests/model_runtime_rig_free.rs`
- Create: `tests/h3_acceptance.rs`
- Modify schema snapshots used by the repository

**Interfaces:**
- Produces mandatory native-provider/Rig-free and optional Rig jobs.
- Produces the H3 exit-gate scenario.

- [ ] **Step 1: Create the Rig-free script**

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo test --workspace \
  --exclude vestrace-rig-spike \
  --exclude vestrace-rig-adapter \
  --no-default-features \
  --features provider-openai-compatible,model-loop-test

for package in vestrace-domain vestrace-application vestrace-infrastructure \
  vestrace-provider-openai-compatible vestrace-http vestrace-mcp; do
  if cargo tree -p "$package" | grep -E '(^| )rig-(agent|core) '; then
    echo "Rig leaked into native provider path: $package" >&2
    exit 1
  fi
done
```

The scripted loop proves the provider/runtime path without assuming ADR selected a native production loop.

- [ ] **Step 2: Add CI jobs**

```text
native-provider-rig-free
all-features
provider-conformance-native
provider-conformance-rig
model-loop-selected
postgres-model-runtime
```

All use local fixtures and no provider credentials.

- [ ] **Step 3: Write Rig-free integration**

Run text completion and strict JSON through native adapter, router, H2 guard/budgets and H3 persistence with Rig excluded. Assert no Rig dependency appears.

- [ ] **Step 4: Write H3 acceptance**

Use three registered models:

```text
A: native adapter, temporarily unavailable
B: Rig provider adapter, eligible fallback
C: higher-quality remote model, policy-ineligible for Restricted data
```

Public data: A fails before completion and B succeeds with new authority/reservation. Restricted data: C is excluded and a local native-compatible model succeeds. Also simulate a lost response and assert `Unknown` without fallback. Verify routing explanations, consumed ticket ordering, budget reconciliation, local schema validation, durable reload, Run-event references and secret-free diagnostics.

- [ ] **Step 5: Run all gates**

```bash
bash scripts/verify-rig-free-model-runtime.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h3_acceptance --test model_runtime_restart
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(model-runtime): add H3 acceptance and Rig-free gates"
```

---

## H3 completion definition

H3 is complete only when the implementation demonstrates:

```text
canonical request
→ policy/data eligibility
→ quality-first routing
→ exact reservation
→ consumed authorization ticket
→ selected provider adapter
→ canonical blocking/streaming result
→ local output validation
→ usage/cost reconciliation
→ controlled fallback or fenced Unknown
→ restart-safe Run continuation
```

Required invariants:

1. Native OpenAI-compatible execution passes the complete conformance suite without Rig.
2. The optional Rig provider adapter passes the same suite.
3. Routing selects model/provider/binding revisions, never a Rust library directly.
4. Dispatch cannot occur before H2 guard consumption.
5. Fallback cannot reuse another attempt's ticket, reservation or approval binding.
6. `Unknown` never creates an automatic replacement request.
7. Safety refusal is not blindly bypassed.
8. Structured output is validated locally.
9. Stream deltas never become authoritative Run state.
10. Every success stores exact routing, policy, budget, provider, price, usage and validation provenance.
11. Secrets are absent from canonical DTOs, events, checkpoints, errors and logs.
12. The production loop matches ADR-0002 and remains replaceable through `ModelLoopPort`.
13. H1 replay performs no provider I/O.
14. Tests require no public network or paid provider.
15. H4 can consume `ToolCallsProposed` without changing provider contracts.

## Explicit non-goals

H3 does not implement real tool execution, sandboxing, artifact ingestion, multimodal processing, conversations, triggers, OAuth connections, the general Credential Broker, planning, delegation, browser automation, provider-specific media APIs, automatic training or a managed provider fleet.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create the branch, change Cargo features, execute H0-RIG, produce ADR-0002, run fixture servers, create migrations `0028`–`0031`, modify CI or write adapter/loop code.
