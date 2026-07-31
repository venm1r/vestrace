# Vestrace H0-RIG Agent Runtime Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to execute this spike task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Approved planning artifact only. Do not execute this spike, create a runtime branch, change dependencies, run experiments or add production code until the user explicitly ends the documentation-only phase.

**Goal:** Determine, with reproducible evidence, whether `rig-agent` can serve as Vestrace's replaceable bounded model↔tool inner-loop engine without owning durable Run state, authorization, budgets, tool execution, recovery or public contracts.

**Architecture:** Build one isolated experimental crate that hand-drives Rig's sans-I/O `AgentRun` through Vestrace-owned spike contracts and deterministic fakes. The spike never calls a real model, tool, credential service or network endpoint. Rig state may be stored as an optional engine checkpoint, but the canonical Vestrace journal, pending-operation ledger and H1/H2 state remain authoritative. The spike ends in an evidence report and one decision ADR: `Accepted`, `AcceptedWithRestrictions` or `Rejected`.

**Tech Stack:** Existing Vestrace v0.1, H1 and H2 contracts; Rust Edition 2024; Tokio; Serde; Schemars; SHA-256; `rig-agent = "=0.41.0"`; `rig-core = "=0.41.0"`; deterministic test doubles; no PostgreSQL migration and no external network.

## Global Constraints

- Complete all five Vestrace v0.1 plans, H1 Durable Run Core and H2 Policy, Approval and Budget Core before executing H0-RIG.
- ADR source: `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`.
- Roadmap source: `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md`.
- Baseline reviewed for this plan: Rig workspace version `0.41.0`, source commit `6cfae6d829da21f9dc9e775e065fee157b264f7e`.
- Pin Rig exactly for the baseline experiment. Do not use a floating semver range or unpinned Git dependency.
- The spike crate is experimental. No production crate may depend on it.
- Rig types may appear only inside `vestrace-rig-spike`.
- No Rig type may appear in `vestrace-domain`, `vestrace-application`, database schemas, public APIs, events, MCP schemas, extension protocols or persisted Vestrace checkpoints.
- Rig never receives permanent credentials, credential leases, raw authorization grants or direct external-tool implementations.
- Every executable tool proposal passes through a Vestrace-owned `ActionGuardPort` and `ToolExecutionPort` fake matching H2/H4 semantics.
- Denial, approval requirement, budget exhaustion and `Unknown` completion are Vestrace states, not generic Rig exceptions.
- A serialized Rig `AgentRun` is optional acceleration state and never the sole recovery source.
- The baseline experiment uses no real provider, no API key, no MCP server, no Docker daemon and no internet access.
- Streaming and non-streaming tests compare final canonical durable events. Provisional streaming deltas are allowed to differ.
- A failed critical gate results in `Rejected`; it cannot be waived by subjective convenience.
- The spike produces no migration and does not alter H1/H2 domain semantics.
- Future execution branch: `spike/h0-rig-agent-runtime`.

---

## Upstream Rig facts locked for the spike

The reviewed Rig baseline exposes a hand-driven state machine with three driver-visible steps:

```text
CallModel
CallTools
Done
```

The state machine owns turn counting, tool-call validation, invalid-call recovery, accumulated history, usage aggregation and response construction, but performs no I/O. It is serializable, and a resumed state re-emits pending tool calls. The upstream documentation explicitly states that serialized state contains accumulated conversation data and carries no cross-version compatibility guarantee.

The spike therefore tests two recovery paths:

```text
Fast path:
  same-version Rig engine checkpoint

Authoritative path:
  Vestrace journal + pending-operation ledger
  → reconstruct bounded loop input
  → continue or fail safely
```

The upstream durable-approval example is treated only as evidence that a hand-driven pause is possible. Its direct `ToolSet::execute` pattern is forbidden in Vestrace; the spike replaces it with Vestrace-owned ports.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock

crates/vestrace-rig-spike/
  Cargo.toml
  README.md
  src/lib.rs
  src/contracts.rs
  src/driver.rs
  src/rig_adapter.rs
  src/checkpoint.rs
  src/normalization.rs
  src/fakes.rs
  src/faults.rs
  tests/pause_before_tool.rs
  tests/policy_and_budget_gate.rs
  tests/checkpoint_resume.rs
  tests/side_effect_idempotency.rs
  tests/unknown_reconciliation.rs
  tests/streaming_equivalence.rs
  tests/normalization.rs
  tests/type_boundary.rs
  tests/checkpoint_compatibility.rs

scripts/
  verify-rig-boundary.sh
  measure-rig-upgrade-scope.sh

docs/superpowers/evaluations/
  2026-07-31-h0-rig-spike-report.md

docs/superpowers/specs/adr/
  0002-rig-agent-runtime-spike-outcome.md
```

No file under `migrations/` is created or modified.

---

## Normative spike contracts

These contracts are private to the experimental crate. They model the intended H3 boundary but do not become production API until the outcome ADR approves them.

### Loop command and effect

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SpikeLoopStart {
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub prompt: CanonicalMessage,
    pub history: Vec<CanonicalMessage>,
    pub advertised_tools: Vec<CanonicalToolDefinition>,
    pub maximum_model_turns: u32,
    pub journal_cursor: ResumeCursor,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub enum SpikeLoopCommand {
    Start(SpikeLoopStart),
    SupplyModelResult(CanonicalModelResult),
    SupplyToolResults(Vec<CanonicalToolResult>),
    SupplyReconciliation(Vec<ReconciliationResult>),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub enum SpikeLoopEffect {
    InvokeModel(CanonicalModelRequest),
    ToolCallsProposed(Vec<CanonicalToolCall>),
    WaitingForReconciliation(Vec<PendingOperation>),
    Completed(CanonicalLoopOutput),
    Failed(CanonicalLoopFailure),
}

pub trait SpikeModelLoopPort {
    fn apply(
        &mut self,
        command: SpikeLoopCommand,
    ) -> Result<SpikeLoopEffect, SpikeAdapterError>;

    fn checkpoint(&self, cursor: ResumeCursor) -> Result<EngineCheckpoint, SpikeAdapterError>;
}
```

`SpikeModelLoopEffect::ToolCallsProposed` is a proposal boundary. Returning it must not execute a tool.

### Canonical tool proposal

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CanonicalToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub normalized_arguments: serde_json::Value,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub enum CanonicalToolStatus {
    Succeeded,
    Denied,
    ApprovalRequired,
    BudgetExceeded,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CanonicalToolResult {
    pub call_id: String,
    pub status: CanonicalToolStatus,
    pub model_presentation: String,
    pub operation_id: Option<String>,
    pub output: Option<serde_json::Value>,
}
```

`model_presentation` is untrusted model-visible text. `status`, `operation_id` and authoritative output remain typed Vestrace data.

### Authorization and execution ports

```rust
pub enum SpikeGuardDecision {
    Permit { authorization_ticket_id: AuthorizationTicketId },
    Deny { reason_code: String },
    RequireApproval { request_id: ApprovalRequestId },
    BudgetExceeded { dimension: String },
}

#[async_trait::async_trait]
pub trait SpikeActionGuardPort: Send + Sync {
    async fn authorize_tool_call(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        step_id: RunStepId,
        call: &CanonicalToolCall,
    ) -> Result<SpikeGuardDecision, ApplicationError>;
}

#[async_trait::async_trait]
pub trait SpikeToolExecutionPort: Send + Sync {
    async fn execute(
        &self,
        context: &RequestContext,
        ticket_id: AuthorizationTicketId,
        call: &CanonicalToolCall,
    ) -> Result<ExecutionObservation, ApplicationError>;

    async fn reconcile(
        &self,
        context: &RequestContext,
        pending: &PendingOperation,
    ) -> Result<ReconciliationResult, ApplicationError>;
}
```

Rig cannot call either port directly. The Vestrace spike driver invokes them after receiving `ToolCallsProposed`.

### Engine checkpoint

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EngineCheckpoint {
    pub engine_name: String,
    pub engine_version: String,
    pub state_schema_version: u32,
    pub serialized_state: Vec<u8>,
    pub state_sha256: String,
    pub canonical_journal_cursor: ResumeCursor,
    pub contains_sensitive_conversation: bool,
}
```

Required values for the baseline:

```text
engine_name = "rig-agent"
engine_version = "0.41.0"
state_schema_version = 1
contains_sensitive_conversation = true
```

Checkpoint deserialization rejects engine/version/schema mismatch before invoking Serde.

### Canonical durable events

```rust
pub enum SpikeCanonicalEvent {
    ModelCallRequested { turn: u32 },
    ModelCallCompleted { turn: u32, usage: CanonicalUsage },
    ToolCallProposed { call: CanonicalToolCall },
    ToolCallDenied { call_id: String, reason_code: String },
    ToolExecutionStarted { call_id: String, operation_id: String },
    ToolExecutionObserved { result: CanonicalToolResult },
    ReconciliationRequired { call_id: String, operation_id: Option<String> },
    ReconciliationCompleted { call_id: String, result: CanonicalToolResult },
    LoopCompleted { output_hash: String },
    LoopFailed { code: String },
}
```

These events are Vestrace-owned. Rig-specific internal IDs may be retained only in adapter diagnostics.

---

## Decision gates

### Critical gates

Failure of any critical gate forces `Rejected`:

1. A tool can execute before Vestrace authorization.
2. Rig receives or directly accesses credentials.
3. A crash can duplicate a committed side effect.
4. `Unknown` is automatically retried as a fresh operation.
5. Rig types escape the experimental adapter boundary.
6. Recovery requires persisted hidden chain-of-thought or an unrecoverable process-local object.
7. Policy denial or hard budget exhaustion can be bypassed by loop recovery.

### Restriction gates

Failure may produce `AcceptedWithRestrictions` only when a safe Vestrace fallback exists:

- native Rig checkpoint cannot survive a version change, but journal reconstruction works;
- hook APIs are unsuitable, but hand-driving `AgentRun` remains safe;
- streaming requires a separate normalization path, but durable canonical events remain equivalent;
- only selected state-machine/parsing components are usable without adopting `AgentRunner`.

### Acceptance matrix

```text
Accepted
  all critical gates pass
  same-version checkpoint passes
  journal reconstruction passes
  streaming equivalence passes
  upgrade changes remain adapter-local

AcceptedWithRestrictions
  all critical gates pass
  one or more restriction gates fail
  exact restrictions and fallback are documented

Rejected
  any critical gate fails
  or evidence is ambiguous/non-reproducible
```

---

### Task 1: Freeze the Rig baseline and scaffold the isolated spike crate

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/vestrace-rig-spike/Cargo.toml`
- Create: `crates/vestrace-rig-spike/README.md`
- Create: `crates/vestrace-rig-spike/src/lib.rs`
- Create: `scripts/verify-rig-boundary.sh`

**Interfaces:**
- Produces an experimental package named `vestrace-rig-spike`.
- Depends on Vestrace crates only through public APIs.
- Pins `rig-agent` and `rig-core` to exact version `0.41.0`.
- No non-spike crate depends on `vestrace-rig-spike`, `rig-agent` or `rig-core` through this task.

- [ ] **Step 1: Write the boundary verification script before adding Rig**

Create `scripts/verify-rig-boundary.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

for package in vestrace-domain vestrace-application vestrace-infrastructure; do
  if cargo tree -p "$package" | grep -E '(^| )rig-(agent|core) '; then
    echo "Rig dependency leaked into $package" >&2
    exit 1
  fi
done

cargo check --workspace --exclude vestrace-rig-spike --all-targets
```

Run:

```bash
bash scripts/verify-rig-boundary.sh
```

Expected: PASS before and after the spike crate is added.

- [ ] **Step 2: Add the experimental crate with exact pins**

`crates/vestrace-rig-spike/Cargo.toml` contains:

```toml
[package]
name = "vestrace-rig-spike"
version = "0.0.0"
publish = false
edition.workspace = true
rust-version.workspace = true

[dependencies]
async-trait.workspace = true
schemars.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror.workspace = true
rig-agent = { version = "=0.41.0", default-features = false, features = ["test-utils"] }
rig-core = { version = "=0.41.0", default-features = false, features = ["test-utils"] }
vestrace-application = { path = "../vestrace-application" }
vestrace-domain = { path = "../vestrace-domain" }

[dev-dependencies]
tokio.workspace = true
```

Add the package as a workspace member, but do not add it as a dependency of any other crate.

- [ ] **Step 3: Document the baseline provenance**

`README.md` records:

```text
Rig package version: 0.41.0
Reviewed source commit: 6cfae6d829da21f9dc9e775e065fee157b264f7e
Purpose: non-production architecture spike
Network: forbidden
Real credentials: forbidden
Production dependency: forbidden
```

- [ ] **Step 4: Verify isolation and commit**

```bash
cargo check -p vestrace-rig-spike
bash scripts/verify-rig-boundary.sh
git add Cargo.toml Cargo.lock crates/vestrace-rig-spike scripts/verify-rig-boundary.sh
git commit -m "spike(rig): scaffold isolated runtime experiment"
```

---

### Task 2: Define Vestrace-owned spike contracts and deterministic fixtures

**Files:**
- Create: `crates/vestrace-rig-spike/src/contracts.rs`
- Create: `crates/vestrace-rig-spike/src/fakes.rs`
- Create: `crates/vestrace-rig-spike/src/faults.rs`
- Modify: `crates/vestrace-rig-spike/src/lib.rs`
- Create: `crates/vestrace-rig-spike/tests/normalization.rs`

**Interfaces:**
- Produces all normative spike contracts defined above.
- Produces `ScriptedModel`, `FakeActionGuard`, `FakeToolRuntime`, `FakeOperationLedger`, `CanonicalEventRecorder` and `FaultInjector`.
- Fakes are deterministic and contain no provider or network client.

- [ ] **Step 1: Write failing canonical argument tests**

```rust
#[test]
fn equivalent_json_arguments_produce_one_canonical_value() {
    let left = serde_json::json!({"b": 2, "a": 1});
    let right = serde_json::json!({"a": 1, "b": 2});
    assert_eq!(normalize_json(left), normalize_json(right));
}
```

Run:

```bash
cargo test -p vestrace-rig-spike --test normalization
```

Expected: FAIL because `normalize_json` and contracts do not exist.

- [ ] **Step 2: Implement canonical DTOs without Rig fields**

Use sorted object keys recursively. Reject non-finite numeric input before constructing a `CanonicalToolCall`. Derive each idempotency key from:

```text
SHA-256(run_id || step_id || call_id || tool_name || canonical_arguments)
```

- [ ] **Step 3: Implement deterministic fakes**

`ScriptedModel` consumes a vector of scripted canonical responses. `FakeActionGuard` maps tool names to permit, deny, approval-required or budget-exceeded. `FakeToolRuntime` records every execution and requires a valid ticket ID. `FakeOperationLedger` stores operation ID, idempotency key, status and result independently of Rig state.

- [ ] **Step 4: Implement crash injection points**

```rust
pub enum FaultPoint {
    AfterModelAccepted,
    BeforeToolAuthorization,
    AfterToolAuthorization,
    AfterSideEffectBeforeObservationPersisted,
    AfterObservationPersistedBeforeRigAcknowledged,
}
```

`FaultInjector::hit(point)` returns a deterministic `InjectedCrash` exactly once for the configured point.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-rig-spike --test normalization
git add crates/vestrace-rig-spike
git commit -m "spike(rig): define canonical contracts and fixtures"
```

---

### Task 3: Hand-drive `rig_agent::agent::run::AgentRun` behind the spike port

**Files:**
- Create: `crates/vestrace-rig-spike/src/rig_adapter.rs`
- Create: `crates/vestrace-rig-spike/src/normalization.rs`
- Create: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/lib.rs`
- Create: `crates/vestrace-rig-spike/tests/pause_before_tool.rs`

**Interfaces:**
- Produces `RigSpikeLoop` implementing `SpikeModelLoopPort`.
- Converts only at the adapter boundary:
  - Vestrace canonical messages → Rig messages;
  - Rig model turn → Vestrace canonical events/tool proposals;
  - Vestrace canonical tool results → Rig tool-result content;
  - Rig prompt response/usage/errors → Vestrace canonical output.
- Does not use `AgentRunner`, `ToolSet::execute` or a Rig memory backend.

- [ ] **Step 1: Write the failing pause-before-tool test**

```rust
#[test]
fn model_tool_call_is_returned_as_proposal_without_execution() {
    let mut loop_adapter = fixture_loop_that_requests_transfer();
    let effect = drive_until_non_model_effect(&mut loop_adapter).unwrap();

    let SpikeLoopEffect::ToolCallsProposed(calls) = effect else {
        panic!("expected tool proposal");
    };

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "transfer_funds");
    assert_eq!(tool_execution_count(), 0);
}
```

Run:

```bash
cargo test -p vestrace-rig-spike --test pause_before_tool
```

Expected: FAIL because `RigSpikeLoop` does not exist.

- [ ] **Step 2: Implement the state-machine translation**

Map Rig steps exactly:

```text
AgentRunStep::CallModel
  → SpikeLoopEffect::InvokeModel

AgentRunStep::CallTools
  → SpikeLoopEffect::ToolCallsProposed

AgentRunStep::Done
  → SpikeLoopEffect::Completed
```

`CallTools` conversion records `ToolCallProposed` but performs no authorization or execution.

- [ ] **Step 3: Resolve invalid tool calls fail-closed**

When Rig returns `ModelTurnOutcome::NeedsResolution`, map unknown or disallowed tool calls to a typed adapter event and use Rig's fail/skip resolution only after Vestrace policy has selected the response. Never repair a tool name by fuzzy matching inside the adapter.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test pause_before_tool
cargo clippy -p vestrace-rig-spike --all-targets -- -D warnings
git add crates/vestrace-rig-spike
git commit -m "spike(rig): hand-drive bounded agent loop"
```

---

### Task 4: Prove policy, approval and budget interception before execution

**Files:**
- Create: `crates/vestrace-rig-spike/tests/policy_and_budget_gate.rs`
- Modify: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/fakes.rs`

**Interfaces:**
- Driver receives `ToolCallsProposed`, calls `SpikeActionGuardPort`, and executes only a `Permit` decision.
- Deny, approval-required and budget-exceeded become typed canonical tool results or durable waiting effects without calling the executor.

- [ ] **Step 1: Write four failing gate tests**

```rust
#[tokio::test]
async fn denied_tool_is_not_executed() { /* assert 0 executions */ }

#[tokio::test]
async fn approval_required_pauses_without_execution() { /* assert waiting state */ }

#[tokio::test]
async fn hard_budget_limit_prevents_execution() { /* assert 0 executions */ }

#[tokio::test]
async fn permitted_tool_requires_ticket_at_execution_port() { /* assert ticket consumed */ }
```

- [ ] **Step 2: Implement the fail-closed driver order**

```text
proposal
→ canonicalize operation
→ ActionGuard authorize
→ persist decision in fake journal
→ if Permit: ToolExecutionPort execute(ticket)
→ otherwise: no execution
```

The executor rejects missing, expired, mismatched or already-consumed tickets.

- [ ] **Step 3: Return model-visible denial without changing authority**

For a denial, feed Rig a tool result such as `denied by runtime policy`; keep the authoritative reason code in `CanonicalToolResult.status` and event metadata. Model-visible content cannot transform a denial into a subsequent permit.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test policy_and_budget_gate
git add crates/vestrace-rig-spike
git commit -m "spike(rig): prove policy and budget interception"
```

---

### Task 5: Prove same-version checkpoint pause and resume

**Files:**
- Create: `crates/vestrace-rig-spike/src/checkpoint.rs`
- Create: `crates/vestrace-rig-spike/tests/checkpoint_resume.rs`
- Create: `crates/vestrace-rig-spike/tests/checkpoint_compatibility.rs`

**Interfaces:**
- Produces `RigCheckpointCodec::encode` and `RigCheckpointCodec::decode`.
- Uses JSON bytes only inside the opaque `serialized_state` field.
- Verifies SHA-256, engine name, engine version and state schema before deserializing.

- [ ] **Step 1: Write a failing pending-tool round-trip test**

Checkpoint after Rig emits `CallTools`, reconstruct a new adapter instance, call `apply`/advance, and assert the same canonical call ID, tool name, normalized arguments and idempotency key are re-emitted.

- [ ] **Step 2: Implement checkpoint encoding**

```rust
pub fn encode(
    run: &rig_agent::agent::run::AgentRun,
    cursor: ResumeCursor,
) -> Result<EngineCheckpoint, SpikeAdapterError>;
```

Set `contains_sensitive_conversation = true` unconditionally because Rig state includes accumulated history.

- [ ] **Step 3: Implement strict compatibility rejection**

Tests reject:

- `engine_name != "rig-agent"`;
- `engine_version != "0.41.0"`;
- `state_schema_version != 1`;
- checksum mismatch;
- malformed serialized state.

No incompatible checkpoint is silently retried or partially decoded.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test checkpoint_resume --test checkpoint_compatibility
git add crates/vestrace-rig-spike
git commit -m "spike(rig): validate checkpoint pause and resume"
```

---

### Task 6: Prove crash recovery cannot duplicate an external side effect

**Files:**
- Create: `crates/vestrace-rig-spike/tests/side_effect_idempotency.rs`
- Modify: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/fakes.rs`
- Modify: `crates/vestrace-rig-spike/src/faults.rs`

**Interfaces:**
- `FakeOperationLedger` is authoritative for whether the external operation occurred.
- Recovery checks ledger state before deciding whether to execute, reconcile or feed an existing result to Rig.

- [ ] **Step 1: Write the critical crash test**

Scenario:

```text
Rig proposes transfer_funds
→ guard permits
→ fake external system commits transfer once
→ injected crash before Rig receives tool result
→ restart from checkpoint
→ Rig re-emits pending call
→ driver finds committed operation by idempotency key
→ no second transfer
→ existing result is supplied to Rig
```

Assertions:

```rust
assert_eq!(external_side_effect_count(), 1);
assert_eq!(execution_attempt_count(), 1);
assert_eq!(final_loop_status(), CanonicalLoopStatus::Completed);
```

- [ ] **Step 2: Implement reconciliation-first resume**

On a re-emitted call with an existing ledger record:

```text
Succeeded → reuse recorded typed result
Failed    → reuse recorded failure
Unknown   → emit WaitingForReconciliation
Missing   → request fresh authorization before execution
```

Never infer completion from Rig history alone.

- [ ] **Step 3: Test crash after observation persistence but before Rig acknowledgement**

The result remains in the canonical ledger and is replayed into the restored Rig state without executing again.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test side_effect_idempotency
git add crates/vestrace-rig-spike
git commit -m "spike(rig): prove side effect recovery is idempotent"
```

A failure of this task is a mandatory `Rejected` outcome.

---

### Task 7: Preserve `Unknown` completion and require explicit reconciliation

**Files:**
- Create: `crates/vestrace-rig-spike/tests/unknown_reconciliation.rs`
- Modify: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/contracts.rs`

**Interfaces:**
- Produces `SpikeLoopEffect::WaitingForReconciliation`.
- Rig remains suspended with pending tool calls until reconciliation produces a typed terminal observation.

- [ ] **Step 1: Write a failing unknown-outcome test**

The fake executor commits an operation but returns a simulated lost-response observation. Assert:

- operation ledger status is `Unknown`;
- loop effect is `WaitingForReconciliation`;
- no fresh execute call occurs;
- no success/failure tool result is fabricated for the model.

- [ ] **Step 2: Implement explicit reconciliation input**

```rust
pub enum ReconciliationResult {
    Succeeded(CanonicalToolResult),
    Failed(CanonicalToolResult),
    StillUnknown(PendingOperation),
}
```

Only `Succeeded` or `Failed` is fed to Rig through `tool_results`. `StillUnknown` keeps the Run waiting.

- [ ] **Step 3: Test repeated restart while still unknown**

Three restart cycles must leave execution count at one and reconciliation count at three. The operation keeps the same ID and idempotency key.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test unknown_reconciliation
git add crates/vestrace-rig-spike
git commit -m "spike(rig): preserve unknown completion for reconciliation"
```

A failure of this task is a mandatory `Rejected` outcome.

---

### Task 8: Prove streaming and non-streaming durable-event equivalence

**Files:**
- Create: `crates/vestrace-rig-spike/tests/streaming_equivalence.rs`
- Modify: `crates/vestrace-rig-spike/src/normalization.rs`
- Modify: `crates/vestrace-rig-spike/src/rig_adapter.rs`

**Interfaces:**
- Produces one canonical accepted model turn from either a complete response or assembled streamed response.
- Provisional delta events are not added to the durable comparison vector.

- [ ] **Step 1: Define one semantic fixture in two transport forms**

Both fixtures represent:

```text
assistant proposes tool search_docs({"query":"durable agents"})
usage: input 120, output 30, total 150
provider message ID: msg-1
```

One fixture supplies a complete model turn. The other supplies deterministic streamed fragments assembled through Rig's streaming state support or a thin Vestrace assembler around the same Rig canonical types.

- [ ] **Step 2: Write the failing equivalence assertion**

```rust
assert_eq!(
    blocking_result.durable_events,
    streaming_result.durable_events,
);
assert_eq!(blocking_result.tool_calls, streaming_result.tool_calls);
assert_eq!(blocking_result.usage, streaming_result.usage);
```

- [ ] **Step 3: Normalize stream-only identifiers**

Internal stream correlation IDs may be recorded in adapter diagnostics but are excluded from Vestrace equality and public events. Provider message ID and canonical tool call ID remain stable.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test streaming_equivalence
git add crates/vestrace-rig-spike
git commit -m "spike(rig): prove streaming event equivalence"
```

---

### Task 9: Prove error, usage and type normalization

**Files:**
- Modify: `crates/vestrace-rig-spike/tests/normalization.rs`
- Create: `crates/vestrace-rig-spike/tests/type_boundary.rs`
- Modify: `scripts/verify-rig-boundary.sh`

**Interfaces:**
- Produces canonical classifications for model usage, tool proposals and adapter failures.
- Keeps provider/Rig debug metadata outside public canonical types.

- [ ] **Step 1: Add usage normalization tests**

Verify zero, partial and complete usage reports map without negative values or overflow. The canonical total is computed only when upstream values are internally consistent; otherwise mark usage as partial rather than inventing tokens.

- [ ] **Step 2: Add failure normalization tests**

Map at least:

```text
max turns exceeded      → loop_limit_exceeded
invalid tool call       → invalid_tool_call
protocol violation      → adapter_protocol_error
malformed checkpoint    → checkpoint_invalid
version mismatch        → checkpoint_incompatible
cancelled by policy     → policy_denied
```

Preserve source error text only in redacted diagnostics, not in public failure messages.

- [ ] **Step 3: Add compile-time boundary checks**

`type_boundary.rs` may import Rig only from the spike crate's private adapter module. Public spike contracts are serialized to JSON Schema and scanned to assert that no title, field or definition contains `rig`, `AgentRun`, `PendingToolCall`, `PromptResponse` or a Rig module path.

- [ ] **Step 4: Verify production crates remain Rig-free**

```bash
bash scripts/verify-rig-boundary.sh
cargo test -p vestrace-rig-spike --test normalization --test type_boundary
```

- [ ] **Step 5: Commit**

```bash
git add crates/vestrace-rig-spike scripts/verify-rig-boundary.sh
git commit -m "spike(rig): verify canonical normalization boundaries"
```

A Rig type escaping the adapter is a mandatory `Rejected` outcome.

---

### Task 10: Measure upgrade blast radius without changing production contracts

**Files:**
- Create: `scripts/measure-rig-upgrade-scope.sh`
- Modify: `crates/vestrace-rig-spike/README.md`
- Create during execution: `target/rig-spike/upgrade-scope.txt` (untracked evidence input)

**Interfaces:**
- Produces a machine-readable list of changed files required to compile the spike against an alternate Rig revision.
- Does not commit an alternate Rig version to the main spike branch.

- [ ] **Step 1: Create the measurement script**

The script accepts an exact alternate dependency specification and a temporary worktree path:

```bash
scripts/measure-rig-upgrade-scope.sh \
  --rig-agent-spec '<exact version or git rev>' \
  --worktree /tmp/vestrace-rig-upgrade
```

It must:

1. create a disposable Git worktree from the baseline spike commit;
2. modify only the two Rig dependency specifications;
3. run `cargo check -p vestrace-rig-spike` and all spike tests;
4. allow compatibility edits only under `crates/vestrace-rig-spike/`;
5. write `git diff --name-only` and test results to `target/rig-spike/upgrade-scope.txt`;
6. delete the disposable worktree after evidence is copied.

- [ ] **Step 2: Select the alternate target deterministically at execution time**

Use the newest published Rig release greater than `0.41.0`. When no newer release exists, use the then-current Rig `main` commit, record its exact SHA in the report, and pin that SHA in the disposable worktree.

- [ ] **Step 3: Apply the blast-radius gate**

```text
PASS:
  compatibility edits are confined to crates/vestrace-rig-spike
  production crate APIs and schemas remain unchanged

RESTRICTION:
  adapter-local changes are substantial but safe and testable

REJECT:
  upgrade requires domain/application/public schema/persistence changes
```

- [ ] **Step 4: Commit the reusable script only**

```bash
git add scripts/measure-rig-upgrade-scope.sh crates/vestrace-rig-spike/README.md
git commit -m "spike(rig): add upgrade blast radius measurement"
```

---

### Task 11: Run the full experiment matrix and write the evidence report

**Files:**
- Create: `docs/superpowers/evaluations/2026-07-31-h0-rig-spike-report.md`
- No production source modification

**Interfaces:**
- Produces one evidence row for every ADR-0001 requirement.
- Includes commands, result, evidence artifact/hash, interpretation and gate impact.

- [ ] **Step 1: Run the complete offline suite**

```bash
cargo fmt --all --check
cargo clippy -p vestrace-rig-spike --all-targets -- -D warnings
cargo test -p vestrace-rig-spike
bash scripts/verify-rig-boundary.sh
```

Expected: all commands exit `0`. Any failing critical experiment remains a failed gate; do not weaken the test to make it green.

- [ ] **Step 2: Run the upgrade-scope experiment**

Execute Task 10's script against the selected exact alternate target and preserve the output hash in the report.

- [ ] **Step 3: Write the report with this exact table**

```markdown
| Requirement | Test/command | Result | Evidence | Gate impact |
|---|---|---|---|---|
| Stop before tool execution | pause_before_tool | Pass/Fail | commit + test name | Critical |
| Policy and budget enforcement | policy_and_budget_gate | Pass/Fail | ... | Critical |
| No direct credentials/tools | type_boundary + code inspection | Pass/Fail | ... | Critical |
| Durable pause | checkpoint_resume | Pass/Fail | ... | Restriction |
| No duplicate side effect | side_effect_idempotency | Pass/Fail | ... | Critical |
| Journal/checkpoint recovery | checkpoint_resume + ledger replay | Pass/Fail | ... | Critical |
| Unknown reconciliation | unknown_reconciliation | Pass/Fail | ... | Critical |
| Streaming equivalence | streaming_equivalence | Pass/Fail | ... | Restriction |
| Canonical normalization | normalization | Pass/Fail | ... | Critical |
| Upgrade isolation | upgrade-scope report | Pass/Fail | ... | Critical/Restriction |
```

- [ ] **Step 4: Record security and privacy observations**

State explicitly that the Rig checkpoint embeds conversation history, how it would be classified, encrypted and retained, and whether Vestrace can operate with engine checkpoints disabled.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/evaluations/2026-07-31-h0-rig-spike-report.md
git commit -m "docs: record H0 Rig spike evidence"
```

---

### Task 12: Publish the binding outcome ADR

**Files:**
- Create: `docs/superpowers/specs/adr/0002-rig-agent-runtime-spike-outcome.md`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md`
- Modify: `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`

**Interfaces:**
- Produces exactly one outcome: `Accepted`, `AcceptedWithRestrictions` or `Rejected`.
- Makes H3's model-loop choice explicit.

- [ ] **Step 1: Select the outcome mechanically**

```text
Any critical failure
  → Rejected

No critical failures + all restriction gates pass
  → Accepted

No critical failures + safe documented restriction failures
  → AcceptedWithRestrictions
```

Human preference cannot override this table without a new ADR that explicitly accepts the failed safety property.

- [ ] **Step 2: Write ADR-0002**

Required sections:

```text
Status
Date
Baseline Rig version and commit
Evidence report hash
Decision
Accepted components
Rejected components
Required restrictions
Checkpoint strategy
Recovery strategy
Upgrade policy
H3 contract consequences
Rollback/replacement path
```

For each possible outcome, H3 is directed as follows:

```text
Accepted
  implement RigModelLoopAdapter behind ModelLoopPort

AcceptedWithRestrictions
  implement only named safe Rig components
  provide NativeVestraceModelLoop for excluded behavior

Rejected
  implement NativeVestraceModelLoop
  retain Rig only as optional provider adapter under ADR-0001
```

- [ ] **Step 3: Update roadmap and supersession links**

Mark H0-RIG complete only after ADR-0002 is accepted. Add a link from ADR-0001 to ADR-0002 without rewriting ADR-0001's historical decision.

- [ ] **Step 4: Run documentation validation and commit**

```bash
rg -n "TBD|TODO|implement later|fill in details" \
  docs/superpowers/specs/adr/0002-rig-agent-runtime-spike-outcome.md \
  docs/superpowers/evaluations/2026-07-31-h0-rig-spike-report.md

git add docs/superpowers/specs/adr docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md
git commit -m "docs: decide H0 Rig runtime outcome"
```

Expected: `rg` returns no matches and ADR-0002 contains exactly one outcome.

---

## Full verification gate

Before declaring H0-RIG complete:

```bash
cargo fmt --all --check
cargo clippy -p vestrace-rig-spike --all-targets -- -D warnings
cargo test -p vestrace-rig-spike
bash scripts/verify-rig-boundary.sh
```

The evidence report must identify the exact baseline and alternate Rig targets, command outputs, Git commit SHAs and every failed or restricted criterion.

## Required H3 handoff

ADR-0002 must hand H3 these exact conclusions:

- selected production loop engine;
- whether native Rig checkpointing is enabled;
- engine checkpoint compatibility policy;
- authoritative journal reconstruction procedure;
- allowed Rig modules and forbidden Rig modules;
- canonical tool-call and result mapping;
- streaming normalization rule;
- error/usage normalization table;
- upgrade and pinning policy;
- mandatory Rig-free build path;
- replacement strategy when Rig becomes incompatible.

## Explicit non-goals

H0-RIG does not:

- implement the production H3 Model Runtime;
- call a real LLM provider;
- implement H4 Tool Runtime;
- execute Docker or shell commands as agent tools;
- add memory, vector store or retrieval integration from Rig;
- adopt `AgentRunner` as Vestrace orchestration;
- adopt Rig workflow, scheduling or persistence abstractions;
- persist hidden chain-of-thought;
- change public Vestrace schemas;
- create database migrations;
- decide provider routing or pricing.

## Completion definition

H0-RIG is complete only when:

1. all experiments are reproducible offline;
2. every ADR-0001 requirement has explicit evidence;
3. critical failures cannot be hidden by retries or mocks;
4. the no-duplicate-side-effect and `Unknown` tests pass;
5. production crates remain Rig-free;
6. upgrade blast radius is measured;
7. the evidence report is committed;
8. ADR-0002 selects exactly one outcome;
9. H3 has an unambiguous model-loop direction.
