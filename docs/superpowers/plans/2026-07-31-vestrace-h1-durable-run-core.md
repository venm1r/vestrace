# Vestrace H1 Durable Run Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the authoritative, restart-safe `AgentRun` lifecycle with steps, append-only execution events, durable checkpoints, optimistic concurrency, leases, PostgreSQL work items and logical replay.

**Architecture:** Extend the existing v0.1 domain, application and infrastructure crates. Logical Run mutations are committed through one Vestrace-owned `RunStorePort`: the updated aggregate, exactly one canonical `RunEvent`, optional step/checkpoint changes and follow-up work are persisted atomically. Worker leases and queue polling are operational state in separate tables, so heartbeats and retries do not increment logical `RunVersion` or affect replay.

**Tech Stack:** Existing Vestrace v0.1 Rust workspace, Rust Edition 2024, Tokio, Serde, Schemars, SQLx, PostgreSQL 17, tracing, proptest and the repository PostgreSQL integration-test harness.

## Global Constraints

- Complete all five Vestrace v0.1 plans before implementing H1.
- PostgreSQL is authoritative for Run state, steps, events, checkpoints, leases and work items.
- Domain and application crates must not depend on SQLx, Axum, Rig or concrete model/tool providers.
- H1 does not implement plans, models, tools, policies, approvals, budgets, artifacts or delegation behavior; it creates stable IDs and ports for later plans.
- Creation starts at `RunVersion(1)` and emits `run.created` with sequence `1`.
- Every later logical mutation increments `RunVersion` once and appends exactly one `RunEvent` whose sequence equals the new version.
- Lease acquisition, heartbeat, queue leasing, retry scheduling and queue completion are operational changes and do not emit `RunEvent` or change `RunVersion`.
- A logical Run mutation, its event, step/checkpoint changes and newly created follow-up work items commit in one transaction.
- Run events and checkpoints are append-only. Applied migrations are never edited.
- `WaitingForInput`, `WaitingForApproval` and `WaitingForDependency` are durable normal states.
- Terminal Run states cannot transition.
- External side effects are outside H1; restart tests use deterministic in-process handlers.
- All H1 workspace tables use forced RLS and existing scoped transactions.
- CI must not require an AI provider, Rig, MCP server or internet access.
- Branch: `feat/h1-durable-run-core`.

---

## Locked file structure

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

## Normative contracts

### Logical versioning

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct RunVersion(u64);

impl RunVersion {
    pub const INITIAL: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            Err(DomainError::InvalidArgument("run version must be positive".into()))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn value(self) -> u64 { self.0 }

    pub fn next(self) -> Result<Self, DomainError> {
        self.0.checked_add(1).map(Self).ok_or_else(|| {
            DomainError::InvalidArgument("run version overflow".into())
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct ResumeCursor(u64);

impl ResumeCursor {
    pub const BEFORE_FIRST: Self = Self(0);
    pub const fn from_version(version: RunVersion) -> Self { Self(version.value()) }
    pub const fn value(self) -> u64 { self.0 }
}
```

### Core value objects

```rust
pub struct RunFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

pub struct RunTerminalResult {
    pub summary: String,
    pub output_references: Vec<RunReference>,
    pub warnings: Vec<String>,
    pub unmet_criteria: Vec<String>,
}

pub struct ParentRunLink {
    pub parent_run_id: AgentRunId,
    pub parent_step_id: RunStepId,
}

pub enum RunActorRef {
    Principal(PrincipalId),
    AgentSnapshot(AgentRuntimeSnapshotId),
    Worker(WorkerId),
    System,
}

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

Strings in `RunFailure` and `RunTerminalResult` are limited to 32 KiB each. Empty machine codes and blank terminal summaries are invalid.

### Atomic commit boundary

```rust
pub struct NewRunCommit {
    pub idempotency_key: String,
    pub run: AgentRun,
    pub event: RunEvent,
    pub enqueue: Vec<WorkItem>,
}

pub enum RunStepChange {
    InsertMany(Vec<RunStep>),
    Replace(RunStep),
}

pub struct RunCommit {
    pub expected_version: RunVersion,
    pub run: AgentRun,
    pub step_changes: Vec<RunStepChange>,
    pub event: RunEvent,
    pub checkpoint: Option<RunCheckpoint>,
    pub enqueue: Vec<WorkItem>,
    pub cancel_pending_work: bool,
}

pub struct AgentRunSnapshot {
    pub run: AgentRun,
    pub steps: Vec<RunStep>,
    pub checkpoint: Option<RunCheckpoint>,
}

#[async_trait::async_trait]
pub trait RunStorePort: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        commit: NewRunCommit,
    ) -> Result<AgentRunSnapshot, ApplicationError>;

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
        after: ResumeCursor,
    ) -> Result<Vec<RunEvent>, ApplicationError>;
}
```

The store validates that `event.run_version == commit.run.version`, `event.sequence == ResumeCursor::from_version(commit.run.version)`, and every persisted step belongs to the Run and workspace.

---

### Task 1: Add Run identifiers, statuses and transition tables

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/run/status.rs`
- Create: `crates/vestrace-domain/src/run/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline unit and property tests

**Interfaces:**
- Produces IDs `AgentRunId`, `RunStepId`, `RunEventId`, `RunCheckpointId`, `PlanRevisionId`, `AgentRuntimeSnapshotId`, `WorkItemId`, `WorkerId`, `BudgetSnapshotId`, `ResourceUsageSnapshotId`, `RunReferenceId`.
- Produces `RunVersion`, `ResumeCursor`, `RunExecutionMode`, `RunStatus`, `RunStepStatus`.

- [ ] **Step 1: Write failing transition tests**

```rust
#[test]
fn running_may_wait_for_input() {
    assert!(RunStatus::Running.can_transition_to(RunStatus::WaitingForInput));
}

#[test]
fn terminal_states_are_closed() {
    for terminal in [
        RunStatus::Succeeded,
        RunStatus::SucceededWithWarnings,
        RunStatus::Partial,
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::Expired,
    ] {
        assert!(!terminal.can_transition_to(RunStatus::Running));
    }
}

#[test]
fn unknown_step_can_be_reconciled() {
    assert!(RunStepStatus::Unknown.can_transition_to(RunStepStatus::Succeeded));
    assert!(RunStepStatus::Unknown.can_transition_to(RunStepStatus::Failed));
}
```

Run:

```bash
cargo test -p vestrace-domain run::status
```

Expected: FAIL because Run types are absent.

- [ ] **Step 2: Extend the existing UUID newtype macro**

Add all listed IDs with the same `new`, `from_uuid`, `as_uuid`, `Display`, `FromStr`, `Default`, Serde and Schemars behavior as existing domain IDs.

- [ ] **Step 3: Implement exact Run transitions**

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
terminal -> none
```

Implement via explicit match expressions, never enum ordering.

- [ ] **Step 4: Implement exact step transitions**

```text
Pending -> Ready | Skipped | Cancelled
Ready -> Running | Skipped | Cancelled
Running -> Waiting | Succeeded | Failed | Cancelled | Unknown
Waiting -> Ready | Running | Failed | Cancelled | Unknown
Unknown -> Waiting | Succeeded | Failed | Cancelled
terminal -> none
```

A retry is a separate `RunStep::retry` operation from `Failed` to `Ready` that increments the attempt counter.

- [ ] **Step 5: Add property tests and commit**

Use `proptest` to prove `RunVersion::next()` is monotonic below `u64::MAX` and terminal states accept no target.

```bash
cargo test -p vestrace-domain run
cargo fmt --all --check
git add crates/vestrace-domain
git commit -m "feat(run): add durable run state types"
```

---

### Task 2: Implement `AgentRun` and `RunStep` aggregates

**Files:**
- Create: `crates/vestrace-domain/src/run/step.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Test: inline unit tests

**Interfaces:**
- Produces `AgentRun`, `NewAgentRun`, `ParentRunLink`, `RunStep`, `NewRunStep`, `RunActorRef`, `RunReference`, `RunFailure`, `RunTerminalResult`.

- [ ] **Step 1: Write failing invariant tests**

```rust
#[test]
fn objective_must_not_be_blank() {
    let result = AgentRun::create(NewAgentRun {
        objective: "  ".into(),
        ..new_run_fixture()
    });
    assert!(result.is_err());
}

#[test]
fn stale_expected_version_is_rejected() {
    let mut run = running_run_fixture();
    let stale = RunVersion::new(run.version.value() + 1).unwrap();
    assert!(matches!(
        run.transition(stale, RunStatus::Paused, None, now()),
        Err(DomainError::RevisionConflict { .. })
    ));
}
```

- [ ] **Step 2: Implement `AgentRun`**

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

`create` trims and validates objective, caps it at 32 KiB, sets version `1`, status `Created`, and uses its own ID as `root_run_id` unless a parent is supplied. Parent and child IDs must differ.

`budget_snapshot_id` and `resource_usage_snapshot_id` are nullable UUID references without H1 foreign keys; H2 creates their tables and adds constraints in new migrations.

- [ ] **Step 3: Implement optimistic mutations**

```rust
pub fn transition(
    &mut self,
    expected: RunVersion,
    target: RunStatus,
    result: Option<RunTerminalResult>,
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
    step: Option<RunStepId>,
    at: Timestamp,
) -> Result<RunCurrentStepChange, DomainError>;
```

Each method checks expected version, validates the mutation, increments once and returns an immutable change record. Terminal targets require a result; non-terminal targets reject one.

- [ ] **Step 4: Implement `RunStep`**

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

Attempt begins at `1`. `plan_step_reference` is an opaque non-empty value capped at 256 bytes until H5 adds typed plan nodes. `retry` is allowed only from `Failed`, clears timing/error and increments with checked arithmetic.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain run
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain/src/run
git commit -m "feat(run): add run and step aggregates"
```

---

### Task 3: Add canonical events, checkpoints and logical replay

**Files:**
- Create: `crates/vestrace-domain/src/run/event.rs`
- Create: `crates/vestrace-domain/src/run/checkpoint.rs`
- Create: `crates/vestrace-application/src/run/replay.rs`
- Create: `crates/vestrace-application/tests/run_replay.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces `RunEvent`, `RunEventPayload`, `RunCheckpoint`, `RunCheckpointPayload`, `RunProjection`, `replay_run`.

- [ ] **Step 1: Write failing replay tests**

```rust
#[test]
fn event_sequence_must_equal_version() {
    let event = RunEvent::new(
        run_id(),
        workspace_id(),
        RunVersion::new(3).unwrap(),
        ResumeCursor::from_version(RunVersion::new(2).unwrap()),
        RunActorRef::System,
        RunEventPayload::RunStatusChanged {
            from: RunStatus::Preparing,
            to: RunStatus::Running,
            result: None,
        },
        correlation_id(),
        None,
        now(),
    );
    assert!(event.is_err());
}

#[test]
fn replay_reconstructs_projection() {
    let projection = replay_run(&scripted_events()).unwrap();
    assert_eq!(projection.status, RunStatus::WaitingForInput);
    assert_eq!(projection.version, RunVersion::new(7).unwrap());
}
```

- [ ] **Step 2: Implement event payloads**

```rust
pub enum RunEventPayload {
    RunCreated {
        objective: String,
        execution_mode: RunExecutionMode,
        coordinator_snapshot_id: AgentRuntimeSnapshotId,
        parent: Option<ParentRunLink>,
    },
    RunStatusChanged {
        from: RunStatus,
        to: RunStatus,
        result: Option<RunTerminalResult>,
    },
    PlanAttached {
        previous: Option<PlanRevisionId>,
        current: PlanRevisionId,
    },
    StepsAdded {
        steps: Vec<RunStep>,
    },
    StepStatusChanged {
        step_id: RunStepId,
        from: RunStepStatus,
        to: RunStepStatus,
        attempt: u32,
    },
    CurrentStepChanged {
        previous: Option<RunStepId>,
        current: Option<RunStepId>,
    },
    CheckpointCreated {
        checkpoint_id: RunCheckpointId,
        resume_cursor: ResumeCursor,
    },
}
```

Event type strings are derived from variants. Operational queue/lease events are intentionally absent.

- [ ] **Step 3: Implement `RunEvent` validation**

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

Rules: sequence equals version; creation is sequence/version `1`; all other events are greater than `1`; causation cannot self-reference.

- [ ] **Step 4: Implement versioned checkpoint payload**

```rust
pub enum RunCheckpointPayload {
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

Checkpoint version must equal cursor. Duplicate IDs within any vector are invalid.

- [ ] **Step 5: Implement pure replay**

```rust
pub fn replay_run(events: &[RunEvent]) -> Result<RunProjection, ApplicationError>;
```

Require first event `RunCreated`, contiguous sequences, one run/workspace, valid state transitions and no I/O. Projection contains status, version, plan, current step, known steps, checkpoint reference and terminal result.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain run
cargo test -p vestrace-application --test run_replay
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(run): add journal checkpoints and replay"
```

---

### Task 4: Define commands, ports and `RunCoordinator`

**Files:**
- Create: `crates/vestrace-application/src/run/commands.rs`
- Create: `crates/vestrace-application/src/run/ports.rs`
- Create: `crates/vestrace-application/src/run/coordinator.rs`
- Create: `crates/vestrace-application/tests/run_coordinator.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`

**Interfaces:**
- Produces `CreateRun`, `TransitionRun`, `AddRunSteps`, `TransitionRunStep`, `CreateCheckpoint`, `PauseRun`, `ResumeRun`, `CancelRun`.
- Produces `RunStorePort`, `RunLeasePort`, `WorkQueuePort`, `RunClockPort`, `RunCoordinator`.

- [ ] **Step 1: Define command DTOs**

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
    pub idempotency_key: String,
}

pub struct CreateCheckpoint {
    pub run_id: AgentRunId,
    pub expected_version: RunVersion,
    pub payload: RunCheckpointPayloadV1,
    pub actor: RunActorRef,
    pub idempotency_key: String,
}
```

All external state-changing commands carry idempotency keys. Internal continuation keys are deterministic from run ID, expected version and action kind.

- [ ] **Step 2: Define operational ports**

```rust
pub struct RunLease {
    pub run_id: AgentRunId,
    pub worker_id: WorkerId,
    pub generation: u64,
    pub acquired_at: Timestamp,
    pub heartbeat_at: Timestamp,
    pub lease_until: Timestamp,
}

pub struct AcquireRunLease {
    pub run_id: AgentRunId,
    pub worker_id: WorkerId,
    pub now: Timestamp,
    pub lease_until: Timestamp,
}

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
    async fn cancel_for_run(&self, context: &RequestContext, run_id: AgentRunId, at: Timestamp) -> Result<u64, ApplicationError>;
    async fn dead_letter(&self, context: &RequestContext, item: &WorkItem, error: RunFailure, at: Timestamp) -> Result<(), ApplicationError>;
}
```

`RunClockPort::now()` provides deterministic time.

- [ ] **Step 3: Implement `create` with fake ports**

Build Run version `1`, `RunCreated` event sequence `1`, and initial `AdvanceRun` item with key `run:{run_id}:version:1:advance`, then call `RunStorePort::create` once. Duplicate external idempotency returns the original snapshot.

- [ ] **Step 4: Implement mutation services**

Each service loads snapshot, validates expected version, mutates domain state, constructs exactly one matching event, optionally adds steps/checkpoint/follow-up work, and calls `commit` once.

Checkpoint creation increments the Run version, writes the immutable checkpoint at that version/cursor and sets `checkpoint_id` in the same transaction.

- [ ] **Step 5: Implement lifecycle helpers**

`pause` creates no follow-up work. `resume` queues one deterministic `ResumeRun` item. `cancel` sets `cancel_pending_work = true`; it preserves step/output history and states explicitly that already performed external actions are not rolled back.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test run_coordinator
cargo clippy -p vestrace-application --all-targets -- -D warnings
git add crates/vestrace-application
git commit -m "feat(run): add coordinator commands and ports"
```

---

### Task 5: Add Run, step, event and checkpoint schema

**Files:**
- Create: `migrations/0019_agent_runs_and_steps.sql`
- Create: `migrations/0020_run_events_and_checkpoints.sql`
- Create: `tests/run_lifecycle.rs`
- Create: `tests/run_replay.rs`
- Modify: `tests/support/mod.rs`

**Interfaces:**
- Produces `agent_runs`, `run_steps`, `run_events`, `run_checkpoints`.

- [ ] **Step 1: Write failing database tests**

Cover blank objective, zero version, cross-workspace parent, current step from another Run, duplicate event sequence, event sequence/version mismatch, checkpoint cursor/version mismatch and application-role update/delete of event/checkpoint rows.

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_lifecycle
```

Expected: FAIL because H1 migrations are absent.

- [ ] **Step 2: Create `0019_agent_runs_and_steps.sql`**

`agent_runs` columns:

```text
id, workspace_id, objective, coordinator_snapshot_id,
active_plan_revision_id, execution_mode, status,
current_step_id, checkpoint_id, parent_run_id, parent_step_id,
root_run_id, budget_snapshot_id, resource_usage_snapshot_id,
run_version, result JSONB, created_at, updated_at, finished_at
```

Use text status/mode checks, objective byte length `1..32768`, version `>= 1`, and terminal/finished-time consistency.

`run_steps` columns:

```text
id, workspace_id, run_id, plan_step_reference,
assigned_actor JSONB, input_references JSONB,
status, attempt, output_references JSONB,
error JSONB, created_at, started_at, finished_at
```

Use deferred constraint triggers for current-step and parent ownership.

- [ ] **Step 3: Create `0020_run_events_and_checkpoints.sql`**

`run_events` requires:

```sql
CHECK (sequence >= 1),
CHECK (run_version >= 1),
CHECK (sequence = run_version),
UNIQUE (run_id, sequence)
```

`run_checkpoints` requires `run_version = resume_cursor` and unique `(run_id, run_version)`. Add `agent_runs.checkpoint_id` foreign key after checkpoint creation. Ordinary application role cannot update/delete events or checkpoints.

- [ ] **Step 4: Add persisted replay fixture**

Insert a seven-event Run, load ordered events, call `replay_run`, and compare projection with persisted Run status, plan, current step and version.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_lifecycle --test run_replay
git add migrations/0019_agent_runs_and_steps.sql migrations/0020_run_events_and_checkpoints.sql tests
git commit -m "feat(storage): add durable run journal schema"
```

---

### Task 6: Add operational leases and work items

**Files:**
- Create: `crates/vestrace-domain/src/run/work.rs`
- Create: `migrations/0021_run_leases_and_work_items.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/run/lease.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/run/work_queue.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/run/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/run_leases.rs`

**Interfaces:**
- Produces `RunLease`, `WorkItem`, `WorkItemKind`, `WorkItemStatus`, `RunWorkPayload`, PostgreSQL `RunLeasePort` and `WorkQueuePort`.

- [ ] **Step 1: Write failing concurrency tests**

Assert worker A acquires, B is blocked before expiry, B acquires after expiry with greater generation, stale A cannot heartbeat/release, heartbeat does not change Run version, concurrent queue consumers receive different rows, duplicate idempotency returns existing item, and expired queue leases do not create duplicates.

- [ ] **Step 2: Implement work types**

```rust
pub enum WorkItemKind {
    AdvanceRun,
    ResumeRun,
    CreateCheckpoint,
    ExpireRun,
}

pub enum WorkItemStatus {
    Ready,
    Leased,
    Completed,
    Failed,
    Cancelled,
    DeadLetter,
}

pub enum RunWorkPayload {
    Advance,
    Resume { checkpoint_id: Option<RunCheckpointId> },
    CreateCheckpoint,
    Expire,
}

pub struct WorkItem {
    pub id: WorkItemId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub step_id: Option<RunStepId>,
    pub expected_run_version: RunVersion,
    pub kind: WorkItemKind,
    pub payload: RunWorkPayload,
    pub status: WorkItemStatus,
    pub priority: i16,
    pub available_at: Timestamp,
    pub deadline: Option<Timestamp>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub idempotency_key: String,
}
```

Constructor validates kind/payload agreement, non-empty key, and `max_attempts >= 1`.

- [ ] **Step 3: Create `0021_run_leases_and_work_items.sql`**

`run_leases`: `workspace_id`, `run_id` primary key, `worker_id`, `generation >= 1`, `acquired_at`, `heartbeat_at`, `lease_until`.

`work_items`: all `WorkItem` fields plus `required_capabilities JSONB`, lease owner/until, last error, timestamps and unique `(workspace_id, idempotency_key)`.

- [ ] **Step 4: Implement lease generation fencing**

Acquire inserts generation `1` or takes an expired lease while incrementing generation. Heartbeat/release predicates include run ID, worker ID and generation. Zero affected rows maps to stable `lease_lost`.

- [ ] **Step 5: Implement work leasing**

Use `FOR UPDATE SKIP LOCKED`, ordered by priority descending, available time, creation time and ID. Increment attempt when handler execution begins. Move exhausted items to `DeadLetter`. `cancel_for_run` affects only `Ready` and expired `Leased` items.

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
- Produces `PostgresRunStore`.

- [ ] **Step 1: Write optimistic-concurrency test**

Create version `1`; concurrently commit two version-`2` transitions with expected `1`. Exactly one succeeds, the other returns `revision_conflict`, and only one sequence-`2` event exists.

- [ ] **Step 2: Write atomic rollback test**

Commit a valid transition with a follow-up work item whose idempotency key already exists. Assert failure leaves Run, event, step and checkpoint state unchanged.

- [ ] **Step 3: Implement `create`**

In one scoped transaction: resolve command idempotency, insert version-`1` Run, append `run.created`, insert initial work, store idempotent result, commit. Duplicate external idempotency returns the original snapshot.

- [ ] **Step 4: Implement `commit`**

Use optimistic SQL:

```sql
UPDATE agent_runs
SET status = $status,
    active_plan_revision_id = $plan,
    current_step_id = $current_step,
    checkpoint_id = $checkpoint,
    result = $result,
    run_version = $new_version,
    updated_at = $updated_at,
    finished_at = $finished_at
WHERE workspace_id = $workspace_id
  AND id = $run_id
  AND run_version = $expected_version;
```

Zero rows maps to `revision_conflict` after reading current version. Apply step changes, event, checkpoint, follow-up work and pending-work cancellation before commit. Ambiguous connection failure returns `operation_unknown`; do not retry automatically.

- [ ] **Step 5: Implement snapshot/event loading**

Snapshot contains ordered steps and latest checkpoint, but no lease. `load_events` returns strictly ascending sequence after the supplied cursor.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_atomicity
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add crates/vestrace-infrastructure tests/run_atomicity.rs
git commit -m "feat(run): persist atomic run mutations"
```

---

### Task 8: Add restart-safe worker loop

**Files:**
- Create: `crates/vestrace-application/src/run/worker.rs`
- Create: `crates/vestrace-application/tests/run_worker.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-cli/src/commands/worker.rs`

**Interfaces:**
- Produces `RunWorkHandler`, `RunWorkHandlerRegistry`, `RunWorker`, `RunWorkerConfig`, `RunWorkOutcome`.

- [ ] **Step 1: Write failing worker tests**

Assert handler runs only after work and Run leases are acquired; stale expected version causes no handler call; stale lease generation cannot commit; retryable error schedules retry; non-retryable error dead-letters; cancelled Run completes/cancels queue work without handler invocation.

- [ ] **Step 2: Define handler contract**

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

Handlers do not modify queue rows directly.

- [ ] **Step 3: Implement `run_once`**

Sequence: lease work, load Run, compare expected version, reject paused/terminal Runs, acquire Run lease, invoke handler, complete/retry/dead-letter work, release lease. Process death relies on lease expiry. Tracing includes IDs/statuses but no objective or payload contents.

- [ ] **Step 4: Implement bounded retries**

```text
attempt 1 -> 1 second
attempt 2 -> 5 seconds
attempt 3 -> 30 seconds
attempt 4+ -> dead letter
```

No random jitter in tests.

- [ ] **Step 5: Wire `vestrace worker`**

Add config `worker.id`, `poll_interval_ms`, `run_lease_ttl_seconds`, `heartbeat_interval_seconds`, `max_concurrency`. H1 accepts only `max_concurrency = 1`; other values return configuration error.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test run_worker
cargo test -p vestrace-cli --test cli
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/vestrace-application crates/vestrace-cli
git commit -m "feat(run): add restart-safe worker loop"
```

---

### Task 9: Add forced RLS and operational indexes

**Files:**
- Create: `migrations/0022_run_rls_and_indexes.sql`
- Modify: `tests/run_lifecycle.rs`
- Modify: `tests/run_leases.rs`
- Modify: `tests/run_atomicity.rs`

**Interfaces:**
- Applies forced RLS to every H1 table.

- [ ] **Step 1: Write cross-workspace negative tests**

Workspace B cannot load/update A's Run, append A's event, add A's step/checkpoint, acquire A's lease or lease A's work. Errors must not reveal row contents or existence through unique-key details.

- [ ] **Step 2: Add forced RLS**

Apply to `agent_runs`, `run_steps`, `run_events`, `run_checkpoints`, `run_leases`, `work_items` using existing `vestrace_current_workspace_id()`.

- [ ] **Step 3: Add indexes**

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

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_lifecycle --test run_leases --test run_atomicity
git add migrations/0022_run_rls_and_indexes.sql tests
git commit -m "feat(security): isolate durable runs with RLS"
```

---

### Task 10: Prove restart, pause/resume and replay end to end

**Files:**
- Create: `tests/run_restart.rs`
- Modify: `tests/run_replay.rs`
- Modify: `docker-compose.yml`
- Modify: `.github/workflows/ci.yml`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md`

**Interfaces:**
- Produces H1 acceptance evidence; no new public contracts.

- [ ] **Step 1: Add deterministic test handler**

The handler performs no external I/O:

```text
Advance 1:
  Created -> Preparing
  add steps A and B
  A: Ready -> Running -> Succeeded
  create checkpoint
  enqueue next AdvanceRun

Advance 2:
  Preparing -> Running
  B: Ready -> Running
  simulate process loss after durable commit

Recovered Advance:
  worker B acquires expired work and Run leases
  resume from checkpoint
  B -> Succeeded
  Run -> Succeeded
```

Each arrow is executed through `RunCoordinator` and therefore produces its own version/event.

- [ ] **Step 2: Write restart acceptance test**

Create Run, execute with worker A, verify checkpoint, expire both leases without completing active work, construct fresh PostgreSQL adapters and worker B, finish Run, assert event sequences exactly `1..=run_version`, replay equals persisted logical state, checkpoint remains immutable, stale worker A generation cannot heartbeat/release.

- [ ] **Step 3: Add pause/resume test**

Pause after checkpoint, verify paused Run work is not handled, issue `ResumeRun`, and verify completed steps are not recreated and execution resumes from recorded cursor.

- [ ] **Step 4: Add cancel test**

Cancel a Run with completed and ready work. Assert status `Cancelled`, queued execution work becomes `Cancelled`, completed step/output history remains, no handler runs afterward, and result warns that previous external effects are not rolled back.

- [ ] **Step 5: Add CI and Compose checks**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test run_restart --test run_replay
```

Compose worker health verifies PostgreSQL reachability and queue polling without requiring pending work.

- [ ] **Step 6: Update Harness roadmap**

Link this plan under H1 and add normative migration sequence `0019`–`0022`. Do not mark H1 complete before acceptance passes.

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

Expected: all commands exit `0`; server, worker and PostgreSQL are healthy; restart produces no duplicate logical mutation; replay equals persisted state.

---

## H1 exit gate

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

- one canonical event per logical Run version;
- contiguous event sequences;
- stale concurrent updates return `revision_conflict`;
- heartbeat does not change Run version;
- stale lease generation cannot mutate operational state;
- failed multi-table commits leave no partial state;
- paused Runs do not advance before explicit resume;
- cancelled Runs preserve completed history and stop queued execution;
- RLS blocks all tested cross-workspace access;
- restart and replay acceptance tests pass.

## Explicit non-goals

- model providers, model-loop engines or Rig;
- actual plan nodes/validation;
- Policy Engine, approvals or capabilities;
- budget reservations/accounting;
- tools, side effects or sandboxing;
- subagent delegation behavior;
- context assembly, Artifact Store or consolidation;
- conversations, triggers, channels or Harness public API;
- distributed consensus or cross-region execution.
