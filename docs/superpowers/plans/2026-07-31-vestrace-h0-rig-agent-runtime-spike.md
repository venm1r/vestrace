# Vestrace H0-RIG Agent Runtime Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to execute this spike task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not execute the spike, create its branch, change dependencies, run experiments or add production code until the user explicitly ends the documentation-only phase.

**Goal:** Determine with reproducible evidence whether `rig-agent` can serve as Vestrace's replaceable bounded model↔tool inner-loop engine without owning durable Run state, authorization, budgets, tool execution, recovery or public contracts.

**Architecture:** Build one isolated experimental crate that hand-drives Rig's sans-I/O `AgentRun` through Vestrace-owned spike contracts and deterministic fakes. The spike never calls a real model, external tool, credential service or network endpoint. A serialized Rig state may be used as an optional same-version acceleration checkpoint, while the Vestrace journal, pending-operation ledger and H1/H2 state remain authoritative. The spike ends in an evidence report and exactly one outcome ADR: `Accepted`, `AcceptedWithRestrictions` or `Rejected`.

**Tech Stack:** Existing Vestrace v0.1, H1 and H2 contracts; Rust Edition 2024; Tokio; Serde; Schemars; SHA-256; `rig-agent = "=0.41.0"`; `rig-core = "=0.41.0"`; deterministic test doubles; no PostgreSQL migration and no external network.

## Global Constraints

- Complete all five Vestrace v0.1 plans, H1 Durable Run Core and H2 Policy, Approval and Budget Core before executing H0-RIG.
- Decision source: `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`.
- Roadmap source: `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md`.
- Reviewed Rig baseline: workspace version `0.41.0`, source commit `6cfae6d829da21f9dc9e775e065fee157b264f7e`.
- Pin Rig exactly for the baseline experiment. Floating semver ranges and unpinned Git dependencies are forbidden.
- The spike crate is experimental. No production crate may depend on it.
- Rig types may appear only in private modules of `vestrace-rig-spike`.
- No Rig type may appear in `vestrace-domain`, `vestrace-application`, PostgreSQL schemas, public APIs, durable events, MCP schemas, extension protocols or Vestrace-owned checkpoints.
- Rig never receives permanent credentials, credential leases, approval grants, authorization tickets or direct external-tool implementations.
- Every executable tool proposal passes through Vestrace-owned action-guard and tool-execution ports matching H2 and the future H4 boundary.
- Denial, approval requirement, budget exhaustion and `Unknown` completion are Vestrace states, not generic Rig exceptions.
- A serialized Rig `AgentRun` is never the sole recovery source.
- Baseline experiments require no provider key, MCP server, Docker daemon, PostgreSQL instance or internet access.
- Streaming and non-streaming tests compare final canonical durable events. Provisional stream deltas may differ.
- A failed critical gate forces `Rejected`; subjective convenience cannot waive it.
- H0 creates no migration and does not change H1/H2 semantics.
- Future execution branch: `spike/h0-rig-agent-runtime`.

---

## Upstream Rig boundary locked for the experiment

The reviewed Rig baseline exposes a hand-driven state machine with three driver-visible actions:

```text
CallModel
CallTools
Done
```

The state machine owns turn counting, invalid tool-call handling, accumulated conversation, usage aggregation and response construction, but performs no I/O. It is serializable and re-emits pending tool calls after same-version restoration. Upstream explicitly states that the serialized state contains accumulated conversation data and has no cross-version stability guarantee.

The spike therefore tests two recovery paths:

```text
Fast path
  same-version Rig engine checkpoint

Authoritative path
  Vestrace journal + pending-operation ledger
  → rebuild bounded-loop input
  → continue or fail safely
```

Rig's durable-approval example is evidence that pause/resume is technically possible. Its direct `ToolSet::execute` pattern is forbidden in Vestrace and is not copied into the spike.

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

These contracts are private to the experimental crate. They model the intended H3 seam but do not become a production API until ADR-0002 approves them.

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
    WaitingForApproval {
        operation_fingerprints: Vec<OperationFingerprint>,
    },
    WaitingForReconciliation(Vec<PendingOperation>),
    Completed(CanonicalLoopOutput),
    Failed(CanonicalLoopFailure),
}

pub trait SpikeModelLoopPort {
    fn apply(
        &mut self,
        command: SpikeLoopCommand,
    ) -> Result<SpikeLoopEffect, SpikeAdapterError>;

    fn checkpoint(
        &self,
        cursor: ResumeCursor,
    ) -> Result<EngineCheckpoint, SpikeAdapterError>;
}
```

Returning `ToolCallsProposed` must not authorize or execute a tool.

### Canonical tool proposal and result

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CanonicalToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub normalized_arguments: CanonicalArguments,
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

`model_presentation` is untrusted text visible to the model. Status, operation identity and authoritative output remain typed Vestrace data.

### Action guard and execution ports

```rust
pub enum SpikeGuardDecision {
    Permit { guarded_action: GuardedAction },
    Deny { reason_code: String },
    RequireApproval {
        operation_fingerprint: OperationFingerprint,
    },
    BudgetExceeded { dimension: BudgetDimension },
}

#[async_trait::async_trait]
pub trait SpikeActionGuardPort: Send + Sync {
    async fn prepare_tool_call(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        step_id: RunStepId,
        call: &CanonicalToolCall,
    ) -> Result<SpikeGuardDecision, ApplicationError>;

    async fn consume(
        &self,
        context: &RequestContext,
        guarded_action: &GuardedAction,
    ) -> Result<(), ApplicationError>;
}

#[async_trait::async_trait]
pub trait SpikeToolExecutionPort: Send + Sync {
    async fn execute(
        &self,
        context: &RequestContext,
        guarded_action: &GuardedAction,
        call: &CanonicalToolCall,
    ) -> Result<ExecutionObservation, ApplicationError>;

    async fn reconcile(
        &self,
        context: &RequestContext,
        pending: &PendingOperation,
    ) -> Result<ReconciliationResult, ApplicationError>;
}
```

The Vestrace driver calls `prepare_tool_call`, consumes the resulting ticket, then calls the execution port. Rig cannot call these ports directly.

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

Baseline values:

```text
engine_name = "rig-agent"
engine_version = "0.41.0"
state_schema_version = 1
contains_sensitive_conversation = true
```

Decode rejects engine, version or schema mismatch and checksum failure before invoking Serde.

### Canonical durable events

```rust
pub enum SpikeCanonicalEvent {
    ModelCallRequested { turn: u32 },
    ModelCallCompleted { turn: u32, usage: CanonicalUsage },
    ToolCallProposed { call: CanonicalToolCall },
    ToolCallDenied { call_id: String, reason_code: String },
    ToolExecutionStarted { call_id: String, operation_id: String },
    ToolExecutionObserved { result: CanonicalToolResult },
    ReconciliationRequired {
        call_id: String,
        operation_id: Option<String>,
    },
    ReconciliationCompleted {
        call_id: String,
        result: CanonicalToolResult,
    },
    LoopCompleted { output_hash: String },
    LoopFailed { code: String },
}
```

Rig-specific internal IDs may be retained only in adapter diagnostics.

---

## Decision gates

### Critical gates

Failure of any item forces `Rejected`:

1. A tool can execute before Vestrace authorization.
2. Rig receives or directly accesses credentials, tickets or approval grants.
3. A crash can duplicate a committed side effect.
4. `Unknown` is automatically retried as a new operation.
5. Rig types escape the adapter boundary.
6. Recovery requires hidden chain-of-thought or an unrecoverable process-local object.
7. Policy denial or hard budget exhaustion can be bypassed during resume.

### Restriction gates

Failure may produce `AcceptedWithRestrictions` only when a safe Vestrace fallback is demonstrated:

- native Rig checkpoint fails across versions, but journal reconstruction succeeds;
- hook APIs are unsuitable, but hand-driving `AgentRun` remains safe;
- streaming needs a separate normalizer, but durable canonical events are equivalent;
- only selected state-machine or parsing components are usable without adopting `AgentRunner`.

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
  at least one restriction gate fails
  every failure has a tested Vestrace fallback

Rejected
  any critical gate fails
  or evidence is ambiguous or non-reproducible
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
- Produces experimental package `vestrace-rig-spike`.
- Pins `rig-agent` and `rig-core` to `0.41.0` exactly.
- No other crate depends on the spike or Rig through this task.

- [ ] **Step 1: Write the boundary script before adding Rig**

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

Expected: PASS before and after adding the experimental crate.

- [ ] **Step 2: Add the exact-pinned crate**

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

Add the crate as a workspace member. Do not add it to any crate's dependencies.

- [ ] **Step 3: Record baseline provenance in the spike README**

```text
Rig package version: 0.41.0
Reviewed source commit: 6cfae6d829da21f9dc9e775e065fee157b264f7e
Purpose: non-production architecture spike
Network: forbidden
Real credentials: forbidden
Production dependency: forbidden
```

- [ ] **Step 4: Verify and commit**

```bash
cargo check -p vestrace-rig-spike
bash scripts/verify-rig-boundary.sh
git add Cargo.toml Cargo.lock crates/vestrace-rig-spike scripts/verify-rig-boundary.sh
git commit -m "spike(rig): scaffold isolated runtime experiment"
```

---

### Task 2: Define canonical contracts and deterministic fixtures

**Files:**
- Create: `crates/vestrace-rig-spike/src/contracts.rs`
- Create: `crates/vestrace-rig-spike/src/fakes.rs`
- Create: `crates/vestrace-rig-spike/src/faults.rs`
- Modify: `crates/vestrace-rig-spike/src/lib.rs`
- Create: `crates/vestrace-rig-spike/tests/normalization.rs`

**Interfaces:**
- Produces the normative spike contracts above.
- Produces `ScriptedModel`, `FakeActionGuard`, `FakeToolRuntime`, `FakeOperationLedger`, `CanonicalEventRecorder` and `FaultInjector`.
- Fakes have no provider or network client.

- [ ] **Step 1: Write the failing canonicalization test**

```rust
#[test]
fn equivalent_json_arguments_have_identical_bytes_and_hashes() {
    let left = CanonicalArguments::new(serde_json::json!({"b": 2, "a": 1})).unwrap();
    let right = CanonicalArguments::new(serde_json::json!({"a": 1, "b": 2})).unwrap();

    assert_eq!(left.bytes(), right.bytes());
    assert_eq!(left.hash(), right.hash());
}
```

Run:

```bash
cargo test -p vestrace-rig-spike --test normalization
```

Expected: FAIL because the spike contracts do not exist.

- [ ] **Step 2: Implement deterministic proposal identity**

Derive each tool idempotency key from this byte sequence:

```text
vestrace-rig-tool-call/v1
run UUID
step UUID
call ID UTF-8
tool name UTF-8
canonical arguments bytes
```

Reject blank call IDs, invalid tool names and argument values that cannot be represented by `CanonicalArguments`.

- [ ] **Step 3: Implement deterministic fakes**

`ScriptedModel` consumes a fixed response vector. `FakeActionGuard` maps tool names to `Permit`, `Deny`, `RequireApproval` or `BudgetExceeded`. `FakeToolRuntime` records every execution and rejects an unconsumed or mismatched `GuardedAction`. `FakeOperationLedger` stores operation ID, idempotency key, status and result independently of Rig.

- [ ] **Step 4: Implement fault points**

```rust
pub enum FaultPoint {
    AfterModelAccepted,
    BeforeToolAuthorization,
    AfterToolAuthorization,
    AfterSideEffectBeforeObservationPersisted,
    AfterObservationPersistedBeforeRigAcknowledged,
}
```

`FaultInjector::hit` returns `InjectedCrash` once at the configured point and returns normally afterward.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test normalization
git add crates/vestrace-rig-spike
git commit -m "spike(rig): define canonical contracts and fixtures"
```

---

### Task 3: Hand-drive Rig behind the spike port

**Files:**
- Create: `crates/vestrace-rig-spike/src/rig_adapter.rs`
- Create: `crates/vestrace-rig-spike/src/normalization.rs`
- Create: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/lib.rs`
- Create: `crates/vestrace-rig-spike/tests/pause_before_tool.rs`

**Interfaces:**
- Produces `RigSpikeLoop` implementing `SpikeModelLoopPort`.
- Converts messages, model turns, tool proposals, tool results, usage and errors only at the adapter boundary.
- Does not use `AgentRunner`, `ToolSet::execute` or any Rig memory backend.

- [ ] **Step 1: Write the failing pause-before-tool test**

```rust
#[test]
fn model_tool_call_is_a_proposal_and_does_not_execute() {
    let fixture = SpikeFixture::transfer_request();
    let mut loop_adapter = fixture.loop_adapter();

    let effect = fixture.drive_until_tool_proposal(&mut loop_adapter).unwrap();
    let calls = match effect {
        SpikeLoopEffect::ToolCallsProposed(calls) => calls,
        other => panic!("expected tool proposal, received {other:?}"),
    };

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, "transfer_funds");
    assert_eq!(fixture.tool_runtime.execution_count(), 0);
}
```

Run:

```bash
cargo test -p vestrace-rig-spike --test pause_before_tool
```

Expected: FAIL because `RigSpikeLoop` does not exist.

- [ ] **Step 2: Implement step translation**

```text
AgentRunStep::CallModel
  → SpikeLoopEffect::InvokeModel

AgentRunStep::CallTools
  → SpikeLoopEffect::ToolCallsProposed

AgentRunStep::Done
  → SpikeLoopEffect::Completed
```

`CallTools` records `ToolCallProposed` and performs no authorization or execution.

- [ ] **Step 3: Handle invalid tool calls fail-closed**

When Rig returns `ModelTurnOutcome::NeedsResolution`, emit a typed adapter diagnostic and use fail/skip resolution only after the Vestrace driver selects the outcome. Fuzzy tool-name repair is forbidden.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test pause_before_tool
cargo clippy -p vestrace-rig-spike --all-targets -- -D warnings
git add crates/vestrace-rig-spike
git commit -m "spike(rig): hand-drive bounded agent loop"
```

---

### Task 4: Prove policy, approval and budget interception

**Files:**
- Create: `crates/vestrace-rig-spike/tests/policy_and_budget_gate.rs`
- Modify: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/fakes.rs`

**Interfaces:**
- Driver authorizes each proposal through `SpikeActionGuardPort`.
- Only `Permit` may reach `SpikeToolExecutionPort`.
- Approval-required and budget-exceeded results pause or deny without execution.

- [ ] **Step 1: Write the four failing gate tests**

```rust
#[tokio::test]
async fn denied_tool_is_not_executed() {
    let fixture = SpikeFixture::denied_transfer();
    let outcome = fixture.drive_one_tool_round().await.unwrap();

    assert_eq!(outcome.tool_results[0].status, CanonicalToolStatus::Denied);
    assert_eq!(fixture.tool_runtime.execution_count(), 0);
}

#[tokio::test]
async fn approval_required_pauses_without_execution() {
    let fixture = SpikeFixture::approval_required_transfer();
    let outcome = fixture.drive_until_wait().await.unwrap();

    assert!(matches!(outcome, SpikeLoopEffect::WaitingForApproval { .. }));
    assert_eq!(fixture.tool_runtime.execution_count(), 0);
}

#[tokio::test]
async fn hard_budget_limit_prevents_execution() {
    let fixture = SpikeFixture::budget_exhausted_transfer();
    let outcome = fixture.drive_one_tool_round().await.unwrap();

    assert_eq!(outcome.tool_results[0].status, CanonicalToolStatus::BudgetExceeded);
    assert_eq!(fixture.tool_runtime.execution_count(), 0);
}

#[tokio::test]
async fn permitted_tool_consumes_guarded_action_before_execution() {
    let fixture = SpikeFixture::permitted_transfer();
    fixture.drive_one_tool_round().await.unwrap();

    assert_eq!(fixture.action_guard.consume_count(), 1);
    assert_eq!(fixture.tool_runtime.execution_count(), 1);
    assert!(fixture.tool_runtime.all_executions_were_guarded());
}
```

- [ ] **Step 2: Implement the fail-closed order**

```text
proposal
→ canonicalize operation
→ ActionGuard prepare
→ persist decision in fake journal
→ if Permit: consume guarded action
→ execute through ToolExecutionPort
→ record typed observation
```

The executor rejects missing, expired, mismatched or already-consumed guarded actions.

- [ ] **Step 3: Preserve authority outside model-visible text**

A denial may be presented to the model as `denied by runtime policy`. The authoritative reason code remains typed and cannot be changed by the model's next message.

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
- Stores Rig JSON only inside opaque `serialized_state`.
- Validates checksum and metadata before deserializing.

- [ ] **Step 1: Write the pending-tool round-trip test**

```rust
#[test]
fn same_version_checkpoint_reemits_identical_tool_proposal() {
    let fixture = SpikeFixture::transfer_request();
    let mut original = fixture.loop_adapter();
    let first = fixture.drive_until_tool_proposal(&mut original).unwrap();
    let checkpoint = original.checkpoint(ResumeCursor::from_version(RunVersion::new(7).unwrap())).unwrap();

    let mut restored = RigCheckpointCodec::decode(checkpoint).unwrap();
    let second = restored.next_effect().unwrap();

    assert_eq!(first, second);
}
```

- [ ] **Step 2: Implement checkpoint encoding**

```rust
pub fn encode(
    run: &rig_agent::agent::run::AgentRun,
    cursor: ResumeCursor,
) -> Result<EngineCheckpoint, SpikeAdapterError>;
```

Always set `contains_sensitive_conversation = true` because Rig state contains accumulated history.

- [ ] **Step 3: Implement strict compatibility tests**

Reject engine name other than `rig-agent`, version other than `0.41.0`, schema other than `1`, checksum mismatch and malformed JSON. No incompatible state is partially decoded.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test checkpoint_resume --test checkpoint_compatibility
git add crates/vestrace-rig-spike
git commit -m "spike(rig): validate checkpoint pause and resume"
```

---

### Task 6: Prove crash recovery cannot duplicate a side effect

**Files:**
- Create: `crates/vestrace-rig-spike/tests/side_effect_idempotency.rs`
- Modify: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/fakes.rs`
- Modify: `crates/vestrace-rig-spike/src/faults.rs`

**Interfaces:**
- `FakeOperationLedger` is authoritative for external completion.
- Resume checks the ledger before deciding to execute, reconcile or replay a result into Rig.

- [ ] **Step 1: Write the critical lost-acknowledgement test**

```rust
#[tokio::test]
async fn crash_after_commit_before_rig_acknowledgement_does_not_repeat_effect() {
    let fixture = SpikeFixture::crash_after_side_effect();
    let checkpoint = fixture.run_until_injected_crash().await.unwrap_err().checkpoint;

    let output = fixture.resume_from(checkpoint).await.unwrap();

    assert_eq!(fixture.external_system.side_effect_count(), 1);
    assert_eq!(fixture.tool_runtime.execution_count(), 1);
    assert!(matches!(output, SpikeLoopEffect::Completed(_)));
}
```

- [ ] **Step 2: Implement reconciliation-first resume**

```text
Succeeded ledger record
  → reuse recorded result

Failed ledger record
  → reuse recorded failure

Unknown ledger record
  → WaitingForReconciliation

No ledger record
  → prepare a new guarded action before execution
```

Never infer completion from Rig history.

- [ ] **Step 3: Add the observation-persisted crash case**

Crash after the canonical observation is stored but before `AgentRun::tool_results`. Resume must replay the stored result into restored Rig state without another execution.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test side_effect_idempotency
git add crates/vestrace-rig-spike
git commit -m "spike(rig): prove side effect recovery is idempotent"
```

Failure of this task forces `Rejected`.

---

### Task 7: Preserve `Unknown` and require reconciliation

**Files:**
- Create: `crates/vestrace-rig-spike/tests/unknown_reconciliation.rs`
- Modify: `crates/vestrace-rig-spike/src/driver.rs`
- Modify: `crates/vestrace-rig-spike/src/contracts.rs`

**Interfaces:**
- Produces `WaitingForReconciliation`.
- Rig remains suspended with pending tool calls until reconciliation reaches a typed terminal observation.

- [ ] **Step 1: Write the unknown-outcome test**

```rust
#[tokio::test]
async fn unknown_completion_is_not_fed_to_rig_as_success_or_failure() {
    let fixture = SpikeFixture::unknown_transfer();
    let effect = fixture.drive_until_wait().await.unwrap();

    assert!(matches!(effect, SpikeLoopEffect::WaitingForReconciliation(_)));
    assert_eq!(fixture.tool_runtime.execution_count(), 1);
    assert_eq!(fixture.loop_adapter.tool_result_count(), 0);
}
```

- [ ] **Step 2: Implement reconciliation input**

```rust
pub enum ReconciliationResult {
    Succeeded(CanonicalToolResult),
    Failed(CanonicalToolResult),
    StillUnknown(PendingOperation),
}
```

Only `Succeeded` and `Failed` are supplied to Rig. `StillUnknown` keeps the Run waiting.

- [ ] **Step 3: Test repeated restart while still unknown**

```rust
#[tokio::test]
async fn repeated_unknown_reconciliation_never_reexecutes() {
    let fixture = SpikeFixture::unknown_transfer();

    fixture.restart_and_reconcile_unknown().await.unwrap();
    fixture.restart_and_reconcile_unknown().await.unwrap();
    fixture.restart_and_reconcile_unknown().await.unwrap();

    assert_eq!(fixture.tool_runtime.execution_count(), 1);
    assert_eq!(fixture.tool_runtime.reconciliation_count(), 3);
    assert_eq!(fixture.operation_ledger.distinct_operation_count(), 1);
}
```

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test unknown_reconciliation
git add crates/vestrace-rig-spike
git commit -m "spike(rig): preserve unknown completion for reconciliation"
```

Failure of this task forces `Rejected`.

---

### Task 8: Prove streaming and blocking durable-event equivalence

**Files:**
- Create: `crates/vestrace-rig-spike/tests/streaming_equivalence.rs`
- Modify: `crates/vestrace-rig-spike/src/normalization.rs`
- Modify: `crates/vestrace-rig-spike/src/rig_adapter.rs`

**Interfaces:**
- Produces one accepted canonical turn from a complete response or assembled stream.
- Provisional deltas are excluded from durable comparison.

- [ ] **Step 1: Define one semantic result in two forms**

Both fixtures represent:

```text
assistant tool proposal: search_docs({"query":"durable agents"})
usage: input 120, output 30, total 150
provider message ID: msg-1
```

The blocking fixture supplies one complete turn. The streaming fixture supplies deterministic fragments assembled through Rig's streamed-turn state support.

- [ ] **Step 2: Write the equivalence test**

```rust
#[test]
fn blocking_and_streaming_create_identical_durable_events() {
    let blocking = run_blocking_fixture().unwrap();
    let streaming = run_streaming_fixture().unwrap();

    assert_eq!(blocking.durable_events, streaming.durable_events);
    assert_eq!(blocking.tool_calls, streaming.tool_calls);
    assert_eq!(blocking.usage, streaming.usage);
}
```

- [ ] **Step 3: Normalize stream-only identifiers**

Internal stream correlation IDs remain adapter diagnostics. Provider message ID and canonical tool call ID remain part of canonical results.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vestrace-rig-spike --test streaming_equivalence
git add crates/vestrace-rig-spike
git commit -m "spike(rig): prove streaming event equivalence"
```

---

### Task 9: Prove usage, error and type normalization

**Files:**
- Modify: `crates/vestrace-rig-spike/tests/normalization.rs`
- Create: `crates/vestrace-rig-spike/tests/type_boundary.rs`
- Modify: `scripts/verify-rig-boundary.sh`

**Interfaces:**
- Produces canonical usage and failure classifications.
- Keeps Rig debug metadata outside canonical contracts.

- [ ] **Step 1: Add usage normalization tests**

Test zero, partial and complete usage. Compute a canonical total only when upstream values are internally consistent; otherwise preserve partial usage without inventing tokens.

- [ ] **Step 2: Add exact failure mappings**

```text
max turns exceeded   → loop_limit_exceeded
invalid tool call    → invalid_tool_call
protocol violation   → adapter_protocol_error
malformed checkpoint → checkpoint_invalid
version mismatch     → checkpoint_incompatible
policy cancellation  → policy_denied
```

Source error text is retained only in redacted diagnostics.

- [ ] **Step 3: Add schema boundary tests**

Serialize public spike contracts to JSON Schema. Assert no definition, title or field contains `rig`, `AgentRun`, `PendingToolCall`, `PromptResponse` or a Rig module path.

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

A leaked Rig type forces `Rejected`.

---

### Task 10: Measure Rig upgrade blast radius

**Files:**
- Create: `scripts/measure-rig-upgrade-scope.sh`
- Modify: `crates/vestrace-rig-spike/README.md`
- Create during execution: `target/rig-spike/upgrade-scope.txt` as untracked evidence

**Interfaces:**
- Produces the list of files changed to compile against one exact alternate Rig target.
- Never commits the alternate target to the baseline spike branch.

- [ ] **Step 1: Create the measurement script**

The script reads `RIG_ALTERNATE_SPEC` and `RIG_UPGRADE_WORKTREE` from the environment:

```bash
set -euo pipefail
: "${RIG_ALTERNATE_SPEC:?set an exact Rig version or 40-character Git SHA}"
: "${RIG_UPGRADE_WORKTREE:=/tmp/vestrace-rig-upgrade}"
```

It must:

1. create a disposable worktree from the baseline spike commit;
2. change only the two Rig dependency specifications;
3. run `cargo check -p vestrace-rig-spike` and every spike test;
4. permit compatibility edits only under `crates/vestrace-rig-spike/`;
5. write changed filenames and command results to `target/rig-spike/upgrade-scope.txt`;
6. remove the disposable worktree after evidence is copied.

- [ ] **Step 2: Select the alternate target at execution time**

Choose the newest published Rig version greater than `0.41.0`. When no newer version exists, choose the then-current Rig `main` commit. Record the exact version or 40-character SHA before running the script.

- [ ] **Step 3: Apply the gate**

```text
PASS
  edits are confined to crates/vestrace-rig-spike
  production APIs and schemas remain unchanged

RESTRICTION
  adapter-local edits are substantial but safe and testable

REJECT
  upgrade requires domain, application, persistence or public-contract changes
```

- [ ] **Step 4: Commit the script**

```bash
git add scripts/measure-rig-upgrade-scope.sh crates/vestrace-rig-spike/README.md
git commit -m "spike(rig): add upgrade blast radius measurement"
```

---

### Task 11: Write the complete evidence report

**Files:**
- Create: `docs/superpowers/evaluations/2026-07-31-h0-rig-spike-report.md`
- No production source modification

**Interfaces:**
- Produces one evidence row for every ADR-0001 requirement.
- Records commands, result, commit, evidence hash, interpretation and gate impact.

- [ ] **Step 1: Run the offline suite**

```bash
cargo fmt --all --check
cargo clippy -p vestrace-rig-spike --all-targets -- -D warnings
cargo test -p vestrace-rig-spike
bash scripts/verify-rig-boundary.sh
```

Any failed critical experiment remains failed; do not weaken its assertion.

- [ ] **Step 2: Run the upgrade experiment**

Set `RIG_ALTERNATE_SPEC` to the selected exact target and run `scripts/measure-rig-upgrade-scope.sh`. Record the output file's SHA-256.

- [ ] **Step 3: Use this exact report table**

```markdown
| Requirement | Test or command | Result | Required evidence | Gate impact |
|---|---|---|---|---|
| Stop before tool execution | `pause_before_tool` | Pass or Fail | test name, commit SHA, output hash | Critical |
| Policy and budget enforcement | `policy_and_budget_gate` | Pass or Fail | four test names, commit SHA, output hash | Critical |
| No direct credentials or tools | `type_boundary` and boundary script | Pass or Fail | schema scan and dependency-tree output hashes | Critical |
| Durable pause | `checkpoint_resume` | Pass or Fail | checkpoint metadata and test output hash | Restriction |
| No duplicate side effect | `side_effect_idempotency` | Pass or Fail | side-effect and execution counters | Critical |
| Journal/checkpoint recovery | checkpoint and ledger replay tests | Pass or Fail | cursor, operation ID and output hash | Critical |
| Unknown reconciliation | `unknown_reconciliation` | Pass or Fail | execution and reconciliation counters | Critical |
| Streaming equivalence | `streaming_equivalence` | Pass or Fail | canonical event-vector hashes | Restriction |
| Canonical normalization | `normalization` | Pass or Fail | schema and fixture hashes | Critical |
| Upgrade isolation | upgrade-scope script | Pass or Fail | alternate target and changed-file list hash | Critical or Restriction |
```

- [ ] **Step 4: Record privacy conclusions**

State that the Rig checkpoint embeds conversation history, assign its Vestrace data classification, define encryption and retention requirements, and state whether operation remains possible with engine checkpoints disabled.

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
- Gives H3 an unambiguous model-loop direction.

- [ ] **Step 1: Select the outcome mechanically**

```text
Any critical failure
  → Rejected

No critical failures and all restriction gates pass
  → Accepted

No critical failures and every failed restriction has a tested fallback
  → AcceptedWithRestrictions
```

Human preference cannot override this table without a separate ADR explicitly accepting the failed safety property.

- [ ] **Step 2: Write ADR-0002 with all required sections**

```text
Status
Date
Baseline Rig version and commit
Alternate target
Evidence report hash
Decision
Accepted Rig components
Rejected Rig components
Required restrictions
Checkpoint strategy
Recovery strategy
Upgrade policy
H3 contract consequences
Replacement path
```

H3 direction by outcome:

```text
Accepted
  implement RigModelLoopAdapter behind ModelLoopPort

AcceptedWithRestrictions
  implement only named safe Rig components
  use NativeVestraceModelLoop for excluded behavior

Rejected
  implement NativeVestraceModelLoop
  retain Rig only as optional provider adapter under ADR-0001
```

- [ ] **Step 3: Update decision links**

Mark H0-RIG complete only after ADR-0002 is accepted. Add a forward link from ADR-0001 without rewriting its historical decision.

- [ ] **Step 4: Validate documentation and commit**

```bash
rg -n "TBD|TODO|implement later|fill in details" \
  docs/superpowers/specs/adr/0002-rig-agent-runtime-spike-outcome.md \
  docs/superpowers/evaluations/2026-07-31-h0-rig-spike-report.md

git add docs/superpowers/specs/adr docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md
git commit -m "docs: decide H0 Rig runtime outcome"
```

Expected: the search returns no matches and ADR-0002 contains exactly one outcome.

---

## Full verification gate

Before declaring H0-RIG complete:

```bash
cargo fmt --all --check
cargo clippy -p vestrace-rig-spike --all-targets -- -D warnings
cargo test -p vestrace-rig-spike
bash scripts/verify-rig-boundary.sh
```

The evidence report identifies exact baseline and alternate Rig targets, command outputs, Git commit SHAs and every failed or restricted criterion.

## Required H3 handoff

ADR-0002 must provide:

- selected production loop engine;
- whether Rig engine checkpoints are enabled;
- checkpoint compatibility policy;
- authoritative journal reconstruction procedure;
- allowed and forbidden Rig modules;
- canonical tool-call and result mapping;
- streaming normalization rule;
- error and usage normalization table;
- dependency pinning and upgrade policy;
- mandatory Rig-free build path;
- replacement strategy when Rig becomes incompatible.

## Explicit non-goals

H0-RIG does not:

- implement production H3 Model Runtime;
- call a real LLM provider;
- implement H4 Tool Runtime;
- execute Docker, shell or external connectors;
- adopt Rig memory, vector stores, workflows, scheduling or persistence;
- adopt `AgentRunner` as Vestrace orchestration;
- persist hidden chain-of-thought;
- change public Vestrace schemas;
- create database migrations;
- decide provider routing or pricing.

## Completion definition

H0-RIG is complete only when:

1. all experiments are reproducible offline;
2. every ADR-0001 requirement has explicit evidence;
3. critical failures cannot be hidden by retries or permissive fakes;
4. no-duplicate-side-effect and `Unknown` tests pass;
5. production crates remain Rig-free;
6. upgrade blast radius is measured;
7. the evidence report is committed;
8. ADR-0002 selects exactly one outcome;
9. H3 has an unambiguous model-loop direction.
