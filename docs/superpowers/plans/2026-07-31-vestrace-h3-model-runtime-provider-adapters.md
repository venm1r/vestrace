# Vestrace H3 Model Runtime and Provider Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, run provider tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement Vestrace-owned model invocation contracts, a production native OpenAI-compatible adapter, an optional Rig provider adapter, canonical streaming and structured outputs, policy- and budget-governed routing/fallback, durable invocation records, restart-safe model work and the bounded model-loop implementation selected by ADR-0002.

**Architecture:** Extend the v0.1 provider/model registry, router and `model_executions` records rather than introducing parallel concepts. One logical `ModelExecution` contains one normalized request and one or more ordered `ModelExecutionAttempt` records when controlled fallback is allowed. Every adapter implements the same Vestrace-owned `ModelProviderPort`; provider-specific types remain inside adapter crates. The native OpenAI-compatible adapter is a complete narrow production path and remains usable when Rig is absent. The bounded model loop is a sans-I/O `ModelLoopPort`; ADR-0002 determines its production implementation. Concrete adapter and loop selection occurs in the CLI/composition root, never in the application crate.

**Tech Stack:** Existing Vestrace v0.1, H1 and H2 Rust workspace; Rust Edition 2024; Tokio; Serde; Schemars; SQLx; PostgreSQL 17; Reqwest; Futures; Tokio Util; SHA-256; JSON Schema validation; deterministic loopback HTTP fixtures; optional exact-pinned `rig-core` and `rig-agent` approved by ADR-0002.

## Global Constraints

- Complete all five v0.1 plans, H1 Durable Run Core, H2 Policy/Approval/Budget Core and the H0-RIG spike before Task 11.
- ADR-0001 remains authoritative: `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`.
- ADR-0002 must exist before Task 11 and contain exactly `Accepted`, `AcceptedWithRestrictions` or `Rejected`.
- Vestrace owns every domain type, application port, persisted schema, durable event, public schema and checkpoint envelope.
- Rig types may appear only in `vestrace-rig-adapter` and the isolated historical `vestrace-rig-spike`; they may not appear in production domain/application signatures, persistence or public contracts.
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

crates/vestrace-cli/src/
  composition/mod.rs
  composition/model_runtime.rs

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

### Canonical messages

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

Every message has at least one part. A text part is at most 4 MiB before later context budgeting. `Tool` requires a non-blank call ID; other roles reject it. `InputReference` requires an authorized reference and matching model modality. The native adapter initially rejects input references before network dispatch.

### Supporting enums and values

```rust
pub enum ModelModality {
    Text,
    Json,
    Image,
    Audio,
}

pub enum ModelFinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Cancelled,
    Unknown,
}

pub enum ModelIndependenceRequirement {
    None,
    DifferentModelRevision,
    DifferentFamily,
    DifferentProvider,
    DifferentProviderAndFamily,
}

pub struct ModelFallbackPolicy {
    pub maximum_attempts: u32,
    pub allow_transient_fallback: bool,
    pub allow_structured_output_correction: bool,
    pub maximum_structured_output_corrections: u32,
}

pub enum FallbackDisposition {
    Stop,
    RetryAfter { delay_ms: u64 },
    TryNextEligibleModel,
    RecontextualizationRequired,
    ReconciliationRequired,
}
```

`maximum_attempts` is `1..=8`; structured-output corrections are `0..=2`.

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

### Logical invocation request

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

### Provider binding and runtime capabilities

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

pub struct RuntimeCapabilityReport {
    pub adapter_kind: ProviderAdapterKind,
    pub adapter_revision: String,
    pub capabilities: std::collections::BTreeSet<ModelCapability>,
    pub modalities: std::collections::BTreeSet<ModelModality>,
    pub supports_streaming: bool,
    pub supports_cancellation: bool,
    pub observed_at: Timestamp,
}

pub struct EffectiveModelRuntimeCapabilities {
    pub capabilities: std::collections::BTreeSet<ModelCapability>,
    pub modalities: std::collections::BTreeSet<ModelModality>,
    pub supports_streaming: bool,
    pub supports_cancellation: bool,
}
```

Initial adapter kinds are `openai-compatible` and `rig`. Configuration contains endpoint, timeout and safe header names, never secret values.

### Usage, tool calls, results and errors

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
  use an already eligible larger-context fallback
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

### Application-owned provider ports

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
pub trait ProviderAdapterCatalogPort: Send + Sync {
    async fn is_registered(&self, kind: &ProviderAdapterKind)
        -> Result<bool, ApplicationError>;
    async fn capability_report(&self, kind: &ProviderAdapterKind)
        -> Result<RuntimeCapabilityReport, ApplicationError>;
}

#[async_trait::async_trait]
pub trait ModelProviderRegistryPort: ProviderAdapterCatalogPort {
    async fn get(&self, kind: &ProviderAdapterKind)
        -> Result<std::sync::Arc<dyn ModelProviderPort>, ApplicationError>;
}

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
```

`EmbeddingRequest` and `EmbeddingResult` reuse the v0.1 retrieval contracts; H3 does not create duplicate embedding types.

### Validation and credential ports

```rust
#[async_trait::async_trait]
pub trait SemanticOutputValidatorPort: Send + Sync {
    async fn validate(
        &self,
        validator_id: &str,
        value: &serde_json::Value,
    ) -> Result<SemanticValidationResult, ApplicationError>;
}

#[async_trait::async_trait]
pub(crate) trait ProviderAuthResolver: Send + Sync {
    async fn resolve(
        &self,
        reference: &SecretReference,
    ) -> Result<ResolvedProviderAuth, ProviderAdapterError>;
}
```

`SemanticValidationResult` contains `accepted`, stable issue codes and JSON Pointer locations. `ProviderAdapterError` and `ResolvedProviderAuth` are adapter-private; auth has redacted `Debug` and is zeroized where supported.

### Durable lifecycle and reconciliation

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

pub enum ReconciliationOutcome {
    Succeeded { result: ProviderInvocationResult },
    FailedSafeToRetry { stable_code: String },
    FailedFinal { stable_code: String },
    StillUnknown,
}

pub struct ModelExecutionRecord {
    pub execution_id: ModelExecutionId,
    pub runtime_status: ModelExecutionStatus,
    pub runtime_revision: u64,
    pub attempts: Vec<ModelExecutionAttemptRecord>,
    pub final_result: Option<ProviderInvocationResult>,
    pub failure_code: Option<String>,
}
```

`Succeeded`, `Failed`, `Unknown` and `Cancelled` stop automatic execution. Reconciliation may resolve `Unknown` without silently dispatching again.

### Sans-I/O model loop

```rust
pub struct ModelToolResultView {
    pub call_id: String,
    pub status: String,
    pub model_presentation: String,
    pub output_reference: Option<RunReference>,
}

pub struct ModelLoopOutput {
    pub text: String,
    pub structured_output: Option<serde_json::Value>,
    pub tool_calls: Vec<ModelToolCall>,
}

pub struct ModelLoopFailure {
    pub stable_code: String,
    pub safe_message: String,
}

pub struct ModelLoopEngineCheckpoint {
    pub engine_name: String,
    pub engine_version: String,
    pub state_schema_version: u32,
    pub serialized_state: Vec<u8>,
    pub state_sha256: [u8; 32],
    pub canonical_journal_cursor: ResumeCursor,
    pub contains_sensitive_conversation: bool,
}

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

pub trait ModelLoopFactoryPort: Send + Sync {
    fn create(&self, start: &ModelLoopStart)
        -> Result<Box<dyn ModelLoopPort>, ApplicationError>;
}
```

The loop performs no provider or tool I/O. A tool proposal is never permission to execute. Concrete factory selection belongs to the composition root.

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

**Interfaces:** Adds all H3 IDs and domain-owned contracts above. It does not define cancellation, provider ports or adapter-private types.

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

Compile JSON Schemas on construction. Reject schemas above 1 MiB, blank validator IDs, empty parts and names outside `[A-Za-z0-9_.-]` or above 128 bytes.

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

Test every matrix row, especially timeout with possible completion.

- [ ] **Step 5: Implement invocation and loop values**

Require non-empty messages/task/idempotency key, `maximum_model_turns` in `1..=64`, unique tool names and H2 canonical arguments for tool calls.

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

**Interfaces:** Produces binding revisions, `ProviderAdapterCatalogPort`, runtime capability reports and intersection-based negotiation.

- [ ] **Step 1: Write failing binding tests**

Test creation, immutable revision history, secret rejection, disabled-binding exclusion and tool-capability mismatch. Expect failure before migration `0028`.

- [ ] **Step 2: Create schema**

Create `provider_adapter_bindings`, `provider_adapter_binding_revisions` and `provider_runtime_capability_reports`; add the current binding revision pointer to `provider_revisions`. Enforce workspace ownership, immutability and hashes.

- [ ] **Step 3: Implement catalog and commands**

Define `ProviderAdapterCatalogPort` in `models/ports.rs`, then implement create, revise, activate and disable commands with idempotency and optimistic concurrency. Activation requires `is_registered = true`.

- [ ] **Step 4: Implement negotiation**

```rust
pub fn negotiate_runtime_capabilities(
    model: &ModelRevision,
    binding: &ProviderAdapterBindingRevision,
    adapter: &RuntimeCapabilityReport,
) -> Result<EffectiveModelRuntimeCapabilities, CapabilityNegotiationError>;
```

The effective set is an intersection; discovery never broadens declarations.

- [ ] **Step 5: Integrate router filters**

Apply binding availability/capability checks before quality/cost ranking and record stable exclusion codes.

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
- Modify: root `Cargo.toml`

**Interfaces:** Produces provider/registry/execution/checkpoint/reconciliation/semantic-validation/loop ports, invocation commands, deterministic providers and `ScriptedModelLoop` behind `model-loop-test`.

- [ ] **Step 1: Write object-safety compile tests**

```rust
fn accepts_provider(_value: std::sync::Arc<dyn ModelProviderPort>) {}
fn accepts_loop(_value: Box<dyn ModelLoopPort>) {}
fn accepts_factory(_value: std::sync::Arc<dyn ModelLoopFactoryPort>) {}
```

- [ ] **Step 2: Define exact repository port methods**

Use typed signatures for create execution, authorize, attach routing, create attempt, mark dispatching/streaming, append events, complete/fail/unknown attempt, complete execution, load and reconcile. Every logical transition carries expected runtime revision.

- [ ] **Step 3: Define root feature forwarding**

```toml
[features]
default = ["provider-openai-compatible"]
provider-openai-compatible = ["dep:vestrace-provider-openai-compatible"]
provider-rig = ["dep:vestrace-rig-adapter"]
model-loop-test = ["vestrace-application/model-loop-test"]
```

Production binary profiles reject `model-loop-test` in Task 11.

- [ ] **Step 4: Implement deterministic fakes**

`ScriptedProvider` supports blocking, streaming, embeddings, pre-dispatch timeout and post-dispatch lost response. `ScriptedModelLoop` emits configured effects/checkpoints without network or provider dependencies.

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

**Interfaces:** Produces `ModelStreamAssembler`, `AssembledProviderOutput`, `StructuredOutputValidator` and `OutputValidationReport`.

- [ ] **Step 1: Write stream-order tests**

Test deterministic text/tool assembly, missing start, duplicate terminal, decreasing indexes, malformed tool JSON and deltas after completion.

- [ ] **Step 2: Implement fail-closed limits**

Limit each delta to 1 MiB, total text to the configured ceiling and tool argument buffers to 4 MiB per call. Violations map to `InvalidResponse`.

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

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p vestrace-application --test model_stream --test structured_output
git add crates/vestrace-application
git commit -m "feat(model-runtime): validate streams and structured outputs"
```

---

### Task 5: Implement the native OpenAI-compatible adapter

**Files:**
- Create: `crates/vestrace-provider-openai-compatible/Cargo.toml`
- Create all ten Rust files listed for this crate in the locked structure
- Modify: root `Cargo.toml`

**Interfaces:** Produces `OpenAiCompatibleProvider: ModelProviderPort`; supports compatible chat completions, SSE and embeddings.

- [ ] **Step 1: Write request-mapping tests**

Cover all roles, strict JSON format, tool definitions, timeout/cancellation, omitted fields and redacted headers. Developer role is sent only when declared supported; otherwise fail before dispatch.

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

Reject URL credentials, fragments, query secrets and authentication headers in `safe_headers`. Plain HTTP requires local/private deployment policy.

- [ ] **Step 3: Implement private auth resolution**

Resolve immediately before request construction; apply to an ephemeral request builder; never expose secret material.

- [ ] **Step 4: Implement completion and embeddings**

Normalize provider request ID, output, tool calls, usage and finish reason. Reject malformed successful responses and safely classify non-success responses.

- [ ] **Step 5: Implement SSE**

Handle comments, separators, split UTF-8, `[DONE]`, provider error frames and cancellation. Only canonical events leave the adapter.

- [ ] **Step 6: Publish baseline capabilities**

Report text, streaming, JSON, tool proposals, embeddings, usage and cancellation; report unsupported media/provider-specific controls honestly.

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

**Interfaces:** Produces a loopback scripted server and `ProviderConformanceSuite::run`.

- [ ] **Step 1: Implement loopback server**

Bind `127.0.0.1:0`; script JSON/SSE, delays, malformed frames, statuses and connection drops. Store redacted captures only.

- [ ] **Step 2: Define cases**

Cover text, streaming equivalence, JSON success/failure, single/parallel tool proposals, usage, embeddings, timeout, cancellation, rate limit, context overflow, malformed success, lost response, secret redaction, unavailable provider and unsupported modality.

- [ ] **Step 3: Run native suite**

Instantiate the native factory and require every case to pass.

- [ ] **Step 4: Add feature-gated Rig shell**

Task 7 supplies the Rig factory; shared assertions remain identical.

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
- Create: `src/lib.rs`, `src/provider.rs`, `src/normalization.rs`
- Modify: root `Cargo.toml`
- Modify: `tests/rig_provider_conformance.rs`
- Modify: `scripts/verify-rig-boundary.sh`

**Interfaces:** Produces `RigModelProviderAdapter: ModelProviderPort`; uses `rig-core` only for provider adaptation.

- [ ] **Step 1: Add exact isolated dependency**

Pin the H0/ADR-approved `rig-core`; `provider-rig` enables only this adapter.

- [ ] **Step 2: Extend boundary verification**

Fail on Rig dependencies in domain, application, native provider, HTTP, MCP or PostgreSQL infrastructure. Permit adapter and spike only.

- [ ] **Step 3: Implement anti-corruption translation**

Translate canonical requests/results/streams/embeddings; bound metadata and never broaden capabilities.

- [ ] **Step 4: Normalize errors and public signatures**

No Rig error/message/tool type crosses the crate interface. Add source-boundary tests.

- [ ] **Step 5: Run unchanged suite and commit**

```bash
cargo test --features provider-rig --test rig_provider_conformance
bash scripts/verify-rig-boundary.sh
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

**Interfaces:** Produces `ModelRuntimeOrchestrator::invoke` and `ModelFallbackController`.

- [ ] **Step 1: Write successful-order test**

```text
create execution
→ route
→ fingerprint model.invoke
→ guard prepare/reserve
→ persist policy/routing refs
→ consume guarded action
→ create attempt
→ dispatch
→ validate
→ reconcile usage/cost
→ complete attempt/execution
```

Assert provider call count is zero before guard consumption.

- [ ] **Step 2: Define fingerprint inputs**

Use execution/model/binding IDs, context/message/tool/output hashes, estimates and deadline; never raw prompts.

- [ ] **Step 3: Implement reservation/reconciliation**

Reserve input/output tokens, invocation count and money microunits. Reconcile using the selected price revision. Missing usage uses conservative configured accounting.

- [ ] **Step 4: Test fallback matrix**

Cover permitted pre-response fallback, ambiguous timeout to Unknown, bounded JSON correction, safety refusal stop, policy exclusion, budget denial and no reconsideration of hard rejects.

- [ ] **Step 5: Require attempt-specific authority**

Every fallback gets a new fingerprint, decision, reservation and ticket.

- [ ] **Step 6: Enforce verifier independence**

Apply revision/family/provider requirements before ranking and persist reasons.

- [ ] **Step 7: Verify and commit**

```bash
cargo test -p vestrace-application --test model_runtime_orchestrator --test model_fallback
git add crates/vestrace-application
git commit -m "feat(model-runtime): govern routing and fallback"
```

---

### Task 9: Persist attempts, events, checkpoints and reconciliation

**Files:**
- Create: `migrations/0029_model_execution_attempts_and_events.sql`
- Create: `migrations/0030_model_loop_checkpoints_and_reconciliation.sql`
- Create all six PostgreSQL files under `model_runtime/` listed in the locked structure
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/model_execution_persistence.rs`
- Create: `tests/model_execution_unknown.rs`

**Interfaces:** Extends `model_executions`; creates attempts, canonical events, loop checkpoints and reconciliation records.

- [ ] **Step 1: Write lifecycle tests**

Verify contiguous attempts/events, exact revisions, append-only rows, workspace consistency, no direct `Unknown → dispatch`, and reconciliation without a new request.

- [ ] **Step 2: Create migration `0029` without duplicate v0.1 columns**

Reuse existing v0.1 routing/status/outcome/usage columns where semantically identical. Add only:

```text
run_id, step_id, acting_agent_snapshot_id,
request_fingerprint, request_hash, output_contract_hash,
context_snapshot_reference JSONB,
policy_decision_id, policy_snapshot_id, budget_reservation_id,
runtime_status, runtime_revision, final_attempt_id,
normalized_result JSONB, normalized_failure JSONB, runtime_finished_at
```

Create attempts/events; never store raw SSE.

- [ ] **Step 3: Create migration `0030`**

Create append-only checkpoints with engine/version/schema/opaque bytes/hash/cursor/sensitivity and reconciliation records with evidence/decision.

- [ ] **Step 4: Implement atomic transitions**

Use optimistic `runtime_revision`. Attempt transition and canonical event commit together. Ambiguous commit returns `operation_unknown` without auto-retry.

- [ ] **Step 5: Implement reconciliation**

Use the exact `ReconciliationOutcome` contract. `FailedSafeToRetry` permits only a later explicit command.

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
- Create: `migrations/0031_model_runtime_rls_indexes_and_run_bindings.sql`
- Modify: `crates/vestrace-domain/src/run/work.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Create: `crates/vestrace-application/src/model_runtime/worker.rs`
- Create: `crates/vestrace-application/tests/model_worker.rs`
- Modify: `crates/vestrace-application/src/run/worker.rs`
- Create: `tests/model_runtime_rls.rs`
- Create: `tests/model_runtime_restart.rs`

**Interfaces:** Adds `InvokeModel` work, Run lifecycle references and `ModelRuntimeWorkHandler`.

- [ ] **Step 1: Extend Run events**

```rust
ModelExecutionStarted { execution_id: ModelExecutionId },
ModelExecutionCompleted { execution_id: ModelExecutionId, result_reference: RunReference },
ModelExecutionFailed { execution_id: ModelExecutionId, failure_code: String },
ModelExecutionUnknown { execution_id: ModelExecutionId, reconciliation_id: ModelReconciliationId },
```

- [ ] **Step 2: Test restart before dispatch**

Crash after Run start event but before ticket consumption; assert no duplicate attempt/reservation.

- [ ] **Step 3: Test lost response**

Drop after accepted request. Restart sends no second request; after deadline mark Unknown, enter durable reconciliation wait and reference the reconciliation record.

- [ ] **Step 4: Implement worker order**

```text
lease work
→ acquire Run lease
→ load versions
→ create/restore guarded attempt
→ mark dispatching before network
→ invoke
→ persist canonical result
→ reconcile budget
→ emit one H1 result mutation
→ complete work/release lease
```

- [ ] **Step 5: Create migration `0031`**

Force RLS, validate all workspace relations, extend work checks and add active-status indexes.

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

### Task 11: Implement and compose the ADR-selected production model loop

**Files always:**
- Create: `crates/vestrace-application/tests/model_loop_conformance.rs`
- Modify: `crates/vestrace-application/src/model_runtime/loop_port.rs`
- Modify: `crates/vestrace-application/src/model_runtime/mod.rs`
- Create: `crates/vestrace-cli/src/composition/mod.rs`
- Create: `crates/vestrace-cli/src/composition/model_runtime.rs`
- Modify: `crates/vestrace-cli/src/main.rs`

**When accepted/restricted:** create Rig `loop_adapter.rs` and `checkpoint.rs`; modify Rig crate manifest/lib.

**When rejected:** create `crates/vestrace-application/src/model_runtime/native_loop.rs`.

**Interfaces:** Produces one ADR-selected default `ModelLoopFactoryPort` in the composition root. Application does not depend on concrete adapters.

- [ ] **Step 1: Add executable ADR gate**

Parse exactly one outcome. The composition default must match it. Test-only loop never satisfies production startup.

- [ ] **Step 2: Write shared conformance**

Test initial model request, tool proposal without execution, direct completion, turn limit, fail-closed invalid tool resolution, versioned checkpoint, safe incompatibility, durable approval/reconciliation waits and absence of hidden-CoT fields.

- [ ] **Step 3A: Implement accepted Rig loop**

Use only H0-approved APIs/restrictions; hand-drive it; no direct tool execution/memory/vector/workflow runtime; opaque checkpoint bytes only.

- [ ] **Step 3B: Implement rejected native loop**

Implement the explicit `NeedModel/AwaitingModel/AwaitingToolResults/Waiting/Completed/Failed` state machine with versioned Vestrace serialization and no I/O.

- [ ] **Step 4: Compose without circular dependencies**

`vestrace-cli` depends on application ports and available concrete adapter crates. Available implementations may compile together under `all-features`; exactly one ADR-permitted default is selected. Production startup rejects `model-loop-test` and any implementation forbidden by ADR.

- [ ] **Step 5: Verify and commit**

Run shared conformance, composition tests and `cargo test --workspace --all-features`. Commit only ADR-applicable implementation files plus shared/composition code.

---

### Task 12: Add Rig-free CI and H3 acceptance

**Files:**
- Create: `scripts/verify-rig-free-model-runtime.sh`
- Modify: `scripts/verify-rig-boundary.sh`
- Modify: `.github/workflows/ci.yml`
- Create: `tests/model_runtime_rig_free.rs`
- Create: `tests/h3_acceptance.rs`
- Modify: schema snapshots used by the repository

**Interfaces:** Produces mandatory native-provider/Rig-free and optional Rig jobs plus H3 exit gate.

- [ ] **Step 1: Create Rig-free script**

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

- [ ] **Step 2: Add CI jobs**

```text
native-provider-rig-free
all-features
provider-conformance-native
provider-conformance-rig
model-loop-selected
postgres-model-runtime
```

- [ ] **Step 3: Write Rig-free integration**

Run text and strict JSON through native adapter, router, H2 guard/budgets and H3 persistence with Rig excluded.

- [ ] **Step 4: Write H3 acceptance**

Use native-unavailable A, Rig-fallback B and policy-ineligible remote C. Test public fallback with new authority, Restricted-data exclusion/local success, and lost-response Unknown without fallback. Verify routing explanations, ticket order, budget reconciliation, schema validation, durable reload, Run references and secret-free diagnostics.

- [ ] **Step 5: Run gates**

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
