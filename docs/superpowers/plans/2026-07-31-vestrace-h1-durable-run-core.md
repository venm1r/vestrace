# Vestrace H1 Durable Run Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the authoritative, restart-safe `AgentRun` lifecycle with steps, append-only execution events, durable checkpoints, optimistic concurrency, leases, PostgreSQL work items and logical replay.

**Architecture:** Extend the existing v0.1 domain, application and infrastructure crates instead of introducing a second orchestration stack. Logical Run mutations are committed through one Vestrace-owned `RunStorePort`: the updated aggregate, exactly one canonical `RunEvent`, optional step/checkpoint changes and newly queued work items are written atomically. Operational worker ownership lives in a separate lease table so heartbeats do not increment logical `run_version` or conflict with state transitions.

**Tech Stack:** Existing Vestrace v0.1 Rust workspace, Rust Edition 2024, Tokio, Serde, Schemars, SQLx, PostgreSQL 17, tracing, proptest, testcontainers or the repository PostgreSQL test harness.

## Global Constraints

- Complete all five Vestrace v0.1 plans before implementing H1.
- PostgreSQL remains the source of truth for Run state, steps, events, checkpoints, leases and work items.
- Domain and application code must not depend on SQLx, Axum, Rig or a concrete model/tool provider.
- H1 does not implement planning, model invocation, tool execution, approvals, policy decisions or resource accounting; it provides stable references and lifecycle seams consumed by later plans.
- Every logical Run mutation increments `RunVersion` exactly once and appends exactly one `RunEvent` with the same sequence number.
- Lease acquisition and heartbeat are operational mutations and must not change `RunVersion`.
- A Run mutation, its event, checkpoint changes and queued work items commit in one database transaction.
- Run events and checkpoints are append-only. Applied migrations are forward-only and never edited.
- `WaitingForInput`, `WaitingForApproval` and `WaitingForDependency` are durable normal states, not errors.
- Terminal Run states cannot transition to another state.
- External side effects are outside H1; restart tests use deterministic in-process handlers only.
- All workspace-owned tables use forced RLS and the existing scoped transaction context.
- CI and acceptance tests must not require an external AI provider, Rig, MCP server or network access.
- Branch name: `feat/h1-durable-run-core`.

---

## Locked file structure additions

```text
crates/vestrace-domain/src/
  run/mod.rs
  run/status.rs
  run/step.rs
  run/event.rs
  run/checkpoint.rs
  run/work.rs

crates/vestrace-application/src/
  run/mod.rs
  run/commands.rs
  run/ports.rs
  run/coordinator.rs
  run/replay.rs
  run/worker.rs

crates/vestrace-application/tests/
  run_coordinator.rs
  run_replay.rs
  run_worker.rs

crates/vestrace-infrastructure/src/postgres/
  run/mod.rs
  run/store.rs
  run/lease.rs
  run/work_queue.rs

crates/vestrace-cli/src/commands/
  worker.rs

migrations/
  0019_agent_runs_and_steps.sql
  0020_run_events_and_checkpoints.sql
  0021_run_leases_and_work_items.sql
  0022_run_rls_and_indexes.sql

tests/
  run_lifecycle.rs
  run_atomicity.rs
  run_leases.rs
  run_replay.rs
  run_restart.rs
```

## Normative H1 contracts

### Logical version and journal sequence

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct RunVersion(u64);

impl RunVersion {
    pub const INITIAL: Self = Self(1);
    pub const fn value(self) -> u64 { self.0 }
    pub fn next(self) -> Result<Self, DomainError> {
        self.0.checked_add(1).map(Self).ok_or_else(|| {
            DomainError::InvalidArgument("run version overflow".into())
        })
    }
}
```

For every committed logical mutation:

```text
new_run_version = previous_run_version + 1
run_event.sequence = new_run_version
```

Creation is version `1` and emits `run.created` with sequence `1`.

### Operational leases

`run_leases` is separate from `agent_runs`. Lease heartbeat changes only `run_leases.heartbeat_at`, `lease_until` and `generation`. It never updates `agent_runs.run_version` and never emits a logical `RunEvent`.

### Atomic commit boundary

```rust
pub struct RunCommit {
    pub expected_version: RunVersion,
    pub run: AgentRun,
    pub step_changes: Vec<RunStepChange>,
    pub event: RunEvent,
    pub checkpoint: Option<RunCheckpoint>,
    pub enqueue: Vec<WorkItem>,
}

#[async_trait::async_trait]
pub trait RunStorePort: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        commit: NewRunCommit,
    ) -> Result<AgentRun, ApplicationError>;

    async fn load(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<AgentRunSnapshot, ApplicationError>;

    async fn commit(
        &self,
        context: &RequestContext,
        commit: RunCommit,
    ) -> Result<AgentRunSnapshot, ApplicationError>;

    async fn load_events(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        after: Option<ResumeCursor>,
    ) -> Result<Vec<RunEvent>, ApplicationError>;
}
```

No application service may update a Run table directly or append an event outside this port.

---

### Task 1: Add Run identifiers, statuses and transition tables

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/run/status.rs`
- Create: `crates/vestrace-domain/src/run/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline unit and property tests

**Interfaces:**
- Consumes existing `WorkspaceId`, `Timestamp` and `DomainError`.
- Produces `AgentRunId`, `RunStepId`, `RunEventId`, `RunCheckpointId`, `PlanRevisionId`, `AgentRuntimeSnapshotId`, `WorkItemId`, `WorkerId`, `BudgetSnapshotId`, `ResourceUsageSnapshotId` and `RunReferenceId`.
- Produces `RunVersion`, `ResumeCursor`, `RunExecutionMode`, `RunStatus` and `RunStepStatus`.

- [ ] **Step 1: Write failing transition tests**

```rust
#[test]
fn running_may_wait_for_input() {
    assert!(RunStatus::Running.can_transition_to(RunStatus::WaitingForInput));
}

#[test]
fn terminal_run_cannot_transition() {
    for status in [
        RunStatus::Succeeded,
        RunStatus::SucceededWithWarnings,
        RunStatus::Partial,
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::Expired,
    ] {
        assert!(!status.can_transition_to(RunStatus::Running));
    }
}

#[test]
fn unknown_step_may_be_reconciled() {
    assert!(RunStepStatus::Unknown.can_transition_to(RunStepStatus::Succeeded));
    assert!(RunStepStatus::Unknown.can_transition_to(RunStepStatus::Failed));
}
```

Run:

```bash
cargo test -p vestrace-domain run::status
```

Expected: FAIL because the Run types do not exist.

- [ ] **Step 2: Add the Run identifier newtypes**

Extend the existing domain ID macro. Every new identifier must implement `new`, `from_uuid`, `as_uuid`, `Display`, `FromStr`, `Default`, Serde and Schemars exactly like existing IDs.

- [ ] **Step 3: Implement Run and step statuses**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Preparing,
    Running,
    WaitingForInput,
    WaitingForApproval,
    WaitingForDependency,
    Paused,
    PausedPolicyChanged,
    Succeeded,
    SucceededWithWarnings,
    Partial,
    Failed,
    Cancelled,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStepStatus {
    Pending,
    Ready,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Skipped,
    Cancelled,
    Unknown,
}
```

Implement explicit `can_transition_to` match expressions. Do not infer transitions from enum ordering.

Run transitions:

```text
Created -> Preparing | Cancelled
Preparing -> Running | WaitingForInput | Failed | Cancelled
Running -> WaitingForInput | WaitingForApproval | WaitingForDependency |
           Paused | PausedPolicyChanged | Succeeded | SucceededWithWarnings |
           Partial | Failed | Cancelled
WaitingForInput -> Running | Paused | Cancelled | Expired
WaitingForApproval -> Running | Paused | Cancelled | Expired
WaitingForDependency -> Running | Paused | Failed | Cancelled | Expired
Paused -> Running | Cancelled | Expired
PausedPolicyChanged -> Running | Cancelled | Expired
terminal -> no transitions
```

Step transitions:

```text
Pending -> Ready | Skipped | Cancelled
Ready -> Running | Skipped | Cancelled
Running -> Waiting | Succeeded | Failed | Cancelled | Unknown
Waiting -> Ready | Running | Failed | Cancelled | Unknown
Unknown -> Waiting | Succeeded | Failed | Cancelled
terminal -> no transitions
```

A failed step is retried by `RunStep::retry`, which increments `attempt` and returns it to `Ready`; it is not represented as an ordinary `Failed -> Ready` transition.

- [ ] **Step 4: Implement `RunVersion`, `ResumeCursor` and execution mode**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExecutionMode {
    Direct,
    Guided,
    Workflow,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct ResumeCursor(u64);
```

`ResumeCursor::from_event_sequence` and `value` are const methods. Cursor `0` means before the first event.

- [ ] **Step 5: Add property tests for terminal-state closure and version monotonicity**

Use `proptest` to confirm `RunVersion::next()` is strictly greater for all generated values below `u64::MAX`, and no terminal status accepts any generated target status.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain run
cargo fmt --all --check
git add crates/vestrace-domain
git commit -m "feat(run): add durable run state types"
```

---

### Task 2: Implement `AgentRun`, `RunStep` and forward-compatible references

**Files:**
- Create: `crates/vestrace-domain/src/run/step.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Test: inline unit tests

**Interfaces:**
- Consumes the H1 status and identifier types.
- Produces `AgentRun`, `ParentRunLink`, `RunStep`, `RunActorRef`, `RunReference`, `RunFailure` and `RunTerminalResult`.
- Produces domain mutation methods that return typed `RunEventPayload` values defined in Task 3.

- [ ] **Step 1: Write failing aggregate-invariant tests**

```rust
#[test]
fn objective_must_not_be_blank() {
    let result = AgentRun::create(NewAgentRun {
        objective: "   ".into(),
        ..new_run_fixture()
    });
    assert!(result.is_err());
}

#[test]
fn expected_version_is_required_for_transition() {
    let mut run = run_fixture(RunStatus::Running, RunVersion::INITIAL);
    let result = run.transition(
        RunVersion::from_value(2).unwrap(),
        RunStatus::Paused,
        now(),
    );
    assert!(matches!(result, Err(DomainError::RevisionConflict { .. })));
}

#[test]
fn parent_link_cannot_reference_same_run() {
    let id = AgentRunId::new();
    assert!(ParentRunLink::new(id, RunStepId::new(), id).is_err());
}
```

- [ ] **Step 2: Implement references and actor types**

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum RunActorRef {
    Principal(PrincipalId),
    AgentSnapshot(AgentRuntimeSnapshotId),
    Worker(WorkerId),
    System,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunReferenceKind {
    Context,
    Artifact,
    Approval,
    ModelInvocation,
    ToolInvocation,
    SubRun,
    External,
}

pub struct RunReference {
    pub id: RunReferenceId,
    pub kind: RunReferenceKind,
}
```

`RunReference` is a stable cross-plan reference. Later plans may add typed adapters, but H1 persistence never interprets the referenced object.

- [ ] **Step 3: Implement `AgentRun`**

```rust
pub struct AgentRun {
    pub id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub objective: String,
    pub coordinator_snapshot_id: AgentRuntimeSnapshotId,
    pub active_plan_revision_id: Option<PlanRevisionId>,
    pub execution_mode: RunExecutionMode,
    pub status: RunStatus,
    pub current_step_id: Option<RunStepId>,
    pub checkpoint_id: Option<RunCheckpointId>,
    pub parent: Option<ParentRunLink>,
    pub root_run_id: AgentRunId,
    pub budget_snapshot_id: Option<BudgetSnapshotId>,
    pub resource_usage_snapshot_id: Option<ResourceUsageSnapshotId>,
    pub version: RunVersion,
    pub result: Option<RunTerminalResult>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}
```

`budget_snapshot_id` and `resource_usage_snapshot_id` are nullable UUID references without foreign keys in H1. H2 creates their tables and adds validated foreign keys in a new migration.

`AgentRun::create` sets status `Created`, version `1`, `root_run_id = id` for root runs, and rejects objectives that are empty after trimming or exceed 32 KiB UTF-8 bytes.

- [ ] **Step 4: Implement optimistic transitions**

```rust
impl AgentRun {
    pub fn transition(
        &mut self,
        expected: RunVersion,
        next: RunStatus,
        at: Timestamp,
    ) -> Result<RunStatusChange, DomainError>;

    pub fn attach_plan(
        &mut self,
        expected: RunVersion,
        plan: PlanRevisionId,
        at: Timestamp,
    ) -> Result<RunPlanChange, DomainError>;

    pub fn select_current_step(
        &mut self,
        expected: RunVersion,
        step_id: Option<RunStepId>,
        at: Timestamp,
    ) -> Result<RunCurrentStepChange, DomainError>;
}
```

Every method checks `expected == self.version`, validates the mutation, advances version exactly once and returns data sufficient to create one event. Terminal transitions require `RunTerminalResult`; non-terminal transitions reject a terminal result.

- [ ] **Step 5: Implement `RunStep`**

```rust
pub struct RunStep {
    pub id: RunStepId,
    pub run_id: AgentRunId,
    pub plan_step_reference: Option<String>,
    pub assigned_actor: RunActorRef,
    pub input_references: Vec<RunReference>,
    pub status: RunStepStatus,
    pub attempt: u32,
    pub output_references: Vec<RunReference>,
    pub error: Option<RunFailure>,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}
```

`plan_step_reference` is an opaque, non-empty identifier limited to 256 bytes until H5 introduces typed plan nodes. `attempt` begins at `1`. `retry` is allowed only from `Failed`, clears terminal timing/error, increments with checked arithmetic and returns status to `Ready`.

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p vestrace-domain run
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain/src/run
git commit -m "feat(run): add run and step aggregates"
```

---

### Task 3: Add canonical Run events, checkpoints and logical replay

**Files:**
- Create: `crates/vestrace-domain/src/run/event.rs`
- Create: `crates/vestrace-domain/src/run/checkpoint.rs`
- Create: `crates/vestrace-application/src/run/replay.rs`
- Create: `crates/vestrace-application/tests/run_replay.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces `RunEvent`, `RunEventPayload`, `RunCheckpoint`, `RunCheckpointPayloadV1`, `RunProjection` and `replay_run`.
- Enforces event sequence equality with the post-mutation `RunVersion`.

- [ ] **Step 1: Write failing event and replay tests**

```rust
#[test]
fn event_sequence_must_equal_run_version() {
    let result = RunEvent::new(
        run_id(),
        workspace_id(),
        RunVersion::from_value(3).unwrap(),
        ResumeCursor::from_value(2),
        RunActorRef::System,
        RunEventPayload::RunPaused,
        correlation_id(),
        None,
        now(),
    );
    assert!(result.is_err());
}

#[test]
fn replay_reconstructs_status_current_step_and_version() {
    let projection = replay_run(&scripted_events()).unwrap();
    assert_eq!(projection.status, RunStatus::WaitingForInput);
    assert_eq!(projection.current_step_id, Some(step_id(2)));
    assert_eq!(projection.version.value(), 7);
}
```

- [ ] **Step 2: Implement typed event payloads**

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RunEventPayload {
    RunCreated { objective: String, execution_mode: RunExecutionMode, coordinator_snapshot_id: AgentRuntimeSnapshotId, parent: Option<ParentRunLink> },
    RunStatusChanged { from: RunStatus, to: RunStatus, result: Option<RunTerminalResult> },
    PlanAttached { previous: Option<PlanRevisionId>, current: PlanRevisionId },
    StepAdded { step: RunStep },
    StepStatusChanged { step_id: RunStepId, from: RunStepStatus, to: RunStepStatus, attempt: u32 },
    CurrentStepChanged { previous: Option<RunStepId>, current: Option<RunStepId> },
    CheckpointCreated { checkpoint_id: RunCheckpointId, resume_cursor: ResumeCursor },
    WorkItemQueued { work_item_id: WorkItemId, kind: WorkItemKind },
}
```

The event type string is derived deterministically from the payload and is not supplied by the caller.

- [ ] **Step 3: Implement `RunEvent`**

```rust
pub struct RunEvent {
    pub id: RunEventId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub sequence: ResumeCursor,
    pub run_version: RunVersion,
    pub actor: RunActorRef,
    pub payload: RunEventPayload,
    pub correlation_id: CorrelationId,
    pub causation_event_id: Option<RunEventId>,
    pub occurred_at: Timestamp,
}
```

Constructor rules:

- sequence equals `run_version.value()`;
- sequence is at least `1`;
- causation cannot reference the event itself;
- `RunCreated` is sequence/version `1` only;
- non-creation events require sequence greater than `1`.

- [ ] **Step 4: Implement versioned checkpoints**

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "schema", content = "state")]
pub enum RunCheckpointPayload {
    #[serde(rename = "run-checkpoint/v1")]
    V1(RunCheckpointPayloadV1),
}

pub struct RunCheckpointPayloadV1 {
    pub completed_steps: Vec<RunStepId>,
    pub ready_steps: Vec<RunStepId>,
    pub pending_work_items: Vec<WorkItemId>,
    pub pending_tool_calls: Vec<RunReferenceId>,
    pub pending_approvals: Vec<RunReferenceId>,
    pub active_subruns: Vec<AgentRunId>,
    pub context_references: Vec<RunReference>,
    pub artifact_references: Vec<RunReference>,
    pub budget_snapshot_id: Option<BudgetSnapshotId>,
}

pub struct RunCheckpoint {
    pub id: RunCheckpointId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub run_version: RunVersion,
    pub active_plan_revision_id: Option<PlanRevisionId>,
    pub resume_cursor: ResumeCursor,
    pub payload: RunCheckpointPayload,
    pub created_at: Timestamp,
}
```

Checkpoint version and cursor must match. Reject duplicate IDs inside each vector so resume behavior is deterministic.

- [ ] **Step 5: Implement the pure replay reducer**

```rust
pub fn replay_run(events: &[RunEvent]) -> Result<RunProjection, ApplicationError>;
```

Requirements:

- first event is `RunCreated` at sequence `1`;
- events are strictly contiguous by sequence;
- all events share workspace and run IDs;
- payload transitions are revalidated through the domain transition tables;
- projection contains status, version, plan, current step, known steps, latest checkpoint reference and terminal result;
- replay performs no I/O and invokes no model/tool code.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain run
cargo test -p vestrace-application --test run_replay
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(run): add journal checkpoints and replay"
```

---

### Task 4: Define Run commands, ports and coordinator services

**Files:**
- Create: `crates/vestrace-application/src/run/commands.rs`
- Create: `crates/vestrace-application/src/run/ports.rs`
- Create: `crates/vestrace-application/src/run/coordinator.rs`
- Create: `crates/vestrace-application/tests/run_coordinator.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`

**Interfaces:**
- Produces `CreateRun`, `TransitionRun`, `AddRunStep`, `TransitionRunStep`, `CreateCheckpoint`, `PauseRun`, `ResumeRun` and `CancelRun` commands.
- Produces `RunStorePort`, `RunLeasePort`, `WorkQueuePort`, `RunClockPort` and `RunCoordinator`.
- Produces `NewRunCommit`, `RunCommit`, `RunStepChange` and `AgentRunSnapshot`.

- [ ] **Step 1: Define exact command DTOs**

```rust
pub struct CreateRun {
    pub objective: String,
    pub coordinator_snapshot_id: AgentRuntimeSnapshotId,
    pub execution_mode: RunExecutionMode,
    pub parent: Option<ParentRunLink>,
    pub idempotency_key: String,
}

pub struct TransitionRun {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub target: RunStatus,
    pub result: Option<RunTerminalResult>,
    pub actor: RunActorRef,
    pub causation_event_id: Option<RunEventId>,
}

pub struct CreateCheckpoint {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub payload: RunCheckpointPayloadV1,
    pub actor: RunActorRef,
}
```

All state-changing public commands carry an idempotency key at the interface layer. Internal continuation commands derive a deterministic key from run ID, expected version and action kind.

- [ ] **Step 2: Define storage and operational ports**

Use the normative `RunStorePort` from this plan. Add:

```rust
#[async_trait::async_trait]
pub trait RunLeasePort: Send + Sync {
    async fn acquire(&self, context: &RequestContext, request: AcquireRunLease) -> Result<RunLease, ApplicationError>;
    async fn heartbeat(&self, context: &RequestContext, lease: &RunLease, extend_until: Timestamp) -> Result<RunLease, ApplicationError>;
    async fn release(&self, context: &RequestContext, lease: &RunLease) -> Result<(), ApplicationError>;
}

#[async_trait::async_trait]
pub trait WorkQueuePort: Send + Sync {
    async fn lease_next(&self, context: &RequestContext, request: LeaseWorkRequest) -> Result<Option<WorkItem>, ApplicationError>;
    async fn complete(&self, context: &RequestContext, item: &WorkItem, at: Timestamp) -> Result<(), ApplicationError>;
    async fn retry(&self, context: &RequestContext, item: &WorkItem, available_at: Timestamp, error: RunFailure) -> Result<(), ApplicationError>;
    async fn dead_letter(&self, context: &RequestContext, item: &WorkItem, error: RunFailure, at: Timestamp) -> Result<(), ApplicationError>;
}
```

`RunClockPort::now()` makes lease and lifecycle tests deterministic.

- [ ] **Step 3: Implement `RunCoordinator::create` with an in-memory fake**

`create` must build:

1. `AgentRun` version `1`;
2. `RunEventPayload::RunCreated` sequence `1`;
3. initial `WorkItemKind::AdvanceRun` with deterministic idempotency key `run:{run_id}:version:1:advance`;
4. one `NewRunCommit` passed to `RunStorePort::create`.

The fake store records the commit and returns the snapshot. Test that a duplicate external idempotency key returns the original Run instead of creating a second one.

- [ ] **Step 4: Implement transition, step and checkpoint services**

Each service:

1. loads `AgentRunSnapshot`;
2. validates expected version;
3. mutates the pure domain object;
4. constructs exactly one event whose sequence equals the new version;
5. optionally adds step/checkpoint changes and follow-up work;
6. calls `RunStorePort::commit` once.

Checkpoint creation advances the Run version, stores the checkpoint with that version/cursor and sets `run.checkpoint_id` in the same commit.

- [ ] **Step 5: Implement lifecycle convenience commands**

```rust
pub async fn pause(&self, context: &RequestContext, command: PauseRun) -> Result<AgentRunSnapshot, ApplicationError>;
pub async fn resume(&self, context: &RequestContext, command: ResumeRun) -> Result<AgentRunSnapshot, ApplicationError>;
pub async fn cancel(&self, context: &RequestContext, command: CancelRun) -> Result<AgentRunSnapshot, ApplicationError>;
```

`resume` queues a deterministic `ResumeRun` work item. `pause` and `cancel` queue no new execution work. Cancellation preserves existing result references and explicitly states that external actions are not rolled back.

- [ ] **Step 6: Verify fake-store atomic command construction and commit**

```bash
cargo test -p vestrace-application --test run_coordinator
cargo clippy -p vestrace-application --all-targets -- -D warnings
git add crates/vestrace-application
git commit -m "feat(run): add coordinator commands and ports"
```

---

### Task 5: Add PostgreSQL Run, step, journal and checkpoint schema

**Files:**
- Create: `migrations/0019_agent_runs_and_steps.sql`
- Create: `migrations/0020_run_events_and_checkpoints.sql`
- Create: `tests/run_lifecycle.rs`
- Create: `tests/run_replay.rs`
- Modify: `tests/support/mod.rs`

**Interfaces:**
- Produces tables `agent_runs`, `run_steps`, `run_events` and `run_checkpoints`.
- Uses text status columns with explicit checks rather than PostgreSQL enum types.
- Enforces unique `(run_id, sequence)` and append-only events/checkpoints.

- [ ] **Step 1: Write failing schema tests**

Test all of the following before adding migrations:

- blank objective is rejected;
- `run_version < 1` is rejected;
- parent Run cannot cross workspace;
- current step must belong to the same Run;
- duplicate event sequence is rejected;
- event sequence different from event `run_version` is rejected;
- event/checkpoint update and delete fail for the application role;
- checkpoint cursor different from checkpoint `run_version` is rejected.

Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_lifecycle
```

Expected: FAIL because migrations `0019` and `0020` do not exist.

- [ ] **Step 2: Create `0019_agent_runs_and_steps.sql`**

Create `agent_runs` with:

```text
id, workspace_id, objective, coordinator_snapshot_id,
active_plan_revision_id, execution_mode, status,
current_step_id, checkpoint_id, parent_run_id, parent_step_id,
root_run_id, budget_snapshot_id, resource_usage_snapshot_id,
run_version, result JSONB, created_at, updated_at, finished_at
```

Use checks for non-blank objective, byte length at most `32768`, valid execution modes/statuses, version at least `1`, and terminal/finished-time consistency.

Create `run_steps` with:

```text
id, workspace_id, run_id, plan_step_reference, assigned_actor JSONB,
input_references JSONB, status, attempt, output_references JSONB,
error JSONB, created_at, started_at, finished_at
```

Use deferred constraint triggers for `current_step_id`, `parent_run_id` and `parent_step_id` ownership so aggregate rows can be inserted atomically.

- [ ] **Step 3: Create `0020_run_events_and_checkpoints.sql`**

`run_events` columns:

```text
id, workspace_id, run_id, sequence, run_version,
event_type, actor JSONB, payload JSONB,
correlation_id, causation_event_id, occurred_at
```

Required constraints:

```sql
CHECK (sequence >= 1),
CHECK (run_version >= 1),
CHECK (sequence = run_version),
UNIQUE (run_id, sequence)
```

`run_checkpoints` columns:

```text
id, workspace_id, run_id, run_version,
active_plan_revision_id, resume_cursor, payload JSONB, created_at
```

Require `run_version = resume_cursor` and unique `(run_id, run_version)`.

Add `agent_runs.checkpoint_id` foreign key after the checkpoint table exists. Use a trigger to reject ordinary `UPDATE` and `DELETE` on `run_events` and `run_checkpoints`.

- [ ] **Step 4: Add database replay fixture**

Insert a seven-event scripted Run, load ordered events through SQL and pass them to `replay_run`. Assert the projection matches the persisted Run status, plan, current step and version.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_lifecycle --test run_replay
git add migrations/0019_agent_runs_and_steps.sql migrations/0020_run_events_and_checkpoints.sql tests
git commit -m "feat(storage): add durable run journal schema"
```

---

### Task 6: Add operational leases and durable work items

**Files:**
- Create: `crates/vestrace-domain/src/run/work.rs`
- Create: `migrations/0021_run_leases_and_work_items.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/run/lease.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/run/work_queue.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/run/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/run_leases.rs`

**Interfaces:**
- Produces `RunLease`, `AcquireRunLease`, `WorkItem`, `WorkItemKind`, `WorkItemStatus`, `RunWorkPayload` and PostgreSQL implementations of `RunLeasePort` and `WorkQueuePort`.

- [ ] **Step 1: Write failing lease and queue tests**

Test:

1. worker A acquires an unleased Run;
2. worker B cannot acquire before expiry;
3. worker B acquires after expiry with a greater generation;
4. stale worker A cannot heartbeat or release worker B's generation;
5. heartbeat does not change `agent_runs.run_version`;
6. two queue consumers using `FOR UPDATE SKIP LOCKED` receive different work items;
7. duplicate `(workspace_id, idempotency_key)` returns the existing work item;
8. expired leased work becomes available without creating a duplicate row.

- [ ] **Step 2: Implement work domain types**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    AdvanceRun,
    ResumeRun,
    CreateCheckpoint,
    ExpireRun,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Ready,
    Leased,
    Completed,
    Failed,
    DeadLetter,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RunWorkPayload {
    Advance,
    Resume { checkpoint_id: Option<RunCheckpointId> },
    CreateCheckpoint,
    Expire,
}
```

`WorkItemKind` and payload must agree; constructor rejects mismatches.

- [ ] **Step 3: Create `00121_run_leases_and_work_items.sql`**

Use the actual filename `migrations/0021_run_leases_and_work_items.sql`.

`run_leases`:

```text
workspace_id, run_id, worker_id, generation,
acquired_at, heartbeat_at, lease_until
```

Primary key is `run_id`. `generation >= 1`. A deferred trigger verifies the Run belongs to the same workspace.

`work_items`:

```text
id, workspace_id, run_id, step_id, expected_run_version,
kind, payload JSONB, required_capabilities JSONB,
status, priority, available_at, deadline,
attempt, max_attempts, lease_owner, lease_until,
idempotency_key, last_error JSONB, created_at, updated_at, completed_at
```

Use unique `(workspace_id, idempotency_key)`, `attempt >= 0`, `max_attempts >= 1`, and text status/kind checks.

- [ ] **Step 4: Implement lease SQL with generation fencing**

Acquire uses one statement or transaction equivalent to:

```sql
INSERT INTO run_leases (... generation ...)
VALUES (..., 1, ...)
ON CONFLICT (run_id) DO UPDATE
SET worker_id = EXCLUDED.worker_id,
    generation = run_leases.generation + 1,
    acquired_at = EXCLUDED.acquired_at,
    heartbeat_at = EXCLUDED.heartbeat_at,
    lease_until = EXCLUDED.lease_until
WHERE run_leases.lease_until <= EXCLUDED.acquired_at
RETURNING ...;
```

Heartbeat and release include `run_id`, `worker_id` and `generation` in the predicate. Zero affected rows maps to stable `lease_lost` application error.

- [ ] **Step 5: Implement work leasing**

Select ready or expired-leased items ordered by priority descending, `available_at`, creation time and ID, using `FOR UPDATE SKIP LOCKED`. Increment `attempt` only when a handler begins, not while polling. Items exceeding `max_attempts` move to `DeadLetter`.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_leases
cargo test -p vestrace-domain run::work
git add crates/vestrace-domain crates/vestrace-infrastructure migrations/0021_run_leases_and_work_items.sql tests/run_leases.rs
git commit -m "feat(run): add leases and durable work queue"
```

---

### Task 7: Implement atomic PostgreSQL `RunStorePort`

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/run/store.rs`
- Create: `tests/run_atomicity.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run/mod.rs`

**Interfaces:**
- Consumes `NewRunCommit`, `RunCommit`, `AgentRunSnapshot` and the existing scoped PostgreSQL transaction helper.
- Produces `PostgresRunStore` implementing `RunStorePort`.

- [ ] **Step 1: Write failing optimistic-concurrency tests**

Create a Run at version `1`. Build two transitions with expected version `1`. Commit them concurrently. Assert exactly one succeeds, the other returns `revision_conflict`, persisted version is `2`, and exactly one sequence-`2` event exists.

- [ ] **Step 2: Write failing atomic-rollback test**

Prepare a valid Run transition plus a work item whose idempotency key conflicts with an existing item. Commit and assert failure. Reload and verify:

- Run remains at the old version/status;
- no event for the proposed version exists;
- no step/checkpoint changes were persisted;
- the pre-existing work item is unchanged.

- [ ] **Step 3: Implement `create` transaction**

Within one scoped SQLx transaction:

1. check external command idempotency through the existing v0.1 idempotency repository;
2. insert `agent_runs` version `1`;
3. insert initial steps if supplied;
4. insert `run.created` sequence `1`;
5. insert initial work items with conflict-safe idempotency;
6. store idempotent command result;
7. commit.

A duplicate external idempotency key returns the previously stored `AgentRunId` and snapshot.

- [ ] **Step 4: Implement `commit` transaction**

Use:

```sql
UPDATE agent_runs
SET ...,
    run_version = $new_version,
    updated_at = $updated_at
WHERE workspace_id = $workspace_id
  AND id = $run_id
  AND run_version = $expected_version;
```

Zero rows maps to `revision_conflict` after loading the current version. Then apply step changes, append the event, insert checkpoint and enqueue work items before commit. Never retry the transaction automatically after an ambiguous connection failure; return `operation_unknown` for caller reconciliation.

- [ ] **Step 5: Implement snapshot loading**

`load` returns the Run, ordered steps, latest checkpoint metadata and no lease data. Lease is queried separately because it is operational state.

- [ ] **Step 6: Verify and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_atomicity
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add crates/vestrace-infrastructure tests/run_atomicity.rs
git commit -m "feat(run): persist atomic run mutations"
```

---

### Task 8: Add the restart-safe worker loop and handler seam

**Files:**
- Create: `crates/vestrace-application/src/run/worker.rs`
- Create: `crates/vestrace-application/tests/run_worker.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-cli/src/commands/worker.rs`

**Interfaces:**
- Produces `RunWorkHandler`, `RunWorkHandlerRegistry`, `RunWorker`, `RunWorkerConfig` and `RunWorkOutcome`.
- Worker consumes `WorkQueuePort`, `RunLeasePort`, `RunStorePort`, `RunClockPort` and tracing.
- H1 production registry contains only lifecycle/maintenance handlers. Tests register deterministic execution handlers; later plans register model/tool/planning handlers.

- [ ] **Step 1: Write failing worker-fencing tests**

Use in-memory fake ports and a manual clock. Assert:

- worker acquires work, then the matching Run lease, before invoking a handler;
- handler is not invoked when Run version differs from `expected_run_version`;
- stale lease generation prevents commit;
- retryable handler error schedules retry with bounded backoff;
- non-retryable error dead-letters the work item;
- cancellation before handler completion prevents follow-up work from being enqueued.

- [ ] **Step 2: Define the handler contract**

```rust
#[async_trait::async_trait]
pub trait RunWorkHandler: Send + Sync {
    fn kind(&self) -> WorkItemKind;

    async fn handle(
        &self,
        context: &RequestContext,
        snapshot: AgentRunSnapshot,
        item: &WorkItem,
        lease: &RunLease,
    ) -> Result<RunWorkOutcome, ApplicationError>;
}

pub enum RunWorkOutcome {
    Completed,
    Retry { available_at: Timestamp, error: RunFailure },
    DeadLetter { error: RunFailure },
    NoopStaleVersion,
}
```

Handlers never mark queue rows directly. `RunWorker` performs queue completion/retry after the handler returns.

- [ ] **Step 3: Implement one worker iteration**

```rust
pub async fn run_once(&self, context: &RequestContext) -> Result<WorkerPollOutcome, ApplicationError>;
```

Sequence:

1. lease one work item;
2. load Run snapshot;
3. compare expected Run version;
4. acquire Run lease with configured TTL;
5. dispatch handler;
6. heartbeat when the handler exposes a checkpoint boundary;
7. complete/retry/dead-letter work;
8. release Run lease;
9. emit structured tracing fields without objective or payload contents.

Use a guard that attempts lease release on every normal error path. Process termination is handled by lease expiry.

- [ ] **Step 4: Add bounded retry policy**

Default deterministic backoff for H1 maintenance work:

```text
attempt 1 -> 1 second
attempt 2 -> 5 seconds
attempt 3 -> 30 seconds
attempt 4+ -> dead letter
```

Store the chosen `available_at` and error code. Do not use random jitter in unit tests; production may add bounded jitter through configuration later.

- [ ] **Step 5: Wire `vestrace worker`**

The command loads existing configuration, creates PostgreSQL ports and runs a loop with graceful shutdown. Add configuration fields:

```text
worker.id
worker.poll_interval_ms
worker.run_lease_ttl_seconds
worker.heartbeat_interval_seconds
worker.max_concurrency
```

H1 supports `max_concurrency = 1` in the command implementation. Reject other values with a clear configuration error; concurrent worker tasks are added only after the single-worker invariants pass.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test run_worker
cargo test -p vestrace-cli --test cli
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/vestrace-application crates/vestrace-cli
git commit -m "feat(run): add restart-safe worker loop"
```

---

### Task 9: Add forced RLS, indexes and cross-workspace negative tests

**Files:**
- Create: `migrations/0022_run_rls_and_indexes.sql`
- Modify: `tests/run_lifecycle.rs`
- Modify: `tests/run_leases.rs`
- Modify: `tests/run_atomicity.rs`

**Interfaces:**
- Applies forced RLS to all H1 tables.
- Adds query indexes used by snapshots, replay, leasing and recovery.

- [ ] **Step 1: Add failing cross-workspace tests**

With two scoped request contexts, verify workspace B cannot:

- load or update workspace A's Run;
- append an event to A's Run;
- create a step/checkpoint for A's Run;
- acquire A's Run lease;
- lease A's work item;
- infer A's existence through a unique-key error.

Expected behavior is empty/not-found or authorization-safe conflict, never leaked row contents.

- [ ] **Step 2: Create forced RLS policies**

Apply and force RLS on:

```text
agent_runs
run_steps
run_events
run_checkpoints
run_leases
work_items
```

Use the existing `vestrace_current_workspace_id()` helper. Administrative purge/recovery roles remain separate and are not used by ordinary repositories.

- [ ] **Step 3: Add required indexes**

```text
agent_runs(workspace_id, status, updated_at)
agent_runs(workspace_id, parent_run_id)
run_steps(workspace_id, run_id, status)
run_events(workspace_id, run_id, sequence)
run_checkpoints(workspace_id, run_id, run_version DESC)
run_leases(workspace_id, lease_until)
work_items(workspace_id, status, available_at, priority DESC)
work_items(workspace_id, run_id, status)
work_items(lease_until) WHERE status = 'leased'
```

Use `EXPLAIN` assertions only for stable index presence, not fragile exact cost values.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_lifecycle --test run_leases --test run_atomicity
git add migrations/0022_run_rls_and_indexes.sql tests
git commit -m "feat(security): isolate durable runs with RLS"
```

---

### Task 10: Prove pause, resume, restart and logical replay end to end

**Files:**
- Create: `tests/run_restart.rs`
- Modify: `tests/run_replay.rs`
- Modify: `docker-compose.yml`
- Modify: `.github/workflows/ci.yml`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md`

**Interfaces:**
- Produces the H1 acceptance scenario and final verification commands.
- Produces no new public runtime contracts.

- [ ] **Step 1: Implement a deterministic checkpointing test handler**

Inside `tests/run_restart.rs`, define a handler available only to the test binary:

```text
Advance 1:
  Created -> Preparing
  add step A and step B
  step A Ready -> Running -> Succeeded
  create checkpoint at resulting version
  enqueue next AdvanceRun

Advance 2:
  Preparing -> Running
  step B Ready -> Running
  simulate process loss after durable checkpoint/work commit

Recovered Advance:
  acquire expired work and Run leases with worker B
  resume from latest checkpoint
  step B -> Succeeded
  Run -> Succeeded
```

The handler uses only Run application services. It performs no external side effects.

- [ ] **Step 2: Write the restart acceptance test**

The test must:

1. create one Run through `RunCoordinator`;
2. execute the first work item with worker A;
3. assert a durable checkpoint exists;
4. simulate worker A loss by advancing the manual/database clock beyond both leases without completing the active queue item;
5. construct a new worker B with fresh port instances;
6. lease and finish the Run;
7. assert final status `Succeeded`;
8. assert event sequences are exactly `1..=run_version` with no gaps or duplicates;
9. replay all events and compare the logical projection with persisted state;
10. assert the original checkpoint remains immutable;
11. assert worker A's stale generation cannot heartbeat, release or commit.

- [ ] **Step 3: Add pause and resume branch to the acceptance suite**

Create a second Run, pause it after the first checkpoint, verify no execution work is leased while paused, issue `ResumeRun`, and verify execution continues from the recorded cursor rather than recreating completed steps.

- [ ] **Step 4: Add cancel behavior test**

Cancel a Run with one completed and one ready step. Verify:

- Run becomes `Cancelled`;
- ready/pending steps become `Cancelled` through explicit step events;
- completed step remains `Succeeded`;
- existing output references remain available;
- no new `AdvanceRun` work is created;
- result states that prior external effects, if any, were not rolled back.

- [ ] **Step 5: Add CI PostgreSQL acceptance command**

CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_restart --test run_replay
```

The Compose worker health check verifies the process can reach PostgreSQL and poll the queue; it does not require a pending item.

- [ ] **Step 6: Update the Harness roadmap with H1 completion evidence**

Add the normative migration sequence `0019`–`0022` and link this detailed plan under H1. Do not mark H1 complete until the acceptance commands pass on the implementation branch.

- [ ] **Step 7: Final verification and commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_lifecycle --test run_atomicity --test run_leases --test run_replay --test run_restart
docker compose config
docker compose up -d postgres
docker compose run --rm migrate
docker compose up -d server worker
docker compose ps

git add tests/run_restart.rs tests/run_replay.rs docker-compose.yml .github/workflows/ci.yml docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md
git commit -m "test(run): prove durable restart and replay"
```

Expected:

- all commands exit `0`;
- `server`, `worker` and PostgreSQL are healthy;
- the restart scenario completes without duplicate logical mutations;
- replay matches persisted state.

---

## H1 exit gate

H1 is complete only when a fresh PostgreSQL database and fresh process instances demonstrate:

```text
Create Run
→ atomically persist run.created and initial work
→ lease work and Run
→ execute deterministic steps
→ checkpoint
→ lose worker
→ expire leases
→ resume with another worker
→ complete Run
→ replay journal
→ obtain the same logical state
```

Required evidence:

- one event per logical Run version;
- no event sequence gaps or duplicates;
- concurrent stale updates fail with `revision_conflict`;
- heartbeat does not alter logical Run version;
- stale lease generation cannot mutate operational state;
- failed multi-table commits leave no partial state;
- paused Runs do not advance until explicit resume;
- cancelled Runs preserve completed outputs and stop new execution;
- RLS prevents every tested cross-workspace read and write;
- restart acceptance and logical replay tests pass.

## Explicit non-goals for H1

- model providers or model-loop engines;
- Rig integration or the H0-RIG spike;
- actual `ExecutionPlan` nodes or validation;
- Policy Engine, approval grants or capability evaluation;
- real resource reservations and budget reconciliation;
- tool definitions, tool execution or sandboxing;
- actual subagent delegation behavior;
- context assembly, Artifact Store or memory consolidation;
- conversations, triggers, channels or public Harness API;
- distributed consensus or cross-region execution.
