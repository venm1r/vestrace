# Vestrace H3 Model Runtime and Provider Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, run provider tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement Vestrace-owned model invocation contracts, production native OpenAI-compatible execution, optional Rig provider integration, canonical streaming and structured outputs, policy- and budget-governed routing/fallback, durable invocation records and the bounded model-loop implementation selected by ADR-0002.

**Architecture:** Extend the v0.1 provider/model registry, router and `model_executions` records rather than creating a parallel registry. A logical `ModelExecution` owns one normalized request and may contain several ordered `ModelExecutionAttempt` records when controlled fallback is allowed. Every provider adapter implements the same Vestrace-owned `ModelProviderPort`; provider-specific types remain inside adapter crates. The native OpenAI-compatible adapter is a complete narrow production path and remains available when Rig is disabled. The bounded model loop is exposed through a sans-I/O `ModelLoopPort`; its production implementation is selected only by the H0-RIG outcome ADR.

**Tech Stack:** Existing Vestrace v0.1, H1 and H2 Rust workspace; Rust Edition 2024; Tokio; Serde; Schemars; SQLx; PostgreSQL 17; Reqwest; Futures; Tokio Util; SHA-256; JSON Schema validation; deterministic local HTTP fixtures; optional exact-pinned `rig-core` and `rig-agent` selected by ADR-0002.

## Global Constraints

- Complete all five v0.1 plans, H1 Durable Run Core, H2 Policy/Approval/Budget Core and the H0-RIG spike before implementing the production loop task.
- ADR-0001 remains authoritative: `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`.
- ADR-0002 produced by H0-RIG must exist before Task 11 begins and must contain exactly `Accepted`, `AcceptedWithRestrictions` or `Rejected`.
- Vestrace owns all domain types, application ports, persistence schemas, durable events, public schemas and checkpoints.
- No Rig type may appear outside `vestrace-rig-adapter` or in a Vestrace-owned persisted/public contract.
- `vestrace-provider-openai-compatible` is a production adapter, not a fallback stub.
- Native support is deliberately limited to text messages, JSON structured output, tool-call proposals, streaming text/tool deltas and embeddings. Unsupported modalities fail before network dispatch.
- Provider adapters never select models, authorize data transfer, approve actions, reserve budgets or execute tools.
- Every provider dispatch requires a consumed H2 `GuardedAction` for action `model.invoke` and an active budget reservation when the operation has non-zero estimated usage.
- Provider credentials are resolved only inside infrastructure through an opaque secret reference. Credential material never enters prompts, domain values, ordinary logs, events or public DTOs.
- The model sees tool definitions and may propose tool calls; H3 never executes real tools. H4 owns tool authorization and execution.
- A provider response, stream chunk, tool result or refusal is untrusted input and cannot modify policy, capabilities, model selection or Run authority.
- Logical `ModelExecution` and each attempt use stable idempotency keys. A fallback is a new attempt under the same logical execution, not a duplicate logical command.
- `Unknown` completion is not retried or failed over until reconciliation establishes that a new attempt is safe.
- Safety refusal is not bypassed by automatic fallback. Any later alternate-model attempt requires an explicit typed policy decision.
- Authentication failure, policy denial, hard budget exhaustion and malformed local configuration are non-fallback failures.
- Streaming events are provisional transport observations. Only normalized durable invocation events and final assembled output are authoritative.
- Structured output must pass both JSON parsing and the exact stored output schema. Semantic validators are explicit application ports and cannot be replaced by provider claims.
- Hidden chain-of-thought is never requested as a persistence requirement and is not stored. Provider-specific reasoning summaries are treated as ordinary untrusted output only when explicitly requested.
- Run mutations continue to follow H1: each logical Run mutation increments `RunVersion` once and appends exactly one `RunEvent`. Stream deltas and provider attempt telemetry do not change `RunVersion`.
- Existing migrations `0014_provider_and_model_registry.sql` and `0015_routing_executions_and_evaluations.sql` are extended only through new forward migrations.
- H3 migrations begin at `0028` and are created once.
- CI provider tests use deterministic local servers and require no public provider key or internet access.
- Mandatory Rig-free feature path: `provider-openai-compatible` without `provider-rig` or a Rig loop implementation.
- Future implementation branch: `feat/h3-model-runtime-provider-adapters`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  models/runtime.rs
  models/message.rs
  models/output.rs
  models/usage.rs
  models/error.rs
  models/mod.rs
  run/event.rs
  run/work.rs
  run/mod.rs
  id.rs

crates/vestrace-application/src/
  model_runtime/mod.rs
  model_runtime/ports.rs
  model_runtime/commands.rs
  model_runtime/orchestrator.rs
  model_runtime/fallback.rs
  model_runtime/stream.rs
  model_runtime/structured_output.rs
  model_runtime/worker.rs
  model_runtime/loop_port.rs
  model_runtime/native_loop.rs
  models/router.rs
  models/ports.rs
  lib.rs

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
  model_runtime/mod.rs
  model_runtime/execution_repository.rs
  model_runtime/attempt_repository.rs
  model_runtime/event_repository.rs
  model_runtime/checkpoint_repository.rs
  model_runtime/reconciliation_repository.rs
  mod.rs

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

---

## Normative contracts

### Existing v0.1 entities remain authoritative

H3 does not create a second model registry or a parallel `model_invocations` table.

```text
ProviderProfile / ProviderRevision
ModelProfile / ModelRevision
ModelCostProfile
RoutingPolicy / RoutingDecision
ModelExecution
QualityEvaluation
```

H3 extends these through new revisions and relations:

```text
ProviderRevision
└── ProviderAdapterBinding

ModelExecution
├── ModelExecutionAttempt 1
├── ModelExecutionAttempt 2 (optional fallback)
├── ModelExecutionEvent stream
└── ModelLoopCheckpoint (optional)
```

`ModelExecutionId` remains the stable logical invocation ID. H3 adds `ModelExecutionAttemptId`, `ModelExecutionEventId`, `ModelLoopCheckpointId`, `ProviderAdapterBindingId` and `ModelReconciliationId`.

### Canonical messages and content

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelMessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ModelContentPart {
    Text { text: String },
    Json { value: serde_json::Value },
    InputReference { reference: RunReference, media_type: String },
}

#[derive(Clone, Debug, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub parts: Vec<ModelContentPart>,
    pub name: Option<String>,
    pub tool_call_id: Option<String>,
}
```

Rules:

- a message has at least one part;
- each text part is at most 4 MiB before later context budgeting;
- `Tool` requires a non-blank `tool_call_id`;
- non-tool messages reject `tool_call_id`;
- `InputReference` is allowed only when the selected model declares the matching modality and an upstream context/artifact layer has authorized the reference;
- the native adapter initially accepts only text and JSON parts and rejects unsupported references before dispatch.

### Output contract

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ModelOutputContract {
    Text,
    Json {
        schema: serde_json::Value,
        strict: bool,
        semantic_validator: Option<String>,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelToolDefinitionView {
    pub stable_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
```

Tool definitions are prompt views only. They contain no execution binding, credential reference or authorization ticket.

### Invocation requirements and request

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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

`minimum_quality_micros` is an integer from `0` to `1_000_000`. Empty task type, messages, idempotency key or output schema are invalid.

### Provider adapter binding

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ProviderAdapterBinding {
    pub id: ProviderAdapterBindingId,
    pub provider_revision_id: ProviderRevisionId,
    pub adapter_kind: ProviderAdapterKind,
    pub adapter_revision: String,
    pub configuration: serde_json::Value,
    pub credential_reference: Option<SecretReference>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct ProviderAdapterKind(String);
```

Initial stable values:

```text
openai-compatible
rig
```

Configuration may contain endpoint, timeout and safe custom header names, but never secret values. Binding revisions are immutable; activation changes the current binding pointer.

### Provider request and result

```rust
#[derive(Clone, Debug)]
pub struct ProviderInvocationRequest {
    pub execution_id: ModelExecutionId,
    pub attempt_id: ModelExecutionAttemptId,
    pub model_revision_id: ModelRevisionId,
    pub provider_revision_id: ProviderRevisionId,
    pub binding: ProviderAdapterBinding,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolDefinitionView>,
    pub output_contract: ModelOutputContract,
    pub cancellation: CancellationToken,
    pub deadline: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_tokens: u64,
    pub provider_reported: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub normalized_arguments: CanonicalArguments,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ProviderInvocationResult {
    pub provider_request_id: Option<String>,
    pub output_text: String,
    pub structured_output: Option<serde_json::Value>,
    pub tool_calls: Vec<ModelToolCall>,
    pub usage: ModelUsage,
    pub finish_reason: ModelFinishReason,
    pub provider_metadata: serde_json::Value,
}
```

Provider metadata is size-limited, secret-scanned and never interpreted as authority.

### Canonical stream events

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
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

A stream is valid only when:

- `Started` is first and occurs once;
- indexes are monotonic within each text/tool stream;
- a single terminal `Completed` exists;
- tool-call fragments assemble into valid canonical JSON arguments;
- final usage is non-decreasing and reconciled against the terminal provider result.

### Provider errors and fallback classification

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
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

Fallback eligibility:

```text
Transient, RateLimited, Unavailable, Timeout
  allowed only when completion_may_have_occurred = false
  and request fallback policy permits another eligible model

InvalidStructuredOutput
  bounded correction or fallback only when configured

ContextTooLarge
  no blind retry; return recontextualization-required unless an already-routed
  compatible fallback has a sufficient context limit

SafetyRefusal
  no automatic fallback

Authentication, UnsupportedCapability, InvalidRequest,
PolicyDenied, BudgetExceeded, Cancelled
  no fallback

UnknownCompletion or completion_may_have_occurred = true
  no fallback before reconciliation
```

### Provider and semantic validation ports

```rust
pub type ProviderEventStream = std::pin::Pin<
    Box<dyn futures_core::Stream<Item = Result<ModelStreamEvent, ProviderError>> + Send>
>;

#[async_trait::async_trait]
pub trait ModelProviderPort: Send + Sync {
    fn adapter_kind(&self) -> ProviderAdapterKind;

    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
    ) -> Result<ProviderInvocationResult, ProviderError>;

    async fn stream(
        &self,
        request: ProviderInvocationRequest,
    ) -> Result<ProviderEventStream, ProviderError>;

    async fn embed(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResult, ProviderError>;
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

### Provider credentials remain infrastructure-private

```rust
#[async_trait::async_trait]
pub(crate) trait ProviderAuthResolver: Send + Sync {
    async fn resolve(
        &self,
        reference: &SecretReference,
    ) -> Result<ResolvedProviderAuth, ProviderAdapterError>;
}
```

`ResolvedProviderAuth` is defined in adapter infrastructure, implements redacted `Debug`, is zeroized where supported and is never returned through `ModelProviderPort`.

### Durable execution lifecycle

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
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

Terminal logical statuses are `Succeeded`, `Failed`, `Unknown` and `Cancelled`. `Unknown` is terminal for automatic execution but may transition through an explicit reconciliation command to `Succeeded` or `Failed` without dispatching a new request.

### Model-loop port

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelLoopStart {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub initial_messages: Vec<ModelMessage>,
    pub advertised_tools: Vec<ModelToolDefinitionView>,
    pub output_contract: ModelOutputContract,
    pub maximum_model_turns: u32,
    pub journal_cursor: ResumeCursor,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub enum ModelLoopInput {
    Start(ModelLoopStart),
    ModelCompleted(ProviderInvocationResult),
    ToolResults(Vec<ModelToolResultView>),
    ReconciliationCompleted(Vec<ModelToolResultView>),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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
    fn apply(&mut self, input: ModelLoopInput) -> Result<ModelLoopEffect, ApplicationError>;
    fn checkpoint(&self, cursor: ResumeCursor) -> Result<ModelLoopEngineCheckpoint, ApplicationError>;
}
```

The loop performs no provider or tool I/O. `ToolCallsProposed` is not permission to execute.

---

### Task 1: Add model-runtime identifiers, canonical messages, outputs, usage and errors

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/models/message.rs`
- Create: `crates/vestrace-domain/src/models/output.rs`
- Create: `crates/vestrace-domain/src/models/usage.rs`
- Create: `crates/vestrace-domain/src/models/error.rs`
- Create: `crates/vestrace-domain/src/models/runtime.rs`
- Modify: `crates/vestrace-domain/src/models/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline unit and property tests

**Interfaces:**
- Adds IDs `ProviderAdapterBindingId`, `ModelExecutionAttemptId`, `ModelExecutionEventId`, `ModelLoopCheckpointId` and `ModelReconciliationId`.
- Produces every canonical domain contract listed above except application ports.

- [ ] **Step 1: Write failing canonical-message tests**

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
fn non_tool_message_rejects_call_id() {
    let result = ModelMessage::new(
        ModelMessageRole::User,
        vec![ModelContentPart::Text { text: "hello".into() }],
        None,
        Some("call-1".into()),
    );
    assert!(result.is_err());
}
```

Run:

```bash
cargo test -p vestrace-domain models::message
```

Expected: FAIL because the runtime message types do not exist.

- [ ] **Step 2: Implement messages and output contracts**

Compile JSON Schemas when `ModelOutputContract::Json` is constructed. Reject schemas larger than 1 MiB, blank semantic-validator IDs and message/tool names outside `[A-Za-z0-9_.-]` or longer than 128 bytes.

- [ ] **Step 3: Implement usage with checked arithmetic**

```rust
impl ModelUsage {
    pub fn checked_add(self, other: Self) -> Result<Self, DomainError>;
    pub fn total_tokens(self) -> Result<u64, DomainError>;
}
```

Overflow returns `DomainError::InvalidArgument`; no saturating accounting is allowed.

- [ ] **Step 4: Implement provider errors and fallback classifier**

```rust
pub fn fallback_disposition(
    error: &ProviderError,
    policy: &ModelFallbackPolicy,
) -> FallbackDisposition;
```

Unit tests cover every matrix row in the normative contract, including `Timeout` with `completion_may_have_occurred = true` mapping to reconciliation rather than fallback.

- [ ] **Step 5: Implement invocation and loop value objects**

Validate non-empty messages, task type, stable idempotency key, maximum turns from `1..=64`, and unique tool names. Tool-call arguments use H2 `CanonicalArguments`.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain models
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(model-runtime): add canonical invocation contracts"
```

---

### Task 2: Add immutable provider adapter bindings and runtime capability negotiation

**Files:**
- Create: `migrations/0028_provider_adapter_bindings_and_runtime_capabilities.sql`
- Modify: `crates/vestrace-domain/src/models/runtime.rs`
- Modify: `crates/vestrace-application/src/models/commands.rs`
- Modify: `crates/vestrace-application/src/models/ports.rs`
- Modify: `crates/vestrace-application/src/models/router.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/provider_repository.rs`
- Create: `tests/provider_binding_registry.rs`

**Interfaces:**
- Produces immutable `ProviderAdapterBinding` revisions and a current binding pointer per provider revision.
- Produces `RuntimeCapabilityReport` and `negotiate_runtime_capabilities`.

- [ ] **Step 1: Write failing binding tests**

Test that:

1. a provider revision can bind to `openai-compatible`;
2. revising the endpoint creates a new binding and preserves the old one;
3. configuration containing `api_key`, `authorization` or a known secret value is rejected;
4. a disabled binding is never router-eligible;
5. a model requiring tool calling is rejected when the binding reports no tool-call support.

Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test provider_binding_registry
```

Expected: FAIL because migration `0028` and binding repositories do not exist.

- [ ] **Step 2: Create the binding schema**

Create:

```text
provider_adapter_bindings
provider_adapter_binding_revisions
provider_runtime_capability_reports
```

Add `current_adapter_binding_revision_id` to `provider_revisions` through this migration. Enforce same-workspace ownership and immutable revision rows. Store configuration and capability reports as validated JSONB with SHA-256 hashes.

- [ ] **Step 3: Implement binding services**

Commands:

```rust
CreateProviderAdapterBinding
ReviseProviderAdapterBinding { expected_revision: u32 }
ActivateProviderAdapterBindingRevision
DisableProviderAdapterBinding
```

All writes require idempotency and expected revision where applicable. Activation checks adapter registration through `ProviderAdapterRegistryPort` before changing the current pointer.

- [ ] **Step 4: Implement capability negotiation**

```rust
pub fn negotiate_runtime_capabilities(
    model: &ModelRevision,
    binding: &ProviderAdapterBindingRevision,
    adapter: &RuntimeCapabilityReport,
) -> Result<EffectiveModelRuntimeCapabilities, CapabilityNegotiationError>;
```

Effective capabilities are an intersection. Configuration or provider discovery can reduce, never broaden, the stored model declaration.

- [ ] **Step 5: Integrate router hard filters**

Insert binding availability and effective runtime capabilities after operational availability and policy eligibility, before quality and cost ranking. Record stable exclusion codes `adapter_disabled`, `adapter_unavailable` and `runtime_capability_missing`.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test provider_binding_registry --test model_routing
git add migrations/0028_provider_adapter_bindings_and_runtime_capabilities.sql crates tests/provider_binding_registry.rs
git commit -m "feat(model-runtime): register provider adapter bindings"
```

---

### Task 3: Define application ports, commands and deterministic test adapters

**Files:**
- Create: `crates/vestrace-application/src/model_runtime/mod.rs`
- Create: `crates/vestrace-application/src/model_runtime/ports.rs`
- Create: `crates/vestrace-application/src/model_runtime/commands.rs`
- Create: `crates/vestrace-application/src/model_runtime/loop_port.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-model-test-support/Cargo.toml`
- Create: `crates/vestrace-model-test-support/src/lib.rs`
- Create: `crates/vestrace-model-test-support/src/scripts.rs`
- Create: `crates/vestrace-model-test-support/src/fixtures.rs`

**Interfaces:**
- Produces `ModelProviderPort`, `ModelProviderRegistryPort`, `ModelExecutionRepositoryPort`, `ModelLoopCheckpointPort`, `ModelReconciliationPort`, `SemanticOutputValidatorPort` and `ModelLoopPort`.
- Produces commands `InvokeModel`, `CancelModelExecution`, `ReconcileModelExecution` and `ContinueModelLoop`.
- Produces `ScriptedProvider`, `ScriptedProviderStream`, `FakeSemanticValidator` and deterministic model fixtures.

- [ ] **Step 1: Write object-safety compile tests**

```rust
fn accepts_provider(_provider: std::sync::Arc<dyn ModelProviderPort>) {}
fn accepts_loop(_loop_engine: Box<dyn ModelLoopPort>) {}
```

Run:

```bash
cargo test -p vestrace-application model_runtime::ports
```

Expected: FAIL because the ports do not exist.

- [ ] **Step 2: Define provider registry and persistence ports**

```rust
#[async_trait::async_trait]
pub trait ModelProviderRegistryPort: Send + Sync {
    async fn get(
        &self,
        kind: &ProviderAdapterKind,
    ) -> Result<std::sync::Arc<dyn ModelProviderPort>, ApplicationError>;

    async fn capability_report(
        &self,
        kind: &ProviderAdapterKind,
    ) -> Result<RuntimeCapabilityReport, ApplicationError>;
}
```

The registry is configured by trusted deployment code; model output cannot register adapters.

- [ ] **Step 3: Define exact persistence operations**

The repository port includes:

```rust
async fn create_execution(...);
async fn mark_authorized(...);
async fn attach_routing_decision(...);
async fn create_attempt(...);
async fn mark_attempt_dispatching(...);
async fn append_attempt_events(...);
async fn complete_attempt(...);
async fn mark_attempt_unknown(...);
async fn complete_execution(...);
async fn load_execution(...);
```

Every transition carries expected logical execution revision. Attempt event append uses a separate monotonic attempt-event sequence and does not alter H1 `RunVersion`.

- [ ] **Step 4: Implement deterministic scripted providers**

`ScriptedProvider` consumes queued results and records normalized requests. It must support blocking, streaming, embeddings, injected timeout before dispatch and injected lost response after dispatch. No fixture opens a network socket in this task.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application model_runtime
cargo test -p vestrace-model-test-support
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-model-test-support
git commit -m "feat(model-runtime): define application runtime ports"
```

---

### Task 4: Implement canonical stream assembly and structured-output validation

**Files:**
- Create: `crates/vestrace-application/src/model_runtime/stream.rs`
- Create: `crates/vestrace-application/src/model_runtime/structured_output.rs`
- Create: `crates/vestrace-application/tests/model_stream.rs`
- Create: `crates/vestrace-application/tests/structured_output.rs`

**Interfaces:**
- Produces `ModelStreamAssembler`, `AssembledProviderOutput`, `StructuredOutputValidator` and `OutputValidationReport`.

- [ ] **Step 1: Write failing stream-order tests**

```rust
#[test]
fn text_and_tool_deltas_assemble_deterministically() {
    let events = vec![
        ModelStreamEvent::Started,
        ModelStreamEvent::TextDelta { index: 0, text: "Result: ".into() },
        ModelStreamEvent::ToolCallDelta {
            index: 0,
            call_id_fragment: "call-".into(),
            tool_name_fragment: "look".into(),
            arguments_fragment: "{\"q\":".into(),
        },
        ModelStreamEvent::ToolCallDelta {
            index: 0,
            call_id_fragment: "1".into(),
            tool_name_fragment: "up".into(),
            arguments_fragment: "\"rust\"}".into(),
        },
        ModelStreamEvent::Completed { finish_reason: ModelFinishReason::ToolCalls },
    ];

    let output = ModelStreamAssembler::assemble(events).unwrap();
    assert_eq!(output.text, "Result: ");
    assert_eq!(output.tool_calls[0].call_id, "call-1");
    assert_eq!(output.tool_calls[0].tool_name, "lookup");
    assert_eq!(output.tool_calls[0].normalized_arguments.value()["q"], "rust");
}
```

Add tests for missing `Started`, duplicate terminal events, decreasing indexes, malformed JSON fragments and text after `Completed`.

- [ ] **Step 2: Implement fail-closed stream assembly**

Limit individual deltas to 1 MiB, total assembled text to the configured output ceiling and tool argument buffers to 4 MiB per call. Return `ProviderErrorKind::InvalidResponse` on protocol violations.

- [ ] **Step 3: Write structured-output tests**

Test JSON parse failure, schema violation with JSON Pointer paths, successful strict validation and semantic-validator rejection. Schema errors return stable machine-readable codes and no provider-specific type.

- [ ] **Step 4: Implement validation order**

```text
assembled provider output
→ JSON extraction
→ JSON parse
→ stored JSON Schema validation
→ optional semantic validator
→ normalized result
```

A provider's `strict=true` claim never skips local validation.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test model_stream --test structured_output
git add crates/vestrace-application
git commit -m "feat(model-runtime): validate streams and structured outputs"
```

---

### Task 5: Implement the production native OpenAI-compatible adapter

**Files:**
- Create: `crates/vestrace-provider-openai-compatible/Cargo.toml`
- Create: `crates/vestrace-provider-openai-compatible/src/lib.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/adapter.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/auth.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/config.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/request.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/response.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/stream.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/embeddings.rs`
- Create: `crates/vestrace-provider-openai-compatible/src/error.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `OpenAiCompatibleProvider` implementing `ModelProviderPort`.
- Supports `/chat/completions`, streaming SSE and `/embeddings` against a configured `/v1`-compatible endpoint.

- [ ] **Step 1: Write request-mapping unit tests**

Assert exact mapping for:

- system/developer/user/assistant/tool messages;
- strict JSON schema response format;
- tool definitions and `tool_choice = auto` only when tools exist;
- timeout and cancellation;
- omission of unsupported or empty optional fields;
- deterministic custom headers without secret values in `Debug`.

- [ ] **Step 2: Implement validated adapter configuration**

```rust
pub struct OpenAiCompatibleConfig {
    pub base_url: url::Url,
    pub request_timeout: std::time::Duration,
    pub connect_timeout: std::time::Duration,
    pub safe_headers: http::HeaderMap,
    pub credential_reference: Option<SecretReference>,
}
```

Accept only `http` for explicitly local/private deployment configuration and `https` elsewhere according to deployment policy. Reject URL credentials, fragments, query-embedded secrets and reserved authentication headers in `safe_headers`.

- [ ] **Step 3: Implement private auth resolution**

Resolve the secret immediately before request construction, apply it to an ephemeral request builder, and never place it in adapter configuration or errors. Redacted debug output includes only secret-reference kind and hash prefix.

- [ ] **Step 4: Implement blocking completion and embeddings**

Normalize provider request ID, output, tool calls, usage and finish reason. Reject successful HTTP responses lacking required response fields. Map non-2xx status and provider error bodies through stable `ProviderErrorKind` without returning raw confidential body text.

- [ ] **Step 5: Implement SSE streaming**

Handle comments, blank separators, split UTF-8 frames, `[DONE]`, provider error frames and cancellation. Feed only canonical events to `ModelStreamAssembler`; never expose raw SSE frames to application code.

- [ ] **Step 6: Implement capability report**

Report native baseline:

```text
text completion
streaming text
JSON structured output
function/tool-call proposals
embeddings
usage accounting
cancellation
```

Multimodal input, audio, image generation, transcription and provider-specific reasoning controls report unsupported.

- [ ] **Step 7: Verify and commit**

```bash
cargo test -p vestrace-provider-openai-compatible
cargo clippy -p vestrace-provider-openai-compatible --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/vestrace-provider-openai-compatible
git commit -m "feat(provider): add native OpenAI-compatible adapter"
```

---

### Task 6: Build the shared provider conformance suite and deterministic HTTP server

**Files:**
- Create: `crates/vestrace-model-test-support/src/server.rs`
- Create: `crates/vestrace-model-test-support/src/conformance.rs`
- Create: `tests/native_provider_conformance.rs`
- Create: `tests/rig_provider_conformance.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `ProviderConformanceSuite::run(provider_factory)`.
- Produces a loopback-only scripted HTTP server with captured requests and deterministic SSE scripts.

- [ ] **Step 1: Implement the loopback fixture server**

Bind to `127.0.0.1:0`. Routes return scripted JSON/SSE, delays, malformed frames, status codes and connection drops. The server rejects any request not carrying the expected test credential and stores a redacted request capture.

- [ ] **Step 2: Define exact conformance cases**

The shared suite covers:

```text
completion text
streaming equivalence
structured JSON success
structured JSON invalid
single tool proposal
parallel tool proposals
usage normalization
embeddings
request timeout before response
cancellation
rate-limit mapping and retry-after
context overflow
malformed success response
lost response / unknown completion
secret redaction
provider unavailable
unsupported modality fails before request
```

Each case asserts canonical result or `ProviderErrorKind`, captured request count and absence of secrets in formatted diagnostics.

- [ ] **Step 3: Run the native adapter through the suite**

```rust
#[tokio::test]
async fn native_adapter_passes_provider_conformance() {
    ProviderConformanceSuite::default()
        .run(OpenAiCompatibleTestFactory::default())
        .await
        .unwrap();
}
```

- [ ] **Step 4: Keep the Rig test feature-gated**

`rig_provider_conformance.rs` compiles only with `provider-rig`. Until Task 7 exists, its expected failure is a missing `RigProviderFactory`; do not weaken the shared suite.

- [ ] **Step 5: Verify and commit**

```bash
cargo test --test native_provider_conformance
git add Cargo.toml Cargo.lock crates/vestrace-model-test-support tests
git commit -m "test(provider): add shared adapter conformance suite"
```

---

### Task 7: Implement the optional Rig provider adapter behind an anti-corruption layer

**Files:**
- Create: `crates/vestrace-rig-adapter/Cargo.toml`
- Create: `crates/vestrace-rig-adapter/src/lib.rs`
- Create: `crates/vestrace-rig-adapter/src/provider.rs`
- Create: `crates/vestrace-rig-adapter/src/normalization.rs`
- Modify: `Cargo.toml`
- Modify: `tests/rig_provider_conformance.rs`
- Modify: `scripts/verify-rig-boundary.sh`

**Interfaces:**
- Produces `RigModelProviderAdapter` implementing the same `ModelProviderPort`.
- Rig types remain private to the adapter crate.
- This task uses `rig-core`; it does not decide or implement the production agent loop.

- [ ] **Step 1: Add feature-isolated exact dependencies**

```toml
[features]
default = []
provider-rig = ["dep:rig-core", "dep:vestrace-rig-adapter"]

[dependencies]
rig-core = { version = "=0.41.0", optional = true, default-features = false }
```

Use the exact version approved by ADR-0001/H0. If ADR-0002 records a different tested version, use that exact version and document the change in the implementation commit.

- [ ] **Step 2: Extend the boundary script**

The script fails when `rig-core` or `rig-agent` appears in dependency trees for domain, application, native provider, HTTP, MCP or PostgreSQL infrastructure crates. It permits them only in `vestrace-rig-adapter` and the historical spike crate.

- [ ] **Step 3: Implement request/result normalization**

Translate canonical messages, tool definitions and output contracts to Rig provider requests; translate completion/streaming/embedding results back to Vestrace types. Provider-specific metadata is stored only in bounded JSON. Unsupported Rig features do not silently broaden the binding's capability report.

- [ ] **Step 4: Map all errors through the same classifier**

No `rig_core::Error`, provider client error or Rig message/tool type crosses the crate's public interface. Add compile-fail or source-boundary checks for forbidden `pub` signatures containing `rig_` paths.

- [ ] **Step 5: Run the unchanged conformance suite**

```bash
cargo test --features provider-rig --test rig_provider_conformance
bash scripts/verify-rig-boundary.sh
```

Both native and Rig adapter tests use the same local server scripts and canonical assertions.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/vestrace-rig-adapter tests/rig_provider_conformance.rs scripts/verify-rig-boundary.sh
git commit -m "feat(provider): add optional Rig adapter"
```

---

### Task 8: Implement policy-governed routing, reservations and controlled fallback

**Files:**
- Create: `crates/vestrace-application/src/model_runtime/orchestrator.rs`
- Create: `crates/vestrace-application/src/model_runtime/fallback.rs`
- Create: `crates/vestrace-application/tests/model_runtime_orchestrator.rs`
- Create: `crates/vestrace-application/tests/model_fallback.rs`
- Modify: `crates/vestrace-application/src/model_runtime/mod.rs`
- Modify: `crates/vestrace-application/src/models/router.rs`

**Interfaces:**
- Produces `ModelRuntimeOrchestrator::invoke` and `ModelFallbackController`.
- Consumes v0.1 router, H2 `ActionGuardService`, provider registry, execution repository, stream/structured validators and pricing revisions.

- [ ] **Step 1: Write the successful invocation test**

Use deterministic fakes and assert this order:

```text
create logical execution
→ route eligible models
→ construct model.invoke fingerprint
→ ActionGuard prepare with estimated token/cost reservation
→ persist policy/routing references
→ consume GuardedAction
→ create attempt
→ dispatch selected provider
→ validate output
→ reconcile actual usage/cost
→ complete attempt and logical execution
```

Assert the provider is never called before `consume`, and the final record references exact model, provider, binding, routing, policy, budget and price revisions.

- [ ] **Step 2: Define operation fingerprint arguments**

Canonical arguments for `model.invoke` contain only stable hashes/references:

```text
execution ID
selected model revision
provider binding revision
context snapshot hash
message bundle hash
tool-catalogue hash
output-contract hash
estimated usage by dimension
deadline
```

Do not include raw prompts in policy-decision rows.

- [ ] **Step 3: Implement cost reservation and reconciliation**

Reserve `InputTokens`, estimated `OutputTokens`, `ModelInvocations` and `MoneyMicrounits` under one H2 reservation request. Actual usage uses the price revision selected at routing time. Missing provider usage triggers conservative configured accounting and `provider_reported = false`.

- [ ] **Step 4: Write fallback matrix tests**

Cover:

- unavailable primary before response → permitted fallback;
- rate limit with retry-after beyond deadline → fallback;
- timeout after request may have completed → `Unknown`, no fallback;
- invalid structured output → one configured correction attempt, then permitted fallback;
- safety refusal → stop with typed refusal;
- policy-denied fallback model → never dispatched;
- fallback budget reservation denied → partial/failed result without provider call;
- already rejected capability/privacy candidate → never reconsidered.

- [ ] **Step 5: Implement attempt-specific authorization**

Every fallback attempt receives a new operation fingerprint, policy evaluation, reservation and guarded action. The original approval or ticket is not reused when the model or binding changes.

- [ ] **Step 6: Implement verifier independence filters**

When `ModelIndependenceRequirement` is set, exclude same model revision, family, provider or prompt-template lineage as required before ranking. Record every exclusion reason.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test model_runtime_orchestrator --test model_fallback
git add crates/vestrace-application
git commit -m "feat(model-runtime): govern routing and fallback"
```

---

### Task 9: Persist attempts, canonical events, loop checkpoints and reconciliation state

**Files:**
- Create: `migrations/0029_model_execution_attempts_and_events.sql`
- Create: `migrations/0030_model_loop_checkpoints_and_reconciliation.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/model_runtime/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_runtime/execution_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_runtime/attempt_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_runtime/event_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_runtime/checkpoint_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_runtime/reconciliation_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/model_execution_persistence.rs`
- Create: `tests/model_execution_unknown.rs`

**Interfaces:**
- Extends existing `model_executions` and produces `model_execution_attempts`, `model_execution_events`, `model_loop_checkpoints` and `model_reconciliations`.
- Produces PostgreSQL implementations of all H3 persistence ports.

- [ ] **Step 1: Write failing lifecycle and append-only tests**

Test:

1. attempt numbers are unique and contiguous per execution;
2. event sequence is unique and contiguous per attempt;
3. exact model/provider/binding/price/policy/budget revisions are required before dispatch;
4. event and checkpoint update/delete fail for the application role;
5. a fallback attempt belongs to the same logical execution/workspace;
6. `Unknown` attempt cannot transition directly to a new dispatch;
7. reconciliation changes logical outcome without creating another provider request.

- [ ] **Step 2: Create migration `0029`**

Alter `model_executions` to add:

```text
run_id, step_id, acting_agent_snapshot_id,
request_fingerprint, request_hash, output_contract_hash,
context_snapshot_reference JSONB,
policy_decision_id, policy_snapshot_id,
routing_decision_id, budget_reservation_id,
status, logical_revision, final_attempt_id,
result JSONB, failure JSONB, finished_at
```

Create `model_execution_attempts` and `model_execution_events`. Attempts store selected model/provider/binding/cost revisions, guarded action/ticket references, provider request ID, status, timing, usage, cost and safe error. Events store canonical event type/payload and monotonic sequence; raw SSE is not persisted.

- [ ] **Step 3: Create migration `0030`**

Create append-only `model_loop_checkpoints` with:

```text
engine_name, engine_version, state_schema_version,
serialized_state BYTEA, state_sha256,
canonical_journal_cursor, contains_sensitive_conversation,
run_id, step_id, model_execution_id, created_at
```

Create `model_reconciliations` with operation status, provider request reference, evidence, decision and timestamps. Checkpoint reads require the same workspace and restricted-content capability when sensitive conversation is present.

- [ ] **Step 4: Implement atomic repository transitions**

Use optimistic `logical_revision` for `model_executions`. Creating/transitioning an attempt and appending its corresponding canonical event occur in one scoped transaction. An ambiguous transaction result maps to `operation_unknown`; repositories do not retry writes automatically.

- [ ] **Step 5: Implement unknown reconciliation**

```rust
async fn record_reconciliation(
    execution_id: ModelExecutionId,
    expected_revision: u64,
    outcome: ReconciliationOutcome,
) -> Result<ModelExecutionRecord, ApplicationError>;
```

`Succeeded` requires verified provider result/evidence; `FailedSafeToRetry` may allow an explicit new command to create another attempt; `StillUnknown` preserves the fence.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test model_execution_persistence --test model_execution_unknown
git add migrations/0029_model_execution_attempts_and_events.sql migrations/0030_model_loop_checkpoints_and_reconciliation.sql crates tests
git commit -m "feat(model-runtime): persist attempts events and checkpoints"
```

---

### Task 10: Integrate restart-safe model work with H1 Run state and queue

**Files:**
- Create: `migrations/0031_model_runtime_rls_indexes_and_run_bindings.sql`
- Modify: `crates/vestrace-domain/src/run/work.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Create: `crates/vestrace-application/src/model_runtime/worker.rs`
- Create: `crates/vestrace-application/tests/model_worker.rs`
- Modify: `crates/vestrace-application/src/run/worker.rs`
- Create: `tests/model_runtime_rls.rs`
- Create: `tests/model_runtime_restart.rs`

**Interfaces:**
- Adds `WorkItemKind::InvokeModel` and `RunWorkPayload::InvokeModel { execution_id }`.
- Adds canonical Run events referencing model execution lifecycle, not stream deltas.
- Produces `ModelRuntimeWorkHandler`.

- [ ] **Step 1: Extend Run work and event contracts**

Add event payloads:

```rust
ModelExecutionStarted { execution_id: ModelExecutionId },
ModelExecutionCompleted { execution_id: ModelExecutionId, result_reference: RunReference },
ModelExecutionFailed { execution_id: ModelExecutionId, failure_code: String },
ModelExecutionUnknown { execution_id: ModelExecutionId, reconciliation_id: ModelReconciliationId },
```

Each payload is emitted as the sole event of its H1 logical mutation. Provider attempt/stream events remain in H3 tables.

- [ ] **Step 2: Write the restart-before-dispatch test**

Create work, record `ModelExecutionStarted`, crash before consuming the guarded action/provider dispatch, restart and assert the same execution/attempt resumes without creating a duplicate attempt or reservation.

- [ ] **Step 3: Write the lost-response restart test**

The fixture server accepts the request and drops the connection after generating a provider request ID. Simulate worker loss before completion persistence. On restart:

- no new provider request is sent;
- the stale dispatching attempt becomes `Unknown` after its deadline;
- Run transitions to `WaitingForDependency` or the configured reconciliation state;
- a `ModelExecutionUnknown` Run event references the reconciliation record.

- [ ] **Step 4: Implement the worker state machine**

```text
lease InvokeModel work
→ acquire Run lease
→ load Run and execution expected versions
→ create/restore guarded attempt
→ mark dispatching before network request
→ invoke provider with cancellation/deadline
→ persist canonical events and final result
→ reconcile H2 actual usage
→ emit one H1 completion/failure/unknown mutation
→ complete work and release lease
```

A stale worker fenced by H1 lease generation cannot complete the Run or execution.

- [ ] **Step 5: Add RLS, foreign-key validation and indexes**

Migration `0031`:

- forces RLS on all H3 tables;
- verifies model execution `run_id`/`step_id` ownership;
- extends work-item kind/payload checks for `InvokeModel`;
- indexes active attempts by deadline/status, events by attempt/sequence, executions by run/step/status and reconciliation queue by status/deadline;
- prevents cross-workspace provider/model/binding references.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test -p vestrace-application --test model_worker
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test model_runtime_rls --test model_runtime_restart
git add migrations/0031_model_runtime_rls_indexes_and_run_bindings.sql crates tests
git commit -m "feat(model-runtime): add restart-safe model worker"
```

---

### Task 11: Implement the production bounded model loop selected by ADR-0002

**Files always modified:**
- Create: `crates/vestrace-application/tests/model_loop_conformance.rs`
- Modify: `crates/vestrace-application/src/model_runtime/loop_port.rs`
- Modify: `crates/vestrace-application/src/model_runtime/mod.rs`

**Files for outcome `Accepted` or `AcceptedWithRestrictions`:**
- Create: `crates/vestrace-rig-adapter/src/loop_adapter.rs`
- Create: `crates/vestrace-rig-adapter/src/checkpoint.rs`
- Modify: `crates/vestrace-rig-adapter/src/lib.rs`
- Modify: `crates/vestrace-rig-adapter/Cargo.toml`

**Files for outcome `Rejected`:**
- Create: `crates/vestrace-application/src/model_runtime/native_loop.rs`

**Interfaces:**
- Produces exactly one production `ModelLoopPort` implementation selected by ADR-0002.
- Both possible implementations pass the same Vestrace conformance tests.

- [ ] **Step 1: Add an executable ADR gate test**

The test reads `docs/superpowers/specs/adr/0002-rig-agent-runtime-spike-outcome.md`, parses exactly one outcome and fails on missing, ambiguous or unsupported content. Build configuration must match:

```text
Accepted
  feature model-loop-rig enabled
  native loop implementation not selected

AcceptedWithRestrictions
  feature model-loop-rig enabled only for components explicitly approved by ADR
  Vestrace fallback components named by ADR remain active

Rejected
  feature model-loop-native enabled
  no production dependency on rig-agent
```

- [ ] **Step 2: Write shared loop conformance tests**

The factory-selected implementation must pass:

```text
first effect requests a model
model tool output becomes ToolCallsProposed without execution
model result without tools completes
maximum turn limit fails deterministically
invalid tool-call resolution is fail-closed
checkpoint records engine/version/schema/cursor/hash
same-version checkpoint resumes deterministically
incompatible checkpoint falls back to canonical reconstruction or fails safely
waiting for approval survives checkpoint
waiting for reconciliation never emits InvokeModel
no hidden chain-of-thought field exists in serialized Vestrace contracts
```

- [ ] **Step 3A: Implement the Rig loop when ADR accepts it**

Use only APIs and restrictions evidenced by ADR-0002. Hand-drive the state machine; do not use direct Rig tool execution, conversation memory, vector stores or workflow runtime. Store Rig state only inside opaque `ModelLoopEngineCheckpoint`. Translate every effect through Vestrace types.

- [ ] **Step 3B: Implement the native loop when ADR rejects Rig**

Implement a minimal deterministic state machine:

```text
NeedModel
→ AwaitingModel
→ ToolCallsProposed | Completed
→ AwaitingToolResults | WaitingForApproval | WaitingForReconciliation
→ NeedModel | Completed | Failed
```

State includes messages, pending tool calls, completed turn count, output contract and cursor. It performs no I/O and serializes as versioned Vestrace JSON.

- [ ] **Step 4: Enforce one selected implementation**

Cargo compile errors when both `model-loop-rig` and `model-loop-native` are enabled or when neither is enabled in a production build. Test-support builds may inject a scripted loop factory.

- [ ] **Step 5: Run and commit**

For accepted Rig outcome:

```bash
cargo test --features provider-rig,model-loop-rig \
  -p vestrace-application --test model_loop_conformance
```

For rejected Rig outcome:

```bash
cargo test --no-default-features \
  --features provider-openai-compatible,model-loop-native \
  -p vestrace-application --test model_loop_conformance
```

Commit only the selected production implementation and shared tests:

```bash
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-rig-adapter
git commit -m "feat(model-runtime): add selected bounded model loop"
```

When `Rejected`, omit unchanged/nonexistent Rig paths from `git add`.

---

### Task 12: Add Rig-free CI, contract verification and the H3 acceptance scenario

**Files:**
- Create: `scripts/verify-rig-free-model-runtime.sh`
- Modify: `scripts/verify-rig-boundary.sh`
- Modify: `.github/workflows/ci.yml`
- Create: `tests/model_runtime_rig_free.rs`
- Create: `tests/h3_acceptance.rs`
- Modify: provider and loop schema snapshots where the repository stores them

**Interfaces:**
- Produces mandatory native-only and optional Rig CI jobs.
- Produces the H3 exit-gate test.

- [ ] **Step 1: Create the Rig-free verification script**

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo test --workspace \
  --exclude vestrace-rig-spike \
  --no-default-features \
  --features provider-openai-compatible,model-loop-native

for package in vestrace-domain vestrace-application vestrace-infrastructure \
  vestrace-provider-openai-compatible vestrace-http vestrace-mcp; do
  if cargo tree -p "$package" | grep -E '(^| )rig-(agent|core) '; then
    echo "Rig leaked into native path: $package" >&2
    exit 1
  fi
done
```

When ADR-0002 accepts Rig as the only production loop, the native-only CI still compiles a minimal native conformance loop behind a test-only feature; the production default remains the ADR-selected implementation. The native provider path must never require Rig.

- [ ] **Step 2: Add CI matrix jobs**

Required jobs:

```text
native-provider-rig-free
all-features
provider-conformance-native
provider-conformance-rig (only when provider-rig is supported)
model-loop-selected
postgres-model-runtime
```

All use local fixtures. No job expects provider credentials.

- [ ] **Step 3: Write the Rig-free integration test**

Compile and run a text completion plus strict structured-output request through the native adapter, router, H2 guard, budget reservation and H3 persistence with Rig disabled. Assert no Rig package appears in the dependency tree captured by the script.

- [ ] **Step 4: Write the full H3 acceptance scenario**

The test registers:

```text
Model A: native OpenAI-compatible, unavailable
Model B: Rig provider adapter, eligible fallback
Model C: remote high-quality model, policy-ineligible for Restricted data
```

Run two sub-scenarios:

1. Public data: A fails before completion, B succeeds; canonical events and exact fallback authorization are persisted.
2. Restricted data: C is excluded by policy; native local A succeeds through the Rig-free path.

For each scenario assert:

- exact routing candidates and exclusion reasons;
- ActionGuard ticket consumed before dispatch;
- reservations and actual usage reconcile;
- structured output validates locally;
- logical execution and attempts survive repository reload;
- Run events reference start and completion without stream-delta noise;
- a simulated lost response becomes `Unknown` and is not failed over;
- adapter diagnostics contain no secret.

- [ ] **Step 5: Run all H3 gates**

```bash
bash scripts/verify-rig-free-model-runtime.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h3_acceptance --test model_runtime_restart
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: all commands exit `0`.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(model-runtime): add H3 acceptance and Rig-free gates"
```

---

## H3 completion definition

H3 is complete only when all twelve tasks pass and evidence demonstrates:

```text
canonical invocation request
→ policy and data-transfer eligibility
→ quality-first model routing
→ exact budget reservation
→ consumed authorization ticket
→ selected provider adapter
→ canonical blocking or streaming result
→ local structured-output validation
→ durable usage/cost reconciliation
→ controlled fallback or fenced Unknown
→ Run continuation from persisted state
```

The completed implementation must satisfy all of these statements:

1. The native OpenAI-compatible adapter passes the complete provider conformance suite and is usable without Rig.
2. The Rig provider adapter, when enabled, passes the same suite without changing canonical expectations.
3. Router selection refers to model/provider/binding revisions, not Rust adapter implementations.
4. A provider cannot be called before H2 authorization and reservation consumption.
5. A fallback cannot reuse another model attempt's ticket, reservation or approval binding.
6. `Unknown` completion never creates an automatic replacement request.
7. Safety refusal is not blindly bypassed.
8. Structured output is validated locally after assembly.
9. Stream deltas never become authoritative Run state.
10. Every successful invocation stores exact routing, policy, budget, provider, price, usage and validation provenance.
11. Provider secrets are absent from canonical DTOs, execution events, checkpoints, errors and logs.
12. The selected production model loop matches ADR-0002 and remains replaceable through `ModelLoopPort`.
13. H1 logical replay does not require replaying provider I/O.
14. H3 tests require no public network or paid provider.
15. H4 can consume `ToolCallsProposed` without changing model-provider contracts.

## Explicit non-goals

H3 does not implement:

- real tool execution or sandboxing;
- tool risk classification and prepare/commit lifecycle;
- artifact ingestion and multimodal artifact transformation;
- conversations, channels or triggers;
- OAuth connections or general Credential Broker leases;
- agent planning or delegation;
- automatic model training or prompt mutation;
- provider-specific audio/image/transcription APIs;
- browser automation;
- public marketplace or managed provider fleet.

These remain assigned to later Harness plans.

## Documentation-only boundary

Creating this document does not authorize implementation. During the current documentation phase, do not:

- create `feat/h3-model-runtime-provider-adapters`;
- change Cargo dependencies or features;
- execute H0-RIG or produce ADR-0002;
- run provider servers or conformance tests;
- create migrations `0028`–`0031`;
- modify CI;
- write native or Rig adapter code.
