# Vestrace H5 Planning and Delegation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement immutable execution plans, deterministic validation and scheduling, bounded replanning, one-level internal SubRun delegation, transport-neutral remote-agent delegation, isolated delegated authority and context, and validated handoff processing that survives restart without duplicating work.

**Architecture:** H5 adds a Vestrace-owned planning layer above H1 Runs, H3 model execution and H4 tools. An immutable `ExecutionPlanRevision` describes intent; H1 `RunStep` records are the executable projection of one activated revision. The Coordinator schedules ready steps but never performs model, tool or remote I/O directly. Internal delegation creates a child `AgentRun`; external delegation creates a separate `RemoteAgentInvocation` through a transport-neutral port. Both produce a bounded `HandoffArtifact` that must pass schema, evidence and trust checks before the parent continues.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H4 Rust workspace; Rust Edition 2024; Tokio; Serde; Schemars; SQLx; PostgreSQL 17; SHA-256 canonical hashing; JSON Schema validation; tracing; proptest; deterministic model/tool/subrun/remote-agent fixtures; repository PostgreSQL integration-test harness.

## Global Constraints

- Complete all five v0.1 plans and H1–H4 before implementing H5.
- Harness design sections `9. Planning` and `10. Delegation and SubRun` are normative.
- ADR-0003 is authoritative for the distinction between internal SubRuns and external remote-agent invocations.
- PostgreSQL is authoritative for plan revisions, validation reports, activations, scheduler state, replanning decisions, delegation grants, SubRun bindings, remote-agent invocations, handoffs and cross-checks.
- Vestrace owns every domain type, application port, persisted schema, checkpoint field, public DTO and lifecycle transition.
- `a2a-rs`, Rig and provider/tool SDK types may not appear in H5 domain/application signatures or PostgreSQL schemas.
- An `ExecutionPlanRevision` is immutable after creation. Runtime status belongs to H1 `RunStep`, not to plan-step rows.
- Every activated plan revision has one deterministic content hash and one successful validation report for the exact same bytes and referenced component revisions.
- A plan proposed by a model is untrusted structured data. It cannot activate itself, create authority, change budgets or bypass validation.
- Direct, Guided and Workflow modes all produce an activated immutable plan revision; Direct mode is not an undocumented execution shortcut.
- Completed steps are historical facts. Replanning cannot rewrite their definition, output contract, dependencies or result references.
- A new plan revision activates only at a scheduler quiescence point: no step may be entering a new external dispatch under the superseded revision.
- Running model/tool/delegation operations are reconciled or reach a durable waiting/terminal state before activation of a replacement revision.
- Replanning is bounded by `maximum_plan_revisions`, revision budget, wall-clock budget and correction-attempt policy.
- The default delegation depth is one. A child SubRun cannot delegate again unless an explicit policy permits a greater depth.
- Internal SubRun and external RemoteAgentInvocation are different aggregates with different ownership and recovery semantics.
- A remote agent is never modeled as an H4 tool or an H1 child Run.
- A child SubRun cannot mutate its parent Run, parent plan, parent scratchpad, parent memory scope, approvals, tools or budget accounts.
- A remote agent never receives Vestrace authorization tickets, approval grants, parent checkpoints, permanent credentials or unrestricted memory access.
- Effective delegated authority is the intersection of parent authority, target eligibility, delegation request, workflow constraints and current H2 policy.
- Delegated context is allowlisted by stable references and classifications; omission is explicit and auditable.
- A delegation token is a one-time Vestrace reference to a durable delegation grant. It is not a bearer credential, is never model-visible and is never sent to a remote agent.
- Delegation creation, remote dispatch, cancellation, replanning and handoff acceptance are protected H2 operations.
- Parent resource allocation uses H2 budget allocations and quotas. Child or remote work cannot reserve beyond the delegated ceiling.
- A remote dispatch with ambiguous completion becomes `Unknown`; it is never automatically repeated.
- Remote-agent transport is not implemented in H5. H5 uses a deterministic `RemoteAgentPort`; H9A later supplies the A2A adapter without changing H5 contracts.
- Agent packages and persistent registries are not implemented in H5. H5 consumes `InternalAgentEligibilityPort`, `RemoteAgentEligibilityPort` and `WorkflowDefinitionPort`; H9 later supplies their production implementations.
- Human channel behavior is not implemented in H5. Human plan-step kinds are defined, but activation rejects them as unavailable until H7 registers the required port.
- Handoff structured output is bounded and stores references, not large binary content. H6 later materializes final Artifacts and provenance without changing H5 history.
- Remote messages, status text, declared skills and handoff content are untrusted data.
- A delegated result is not accepted merely because the child Run or remote system reports success.
- H1 logical Run mutations each increment `RunVersion` once and append exactly one canonical `RunEvent`. Scheduler polling, remote progress and validation telemetry do not increment `RunVersion`.
- Replay never starts a model invocation, tool invocation, SubRun or remote dispatch.
- Existing migrations `0014`–`0036` are never edited. H5 migrations are `0037`–`0041` and are created once.
- CI requires no public model, external agent, A2A server, paid account or permanent credential.
- Future implementation branch: `feat/h5-planning-delegation`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  planning/mod.rs
  planning/mode.rs
  planning/plan.rs
  planning/step.rs
  planning/criteria.rs
  planning/validation.rs
  planning/replanning.rs
  delegation/mod.rs
  delegation/request.rs
  delegation/scope.rs
  delegation/grant.rs
  delegation/subrun.rs
  delegation/remote.rs
  delegation/handoff.rs
  delegation/cross_check.rs
  run/event.rs
  run/work.rs
  run/checkpoint.rs
  run/mod.rs

crates/vestrace-application/src/
  lib.rs
  planning/mod.rs
  planning/ports.rs
  planning/commands.rs
  planning/direct.rs
  planning/proposal.rs
  planning/validator.rs
  planning/activation.rs
  planning/scheduler.rs
  planning/replanner.rs
  delegation/mod.rs
  delegation/ports.rs
  delegation/commands.rs
  delegation/eligibility.rs
  delegation/scope.rs
  delegation/subrun_service.rs
  delegation/remote_service.rs
  delegation/reconciliation.rs
  delegation/handoff_service.rs
  delegation/cross_check.rs
  coordinator/mod.rs
  coordinator/service.rs
  coordinator/worker.rs

crates/vestrace-application/tests/
  direct_planning.rs
  guided_planning.rs
  workflow_planning.rs
  plan_validation.rs
  plan_scheduler.rs
  replanning.rs
  delegation_scope.rs
  subrun_service.rs
  remote_agent_service.rs
  handoff_validation.rs
  coordinator_restart.rs

crates/vestrace-planning-test-support/
  Cargo.toml
  src/lib.rs
  src/fakes.rs
  src/fixtures.rs
  src/plan_builder.rs
  src/remote_agent.rs
  src/conformance.rs

crates/vestrace-infrastructure/src/postgres/
  mod.rs
  planning/mod.rs
  planning/plan_repository.rs
  planning/validation_repository.rs
  planning/activation_repository.rs
  planning/scheduler_repository.rs
  planning/replanning_repository.rs
  delegation/mod.rs
  delegation/grant_repository.rs
  delegation/subrun_repository.rs
  delegation/remote_repository.rs
  delegation/handoff_repository.rs
  delegation/cross_check_repository.rs

migrations/
  0037_execution_plans_revisions_steps_validation.sql
  0038_plan_activations_scheduler_and_replanning.sql
  0039_delegations_subruns_and_remote_invocations.sql
  0040_handoffs_cross_checks_and_remote_events.sql
  0041_planning_delegation_rls_indexes_and_run_bindings.sql

tests/
  plan_persistence.rs
  plan_activation_atomicity.rs
  plan_scheduler_persistence.rs
  plan_replanning_persistence.rs
  delegation_persistence.rs
  delegation_budget_isolation.rs
  subrun_atomicity.rs
  remote_invocation_persistence.rs
  remote_unknown_reconciliation.rs
  handoff_persistence.rs
  planning_delegation_rls.rs
  h5_acceptance.rs

scripts/
  verify-planning-boundary.sh
  verify-delegation-boundary.sh
```

---

## Normative contracts

### Execution modes

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Direct,
    Guided,
    Workflow,
}
```

All modes persist an `ExecutionPlanRevision` before work begins:

```text
Direct
→ deterministic DirectPlanBuilder
→ validation
→ activation

Guided
→ H3 structured plan proposal
→ normalization
→ validation
→ activation

Workflow
→ immutable WorkflowDefinition revision
→ parameter binding
→ validation
→ activation
```

The source of the plan changes; validation, activation, scheduling and replay do not.

### Stable plan identities

```rust
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct PlanStepKey(String);

impl PlanStepKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError>;
}

pub struct PlanStepRef {
    pub plan_revision_id: ExecutionPlanRevisionId,
    pub step_key: PlanStepKey,
}
```

`PlanStepKey` uses lowercase ASCII dotted segments, each 1–64 bytes and total length at most 255 bytes. It is stable across compatible revisions and unique within one plan revision.

### Success criteria

```rust
pub struct PlanSuccessCriteria {
    pub required_outputs: Vec<RequiredOutput>,
    pub output_schema: Option<serde_json::Value>,
    pub deterministic_assertions: Vec<DeterministicAssertion>,
    pub evidence_requirements: Vec<EvidenceRequirement>,
    pub quality_threshold: Option<QualityThreshold>,
    pub prohibited_outcomes: Vec<ProhibitedOutcome>,
    pub completion_policy: PlanCompletionPolicy,
}

pub enum PlanCompletionPolicy {
    AllRequired,
    AllowPartialWithWarnings,
    RequireHumanDecision,
}
```

H5 evaluates structural completion and deterministic requirements that are available. H10 later adds richer evaluation without changing these stored contracts.

### Plan step definitions

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepKind {
    Reason,
    ToolCall,
    DelegateInternal,
    DelegateRemote,
    HumanInput,
    HumanApproval,
    Evaluate,
    Synthesize,
    End,
}

pub enum PlanStepAction {
    Model {
        purpose: ModelStepPurpose,
        output_schema: serde_json::Value,
    },
    Tool {
        tool_revision_id: ToolRevisionId,
        argument_template: serde_json::Value,
    },
    InternalDelegation {
        target_agent_snapshot_id: AgentRuntimeSnapshotId,
        required_skill_ids: Vec<String>,
    },
    RemoteDelegation {
        target_remote_agent_revision_id: RemoteAgentDefinitionRevisionId,
        required_skill_ids: Vec<String>,
    },
    Human {
        interaction_kind: HumanInteractionKind,
        input_schema: serde_json::Value,
    },
    End,
}

pub struct PlanStepDefinition {
    pub key: PlanStepKey,
    pub kind: PlanStepKind,
    pub action: PlanStepAction,
    pub dependencies: Vec<PlanStepKey>,
    pub input_references: Vec<RunReference>,
    pub output_schema: serde_json::Value,
    pub required_capabilities: std::collections::BTreeSet<Capability>,
    pub allowed_tool_revision_ids: std::collections::BTreeSet<ToolRevisionId>,
    pub requested_budget: ResourceBudgetRequest,
    pub timeout_ms: u64,
    pub retry_policy: PlanStepRetryPolicy,
    pub completion_criteria: StepCompletionCriteria,
    pub optional: bool,
    pub definition_hash: [u8; 32],
}
```

Rules:

- `kind` and `action` must agree;
- a Tool step references exactly one active immutable H4 Tool revision;
- an internal delegation references one eligible internal Agent snapshot;
- a remote delegation references one eligible remote-agent definition revision through a port, never an A2A type;
- `End` has no action payload and no outgoing dependants;
- Human steps cannot activate until H7 registers `HumanInteractionPlanningPort`;
- empty or unbounded timeouts and retry counts are rejected;
- schemas are compiled at plan creation and limited to 1 MiB each;
- retry policy cannot override H3/H4/delegation safety rules.

### Execution plan revision

```rust
pub struct ExecutionPlanRevision {
    pub id: ExecutionPlanRevisionId,
    pub plan_id: ExecutionPlanId,
    pub run_id: AgentRunId,
    pub revision: u32,
    pub mode: ExecutionMode,
    pub objective: String,
    pub assumptions: Vec<PlanAssumption>,
    pub success_criteria: PlanSuccessCriteria,
    pub steps: Vec<PlanStepDefinition>,
    pub requested_budget: ResourceBudgetRequest,
    pub required_approval_bindings: Vec<RequiredPlanApproval>,
    pub source: PlanSource,
    pub supersedes_revision_id: Option<ExecutionPlanRevisionId>,
    pub content_hash: [u8; 32],
    pub created_by: RunActorRef,
    pub created_at: Timestamp,
}
```

Objective, assumption and explanation strings are bounded to 32 KiB. `revision` starts at 1 and increases contiguously within one plan. Plan hashes include exact referenced component revision IDs.

### Validation report

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanValidationSeverity {
    Error,
    Warning,
}

pub enum PlanValidationCode {
    EmptyPlan,
    DuplicateStepKey,
    MissingDependency,
    CycleDetected,
    UnreachableStep,
    MissingEndPath,
    InvalidKindAction,
    InvalidSchema,
    UnknownToolRevision,
    UnknownInternalAgent,
    UnknownRemoteAgent,
    MissingRequiredSkill,
    CapabilityEscalation,
    ToolOutsideCeiling,
    BudgetExceeded,
    ApprovalMissing,
    ContextUnavailable,
    DelegationDepthExceeded,
    ParallelismExceeded,
    UnsupportedStepKind,
    CompletedStepChanged,
    RunningStepConflict,
    RevisionLimitExceeded,
}

pub struct PlanValidationIssue {
    pub code: PlanValidationCode,
    pub severity: PlanValidationSeverity,
    pub step_key: Option<PlanStepKey>,
    pub path: String,
    pub message: String,
}

pub struct PlanValidationReport {
    pub id: PlanValidationReportId,
    pub plan_revision_id: ExecutionPlanRevisionId,
    pub plan_content_hash: [u8; 32],
    pub referenced_component_hash: [u8; 32],
    pub validator_version: String,
    pub issues: Vec<PlanValidationIssue>,
    pub valid: bool,
    pub validated_at: Timestamp,
}
```

Only a report with `valid = true`, no Error issues and exact matching hashes may activate its revision.

### Scheduler state

```rust
pub struct PlanActivation {
    pub id: PlanActivationId,
    pub run_id: AgentRunId,
    pub plan_revision_id: ExecutionPlanRevisionId,
    pub validation_report_id: PlanValidationReportId,
    pub activation_number: u32,
    pub activated_at_run_version: RunVersion,
    pub superseded_activation_id: Option<PlanActivationId>,
    pub activated_by: RunActorRef,
}

pub struct PlanSchedulerSnapshot {
    pub run_id: AgentRunId,
    pub activation_id: PlanActivationId,
    pub ready_step_keys: Vec<PlanStepKey>,
    pub blocked_step_keys: Vec<PlanStepKey>,
    pub inflight_step_keys: Vec<PlanStepKey>,
    pub terminal_step_keys: Vec<PlanStepKey>,
    pub scheduler_revision: u64,
}
```

The snapshot is a durable projection and may be rebuilt from the active plan plus H1 steps. It does not replace H1 `RunStep` authority.

### Replanning

```rust
pub struct ReplanPolicy {
    pub maximum_plan_revisions: u32,
    pub maximum_replan_attempts_per_failure: u16,
    pub maximum_additional_steps: u32,
    pub require_quiescence: bool,
    pub preserve_completed_step_keys: bool,
}

pub struct ReplanRequest {
    pub run_id: AgentRunId,
    pub current_plan_revision_id: ExecutionPlanRevisionId,
    pub trigger: ReplanTrigger,
    pub bounded_failure_context: Vec<RunReference>,
    pub allowed_change_scope: ReplanChangeScope,
    pub requested_budget: ResourceBudgetRequest,
}

pub struct PlanRevisionDiff {
    pub unchanged_steps: Vec<PlanStepKey>,
    pub added_steps: Vec<PlanStepKey>,
    pub removed_pending_steps: Vec<PlanStepKey>,
    pub changed_pending_steps: Vec<PlanStepKey>,
    pub preserved_completed_steps: Vec<PlanStepKey>,
}
```

A replacement revision must retain completed steps with the exact same `definition_hash`. Running steps are not changed or removed. Activation waits until the scheduler reaches the declared quiescence condition.

### Delegation target and request

```rust
pub enum DelegationTargetRef {
    InternalAgent {
        agent_snapshot_id: AgentRuntimeSnapshotId,
    },
    RemoteAgent {
        remote_agent_revision_id: RemoteAgentDefinitionRevisionId,
    },
}

pub struct DelegationRequest {
    pub id: DelegationRequestId,
    pub parent_run_id: AgentRunId,
    pub parent_step_id: RunStepId,
    pub plan_step_ref: PlanStepRef,
    pub target: DelegationTargetRef,
    pub objective: String,
    pub expected_output_schema: serde_json::Value,
    pub delegated_context: Vec<DelegatedContextReference>,
    pub allowed_tool_revision_ids: std::collections::BTreeSet<ToolRevisionId>,
    pub capability_ceiling: std::collections::BTreeSet<Capability>,
    pub memory_scope: DelegatedMemoryScope,
    pub resource_allocation: ResourceBudgetRequest,
    pub completion_policy: DelegationCompletionPolicy,
    pub trust_policy: DelegationTrustPolicy,
    pub deadline: Timestamp,
    pub request_hash: [u8; 32],
}
```

The request hash includes every authority-bearing field and exact target revision.

### Delegated scope and grant

```rust
pub struct DelegatedContextReference {
    pub reference: RunReference,
    pub classification: DataClassification,
    pub purpose: String,
}

pub struct DelegationScope {
    pub effective_capabilities: std::collections::BTreeSet<Capability>,
    pub effective_tool_revision_ids: std::collections::BTreeSet<ToolRevisionId>,
    pub effective_context: Vec<DelegatedContextReference>,
    pub effective_memory_scope: DelegatedMemoryScope,
    pub budget_allocation_id: BudgetAllocationId,
    pub maximum_depth: u16,
    pub maximum_parallel_children: u16,
    pub expires_at: Timestamp,
    pub scope_hash: [u8; 32],
}

pub struct DelegationGrant {
    pub id: DelegationGrantId,
    pub request_id: DelegationRequestId,
    pub parent_run_id: AgentRunId,
    pub parent_step_id: RunStepId,
    pub target: DelegationTargetRef,
    pub scope: DelegationScope,
    pub policy_decision_id: PolicyDecisionId,
    pub authorization_ticket_id: AuthorizationTicketId,
    pub status: DelegationGrantStatus,
    pub consumed_by: Option<DelegationConsumerRef>,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct DelegationToken(DelegationGrantId);
```

`DelegationToken` is only a typed one-time reference. Consumption loads the durable grant, checks workspace, target, parent lineage, expiration and unused state, then atomically binds it to one child Run or remote invocation.

### SubRun binding

```rust
pub struct SubRunBinding {
    pub id: SubRunBindingId,
    pub delegation_request_id: DelegationRequestId,
    pub delegation_grant_id: DelegationGrantId,
    pub parent_run_id: AgentRunId,
    pub parent_step_id: RunStepId,
    pub child_run_id: AgentRunId,
    pub child_agent_snapshot_id: AgentRuntimeSnapshotId,
    pub lineage_depth: u16,
    pub context_manifest_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

The child Run uses H1 `ParentRunLink`. The binding is immutable and one-to-one with the consumed grant.

### Remote-agent invocation

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemoteAgentInvocationStatus {
    Prepared,
    Dispatching,
    Working,
    WaitingForRemoteInput,
    WaitingForAuthentication,
    WaitingForDependency,
    Succeeded,
    Failed,
    Rejected,
    Cancelled,
    Unknown,
}

pub struct RemoteAgentInvocation {
    pub id: RemoteAgentInvocationId,
    pub workspace_id: WorkspaceId,
    pub parent_run_id: AgentRunId,
    pub parent_step_id: RunStepId,
    pub delegation_request_id: DelegationRequestId,
    pub delegation_grant_id: DelegationGrantId,
    pub remote_agent_revision_id: RemoteAgentDefinitionRevisionId,
    pub status: RemoteAgentInvocationStatus,
    pub external_task_id: Option<String>,
    pub external_context_id: Option<String>,
    pub last_event_cursor: Option<String>,
    pub logical_revision: u64,
    pub dispatch_idempotency_key: String,
    pub reconciliation_id: Option<RemoteAgentReconciliationId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

External IDs and cursors are bounded to 4 KiB and treated as opaque untrusted strings.

### Remote port

```rust
pub struct RemoteAgentDispatchRequest {
    pub invocation: RemoteAgentInvocation,
    pub target_revision_id: RemoteAgentDefinitionRevisionId,
    pub objective: String,
    pub input_manifest: DelegatedInputManifest,
    pub expected_output_schema: serde_json::Value,
    pub deadline: Timestamp,
}

#[async_trait::async_trait]
pub trait RemoteAgentPort: Send + Sync {
    async fn dispatch(
        &self,
        request: RemoteAgentDispatchRequest,
    ) -> Result<RemoteAgentObservation, RemoteAgentError>;

    async fn observe(
        &self,
        request: RemoteAgentObserveRequest,
    ) -> Result<RemoteAgentObservation, RemoteAgentError>;

    async fn cancel(
        &self,
        request: RemoteAgentCancelRequest,
    ) -> Result<RemoteAgentObservation, RemoteAgentError>;

    async fn reconcile(
        &self,
        request: RemoteAgentReconcileRequest,
    ) -> Result<RemoteAgentReconciliationObservation, RemoteAgentError>;
}
```

H5 provides only deterministic fixtures. H9A implements this port with A2A.

### Handoff artifact

```rust
pub struct HandoffArtifact {
    pub id: HandoffArtifactId,
    pub delegation_request_id: DelegationRequestId,
    pub producer: DelegationConsumerRef,
    pub summary: String,
    pub structured_output: serde_json::Value,
    pub supporting_sources: Vec<RunReference>,
    pub confidence_basis: Vec<ConfidenceEvidence>,
    pub unresolved_questions: Vec<String>,
    pub warnings: Vec<String>,
    pub produced_output_candidates: Vec<RunReference>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}

pub enum HandoffDecisionKind {
    Accept,
    AcceptWithWarnings,
    RequestRevision,
    RequestCrossCheck,
    Reject,
}

pub struct HandoffValidationReport {
    pub id: HandoffValidationReportId,
    pub handoff_id: HandoffArtifactId,
    pub schema_valid: bool,
    pub evidence_satisfied: bool,
    pub trust_satisfied: bool,
    pub deterministic_checks: Vec<HandoffCheckResult>,
    pub issues: Vec<HandoffIssue>,
    pub recommendation: HandoffDecisionKind,
    pub validated_at: Timestamp,
}
```

`Accept` requires schema, evidence and trust checks to pass. A claimed confidence number is never sufficient evidence by itself.

---

### Task 1: Add immutable plan, step and criteria domain contracts

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/planning/mod.rs`
- Create: `crates/vestrace-domain/src/planning/mode.rs`
- Create: `crates/vestrace-domain/src/planning/plan.rs`
- Create: `crates/vestrace-domain/src/planning/step.rs`
- Create: `crates/vestrace-domain/src/planning/criteria.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline domain unit and property tests

**Interfaces:**
- Adds `ExecutionPlanId`, `ExecutionPlanRevisionId`, `PlanValidationReportId`, `PlanActivationId`, `ReplanRequestId`, `ReplanDecisionId`, `DelegationRequestId`, `DelegationGrantId`, `SubRunBindingId`, `RemoteAgentDefinitionRevisionId`, `RemoteAgentInvocationId`, `RemoteAgentEventId`, `RemoteAgentReconciliationId`, `HandoffArtifactId`, `HandoffValidationReportId`, `HandoffDecisionId` and `CrossCheckRequestId`.
- Produces `ExecutionMode`, `PlanStepKey`, `PlanStepRef`, `PlanStepKind`, `PlanStepAction`, `PlanStepDefinition`, `PlanSuccessCriteria`, `ExecutionPlanRevision` and builders.

- [ ] **Step 1: Write failing plan invariant tests**

```rust
#[test]
fn plan_step_keys_are_stable_dotted_identifiers() {
    assert!(PlanStepKey::parse("research.collect").is_ok());
    assert!(PlanStepKey::parse("Research Collect").is_err());
    assert!(PlanStepKey::parse("research..collect").is_err());
}

#[test]
fn step_kind_must_match_action() {
    let result = PlanStepDefinitionBuilder::test_default()
        .kind(PlanStepKind::ToolCall)
        .action(PlanStepAction::End)
        .build();
    assert!(result.is_err());
}

#[test]
fn direct_mode_still_requires_a_persistable_plan() {
    let plan = DirectPlanBuilder::test_reason_step().build().unwrap();
    assert_eq!(plan.mode, ExecutionMode::Direct);
    assert!(!plan.steps.is_empty());
    assert!(plan.steps.iter().any(|step| step.kind == PlanStepKind::End));
}
```

Run:

```bash
cargo test -p vestrace-domain planning
```

Expected: FAIL because the planning domain does not exist.

- [ ] **Step 2: Implement bounded identifiers, text and schemas**

Implement the normative parsing and size constraints. Compile every schema at creation. Reject empty objectives, revision zero, duplicate step keys, blank capability/tool collections where the action requires them and mismatched content hashes.

- [ ] **Step 3: Implement deterministic plan hashing**

Canonical bytes include:

```text
vestrace-plan/v1
workspace UUID
run UUID
plan UUID
revision
mode
objective
ordered assumptions
success criteria
steps sorted by PlanStepKey
for each step: exact definition and referenced revision IDs
requested budget
required approval bindings
source identity
superseded revision or nil
```

Dependencies are sorted by key; user-visible ordered lists such as assumptions retain order.

- [ ] **Step 4: Implement step-definition safety validation**

Reject Tool actions without a tool revision, delegation actions with the wrong target kind, human actions without an input schema, End actions with payloads and retry/timeout values above repository constants.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain planning
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(planning): add immutable plan contracts"
```

---

### Task 2: Implement plan proposal ports and deterministic Direct, Guided and Workflow sources

**Files:**
- Create: `crates/vestrace-application/src/planning/mod.rs`
- Create: `crates/vestrace-application/src/planning/ports.rs`
- Create: `crates/vestrace-application/src/planning/commands.rs`
- Create: `crates/vestrace-application/src/planning/direct.rs`
- Create: `crates/vestrace-application/src/planning/proposal.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-planning-test-support/Cargo.toml`
- Create: `crates/vestrace-planning-test-support/src/lib.rs`
- Create: `crates/vestrace-planning-test-support/src/fakes.rs`
- Create: `crates/vestrace-planning-test-support/src/fixtures.rs`
- Create: `crates/vestrace-planning-test-support/src/plan_builder.rs`
- Modify: root `Cargo.toml`
- Test: `crates/vestrace-application/tests/direct_planning.rs`
- Test: `crates/vestrace-application/tests/guided_planning.rs`
- Test: `crates/vestrace-application/tests/workflow_planning.rs`

**Interfaces:**
- Produces `PlanProposalPort`, `WorkflowDefinitionPort`, `InternalAgentEligibilityPort`, `RemoteAgentEligibilityPort`, `HumanInteractionPlanningPort`, `DirectPlanBuilder`, `PlanProposalService` and deterministic fakes.

- [ ] **Step 1: Write object-safety and source tests**

```rust
fn accepts_plan_proposer(_port: std::sync::Arc<dyn PlanProposalPort>) {}
fn accepts_workflows(_port: std::sync::Arc<dyn WorkflowDefinitionPort>) {}
fn accepts_internal_agents(_port: std::sync::Arc<dyn InternalAgentEligibilityPort>) {}
fn accepts_remote_agents(_port: std::sync::Arc<dyn RemoteAgentEligibilityPort>) {}
```

Guided tests assert a model proposal cannot choose `run_id`, `created_by`, policy IDs, approval IDs or budget account IDs. Workflow tests assert parameters cannot mutate the immutable workflow graph outside declared bindings.

- [ ] **Step 2: Implement DirectPlanBuilder**

Direct requests use an explicit `DirectExecutionDirective` selected by trusted application code:

```rust
pub enum DirectExecutionDirective {
    ModelReason { purpose: ModelStepPurpose, output_schema: serde_json::Value },
    ToolCall { tool_revision_id: ToolRevisionId, arguments: serde_json::Value },
}
```

The builder creates one actionable step plus `direct.end`, explicit success criteria and bounded budget. It does not infer a tool ID from arbitrary model text.

- [ ] **Step 3: Implement Guided proposal normalization**

`PlanProposalPort` calls H3 structured output through an adapter supplied in composition. Normalize only plan fields allowed by `PlanProposalDraft`. Generate authoritative IDs, Run binding, budgets and source metadata in application code. Reject unknown fields and content exceeding limits before validation.

- [ ] **Step 4: Implement Workflow source boundary**

```rust
#[async_trait::async_trait]
pub trait WorkflowDefinitionPort: Send + Sync {
    async fn instantiate(
        &self,
        context: &RequestContext,
        workflow_revision_id: WorkflowDefinitionRevisionId,
        parameters: CanonicalArguments,
        run_id: AgentRunId,
    ) -> Result<ExecutionPlanRevision, ApplicationError>;
}
```

H5 provides a deterministic fixture. H9 later supplies production workflow storage without changing this signature.

- [ ] **Step 5: Keep human planning unavailable until H7**

The default `NoHumanInteractionPlanning` reports `unsupported_step_kind`. A plan containing HumanInput or HumanApproval fails activation until a registered implementation reports support for the exact interaction kind.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application \
  --test direct_planning --test guided_planning --test workflow_planning
cargo test -p vestrace-planning-test-support
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-planning-test-support
git commit -m "feat(planning): add plan source boundaries"
```

---

### Task 3: Implement deterministic PlanValidator and reference resolution

**Files:**
- Create: `crates/vestrace-domain/src/planning/validation.rs`
- Create: `crates/vestrace-application/src/planning/validator.rs`
- Create: `crates/vestrace-application/tests/plan_validation.rs`
- Modify: `crates/vestrace-domain/src/planning/mod.rs`

**Interfaces:**
- Produces `PlanValidationIssue`, `PlanValidationReport`, `PlanReferenceSnapshot`, `PlanValidator` and pure graph helpers.
- Consumes H2 budget/policy summaries, H4 Tool Registry, agent eligibility ports, context-reference availability and human-step support.

- [ ] **Step 1: Write graph validation tests**

Cover duplicate keys, missing dependencies, a two-node cycle, self-cycle, unreachable branch, no reachable End and an End step with dependants.

```rust
#[test]
fn cycle_is_reported_with_stable_step_path() {
    let report = validate_fixture(plan_with_cycle("a", "b"));
    assert_issue(&report, PlanValidationCode::CycleDetected, Some("a"));
}
```

- [ ] **Step 2: Implement deterministic graph analysis**

Use sorted `PlanStepKey` traversal. Produce stable issue ordering by severity, step key, code and path. Reject unlimited loops rather than attempting runtime loop detection.

- [ ] **Step 3: Implement component and authority validation**

Resolve exact active Tool revisions, internal agents, remote agents and workflow constraints. Check required skills, capability ceilings, allowed tools, data classification, delegation depth and activation status. Remote declared skills are eligibility inputs only and cannot grant authority.

- [ ] **Step 4: Implement budget, approval and parallelism validation**

Aggregate worst-case requested resources using bounded retry counts. Validate against H2 available allocation summaries. Count maximum simultaneously ready internal and remote delegation steps and enforce configured ceilings. Verify every statically known required approval binding exists in the plan.

- [ ] **Step 5: Implement context reachability and output-path validation**

Every input reference must be available initially or produced by a dependency with a compatible output contract. Required plan outputs must have at least one reachable producer and a path to End.

- [ ] **Step 6: Hash the reference snapshot**

Store exact resolved Tool revision IDs/hashes, agent eligibility revisions, workflow revision, policy snapshot ID, budget snapshot reference and validator version. Activation later rejects a stale report if these hashes differ.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test plan_validation
cargo test -p vestrace-domain planning::validation
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(planning): validate plan graphs and authority"
```

---

### Task 4: Persist plans, validation reports and immutable revisions

**Files:**
- Create: `migrations/0037_execution_plans_revisions_steps_validation.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/planning/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/planning/plan_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/planning/validation_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/plan_persistence.rs`

**Interfaces:**
- Creates `execution_plans`, `execution_plan_revisions`, `execution_plan_steps`, `execution_plan_dependencies`, `plan_validation_reports`, `plan_validation_issues` and `plan_reference_snapshots`.
- Produces `ExecutionPlanRepositoryPort` and `PlanValidationRepositoryPort` PostgreSQL implementations.

- [ ] **Step 1: Write failing immutability tests**

Persist revision 1, create revision 2, and assert revision 1 plus its steps/dependencies remain byte-for-byte readable. Application-role update/delete of revision, step, dependency, report or issue rows must fail.

- [ ] **Step 2: Create migration `0037`**

Use normalized rows for graph topology and JSONB for bounded policies/schemas. Add unique constraints for `(plan_id, revision)`, `(plan_revision_id, step_key)` and dependency edges. Store content, reference and schema hashes.

- [ ] **Step 3: Implement atomic plan-revision creation**

Creation writes plan identity if new, one revision, all steps, dependencies and source metadata in one transaction. Duplicate idempotency keys return the existing identical revision; a conflicting payload fails.

- [ ] **Step 4: Persist validation reports append-only**

A report references exact plan content and component hashes. Multiple reports may exist, but activation can use only a valid report from the current validator compatibility range.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test plan_persistence
git add migrations/0037_execution_plans_revisions_steps_validation.sql crates tests/plan_persistence.rs
git commit -m "feat(planning): persist immutable plan revisions"
```

---

### Task 5: Implement activation, scheduler projection and mode-independent step routing

**Files:**
- Create: `migrations/0038_plan_activations_scheduler_and_replanning.sql`
- Create: `crates/vestrace-application/src/planning/activation.rs`
- Create: `crates/vestrace-application/src/planning/scheduler.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/planning/activation_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/planning/scheduler_repository.rs`
- Create: `crates/vestrace-application/tests/plan_scheduler.rs`
- Create: `tests/plan_activation_atomicity.rs`
- Create: `tests/plan_scheduler_persistence.rs`

**Interfaces:**
- Produces `PlanActivationService`, `PlanScheduler`, `StepExecutionRouter`, `PlanActivationRepositoryPort` and `PlanSchedulerRepositoryPort`.
- Creates `plan_activations`, `plan_scheduler_snapshots`, `plan_step_dispatches` and initial replanning tables owned by migration `0038`.

- [ ] **Step 1: Write activation stale-report tests**

Change a Tool revision eligibility or policy reference after validation. Activation must fail with `stale_validation_report` and perform no Run mutation or RunStep creation.

- [ ] **Step 2: Implement atomic activation**

```text
load Run and exact plan revision
→ load valid matching validation report
→ recheck component/reference hash
→ create PlanActivation
→ project immutable steps into H1 RunStep rows
→ create scheduler snapshot
→ emit one PlanActivated RunEvent
→ enqueue SchedulePlan
```

All rows and the H1 logical mutation commit atomically through a scoped persistence boundary.

- [ ] **Step 3: Implement readiness calculation**

A step is Ready only when every dependency is terminal-successful or satisfies an explicit optional-dependency rule. Failed required dependencies make the step Skipped or block the plan according to completion policy. Traversal and dispatch order use stable topological order then step key.

- [ ] **Step 4: Implement routing without direct I/O**

```text
Reason / Evaluate / Synthesize
→ H3 model execution command

ToolCall
→ H4 create ToolInvocation command

DelegateInternal
→ H5 StartInternalDelegation work

DelegateRemote
→ H5 StartRemoteDelegation work

HumanInput / HumanApproval
→ H7 port, unavailable in H5 default composition

End
→ completion evaluation only
```

The scheduler records one dispatch binding before enqueueing downstream work and never calls provider/tool/remote ports inline.

- [ ] **Step 5: Persist scheduler projection**

Use optimistic `scheduler_revision`. Snapshot updates, dispatch bindings and follow-up work are atomic. Scheduler retries rebuild the projection from H1 steps when revision conflict occurs; they do not duplicate downstream logical commands.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test plan_scheduler
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test plan_activation_atomicity --test plan_scheduler_persistence
git add migrations/0038_plan_activations_scheduler_and_replanning.sql crates tests
git commit -m "feat(planning): activate and schedule plans"
```

---

### Task 6: Implement bounded replanning and immutable completed-step preservation

**Files:**
- Create: `crates/vestrace-domain/src/planning/replanning.rs`
- Create: `crates/vestrace-application/src/planning/replanner.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/planning/replanning_repository.rs`
- Create: `crates/vestrace-application/tests/replanning.rs`
- Create: `tests/plan_replanning_persistence.rs`
- Modify: `crates/vestrace-domain/src/planning/mod.rs`

**Interfaces:**
- Produces `ReplanPolicy`, `ReplanRequest`, `PlanRevisionDiff`, `ReplanDecision`, `PlanReplanner` and `ReplanningRepositoryPort`.

- [ ] **Step 1: Write completed-step preservation tests**

```rust
#[test]
fn replacement_cannot_change_completed_step_definition() {
    let current = completed_step_fixture("research.collect");
    let proposed = change_completed_output_schema(current.plan());
    let result = compare_replan(current.snapshot(), proposed);
    assert_issue(result, PlanValidationCode::CompletedStepChanged);
}
```

Add tests for removing completed steps, changing their dependencies, changing a running step and safely replacing a pending step.

- [ ] **Step 2: Implement quiescence protocol**

Replanning marks the Run as planning-paused for new dispatch, records a requested barrier and waits until inflight steps are terminal or durable waiting. It never cancels an H4 commit or remote dispatch merely to activate a new graph.

- [ ] **Step 3: Implement bounded proposal context**

The replanner supplies failure codes, plan/step references, verification outcomes and bounded safe summaries. It excludes hidden reasoning, secrets, raw provider/tool metadata and the unrestricted parent transcript.

- [ ] **Step 4: Implement revision diff and validation**

Create a full new immutable revision. Preserve exact definitions for completed/running keys, allow additions and changes only within `allowed_change_scope`, then run the same PlanValidator. A diff is persisted before activation.

- [ ] **Step 5: Enforce revision and resource limits**

Check H2 reservation for `RunSteps`, model invocations and wall time. When limits are exhausted, transition to `Partial`, `WaitingForInput` or `Failed` according to completion policy instead of attempting another replan.

- [ ] **Step 6: Persist decisions from migration `0038`**

Use the previously created `replan_requests`, `replan_decisions` and `plan_revision_diffs` tables. Decisions are append-only and reference both old and proposed revisions.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test replanning
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test plan_replanning_persistence
git add crates/vestrace-domain crates/vestrace-application crates/vestrace-infrastructure tests
git commit -m "feat(planning): add bounded immutable replanning"
```

---

### Task 7: Add delegation scope, eligibility and one-time grant contracts

**Files:**
- Create: `crates/vestrace-domain/src/delegation/mod.rs`
- Create: `crates/vestrace-domain/src/delegation/request.rs`
- Create: `crates/vestrace-domain/src/delegation/scope.rs`
- Create: `crates/vestrace-domain/src/delegation/grant.rs`
- Create: `crates/vestrace-application/src/delegation/mod.rs`
- Create: `crates/vestrace-application/src/delegation/ports.rs`
- Create: `crates/vestrace-application/src/delegation/commands.rs`
- Create: `crates/vestrace-application/src/delegation/eligibility.rs`
- Create: `crates/vestrace-application/src/delegation/scope.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-application/tests/delegation_scope.rs`

**Interfaces:**
- Produces `DelegationTargetRef`, `DelegationRequest`, `DelegationScope`, `DelegationGrant`, `DelegationToken`, `DelegationEligibilityService` and `DelegationScopeService`.

- [ ] **Step 1: Write authority-intersection tests**

Parent has capabilities `{read, write}`, target allows `{read, search}`, request asks `{read, write, search}` and policy permits `{read}`. Effective capability set must be `{read}`. Repeat for tools, context classifications, memory scope and budgets.

- [ ] **Step 2: Write depth/cycle/parallelism tests**

Reject depth above policy ceiling, parent lineage containing the proposed child Run, a consumed grant, a second child for a one-use grant and creation above internal/remote parallelism limits.

- [ ] **Step 3: Implement eligibility snapshots**

Internal eligibility returns exact Agent snapshot ID, declared capabilities/skills, allowed tools and lifecycle. Remote eligibility returns exact remote revision ID, locally trusted identity/status, locally allowed skills/transports/data classifications and health. Remote declarations cannot widen local allowances.

- [ ] **Step 4: Compute delegated scope fail-closed**

```text
parent capability/tool/context/budget ceiling
∩ target eligibility
∩ DelegationRequest
∩ workflow constraints
∩ current H2 policy and obligations
```

Record every omitted requested capability, tool and context reference with a stable reason code.

- [ ] **Step 5: Issue one-time grant through H2**

`delegation.create_internal` and `delegation.create_remote` use different actions and resources. H2 `ActionGuardService` creates/consumes authorization and budget allocations before the grant becomes consumable.

- [ ] **Step 6: Implement token semantics**

The token contains only `DelegationGrantId`. Consumption loads the grant and verifies target kind, parent Run/step, workspace, expiration, H2 allocation, unused status and request/scope hashes. Never serialize it into model or remote input.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test delegation_scope
cargo test -p vestrace-domain delegation
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(delegation): add narrowed delegation grants"
```

---

### Task 8: Persist delegation grants, SubRun bindings and remote invocations

**Files:**
- Create: `migrations/0039_delegations_subruns_and_remote_invocations.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/delegation/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/delegation/grant_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/delegation/subrun_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/delegation/remote_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/delegation_persistence.rs`
- Create: `tests/delegation_budget_isolation.rs`
- Create: `tests/remote_invocation_persistence.rs`

**Interfaces:**
- Creates `delegation_requests`, `delegation_request_context`, `delegation_request_tools`, `delegation_grants`, `delegation_scope_capabilities`, `delegation_scope_tools`, `delegation_omissions`, `subrun_bindings`, `remote_agent_invocations`, `remote_agent_events` and initial reconciliation rows.
- Produces PostgreSQL `DelegationRepositoryPort`, `SubRunBindingRepositoryPort` and `RemoteAgentInvocationRepositoryPort`.

- [ ] **Step 1: Write ownership and one-time-use tests**

Assert same workspace for parent Run/step, grant and allocation; one consumed grant binds exactly one consumer; internal grants cannot bind remote invocations; remote grants cannot bind child Runs; lineage depth is monotonic; application-role update/delete of immutable grants/bindings fails.

- [ ] **Step 2: Create migration `0039`**

Persist request and effective scope separately so requested authority cannot be confused with granted authority. Store only context references/classifications, not copied content. Remote definition IDs are opaque validated UUIDs until H9 provides the registry tables.

- [ ] **Step 3: Implement atomic grant consumption**

Lock the grant, H2 allocation and active parallelism counters. Mark consumed and create either `SubRunBinding` or `RemoteAgentInvocation` in one transaction. Conflicting consumption returns `delegation_already_consumed` without creating a second consumer.

- [ ] **Step 4: Persist remote events append-only**

Events contain canonical status, bounded safe message, external IDs/cursor hashes and output candidate references. Raw transport frames, credentials and protocol SDK values are never stored.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test delegation_persistence \
             --test delegation_budget_isolation \
             --test remote_invocation_persistence
git add migrations/0039_delegations_subruns_and_remote_invocations.sql crates tests
git commit -m "feat(delegation): persist grants subruns and remote invocations"
```

---

### Task 9: Implement atomic internal SubRun creation and isolated execution context

**Files:**
- Create: `crates/vestrace-domain/src/delegation/subrun.rs`
- Create: `crates/vestrace-application/src/delegation/subrun_service.rs`
- Create: `crates/vestrace-application/tests/subrun_service.rs`
- Create: `tests/subrun_atomicity.rs`
- Modify: `crates/vestrace-domain/src/delegation/mod.rs`

**Interfaces:**
- Produces `SubRunBinding`, `SubRunCreationService`, `DelegatedInputManifest` and `ChildRunBootstrap`.
- Consumes H1 Run creation, H2 allocations, eligibility/scope service and delegation persistence.

- [ ] **Step 1: Write isolated-child bootstrap test**

Create a parent with extra memory, tools and context. Delegate only one context reference, one Tool and read capability. Assert the child bootstrap contains only that manifest and the child cannot resolve omitted parent references.

- [ ] **Step 2: Write atomicity failure tests**

Inject failures after grant lock, after child Run creation draft and after binding draft. Assert either all of child Run, H1 `ParentRunLink`, binding, consumed grant, budget allocation and parent waiting event commit, or none commit.

- [ ] **Step 3: Implement child Run bootstrap**

The child Run receives:

```text
new Run ID
ParentRunLink
exact target AgentRuntimeSnapshotId
delegated objective
expected output schema
DelegatedInputManifest references
effective capability/tool/memory scope
child budget allocation reference
lineage depth
completion policy
```

It does not receive parent transcript, scratchpad, checkpoints, approval grants or undelegated memory.

- [ ] **Step 4: Enforce one-level default**

Child execution context carries lineage and maximum depth. A later delegation request from the child fails when depth equals the effective ceiling, even if the target Agent declares broader permissions.

- [ ] **Step 5: Bind parent waiting state**

Parent step becomes Waiting and parent Run uses `WaitingForDependency` when no other ready work exists. Child progress does not mutate parent RunVersion. Only child terminal/handoff events enqueue parent continuation.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test subrun_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test subrun_atomicity
git add crates/vestrace-domain crates/vestrace-application tests/subrun_atomicity.rs
git commit -m "feat(delegation): create isolated internal subruns"
```

---

### Task 10: Implement transport-neutral remote-agent dispatch and reconciliation

**Files:**
- Create: `crates/vestrace-domain/src/delegation/remote.rs`
- Create: `crates/vestrace-application/src/delegation/remote_service.rs`
- Create: `crates/vestrace-application/src/delegation/reconciliation.rs`
- Create: `crates/vestrace-planning-test-support/src/remote_agent.rs`
- Create: `crates/vestrace-application/tests/remote_agent_service.rs`
- Create: `tests/remote_unknown_reconciliation.rs`
- Modify: `crates/vestrace-domain/src/delegation/mod.rs`

**Interfaces:**
- Produces `RemoteAgentInvocation`, `RemoteAgentPort`, `RemoteAgentDispatchService`, `RemoteAgentObservationService`, `RemoteAgentReconciliationService` and deterministic remote fixture.

- [ ] **Step 1: Write dispatch-order test**

Assert:

```text
load Prepared invocation and exact grant
→ rebuild delegated input manifest hash
→ H2 ActionGuard for remote_agent.dispatch
→ consume exact ticket and allocation
→ persist Dispatching
→ call RemoteAgentPort
→ persist canonical observation
→ schedule observe, handoff validation or reconciliation
```

The remote fixture must observe that Dispatching and ticket consumption occurred before its first call.

- [ ] **Step 2: Write lost-response test**

The deterministic remote accepts one external task, records it by dispatch idempotency key and drops the response. Assert invocation becomes `Unknown`, one reconciliation exists, restart causes no second dispatch and the same external task is found during reconciliation.

- [ ] **Step 3: Implement canonical observation mapping**

Normalize fixture observations to Prepared, Dispatching, Working, waiting states, terminal candidates or Unknown. A remote `Succeeded` observation stores a handoff candidate and enters validation; it does not directly complete the parent step.

- [ ] **Step 4: Implement cancellation semantics**

Cancellation has its own H2 operation fingerprint and records a request. A Cancelled observation does not claim remote side effects were rolled back. Ambiguous cancellation remains Unknown or WaitingForDependency.

- [ ] **Step 5: Implement reconciliation outcomes**

```rust
pub enum RemoteAgentReconciliationOutcome {
    FoundWorking,
    FoundSucceeded,
    FoundFailed,
    FoundCancelled,
    FailedSafeToRedispatch,
    StillUnknown,
}
```

Only `FailedSafeToRedispatch` permits a new dispatch, and it enqueues an explicit protected retry rather than calling the port inline.

- [ ] **Step 6: Prove SDK independence**

The H5 workspace and tests compile without any `a2a` crate. A script later fails if `a2a` types or dependencies appear in H5 domain/application crates.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test remote_agent_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test remote_unknown_reconciliation
git add crates/vestrace-domain crates/vestrace-application \
  crates/vestrace-planning-test-support tests/remote_unknown_reconciliation.rs
git commit -m "feat(delegation): add remote invocation boundary"
```

---

### Task 11: Implement handoff validation, acceptance and bounded cross-checks

**Files:**
- Create: `crates/vestrace-domain/src/delegation/handoff.rs`
- Create: `crates/vestrace-domain/src/delegation/cross_check.rs`
- Create: `crates/vestrace-application/src/delegation/handoff_service.rs`
- Create: `crates/vestrace-application/src/delegation/cross_check.rs`
- Create: `crates/vestrace-application/tests/handoff_validation.rs`
- Create: `migrations/0040_handoffs_cross_checks_and_remote_events.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/delegation/handoff_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/delegation/cross_check_repository.rs`
- Create: `tests/handoff_persistence.rs`

**Interfaces:**
- Produces `HandoffArtifact`, `HandoffValidationReport`, `HandoffDecision`, `HandoffValidationService`, `CrossCheckRequest`, `CrossCheckService` and persistence ports.

- [ ] **Step 1: Write schema/evidence/trust tests**

Cover invalid structured output, missing required source, untrusted remote result for a strict policy, output candidate with unavailable reference, unsupported claimed confidence and a valid internal handoff.

- [ ] **Step 2: Implement canonical handoff hashing and size limits**

Hash summary, structured output, source/output references, warnings and unresolved questions. Store no binary bytes. Bound summary to 64 KiB, structured JSON to 4 MiB, each list count and individual string length.

- [ ] **Step 3: Implement validation order**

```text
producer identity and delegation binding
→ content hash
→ expected output JSON Schema
→ required output/reference availability
→ deterministic assertions
→ evidence requirements
→ local trust policy
→ recommendation
```

Remote claims, model prose and confidence fields cannot override failed deterministic checks.

- [ ] **Step 4: Implement protected acceptance**

`handoff.accept` uses H2 when policy requires review. Acceptance atomically stores the decision, marks the delegation consumer complete, updates the parent step output references, emits one parent RunEvent and enqueues scheduling. Reject and RequestRevision do not expose broader context or budget to the producer.

- [ ] **Step 5: Implement bounded cross-check**

A cross-check is a new internal SubRun or remote invocation with a distinct target where policy requires independence. It receives the handoff plus selected evidence references, not producer scratchpad. Enforce maximum rounds, budget and target-distinctness rules.

- [ ] **Step 6: Create migration `0040`**

Create append-only `handoff_artifacts`, `handoff_sources`, `handoff_output_candidates`, `handoff_validation_reports`, `handoff_issues`, `handoff_decisions`, `cross_check_requests`, `cross_check_results` and extended remote event/reconciliation tables. Unique constraints prevent two terminal acceptance decisions for one handoff.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test handoff_validation
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test handoff_persistence
git add migrations/0040_handoffs_cross_checks_and_remote_events.sql crates tests/handoff_persistence.rs
git commit -m "feat(delegation): validate and accept handoffs"
```

---

### Task 12: Integrate Coordinator work, checkpoint V3, Run events and restart recovery

**Files:**
- Create: `crates/vestrace-application/src/coordinator/mod.rs`
- Create: `crates/vestrace-application/src/coordinator/service.rs`
- Create: `crates/vestrace-application/src/coordinator/worker.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/work.rs`
- Modify: `crates/vestrace-domain/src/run/checkpoint.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Modify: `crates/vestrace-application/src/run/worker.rs`
- Create: `crates/vestrace-application/tests/coordinator_restart.rs`

**Interfaces:**
- Produces `CoordinatorService`, `CoordinatorWorkHandler` and `RunCheckpointV3` planning/delegation fields.
- Adds H1 work kinds for plan validation/activation/scheduling/replanning, internal/remote delegation, remote observation/reconciliation, handoff validation and cross-checks.

- [ ] **Step 1: Add canonical Run events**

```rust
PlanRevisionCreated { plan_revision_id: ExecutionPlanRevisionId },
PlanActivated { activation_id: PlanActivationId, plan_revision_id: ExecutionPlanRevisionId },
PlanReplanRequested { request_id: ReplanRequestId },
PlanRevisionSuperseded { old_revision_id: ExecutionPlanRevisionId, new_revision_id: ExecutionPlanRevisionId },
InternalDelegationCreated { request_id: DelegationRequestId, child_run_id: AgentRunId },
RemoteDelegationCreated { request_id: DelegationRequestId, invocation_id: RemoteAgentInvocationId },
DelegationWaiting { request_id: DelegationRequestId, reason_code: String },
HandoffAccepted { handoff_id: HandoffArtifactId, decision_id: HandoffDecisionId },
HandoffRejected { handoff_id: HandoffArtifactId, decision_id: HandoffDecisionId },
```

Remote progress, scheduler snapshots and validation issues remain H5 events/projections and do not increment parent RunVersion.

- [ ] **Step 2: Add work kinds**

```text
ProposePlan
ValidatePlan
ActivatePlan
SchedulePlan
RequestReplan
StartInternalDelegation
StartRemoteDelegation
ObserveRemoteDelegation
ReconcileRemoteDelegation
ValidateHandoff
RunCrossCheck
```

Every work item contains stable IDs and expected logical revisions, never large prompts, schemas, secrets or external transport payloads.

- [ ] **Step 3: Define `RunCheckpointV3` additions**

```rust
pub struct PlanningCheckpoint {
    pub active_plan_revision_id: Option<ExecutionPlanRevisionId>,
    pub active_plan_activation_id: Option<PlanActivationId>,
    pub scheduler_revision: Option<u64>,
    pub pending_replan_request_id: Option<ReplanRequestId>,
    pub active_internal_delegation_ids: Vec<DelegationRequestId>,
    pub active_remote_invocation_ids: Vec<RemoteAgentInvocationId>,
    pub pending_handoff_ids: Vec<HandoffArtifactId>,
}
```

Checkpoint envelope decoding remains compatible with older versions. Durable plan/delegation tables remain authoritative; checkpoint fields accelerate resume.

- [ ] **Step 4: Write critical restart-window tests**

Cover crashes:

```text
after valid plan persisted before activation
→ activate same revision once

after activation before scheduler work completes
→ rebuild same readiness projection

after child Run created before parent waiting event
→ atomic commit proves both or neither

after remote Dispatching before response
→ observe/reconcile, never duplicate dispatch

after child terminal before handoff validation
→ validate stored handoff without rerunning child

after handoff accepted before parent scheduler wake-up
→ enqueue/recover continuation once

after replan validated before superseding activation
→ old revision remains active until atomic activation
```

- [ ] **Step 5: Implement Coordinator state transitions**

The Coordinator acquires H1 Run lease for parent logical mutations, checks RunVersion and scheduler/invocation revisions, then calls application services. It never holds a database transaction across model, tool, child worker or remote I/O.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test coordinator_restart
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(coordinator): integrate planning and delegation recovery"
```

---

### Task 13: Add RLS, operational indexes, boundary gates and the H5 acceptance scenario

**Files:**
- Create: `migrations/0041_planning_delegation_rls_indexes_and_run_bindings.sql`
- Create: `scripts/verify-planning-boundary.sh`
- Create: `scripts/verify-delegation-boundary.sh`
- Modify: `.github/workflows/ci.yml`
- Create: `tests/planning_delegation_rls.rs`
- Create: `tests/h5_acceptance.rs`
- Modify: schema snapshots where the repository stores them

**Interfaces:**
- Produces forced-RLS, consistency and acceptance gates for H5.

- [ ] **Step 1: Create migration `0041`**

Force RLS on every H5 table. Add same-workspace and parent/step/plan consistency triggers, immutable/append-only guards, active-status indexes, remote-observation deadlines, scheduler/replan queues, lineage indexes and constraints preventing self-parent or cyclic SubRun bindings.

- [ ] **Step 2: Add boundary scripts**

`verify-planning-boundary.sh` rejects provider, Rig, Reqwest, RMCP, Docker and A2A types in planning/domain signatures. `verify-delegation-boundary.sh` rejects:

```text
RemoteAgentInvocation aliasing AgentRun or SubRun
remote-agent binding in H4 Tool Runtime
a2a-rs dependency in H5 crates
DelegationToken fields in model/remote DTOs
credential or approval material in delegated input
parent mutation methods exposed to child services
```

- [ ] **Step 3: Add CI jobs**

Required jobs:

```text
planning-domain-and-validator
planning-postgres
internal-subrun-delegation
remote-agent-fixture
handoff-validation
planning-delegation-boundaries
h5-acceptance
```

All use deterministic local fixtures and no internet/provider credentials.

- [ ] **Step 4: Write mandatory H5 acceptance scenario**

Create a Guided plan with:

```text
research.internal.a
research.internal.b
research.remote.c
synthesize.result
end
```

Assert:

1. the model plan proposal is normalized and cannot set authority-bearing IDs;
2. PlanValidator creates one deterministic valid report;
3. activation creates immutable H1 RunSteps and a scheduler snapshot;
4. two internal delegations and one deterministic remote invocation receive narrower capabilities, tools, context and H2 allocations;
5. internal children cannot read omitted context or mutate the parent;
6. remote fixture receives no Vestrace ticket, approval, credential or parent checkpoint;
7. parallelism and depth ceilings are enforced;
8. restart while all three delegations are active creates no duplicate child Run or remote task;
9. one internal handoff with an invalid evidence contract is rejected;
10. the second internal handoff is accepted;
11. the remote handoff is schema-valid but requires an independent cross-check under its trust policy;
12. the cross-check passes within its bounded budget;
13. synthesis becomes Ready only after accepted required handoffs;
14. a failed synthesis triggers one bounded replan that preserves all completed delegation-step definitions and outputs;
15. the replacement revision activates only after quiescence;
16. the parent completes only after success criteria pass;
17. replay reconstructs the same logical plan/delegation state without model, tool, child or remote I/O;
18. no SDK-specific, secret, raw transport or hidden-reasoning data appears in durable records.

- [ ] **Step 5: Add negative scenarios**

Test a plan cycle, capability escalation, excessive budget, unavailable Human step, second-level delegation without policy, ambiguous remote dispatch, stale validation report, altered completed step and acceptance of a handoff with missing evidence. Every scenario must fail closed with a stable code.

- [ ] **Step 6: Run all H5 gates**

```bash
bash scripts/verify-planning-boundary.sh
bash scripts/verify-delegation-boundary.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h5_acceptance --test planning_delegation_rls
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: every command exits `0` without external services.

- [ ] **Step 7: Commit**

```bash
git add migrations/0041_planning_delegation_rls_indexes_and_run_bindings.sql \
  .github/workflows/ci.yml scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(planning): add H5 acceptance and boundary gates"
```

---

## H5 completion definition

H5 is complete only when all thirteen tasks pass and evidence demonstrates:

```text
user objective
→ Direct / Guided / Workflow plan source
→ immutable ExecutionPlanRevision
→ deterministic PlanValidationReport
→ atomic PlanActivation
→ H1 RunStep projection
→ stable dependency scheduling
→ H3/H4/internal/remote routing
→ narrowed delegation scope
→ internal SubRun or RemoteAgentInvocation
→ validated HandoffArtifact
→ optional bounded cross-check
→ parent continuation
→ bounded replan at quiescence
→ verified plan completion
```

The completed implementation must satisfy all of these statements:

1. Every execution mode produces a durable validated plan revision.
2. Model-proposed plans cannot assign authority-bearing IDs, budgets, policies or approvals.
3. Plan graph validation is deterministic and rejects cycles, missing dependencies and missing completion paths.
4. Referenced Tool, agent, workflow, policy and budget revisions are snapshotted for validation freshness.
5. An activated plan revision and its H1 RunStep projection are immutable historical records.
6. Scheduler polling and readiness projections do not alter RunVersion.
7. Replanning cannot rewrite completed or running step definitions.
8. Replacement revisions activate only after the declared quiescence barrier.
9. Replanning is bounded by revision, attempt, resource and wall-time ceilings.
10. Internal SubRun creation is atomic with one-time grant consumption, parent binding and H2 allocation.
11. Child Runs receive only delegated context, memory, capabilities, tools and budgets.
12. Default delegation depth is one and parallelism limits are enforced transactionally.
13. Remote invocations are separate from AgentRun, SubRun and ToolInvocation.
14. H5 remote contracts compile and test without `a2a-rs`.
15. Ambiguous remote dispatch becomes Unknown and reconciliation, not duplicate work.
16. Remote success is only an observation until handoff validation passes.
17. Internal and external results share one Handoff validation contract without sharing runtime ownership.
18. Handoff acceptance requires schema, evidence and trust conditions.
19. Cross-checks use distinct bounded delegations and cannot recurse without policy.
20. Parent logical state changes emit one canonical H1 event each.
21. Restart at every critical planning/delegation window does not duplicate a child Run, remote task, handoff decision or replan activation.
22. Replay performs no model, tool, child or remote I/O.
23. H6 can materialize handoff/output references as Artifacts without rewriting H5 records.
24. H7 can implement Human steps and remote-input continuation through the reserved ports.
25. H8 can supply delegated connections and request-scoped credentials without changing delegation scope persistence.
26. H9 can implement agent/workflow/remote registries behind existing eligibility ports.
27. H9A can implement A2A through `RemoteAgentPort` without exposing SDK types to H5.
28. H10 can add richer evaluation and trust scoring without changing accepted handoff history.
29. H5 tests require no public model, agent network, A2A server or permanent credential.

## Explicit non-goals

H5 does not implement:

- A2A, gRPC, SLIMRPC or any remote transport SDK;
- persistent Agent Package, Workflow or Remote Agent registries;
- user-facing human input, approval or authentication channels;
- OAuth/API-key acquisition or Credential Broker behavior;
- final Artifact storage, quarantine, representations or export;
- browser automation;
- unrestricted nested delegation;
- autonomous policy, budget or permission expansion;
- unbounded correction or replanning loops;
- rich model-based evaluation owned by H10;
- production replay of external operations.

These remain assigned to H6–H10 and H9A.

## Documentation-only boundary

Creating this document does not authorize implementation. During the current documentation phase, do not:

- create `feat/h5-planning-delegation`;
- change Cargo dependencies or workspace members;
- create migrations `0037`–`0041`;
- modify Run, model, tool or policy code;
- start child workers or remote fixtures;
- change CI;
- execute validation, PostgreSQL or acceptance tests.
