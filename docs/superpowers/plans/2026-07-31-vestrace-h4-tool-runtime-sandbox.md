# Vestrace H4 Tool Runtime and Sandbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, run containers, create migrations or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement a versioned, policy-governed and restart-safe Tool Runtime with exact preparation/approval/commit/verification semantics, typed retry and reconciliation, production native/HTTP/MCP adapters, and an isolated Docker sandbox with controlled mounts, resources, network access and output collection.

**Architecture:** Extend H1 durable Runs, H2 `ActionGuardService` and H3 model tool proposals without making adapters authoritative. One logical `ToolInvocation` owns an exact tool revision and canonical arguments; preparation, commit, verification and reconciliation are separate durable phases with ordered attempt/event records. Tool adapters perform transport-specific I/O only after Vestrace authorization. Sandboxed execution is delegated through a Vestrace-owned `SandboxProviderPort`; Docker, filesystem staging and egress enforcement remain infrastructure details.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H3 Rust workspace; Rust Edition 2024; Tokio; Serde; Schemars; SQLx; PostgreSQL 17; Reqwest; RMCP-compatible client transport; Docker Engine API; SHA-256; JSON Schema validation; tracing; proptest; deterministic loopback HTTP/MCP fixtures; Linux container integration tests.

## Global Constraints

- Complete all five v0.1 plans, H1 Durable Run Core, H2 Policy/Approval/Budget Core and H3 Model Runtime before implementing H4.
- The Harness design sections `11. Tool Runtime`, `12. Execution environments and sandbox` and `13. Durable state, checkpoints and recovery` are normative.
- PostgreSQL is authoritative for tool definitions, binding revisions, invocations, attempts, preparations, previews, canonical events, verification, reconciliation, sandbox profiles and sandbox sessions.
- Vestrace owns all domain values, application ports, persisted schemas, public DTOs, idempotency rules and lifecycle transitions.
- Adapter-specific types from Reqwest, RMCP, Docker or any future SDK may not appear in domain/application signatures or PostgreSQL schemas.
- H2 `ActionGuardService` is the only path to an executable tool phase. Adapters never interpret approval grants, policies or budgets.
- A model tool call is a proposal. It does not create authority and cannot execute directly.
- A2A remote-agent delegation is not a Tool Runtime adapter. It is governed by ADR-0003 and H9A.
- Initial executable binding kinds are `native`, `http`, `mcp` and `sandbox-command`.
- `human`, `browser`, `subrun` and `remote-agent` binding names may be reserved, but H4 must reject them as unavailable rather than simulate them.
- Tool risk and side-effect classes come from an activated immutable tool revision plus policy restrictions, never from the model-visible name, description or remote annotations.
- Important operations use `Prepare → Preview → Approve → Commit → Verify`. Read-only operations may collapse preparation and approval only when the active policy permits it.
- Approval for commit binds the exact tool revision, canonical arguments, preparation hash, preview hash, principal, Run, step and execution environment.
- Every external dispatch has a stable invocation idempotency key and an attempt-specific dispatch key.
- `Unknown` is mandatory whenever an external side effect may have occurred but the response is not durably known.
- `Unknown` never becomes an automatic retry. Reconciliation must establish a terminal outcome or `FailedSafeToRetry` first.
- Compensation is a separate tool invocation or workflow linked to the original invocation. It is never represented as a universal rollback.
- Only a required verification result of `Verified` or explicitly permitted `VerifiedWithWarnings` can produce logical `Succeeded`.
- Tool output, MCP metadata, HTTP bodies, sandbox logs and discovered schemas are untrusted input.
- Tool binding configuration contains no secret values. H4 implements unauthenticated transport fixtures and safe auth-decorator SPIs; H8 supplies request-scoped credential leases later.
- A binding that requires credentials returns `AuthenticationRequired` until H8 provides a valid request-scoped decorator. Permanent credentials are never accepted in tool configuration.
- Sandbox command arguments are an argv vector, never an interpolated shell command.
- Docker image references are digest-pinned before profile activation. Mutable tags are not persisted as executable identity.
- Sandbox root filesystems are read-only; input mounts are read-only; writable paths and export paths are explicit.
- Default sandbox network mode is `DenyAll`. `Unrestricted` is not implemented in H4.
- `AllowListedDomains` is enforced through a controlled egress gateway; workload containers have no direct external route.
- Sandbox output collection rejects path traversal, symlink escape, hard-link escape, device files, sockets and output-limit violations.
- Run replay and safe evaluation never redispatch production writes.
- Existing migrations `0014`–`0031` are never edited. H4 migrations are `0032`–`0036` and are created once.
- CI unit and PostgreSQL suites require no public service, paid account or permanent credential. Docker tests run only in the declared Linux container job.
- Future implementation branch: `feat/h4-tool-runtime-sandbox`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  tool/mod.rs
  tool/definition.rs
  tool/lifecycle.rs
  tool/preparation.rs
  tool/verification.rs
  tool/reconciliation.rs
  sandbox/mod.rs
  sandbox/profile.rs
  sandbox/session.rs
  sandbox/network.rs
  run/event.rs
  run/work.rs
  run/mod.rs

crates/vestrace-application/src/
  lib.rs
  tool_runtime/mod.rs
  tool_runtime/ports.rs
  tool_runtime/commands.rs
  tool_runtime/registry.rs
  tool_runtime/prepare.rs
  tool_runtime/commit.rs
  tool_runtime/verify.rs
  tool_runtime/reconcile.rs
  tool_runtime/retry.rs
  tool_runtime/projection.rs
  tool_runtime/worker.rs
  sandbox/mod.rs
  sandbox/ports.rs
  sandbox/service.rs
  sandbox/output.rs

crates/vestrace-application/tests/
  tool_registry.rs
  tool_prepare.rs
  tool_commit.rs
  tool_retry.rs
  tool_reconciliation.rs
  tool_verification.rs
  tool_worker.rs
  sandbox_service.rs

crates/vestrace-tool-adapter-native/
  Cargo.toml
  src/lib.rs
  src/registry.rs
  src/adapter.rs

crates/vestrace-tool-adapter-http/
  Cargo.toml
  src/lib.rs
  src/adapter.rs
  src/auth.rs
  src/config.rs
  src/request.rs
  src/response.rs
  src/error.rs
  src/reconcile.rs

crates/vestrace-tool-adapter-mcp/
  Cargo.toml
  src/lib.rs
  src/adapter.rs
  src/auth.rs
  src/client.rs
  src/discovery.rs
  src/normalization.rs
  src/error.rs

crates/vestrace-sandbox-docker/
  Cargo.toml
  src/lib.rs
  src/provider.rs
  src/client.rs
  src/config.rs
  src/image.rs
  src/mounts.rs
  src/network.rs
  src/output.rs
  src/security.rs
  src/reconcile.rs

crates/vestrace-sandbox-egress-proxy/
  Cargo.toml
  src/lib.rs
  src/policy.rs
  src/resolver.rs
  src/proxy.rs
  src/audit.rs

crates/vestrace-tool-test-support/
  Cargo.toml
  src/lib.rs
  src/fakes.rs
  src/http_server.rs
  src/mcp_server.rs
  src/conformance.rs
  src/sandbox_fixtures.rs

crates/vestrace-infrastructure/src/postgres/
  mod.rs
  tool_runtime/mod.rs
  tool_runtime/registry_repository.rs
  tool_runtime/invocation_repository.rs
  tool_runtime/event_repository.rs
  tool_runtime/preparation_repository.rs
  tool_runtime/verification_repository.rs
  tool_runtime/reconciliation_repository.rs
  sandbox/mod.rs
  sandbox/profile_repository.rs
  sandbox/session_repository.rs
  sandbox/event_repository.rs

migrations/
  0032_tool_registry_and_bindings.sql
  0033_tool_invocations_attempts_and_events.sql
  0034_tool_preparations_verifications_and_reconciliation.sql
  0035_sandbox_profiles_sessions_and_events.sql
  0036_tool_sandbox_rls_indexes_and_run_bindings.sql

tests/
  tool_registry_persistence.rs
  tool_invocation_persistence.rs
  tool_approval_binding.rs
  tool_unknown_reconciliation.rs
  tool_adapter_native_conformance.rs
  tool_adapter_http_conformance.rs
  tool_adapter_mcp_conformance.rs
  tool_runtime_restart.rs
  tool_runtime_rls.rs
  sandbox_profile_persistence.rs
  sandbox_docker_isolation.rs
  sandbox_network_policy.rs
  sandbox_output_collection.rs
  h4_acceptance.rs

scripts/
  verify-tool-adapter-boundary.sh
  verify-sandbox-security.sh
```

---

## Normative contracts

### Tool definition and immutable revisions

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolSideEffectClass {
    ReadOnly,
    IdempotentWrite,
    NonIdempotentWrite,
    Destructive,
    Irreversible,
    ExternalCommitment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolBindingKind {
    Native,
    Http,
    Mcp,
    SandboxCommand,
    Human,
    Browser,
    SubRun,
    RemoteAgent,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ToolRevision {
    pub id: ToolRevisionId,
    pub tool_id: ToolId,
    pub revision: u32,
    pub stable_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub required_capabilities: std::collections::BTreeSet<Capability>,
    pub risk: RiskLevel,
    pub side_effect: ToolSideEffectClass,
    pub idempotency: ToolIdempotencyPolicy,
    pub preparation: ToolPreparationPolicy,
    pub timeout: ToolTimeoutPolicy,
    pub retry: ToolRetryPolicy,
    pub verification: ToolVerificationPolicy,
    pub data_policy: ToolDataPolicy,
    pub binding_revision_id: ToolBindingRevisionId,
    pub lifecycle: LifecycleStatus,
}
```

Rules:

- `stable_name` uses lowercase dotted segments and is unique per workspace among active tools;
- input and output JSON Schemas are compiled at revision creation;
- a tool revision is immutable after creation;
- model-visible prompt views omit binding configuration, endpoints, credentials, internal risk explanations and verification probes;
- `RemoteAgent` is always rejected with `unsupported_binding`; ADR-0003 owns remote delegation;
- `Human`, `Browser` and `SubRun` remain inactive until their later plans provide an implementation.

### Idempotency and retry declarations

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolIdempotencySupport {
    None,
    BestEffortClientKey,
    StrongClientKey,
    ExternalOperationLookup,
    StrongClientKeyAndLookup,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ToolIdempotencyPolicy {
    pub support: ToolIdempotencySupport,
    pub key_scope: ToolIdempotencyKeyScope,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ToolRetryPolicy {
    pub maximum_attempts: u16,
    pub initial_backoff_ms: u64,
    pub maximum_backoff_ms: u64,
    pub retryable_errors: std::collections::BTreeSet<ToolErrorKind>,
}
```

Activation validation is restrictive:

```text
ReadOnly
  maximum_attempts may exceed 1

IdempotentWrite
  maximum_attempts may exceed 1 only with StrongClientKey
  or StrongClientKeyAndLookup

NonIdempotentWrite / Destructive / Irreversible / ExternalCommitment
  maximum_attempts must equal 1 after dispatch
  unless reconciliation returns FailedSafeToRetry
```

No retry policy can override this matrix.

### Logical invocation and attempts

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationStatus {
    Created,
    Preparing,
    Prepared,
    AwaitingApproval,
    ReadyToCommit,
    Dispatching,
    Verifying,
    Succeeded,
    SucceededWithWarnings,
    VerificationFailed,
    Failed,
    Unknown,
    Cancelled,
    Compensated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolAttemptStatus {
    Prepared,
    Dispatching,
    Observed,
    Failed,
    Unknown,
    Cancelled,
}

pub struct ToolInvocation {
    pub id: ToolInvocationId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub tool_revision_id: ToolRevisionId,
    pub normalized_arguments: CanonicalArguments,
    pub invocation_fingerprint: OperationFingerprint,
    pub invocation_idempotency_key: String,
    pub status: ToolInvocationStatus,
    pub logical_revision: u64,
    pub active_preparation_id: Option<ToolPreparationId>,
    pub final_attempt_id: Option<ToolExecutionAttemptId>,
    pub verification_id: Option<ToolVerificationId>,
    pub reconciliation_id: Option<ToolReconciliationId>,
    pub compensation_for: Option<ToolInvocationId>,
}
```

`invocation_fingerprint` identifies the stable logical proposal. Phase-specific H2 operation fingerprints are created separately for `tool.prepare`, `tool.commit`, `tool.verify`, `tool.cancel`, `tool.reconcile` and `tool.compensate`.

### Preparation and preview

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolPreparationMode {
    None,
    LocalPreview,
    AdapterPrepare,
    AdapterPrepareRequired,
}

pub struct ToolPreview {
    pub summary: String,
    pub affected_resources: Vec<ResourceRef>,
    pub external_recipients: Vec<String>,
    pub expected_side_effects: Vec<String>,
    pub irreversible_warnings: Vec<String>,
    pub estimated_usage: BudgetEstimate,
    pub rendered_details: serde_json::Value,
}

pub struct ToolPreparation {
    pub id: ToolPreparationId,
    pub invocation_id: ToolInvocationId,
    pub input_hash: [u8; 32],
    pub preview: ToolPreview,
    pub preview_hash: [u8; 32],
    pub adapter_prepare_handle: Option<OpaqueAdapterHandle>,
    pub external_state_fence: Option<ExternalStateFence>,
    pub prepared_operation_hash: [u8; 32],
    pub expires_at: Timestamp,
}
```

`OpaqueAdapterHandle` is bounded, secret-scanned adapter data. It is never executable authority by itself. The commit operation fingerprint includes the preparation ID, `prepared_operation_hash`, `preview_hash`, selected tool binding revision, execution environment and invocation idempotency key.

### Adapter SPI

```rust
#[derive(Clone, Debug)]
pub struct ToolPrepareRequest {
    pub invocation: ToolInvocation,
    pub tool: ToolRevision,
    pub binding: ToolBindingRevision,
    pub deadline: Timestamp,
}

#[derive(Clone, Debug)]
pub struct ToolCommitRequest {
    pub invocation: ToolInvocation,
    pub preparation: ToolPreparation,
    pub attempt_id: ToolExecutionAttemptId,
    pub dispatch_idempotency_key: String,
    pub execution_environment: ToolExecutionEnvironment,
    pub deadline: Timestamp,
}

#[async_trait::async_trait]
pub trait ToolAdapterPort: Send + Sync {
    fn binding_kind(&self) -> ToolBindingKind;

    fn capabilities(&self) -> ToolAdapterCapabilities;

    async fn prepare(
        &self,
        request: ToolPrepareRequest,
    ) -> Result<ToolPreparationObservation, ToolAdapterError>;

    async fn commit(
        &self,
        request: ToolCommitRequest,
    ) -> Result<ToolExecutionObservation, ToolAdapterError>;

    async fn verify(
        &self,
        request: ToolVerifyRequest,
    ) -> Result<ToolVerificationObservation, ToolAdapterError>;

    async fn reconcile(
        &self,
        request: ToolReconcileRequest,
    ) -> Result<ToolReconciliationObservation, ToolAdapterError>;

    async fn cancel(
        &self,
        request: ToolCancelRequest,
    ) -> Result<ToolCancellationObservation, ToolAdapterError>;
}
```

An adapter may report unsupported optional phases. The runtime supplies local preview and schema verification only when the active revision permits those fallbacks. A tool requiring adapter preparation or external-state verification cannot activate against an adapter lacking those capabilities.

### Execution observation and errors

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionOutcomeKind {
    Succeeded,
    Failed,
    Unknown,
}

pub struct ToolExecutionObservation {
    pub outcome: ToolExecutionOutcomeKind,
    pub provider_operation_id: Option<String>,
    pub output: Option<serde_json::Value>,
    pub output_candidates: Vec<ToolOutputCandidate>,
    pub side_effect_evidence: Vec<ToolEvidenceRef>,
    pub actual_usage: ResourceUsageDelta,
    pub safe_error: Option<ToolFailure>,
    pub completion_may_have_occurred: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorKind {
    Transient,
    RateLimited,
    Unavailable,
    Timeout,
    AuthenticationRequired,
    InvalidConfiguration,
    InvalidArguments,
    InvalidResponse,
    OutputSchemaViolation,
    PolicyDenied,
    ApprovalRequired,
    BudgetExceeded,
    SandboxViolation,
    Cancelled,
    UnknownCompletion,
}
```

Any adapter error with `completion_may_have_occurred = true` normalizes to `UnknownCompletion` regardless of its transport-specific cause.

### Verification

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolVerificationKind {
    None,
    OutputSchema,
    Deterministic,
    ExternalState,
    AdapterAndDeterministic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolVerificationOutcome {
    Verified,
    VerifiedWithWarnings,
    Failed,
    Inconclusive,
}

pub struct ToolVerificationResult {
    pub id: ToolVerificationId,
    pub invocation_id: ToolInvocationId,
    pub outcome: ToolVerificationOutcome,
    pub checks: Vec<ToolVerificationCheck>,
    pub evidence: Vec<ToolEvidenceRef>,
    pub warnings: Vec<String>,
    pub verified_at: Timestamp,
}
```

`Inconclusive` never silently becomes success. Policy decides whether it creates `WaitingForDependency`, `Partial` or `VerificationFailed`.

### Reconciliation

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolReconciliationOutcome {
    Succeeded,
    FailedSafeToRetry,
    FailedTerminal,
    StillUnknown,
    Compensated,
}
```

`FailedSafeToRetry` is the only reconciliation result that may unlock a new attempt for a non-read-only operation. The new attempt receives a new attempt ID but retains the same logical invocation and invocation idempotency key.

### Sandbox profiles and sessions

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SandboxNetworkMode {
    DenyAll,
    AllowListedDomains,
    ProviderManaged,
    Unrestricted,
}

pub struct SandboxProfileRevision {
    pub id: SandboxProfileRevisionId,
    pub profile_id: SandboxProfileId,
    pub revision: u32,
    pub provider_kind: SandboxProviderKind,
    pub environment_class: ExecutionEnvironmentClass,
    pub image_digest: Option<String>,
    pub root_filesystem_read_only: bool,
    pub run_as_user: String,
    pub resource_limits: SandboxResourceLimits,
    pub network_ceiling: SandboxNetworkPolicy,
    pub maximum_lifetime_ms: u64,
    pub maximum_output_bytes: u64,
    pub maximum_output_files: u32,
    pub lifecycle: LifecycleStatus,
}

pub struct SandboxExecutionRequest {
    pub session_id: SandboxSessionId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub tool_invocation_id: ToolInvocationId,
    pub profile_revision_id: SandboxProfileRevisionId,
    pub argv: Vec<String>,
    pub working_directory: String,
    pub environment: std::collections::BTreeMap<String, String>,
    pub input_mounts: Vec<SandboxInputMount>,
    pub writable_paths: Vec<String>,
    pub output_paths: Vec<String>,
    pub network_policy: SandboxNetworkPolicy,
    pub deadline: Timestamp,
}
```

H4 environment values must be non-secret configuration. H8 later adds operation-bound secret delivery without changing persisted sandbox requests.

```rust
#[async_trait::async_trait]
pub trait SandboxProviderPort: Send + Sync {
    async fn create(
        &self,
        request: SandboxExecutionRequest,
    ) -> Result<SandboxSessionObservation, SandboxError>;

    async fn inspect(
        &self,
        session_id: SandboxSessionId,
    ) -> Result<SandboxSessionObservation, SandboxError>;

    async fn cancel(
        &self,
        session_id: SandboxSessionId,
    ) -> Result<SandboxSessionObservation, SandboxError>;

    async fn collect_outputs(
        &self,
        session_id: SandboxSessionId,
    ) -> Result<Vec<SandboxOutputCandidate>, SandboxError>;

    async fn cleanup(
        &self,
        session_id: SandboxSessionId,
    ) -> Result<(), SandboxError>;
}
```

---

### Task 1: Add Tool Registry identifiers, safety classifications and revision invariants

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/tool/mod.rs`
- Create: `crates/vestrace-domain/src/tool/definition.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline domain unit and property tests

**Interfaces:**
- Adds `ToolId`, `ToolRevisionId`, `ToolBindingId`, `ToolBindingRevisionId`, `ToolImportCandidateId`, `ToolInvocationId`, `ToolExecutionAttemptId`, `ToolEventId`, `ToolPreparationId`, `ToolVerificationId`, `ToolReconciliationId`, `SandboxProfileId`, `SandboxProfileRevisionId`, `SandboxSessionId`, `SandboxEventId` and `SandboxOutputCandidateId`.
- Produces `ToolSideEffectClass`, `ToolBindingKind`, `ToolIdempotencySupport`, `ToolIdempotencyPolicy`, `ToolRetryPolicy`, `ToolPreparationMode`, `ToolPreparationPolicy`, `ToolVerificationKind`, `ToolVerificationPolicy`, `ToolDataPolicy`, `ToolBindingRevision` and `ToolRevision`.

- [ ] **Step 1: Write failing tool revision invariant tests**

```rust
#[test]
fn non_idempotent_write_rejects_automatic_retries() {
    let result = ToolRevisionBuilder::test_default()
        .side_effect(ToolSideEffectClass::NonIdempotentWrite)
        .idempotency(ToolIdempotencySupport::None)
        .maximum_attempts(2)
        .build();

    assert!(matches!(result, Err(DomainError::InvalidArgument(_))));
}

#[test]
fn irreversible_tool_requires_preview_and_verification() {
    let result = ToolRevisionBuilder::test_default()
        .side_effect(ToolSideEffectClass::Irreversible)
        .preparation_mode(ToolPreparationMode::None)
        .verification_kind(ToolVerificationKind::None)
        .build();

    assert!(result.is_err());
}

#[test]
fn remote_agent_binding_is_never_executable_as_a_tool() {
    let result = ToolBindingRevision::new_test(ToolBindingKind::RemoteAgent);
    assert!(result.is_err());
}
```

Run:

```bash
cargo test -p vestrace-domain tool::definition
```

Expected: FAIL because the tool domain does not exist.

- [ ] **Step 2: Implement stable names and bounded descriptions**

`stable_name` accepts lowercase ASCII dotted segments, each 1–64 bytes and total length at most 255 bytes. Descriptions are bounded to 32 KiB. Compile input/output schemas at creation and reject schemas larger than 1 MiB.

- [ ] **Step 3: Implement the restrictive retry matrix**

Validate the normative matrix in one pure function:

```rust
pub fn validate_retry_safety(
    side_effect: ToolSideEffectClass,
    idempotency: &ToolIdempotencyPolicy,
    retry: &ToolRetryPolicy,
) -> Result<(), DomainError>;
```

Property tests assert that increasing risk or weakening idempotency never permits more retries.

- [ ] **Step 4: Implement activation compatibility**

```rust
pub fn validate_tool_adapter_compatibility(
    tool: &ToolRevision,
    adapter: &ToolAdapterCapabilities,
) -> Result<(), ToolActivationError>;
```

`AdapterPrepareRequired` requires adapter preparation; `ExternalState` requires verification/reconciliation support; credentials-required bindings remain registrable but non-executable before H8.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p vestrace-domain tool
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(tool-runtime): add versioned tool safety contracts"
```

---

### Task 2: Add invocation, preparation, verification and reconciliation state machines

**Files:**
- Create: `crates/vestrace-domain/src/tool/lifecycle.rs`
- Create: `crates/vestrace-domain/src/tool/preparation.rs`
- Create: `crates/vestrace-domain/src/tool/verification.rs`
- Create: `crates/vestrace-domain/src/tool/reconciliation.rs`
- Modify: `crates/vestrace-domain/src/tool/mod.rs`
- Test: inline unit and property tests

**Interfaces:**
- Produces `ToolInvocation`, `ToolInvocationStatus`, `ToolExecutionAttempt`, `ToolAttemptStatus`, `ToolPreview`, `ToolPreparation`, `ToolExecutionObservation`, `ToolVerificationResult`, `ToolVerificationOutcome`, `ToolReconciliationOutcome`, `ToolFailure` and transition methods.

- [ ] **Step 1: Write failing transition tests**

```rust
#[test]
fn commit_cannot_start_without_current_preparation() {
    let mut invocation = ToolInvocation::test_created();
    assert!(invocation.start_commit(None).is_err());
}

#[test]
fn unknown_cannot_retry_without_reconciliation() {
    let mut invocation = ToolInvocation::test_unknown();
    assert!(invocation.create_retry_attempt().is_err());
}

#[test]
fn verified_success_requires_matching_invocation() {
    let mut invocation = ToolInvocation::test_verifying();
    let verification = ToolVerificationResult::verified_for(ToolInvocationId::new());
    assert!(invocation.apply_verification(verification).is_err());
}
```

- [ ] **Step 2: Implement explicit transitions**

Allowed high-level path:

```text
Created
→ Preparing
→ Prepared
→ AwaitingApproval | ReadyToCommit
→ Dispatching
→ Verifying
→ Succeeded | SucceededWithWarnings | VerificationFailed
```

Alternative terminal/wait paths are `Failed`, `Unknown`, `Cancelled` and `Compensated`. Cancellation after dispatch records a cancellation request but never claims the side effect was undone.

- [ ] **Step 3: Implement preparation hashing**

Canonical bytes include:

```text
vestrace-tool-preparation/v1
workspace UUID
run UUID
step UUID
invocation UUID
tool revision UUID
canonical argument hash
preview hash
adapter prepare handle hash or nil
external state fence hash or nil
execution environment hash
expiration timestamp
```

The resulting `prepared_operation_hash` is checked again before commit.

- [ ] **Step 4: Implement reconciliation fences**

Only `FailedSafeToRetry` clears the no-retry fence. `Succeeded` transitions to verification without a second dispatch. `StillUnknown` preserves the invocation and attempt IDs.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain tool::lifecycle tool::preparation tool::verification tool::reconciliation
git add crates/vestrace-domain
git commit -m "feat(tool-runtime): add durable invocation lifecycle"
```

---

### Task 3: Define application ports, commands and deterministic test support

**Files:**
- Create: `crates/vestrace-application/src/tool_runtime/mod.rs`
- Create: `crates/vestrace-application/src/tool_runtime/ports.rs`
- Create: `crates/vestrace-application/src/tool_runtime/commands.rs`
- Create: `crates/vestrace-application/src/tool_runtime/registry.rs`
- Create: `crates/vestrace-application/src/tool_runtime/retry.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-tool-test-support/Cargo.toml`
- Create: `crates/vestrace-tool-test-support/src/lib.rs`
- Create: `crates/vestrace-tool-test-support/src/fakes.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `ToolRuntimePort`, `ToolRegistryPort`, `ToolInvocationRepositoryPort`, `ToolAdapterPort`, `ToolAdapterRegistryPort`, `ToolDeterministicVerifierPort`, `SandboxProviderPort` and `SandboxProfileRepositoryPort`.
- Produces commands `CreateToolInvocation`, `PrepareToolInvocation`, `CommitToolInvocation`, `CancelToolInvocation`, `VerifyToolInvocation`, `ReconcileToolInvocation` and `CompensateToolInvocation`.
- Produces deterministic `FakeToolAdapter`, `FakeToolRepository`, `FakeVerifier`, `FakeSandboxProvider` and `ToolFaultInjector`.

- [ ] **Step 1: Write object-safety compile tests**

```rust
fn accepts_adapter(_adapter: std::sync::Arc<dyn ToolAdapterPort>) {}
fn accepts_runtime(_runtime: std::sync::Arc<dyn ToolRuntimePort>) {}
fn accepts_sandbox(_sandbox: std::sync::Arc<dyn SandboxProviderPort>) {}
```

Run:

```bash
cargo test -p vestrace-application tool_runtime::ports
```

Expected: FAIL because the ports do not exist.

- [ ] **Step 2: Define exact repository operations**

The invocation repository includes explicit optimistic transitions:

```rust
async fn create_invocation(...);
async fn begin_preparation(...);
async fn store_preparation(...);
async fn mark_awaiting_approval(...);
async fn mark_ready_to_commit(...);
async fn create_attempt(...);
async fn mark_attempt_dispatching(...);
async fn record_observation(...);
async fn store_verification(...);
async fn mark_unknown(...);
async fn record_reconciliation(...);
async fn link_compensation(...);
async fn load_invocation(...);
```

Every logical transition carries `expected_logical_revision`. Attempt event append has its own monotonic sequence and does not change H1 `RunVersion`.

- [ ] **Step 3: Implement deterministic fault points**

```rust
pub enum ToolFaultPoint {
    AfterInvocationCreated,
    AfterPreparationStored,
    AfterApprovalChecked,
    AfterAttemptDispatchingStored,
    AfterExternalSideEffectBeforeObservation,
    AfterObservationBeforeVerification,
    AfterVerificationBeforeRunMutation,
}
```

The fault injector fires once and enables restart tests without real external systems.

- [ ] **Step 4: Implement the retry classifier**

```rust
pub fn classify_tool_retry(
    tool: &ToolRevision,
    attempt: &ToolExecutionAttempt,
    error: &ToolAdapterError,
) -> ToolRetryDisposition;
```

Dispositions are `RetrySameAttemptKey`, `Reconcile`, `FailTerminal`, `WaitForApproval`, `WaitForAuthentication` and `Cancelled`.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application tool_runtime
cargo test -p vestrace-tool-test-support
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-tool-test-support
git commit -m "feat(tool-runtime): define runtime ports and fixtures"
```

---

### Task 4: Persist the versioned Tool Registry and inactive discovery candidates

**Files:**
- Create: `migrations/0032_tool_registry_and_bindings.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/tool_runtime/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/tool_runtime/registry_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/tool_registry_persistence.rs`

**Interfaces:**
- Creates `tools`, `tool_revisions`, `tool_binding_revisions`, `tool_revision_capabilities`, `tool_import_candidates` and current-revision pointers.
- Produces PostgreSQL `ToolRegistryPort`.

- [ ] **Step 1: Write failing immutable-revision tests**

Create revision 1, revise description and binding at expected revision 1, assert revision 2 becomes current while revision 1 remains readable and unchanged. Attempt direct update/delete through the application role and assert failure.

- [ ] **Step 2: Create registry schema**

Persist normalized schemas, safety policies and bounded adapter configuration as JSONB plus content hashes. Binding configuration rejects keys matching `authorization`, `api_key`, `token`, `password`, `secret`, private-key material or configured known-secret hashes.

- [ ] **Step 3: Create inactive import candidates**

A discovery candidate stores source kind, source identity, raw schema snapshot, normalized proposal, warnings and content hash. It cannot be routed or shown to models until an explicit management command creates a reviewed Tool revision.

- [ ] **Step 4: Add activation validation**

Activation checks adapter registration and capabilities, schema compilation, retry safety, preparation/verification compatibility, fixed endpoint/path restrictions and absence of secrets.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test tool_registry_persistence

git add migrations/0032_tool_registry_and_bindings.sql crates tests/tool_registry_persistence.rs
git commit -m "feat(tool-runtime): persist versioned tool registry"
```

---

### Task 5: Persist invocations, attempts, preparations, events, verification and reconciliation

**Files:**
- Create: `migrations/0033_tool_invocations_attempts_and_events.sql`
- Create: `migrations/0034_tool_preparations_verifications_and_reconciliation.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/tool_runtime/invocation_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/tool_runtime/event_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/tool_runtime/preparation_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/tool_runtime/verification_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/tool_runtime/reconciliation_repository.rs`
- Create: `tests/tool_invocation_persistence.rs`
- Create: `tests/tool_unknown_reconciliation.rs`

**Interfaces:**
- Creates `tool_invocations`, `tool_execution_attempts`, `tool_invocation_events`, `tool_preparations`, `tool_verifications`, `tool_reconciliations`, `tool_compensation_links` and adapter-handle storage.
- Produces PostgreSQL implementations of H4 invocation ports.

- [ ] **Step 1: Write failing append-only and ownership tests**

Assert:

1. attempt numbers and event sequences are unique and contiguous;
2. tool revision, Run and step belong to the same workspace;
3. preparation, attempt, verification and reconciliation belong to one invocation;
4. application-role update/delete of canonical events, preparation snapshots and verification records fails;
5. only one active unresolved reconciliation exists per invocation;
6. compensation links cannot form cycles;
7. a terminal invocation cannot create a new attempt.

- [ ] **Step 2: Create migration `0033`**

`tool_invocations` stores exact tool/binding revisions, canonical argument hash, invocation fingerprint, idempotency key, status, logical revision and H1 references. Attempts store dispatch key, adapter kind/revision, guarded-action references, provider operation ID, timing, status, usage and safe errors. Events store canonical type/payload and monotonic sequence; raw HTTP/MCP frames are not persisted.

- [ ] **Step 3: Create migration `0034`**

Preparations store preview and hashes, opaque handle metadata, external-state fence and expiration. Verification stores check outcomes and evidence references. Reconciliation stores query identity, observations, decision and retry fence. Adapter handles are encrypted or metadata-only according to deployment storage policy and always secret-scanned.

- [ ] **Step 4: Implement atomic transitions**

Each logical transition, canonical invocation event and follow-up H1 work item are committed in one scoped transaction. An ambiguous commit result maps to `operation_unknown`; repositories never retry write transactions automatically.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test tool_invocation_persistence --test tool_unknown_reconciliation

git add migrations/0033_tool_invocations_attempts_and_events.sql \
  migrations/0034_tool_preparations_verifications_and_reconciliation.sql \
  crates tests/tool_invocation_persistence.rs tests/tool_unknown_reconciliation.rs

git commit -m "feat(tool-runtime): persist invocation lifecycle"
```

---

### Task 6: Implement prepare, preview and exact approval binding

**Files:**
- Create: `crates/vestrace-application/src/tool_runtime/prepare.rs`
- Create: `crates/vestrace-application/tests/tool_prepare.rs`
- Create: `tests/tool_approval_binding.rs`
- Modify: `crates/vestrace-application/src/tool_runtime/mod.rs`

**Interfaces:**
- Produces `ToolPreparationService::prepare` and `ToolCommitAuthorizationBuilder`.
- Consumes Tool Registry, H2 policy evaluation/Action Guard, budget estimation, adapter registry and invocation repository.

- [ ] **Step 1: Write the local-preview preparation test**

```rust
#[tokio::test]
async fn destructive_tool_stops_after_preview_without_approval() {
    let fixture = ToolFixture::destructive_local_preview();
    let result = fixture.prepare().await.unwrap();

    assert_eq!(result.status, ToolInvocationStatus::AwaitingApproval);
    assert_eq!(fixture.adapter.commit_count(), 0);
    assert!(result.preparation.preview.irreversible_warnings.len() > 0);
}
```

- [ ] **Step 2: Write the stale-preview binding test**

Prepare operation A, create an approval for A, then revise the arguments or regenerate a different preview. `commit` must return `ApprovalRequired` because the commit fingerprint no longer matches.

- [ ] **Step 3: Implement fail-closed preparation order**

```text
load immutable tool and binding revisions
→ validate canonical arguments against input schema
→ create logical invocation
→ ActionGuard prepare for tool.prepare
→ consume preparation ticket
→ reserve preparation budget
→ local or adapter preparation
→ normalize and hash preview
→ store preparation and event
→ construct exact tool.commit authorization request
→ ReadyToCommit or AwaitingApproval
```

The adapter is not called when argument validation, policy or preparation budget fails.

- [ ] **Step 4: Build exact commit fingerprint arguments**

Canonical arguments contain stable references and hashes only:

```text
invocation ID
tool revision ID
binding revision ID
canonical argument hash
preparation ID
prepared operation hash
preview hash
external state fence hash or nil
execution environment hash
invocation idempotency key
```

Raw secrets and full preview prose are not copied to policy-decision rows.

- [ ] **Step 5: Validate approval use through H2**

`ToolPreparationService` never interprets grants itself. It supplies grant IDs to `ActionGuardService`, which verifies operation binding, validity, uses and policy snapshot before issuing `GuardedAction` for commit.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test tool_prepare
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test tool_approval_binding

git add crates/vestrace-application tests/tool_approval_binding.rs
git commit -m "feat(tool-runtime): prepare and bind tool approvals"
```

---

### Task 7: Implement commit, retry fencing, Unknown and reconciliation

**Files:**
- Create: `crates/vestrace-application/src/tool_runtime/commit.rs`
- Create: `crates/vestrace-application/src/tool_runtime/reconcile.rs`
- Create: `crates/vestrace-application/tests/tool_commit.rs`
- Create: `crates/vestrace-application/tests/tool_retry.rs`
- Create: `crates/vestrace-application/tests/tool_reconciliation.rs`

**Interfaces:**
- Produces `ToolCommitService`, `ToolReconciliationService` and `ToolRetryController`.

- [ ] **Step 1: Write dispatch-order tests**

Assert this sequence:

```text
load current preparation
→ rebuild and compare prepared-operation hash
→ ActionGuard prepare for tool.commit
→ consume exact guarded action and approval use
→ reserve execution resources
→ create attempt
→ persist Dispatching
→ call adapter
→ persist typed observation
→ reconcile actual usage
→ schedule verification or reconciliation
```

The fake adapter must observe that the guarded action was consumed before its first commit call.

- [ ] **Step 2: Write the lost-response test**

The adapter records one external side effect and returns `UnknownCompletion`. Assert:

- exactly one adapter commit call;
- attempt and invocation become `Unknown`;
- a reconciliation record/work item exists;
- no second attempt is created after worker restart;
- H2 reservation remains reconciled conservatively rather than silently released.

- [ ] **Step 3: Write retry matrix tests**

Cover:

```text
ReadOnly transient before response
  → retry permitted

IdempotentWrite + StrongClientKey transient
  → retry same dispatch key

IdempotentWrite + BestEffortClientKey ambiguous response
  → reconciliation

NonIdempotentWrite after Dispatching
  → reconciliation

Irreversible timeout
  → Unknown, never retry

FailedSafeToRetry reconciliation
  → explicit new attempt allowed
```

- [ ] **Step 4: Implement conservative dispatch fencing**

Once `Dispatching` is durably stored, process death is treated as potentially dispatched unless the adapter can prove no request left the process. Read-only and strong-idempotency retries reuse the same dispatch key.

- [ ] **Step 5: Implement reconciliation outcomes**

`Succeeded` stores the recovered observation and schedules verification. `FailedSafeToRetry` clears the fence but does not automatically dispatch; it enqueues an explicit retry work item under policy and remaining budget. `StillUnknown` reschedules with bounded backoff. `Compensated` links the compensation evidence.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application \
  --test tool_commit --test tool_retry --test tool_reconciliation

git add crates/vestrace-application
git commit -m "feat(tool-runtime): fence commits and reconcile unknowns"
```

---

### Task 8: Implement verification, compensation links and H3 model-result projection

**Files:**
- Create: `crates/vestrace-application/src/tool_runtime/verify.rs`
- Create: `crates/vestrace-application/src/tool_runtime/projection.rs`
- Create: `crates/vestrace-application/tests/tool_verification.rs`
- Modify: `crates/vestrace-application/src/model_runtime/loop_port.rs`

**Interfaces:**
- Produces `ToolVerificationService` and `ToolModelResultProjector`.
- Converts a terminal or waiting Tool invocation into H3 `ModelToolResultView` without exposing authoritative policy fields to the model.

- [ ] **Step 1: Write verification outcome tests**

```rust
#[tokio::test]
async fn committed_side_effect_is_not_success_when_external_check_fails() {
    let fixture = ToolFixture::committed_with_failed_external_verification();
    let result = fixture.verify().await.unwrap();

    assert_eq!(result.status, ToolInvocationStatus::VerificationFailed);
    assert_eq!(result.verification.outcome, ToolVerificationOutcome::Failed);
}
```

Add schema violation, deterministic success, adapter+deterministic disagreement and inconclusive tests.

- [ ] **Step 2: Implement verification order**

```text
typed execution observation
→ output JSON Schema
→ deterministic verifier
→ optional adapter/external-state verifier
→ evidence normalization
→ final verification outcome
```

Adapter claims cannot override failed deterministic checks.

- [ ] **Step 3: Implement model-safe projections**

The model projection contains call ID, safe status, bounded presentation text and approved output. It excludes authorization tickets, grant IDs, policy explanations, credential references, sandbox handles and raw external metadata.

- [ ] **Step 4: Implement compensation relationships**

A compensation command creates a new Tool invocation with `compensation_for`. It receives independent policy, approval and budget checks. Completing compensation changes the original status to `Compensated` only when verification evidence confirms the intended compensating state.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test tool_verification
cargo test -p vestrace-application model_runtime::loop_port

git add crates/vestrace-application
git commit -m "feat(tool-runtime): verify outcomes and project results"
```

---

### Task 9: Implement native and HTTP tool adapters with a shared conformance suite

**Files:**
- Create: `crates/vestrace-tool-adapter-native/Cargo.toml`
- Create: `crates/vestrace-tool-adapter-native/src/lib.rs`
- Create: `crates/vestrace-tool-adapter-native/src/registry.rs`
- Create: `crates/vestrace-tool-adapter-native/src/adapter.rs`
- Create: `crates/vestrace-tool-adapter-http/Cargo.toml`
- Create: `crates/vestrace-tool-adapter-http/src/lib.rs`
- Create: `crates/vestrace-tool-adapter-http/src/adapter.rs`
- Create: `crates/vestrace-tool-adapter-http/src/auth.rs`
- Create: `crates/vestrace-tool-adapter-http/src/config.rs`
- Create: `crates/vestrace-tool-adapter-http/src/request.rs`
- Create: `crates/vestrace-tool-adapter-http/src/response.rs`
- Create: `crates/vestrace-tool-adapter-http/src/error.rs`
- Create: `crates/vestrace-tool-adapter-http/src/reconcile.rs`
- Create: `crates/vestrace-tool-test-support/src/http_server.rs`
- Create: `crates/vestrace-tool-test-support/src/conformance.rs`
- Create: `tests/tool_adapter_native_conformance.rs`
- Create: `tests/tool_adapter_http_conformance.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `NativeToolAdapter`, `NativeToolHandlerRegistry`, `HttpToolAdapter` and `ToolAdapterConformanceSuite`.

- [ ] **Step 1: Define unchanged conformance cases**

The suite covers:

```text
input mapping and schema preservation
local or adapter preparation
preview normalization
stable dispatch idempotency key
successful commit
output schema violation
timeout before dispatch
lost response / Unknown
cancellation
verification
reconciliation
response and log size limits
secret redaction
unsupported phase fails before dispatch
```

- [ ] **Step 2: Implement the native registry**

Native handlers are registered by trusted composition code against exact binding revision IDs. Model content cannot register or replace handlers. Handler inputs/outputs are canonical Vestrace values and panic is converted to a terminal adapter failure without unwinding across the runtime boundary.

- [ ] **Step 3: Implement fixed-origin HTTP configuration**

```rust
pub struct HttpToolBindingConfig {
    pub base_url: url::Url,
    pub method: http::Method,
    pub path_template: String,
    pub request_timeout_ms: u64,
    pub maximum_response_bytes: u64,
    pub allowed_response_media_types: Vec<String>,
    pub idempotency_header_name: Option<http::HeaderName>,
    pub reconciliation_path_template: Option<String>,
    pub requires_connection: bool,
}
```

Arguments may fill validated path/query/body fields but cannot alter scheme, host, port or base path. Redirects are disabled. URL credentials, fragments, IP-literal loopback/private destinations and reserved auth headers are rejected unless deployment policy explicitly marks the binding local and trusted.

- [ ] **Step 4: Define the future H8 auth hook safely**

`HttpToolAuthDecorator` is adapter-infrastructure SPI. H4 ships `NoHttpToolAuth`, which permits only `requires_connection = false`. It returns `AuthenticationRequired` otherwise. The decorator uses redacted debug and never returns credential material to application code.

- [ ] **Step 5: Implement response and error normalization**

Apply response byte, media-type and timeout limits. Raw confidential error bodies are not returned. Connection loss after request dispatch maps to `UnknownCompletion` for non-read-only operations unless strong external idempotency and safe retry are both declared.

- [ ] **Step 6: Run conformance and commit**

```bash
cargo test --test tool_adapter_native_conformance --test tool_adapter_http_conformance
cargo clippy -p vestrace-tool-adapter-native -p vestrace-tool-adapter-http \
  --all-targets -- -D warnings

git add Cargo.toml Cargo.lock crates/vestrace-tool-adapter-native \
  crates/vestrace-tool-adapter-http crates/vestrace-tool-test-support tests

git commit -m "feat(tool-adapters): add native and HTTP execution"
```

---

### Task 10: Implement the MCP adapter and untrusted discovery import

**Files:**
- Create: `crates/vestrace-tool-adapter-mcp/Cargo.toml`
- Create: `crates/vestrace-tool-adapter-mcp/src/lib.rs`
- Create: `crates/vestrace-tool-adapter-mcp/src/adapter.rs`
- Create: `crates/vestrace-tool-adapter-mcp/src/auth.rs`
- Create: `crates/vestrace-tool-adapter-mcp/src/client.rs`
- Create: `crates/vestrace-tool-adapter-mcp/src/discovery.rs`
- Create: `crates/vestrace-tool-adapter-mcp/src/normalization.rs`
- Create: `crates/vestrace-tool-adapter-mcp/src/error.rs`
- Create: `crates/vestrace-tool-test-support/src/mcp_server.rs`
- Create: `tests/tool_adapter_mcp_conformance.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `McpToolAdapter`, `McpToolDiscoveryImporter` and `NoMcpToolAuth`.
- Uses the same `ToolAdapterPort` and unchanged conformance assertions.

- [ ] **Step 1: Write malicious-discovery tests**

A fixture MCP server advertises a tool named `delete_everything` while claiming it is read-only and injecting instructions into its description. Import must create an inactive candidate, preserve the source snapshot, classify annotations as untrusted and never activate or lower risk automatically.

- [ ] **Step 2: Implement fixed server bindings**

MCP server identity and transport are configured by trusted binding revision. Tool arguments cannot select another server, executable, socket or URL. H4 supports configured stdio and Streamable HTTP clients only where the deployment profile permits them.

- [ ] **Step 3: Normalize calls and results**

Map canonical arguments to one exact MCP tool name. Normalize text, structured content, resources and errors into bounded Vestrace values. Server-supplied resource URLs are output candidates and receive the same trust treatment as external artifacts.

- [ ] **Step 4: Implement auth hook behavior**

`NoMcpToolAuth` permits unauthenticated fixtures only. Credential-required bindings return `AuthenticationRequired` before transport creation; H8 later supplies a request-scoped decorator.

- [ ] **Step 5: Run unchanged conformance tests**

```bash
cargo test --test tool_adapter_mcp_conformance
cargo clippy -p vestrace-tool-adapter-mcp --all-targets -- -D warnings
```

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/vestrace-tool-adapter-mcp \
  crates/vestrace-tool-test-support tests/tool_adapter_mcp_conformance.rs

git commit -m "feat(tool-adapters): add MCP execution and discovery"
```

---

### Task 11: Add sandbox domain services, profiles and durable sessions

**Files:**
- Create: `crates/vestrace-domain/src/sandbox/mod.rs`
- Create: `crates/vestrace-domain/src/sandbox/profile.rs`
- Create: `crates/vestrace-domain/src/sandbox/session.rs`
- Create: `crates/vestrace-domain/src/sandbox/network.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Create: `crates/vestrace-application/src/sandbox/mod.rs`
- Create: `crates/vestrace-application/src/sandbox/ports.rs`
- Create: `crates/vestrace-application/src/sandbox/service.rs`
- Create: `crates/vestrace-application/src/sandbox/output.rs`
- Create: `crates/vestrace-application/tests/sandbox_service.rs`
- Create: `migrations/0035_sandbox_profiles_sessions_and_events.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/sandbox/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/sandbox/profile_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/sandbox/session_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/sandbox/event_repository.rs`
- Create: `tests/sandbox_profile_persistence.rs`

**Interfaces:**
- Produces the sandbox contracts above, `SandboxService`, profile/session repositories and `SandboxCommandToolAdapter` integration point.

- [ ] **Step 1: Write failing profile safety tests**

Reject:

```text
mutable image tag without resolved digest
root filesystem writable for IsolatedContainer
run_as_user = root or UID 0
Unrestricted network mode
absolute/parent-traversing output path
writable input mount
zero or excessive resource limits
blank argv
shell-string command mode
```

- [ ] **Step 2: Implement profile and request validation**

Paths are normalized as POSIX relative paths. Environment variable names use `[A-Z_][A-Z0-9_]*`; values are bounded and secret-scanned. Reserved credential/environment names are rejected in H4.

- [ ] **Step 3: Create migration `0035`**

Create immutable `sandbox_profiles`, `sandbox_profile_revisions`, `sandbox_sessions`, `sandbox_session_events`, `sandbox_mounts` and `sandbox_output_candidates`. Persist provider handles as opaque bounded metadata, not host paths or Docker credentials.

- [ ] **Step 4: Implement `SandboxService` order**

```text
load active profile
→ check H2 execution-environment obligation
→ validate request below profile ceilings
→ create durable session
→ call provider create
→ persist observation/events
→ inspect or reconcile after restart
→ collect staged output candidates
→ schedule cleanup
```

- [ ] **Step 5: Implement output candidate validation independent of H6**

H4 records staging reference, relative path, media type, byte size and SHA-256. It does not create final `Artifact` records. H6 later ingests candidates through quarantine without changing sandbox execution semantics.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain sandbox
cargo test -p vestrace-application --test sandbox_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test sandbox_profile_persistence

git add migrations/0035_sandbox_profiles_sessions_and_events.sql crates tests
git commit -m "feat(sandbox): add profiles sessions and service"
```

---

### Task 12: Implement the Docker sandbox, controlled egress and output collection

**Files:**
- Create: `crates/vestrace-sandbox-docker/Cargo.toml`
- Create: `crates/vestrace-sandbox-docker/src/lib.rs`
- Create: `crates/vestrace-sandbox-docker/src/provider.rs`
- Create: `crates/vestrace-sandbox-docker/src/client.rs`
- Create: `crates/vestrace-sandbox-docker/src/config.rs`
- Create: `crates/vestrace-sandbox-docker/src/image.rs`
- Create: `crates/vestrace-sandbox-docker/src/mounts.rs`
- Create: `crates/vestrace-sandbox-docker/src/network.rs`
- Create: `crates/vestrace-sandbox-docker/src/output.rs`
- Create: `crates/vestrace-sandbox-docker/src/security.rs`
- Create: `crates/vestrace-sandbox-docker/src/reconcile.rs`
- Create: `crates/vestrace-sandbox-egress-proxy/Cargo.toml`
- Create: `crates/vestrace-sandbox-egress-proxy/src/lib.rs`
- Create: `crates/vestrace-sandbox-egress-proxy/src/policy.rs`
- Create: `crates/vestrace-sandbox-egress-proxy/src/resolver.rs`
- Create: `crates/vestrace-sandbox-egress-proxy/src/proxy.rs`
- Create: `crates/vestrace-sandbox-egress-proxy/src/audit.rs`
- Create: `crates/vestrace-tool-test-support/src/sandbox_fixtures.rs`
- Create: `tests/sandbox_docker_isolation.rs`
- Create: `tests/sandbox_network_policy.rs`
- Create: `tests/sandbox_output_collection.rs`
- Create: `scripts/verify-sandbox-security.sh`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `DockerSandboxProvider` implementing `SandboxProviderPort` and `SandboxEgressGateway` for allowlisted HTTP/HTTPS destinations.

- [ ] **Step 1: Implement the mandatory Docker security specification test**

Before starting a container, assert the generated specification contains:

```text
read-only root filesystem
non-root user
capabilities dropped ALL
no-new-privileges
no privileged mode
no host PID/IPC/network namespace
no Docker socket or host device mounts
seccomp enabled
PID, CPU, memory and storage limits
timeout and kill grace period
read-only input mounts
explicit writable workspace/tmpfs
```

- [ ] **Step 2: Implement digest-pinned image resolution**

Activation or trusted administration resolves a tag to a digest; execution accepts only the digest. Pull policy and registry access are deployment configuration, not model input.

- [ ] **Step 3: Implement secure mount staging**

Stage authorized input references into a per-session directory owned by the sandbox broker. Reject symlinks, hard links crossing the staging root, FIFOs, sockets, block/character devices and path collisions. The workload receives read-only bind mounts only.

- [ ] **Step 4: Implement `DenyAll` networking**

Use an isolated no-egress network or Docker `none`. Tests attempt DNS, loopback host access, metadata-service addresses and public HTTP; all fail while local process execution succeeds.

- [ ] **Step 5: Implement allowlisted egress through a gateway**

The workload container joins an internal network with only the egress gateway. The gateway joins a second outbound network. It:

- accepts only configured HTTP/HTTPS or CONNECT ports;
- rejects IP literals unless explicitly allowed by deployment policy;
- normalizes IDNA hostnames;
- requires exact or approved wildcard domain match;
- resolves DNS itself and rejects loopback, link-local, private, multicast, documentation and reserved ranges;
- pins the approved resolved IP for the connection to reduce DNS rebinding;
- applies request, byte and time ceilings;
- emits metadata-only audit records.

No workload route bypasses the gateway.

- [ ] **Step 6: Implement output collection**

Walk only declared output paths under the writable workspace using descriptor-relative operations. Reject traversal and link escape. Enforce per-file, total-byte and file-count ceilings. Hash content while copying to staging and record immutable candidates.

- [ ] **Step 7: Implement restart reconciliation and cleanup**

Docker labels include workspace/session/invocation IDs and no secrets. `inspect` locates a matching container, validates labels and records running/exited/unknown state. Cleanup is idempotent and removes container, internal network, staged files and gateway state while preserving durable metadata.

- [ ] **Step 8: Run container gates and commit**

```bash
bash scripts/verify-sandbox-security.sh
cargo test --test sandbox_docker_isolation \
  --test sandbox_network_policy --test sandbox_output_collection
cargo clippy -p vestrace-sandbox-docker -p vestrace-sandbox-egress-proxy \
  --all-targets -- -D warnings

git add Cargo.toml Cargo.lock crates/vestrace-sandbox-docker \
  crates/vestrace-sandbox-egress-proxy crates/vestrace-tool-test-support \
  tests scripts/verify-sandbox-security.sh

git commit -m "feat(sandbox): add isolated Docker execution"
```

---

### Task 13: Integrate restart-safe Tool work with H1 Run state, RLS and queues

**Files:**
- Create: `migrations/0036_tool_sandbox_rls_indexes_and_run_bindings.sql`
- Modify: `crates/vestrace-domain/src/run/work.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Create: `crates/vestrace-application/src/tool_runtime/worker.rs`
- Create: `crates/vestrace-application/tests/tool_worker.rs`
- Modify: `crates/vestrace-application/src/run/worker.rs`
- Create: `tests/tool_runtime_restart.rs`
- Create: `tests/tool_runtime_rls.rs`

**Interfaces:**
- Adds work kinds `PrepareTool`, `CommitTool`, `VerifyTool`, `ReconcileTool` and `CleanupSandbox`.
- Adds canonical H1 Run events referencing Tool invocation lifecycle.
- Produces `ToolRuntimeWorkHandler`.

- [ ] **Step 1: Add Run event payloads**

```rust
ToolInvocationCreated { invocation_id: ToolInvocationId },
ToolInvocationPrepared { invocation_id: ToolInvocationId, preparation_id: ToolPreparationId },
ToolInvocationWaitingApproval { invocation_id: ToolInvocationId },
ToolInvocationCommitted { invocation_id: ToolInvocationId, attempt_id: ToolExecutionAttemptId },
ToolInvocationVerified { invocation_id: ToolInvocationId, verification_id: ToolVerificationId },
ToolInvocationUnknown { invocation_id: ToolInvocationId, reconciliation_id: ToolReconciliationId },
ToolInvocationFailed { invocation_id: ToolInvocationId, failure_code: String },
ToolInvocationCancelled { invocation_id: ToolInvocationId },
```

Each payload is the sole event of its H1 logical mutation. Adapter frames, logs and sandbox telemetry remain H4 events.

- [ ] **Step 2: Write restart-window tests**

Cover crashes:

```text
after preparation before approval wait
→ resume same preparation

after approval consumed before Dispatching
→ no second approval use

after Dispatching before adapter response
→ retry only when safety matrix permits, otherwise Unknown

after external effect before observation
→ reconciliation, no duplicate effect

after observation before verification
→ verify without redispatch

after verification before Run mutation
→ replay stored verification and emit one Run event
```

- [ ] **Step 3: Implement worker fencing**

The worker acquires H1 Run lease and checks generation before every logical Run mutation. Invocation optimistic revision, attempt state and work-item lease prevent stale workers from recording terminal outcomes.

- [ ] **Step 4: Create migration `0036`**

Force RLS on all H4 tables; validate same-workspace Run, step, tool, binding, sandbox and policy references; add active-status/deadline indexes; extend work-item kind/payload checks; add append-only triggers for canonical events and immutable snapshots.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test tool_worker
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test tool_runtime_restart --test tool_runtime_rls

git add migrations/0036_tool_sandbox_rls_indexes_and_run_bindings.sql crates tests
git commit -m "feat(tool-runtime): integrate restart-safe Run work"
```

---

### Task 14: Add boundary verification, complete conformance and the H4 acceptance scenario

**Files:**
- Create: `scripts/verify-tool-adapter-boundary.sh`
- Modify: `.github/workflows/ci.yml`
- Create: `tests/h4_acceptance.rs`
- Modify: adapter, Run-event and public schema snapshots where the repository stores them

**Interfaces:**
- Produces mandatory tool-adapter, PostgreSQL and Docker CI gates.
- Produces the H4 exit-gate scenario.

- [ ] **Step 1: Add adapter-boundary checks**

The script fails when Reqwest, RMCP or Docker SDK types appear in domain/application public signatures or when adapter crates are dependencies of the domain crate. It also rejects `a2a` dependencies in Tool Runtime crates.

- [ ] **Step 2: Add CI jobs**

Required jobs:

```text
tool-domain-and-application
tool-adapter-native-conformance
tool-adapter-http-conformance
tool-adapter-mcp-conformance
postgres-tool-runtime
docker-sandbox-linux
h4-acceptance
```

All network-facing tests use loopback fixtures. The Docker job uses disposable images built from repository fixtures and no registry credential.

- [ ] **Step 3: Write the mandatory H4 acceptance scenario**

Register:

```text
read.catalogue
  ReadOnly HTTP tool
  safe transient retry

commit.transfer
  ExternalCommitment tool
  AdapterPrepareRequired
  exact approval
  external-state verification
  reconciliation lookup

sandbox.transform
  SandboxCommand tool
  Docker profile with DenyAll network
  one read-only input mount
  one declared output path
```

Execute and assert:

1. the model proposal does not execute anything;
2. `read.catalogue` retries safely after a pre-response transient failure;
3. `commit.transfer` produces a preview and cannot commit with a missing, stale or altered approval;
4. the approved commit consumes one exact ticket and one approval use;
5. the fixture accepts the transfer but drops the response;
6. restart does not create a second transfer;
7. reconciliation finds the original operation and verification confirms external state;
8. `sandbox.transform` cannot access the network, host filesystem, undelegated input or Docker socket;
9. only the declared output is staged with hash and provenance candidate;
10. Run events contain logical preparation/wait/commit/verify transitions but no raw transport or log noise;
11. H2 budgets reconcile actual tool, network, CPU, memory and storage usage;
12. no secret or authorization material appears in persisted configuration, events, logs or model result projections.

- [ ] **Step 4: Add negative trust tests**

A malicious MCP server advertises lower risk and returns prompt-injection text. The activated local tool revision retains its configured risk; output remains untrusted data; no policy, tool registry, approval or subsequent tool visibility changes.

- [ ] **Step 5: Run all H4 gates**

```bash
bash scripts/verify-tool-adapter-boundary.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h4_acceptance --test tool_runtime_restart
bash scripts/verify-sandbox-security.sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: all commands exit `0` in the supported Linux CI environment.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(tool-runtime): add H4 acceptance and safety gates"
```

---

## H4 completion definition

H4 is complete only when all fourteen tasks pass and evidence demonstrates:

```text
model tool proposal
→ immutable Tool revision lookup
→ canonical argument validation
→ policy-governed preparation
→ deterministic or adapter preview
→ exact approval binding
→ consumed commit ticket
→ durably fenced dispatch
→ typed observation
→ verification or Unknown reconciliation
→ model-safe result projection
→ H1 Run continuation
```

The completed implementation must satisfy all of these statements:

1. A model cannot execute a tool directly or alter its risk, side-effect, retry or binding classification.
2. A destructive, irreversible or external-commitment action cannot commit without its exact approved preparation when policy requires approval.
3. A changed argument, preview, binding, environment or external-state fence invalidates prior commit authority.
4. Read-only and strongly idempotent retries reuse stable dispatch identity; unsafe writes never retry blindly.
5. An ambiguous response creates durable `Unknown` and reconciliation rather than a duplicate side effect.
6. Verification is separate from execution observation and required verification cannot be skipped.
7. Compensation is independently authorized and verified.
8. Native, HTTP and MCP adapters pass one canonical conformance suite.
9. MCP discovery creates inactive untrusted candidates and never silently activates tools.
10. Credentials are absent from Tool definitions, binding configuration, durable requests and model-visible results; credential-required tools wait for H8.
11. Docker workloads run non-root with read-only root filesystems, dropped privileges, bounded resources and no direct external route.
12. Allowlisted network traffic can only pass through the controlled egress gateway.
13. Sandbox inputs are read-only and outputs cannot escape declared paths or limits.
14. Worker restart at every critical window does not duplicate writes or lose a known committed observation.
15. H1 replay never performs adapter or sandbox I/O.
16. A2A remote agents remain outside Tool Runtime.
17. H5 can consume terminal Tool results and verification evidence without changing H4 adapter contracts.
18. H6 can ingest sandbox/external output candidates through quarantine without changing execution history.
19. H8 can add request-scoped credential decorators without placing secret values in H4 domain or persistence contracts.
20. H4 tests require no public service or permanent credential.

## Explicit non-goals

H4 does not implement:

- remote-agent delegation or A2A transport;
- browser automation;
- internal SubRun creation;
- human communication channels or approval UI;
- OAuth/API-key acquisition and the Credential Broker;
- final Artifact ingestion, representation generation or export;
- workflow planning and delegation;
- extension/package installation;
- unrestricted sandbox networking;
- Kubernetes or remote sandbox scheduling;
- universal rollback of external actions;
- production replay of write actions.

These remain assigned to H5–H9A and later plans.

## Documentation-only boundary

Creating this document does not authorize implementation. During the current documentation phase, do not:

- create `feat/h4-tool-runtime-sandbox`;
- change Cargo dependencies or workspace members;
- create migrations `0032`–`0036`;
- start Docker containers or network proxies;
- create adapter or sandbox crates;
- modify CI;
- execute conformance or acceptance tests.
