# Vestrace H2 Policy, Approval and Budget Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the centralized, versioned policy decision layer, operation-bound approval grants, hierarchical resource budgets, atomic reservations, Run/SubRun allocations and workspace quotas required to govern every later Harness action.

**Architecture:** Extend the v0.1 capability and approval foundation instead of replacing it. Existing capability authorization remains the base ceiling; H2 policy bundles may deny, restrict to preparation, require approval or add enforceable obligations, but can never grant a missing capability. A Vestrace-owned `ActionGuardService` combines a canonical operation fingerprint, a policy snapshot, matching approval grants and an atomic budget reservation into a short-lived authorization ticket that an enforcement point must consume before protected work begins.

**Tech Stack:** Existing Vestrace v0.1 and H1 Rust workspace, Rust Edition 2024, Tokio, Serde, Schemars, SQLx, PostgreSQL 17, SHA-256, tracing, proptest and the repository PostgreSQL integration-test harness.

## Global Constraints

- Complete all five v0.1 plans and `2026-07-31-vestrace-h1-durable-run-core.md` before implementing H2.
- Existing v0.1 `Capability`, `Sensitivity`, policy bindings, `ApprovalRecord` and audit contracts remain valid and are extended rather than replaced.
- PostgreSQL is authoritative for policy revisions, snapshots, decisions, authorization tickets, approval grants, budget accounts, reservations, allocations, quota leases and accounting journals.
- Domain and application crates must not depend on SQLx, Axum, Rig, a model provider or a tool adapter.
- Authorization defaults to deny when base capability authorization fails, a policy explicitly denies, a required approval is absent or an authorization ticket is invalid.
- H2 policy rules can only preserve or reduce authority granted by the v0.1 capability layer.
- An approval never overrides an explicit deny, a missing base capability or a hard budget limit.
- Every protected operation is bound to canonical action, resource, normalized arguments, workspace, Run and optional step identifiers.
- Authorization tickets and approval grants are not reusable bearer permissions outside their bound operation and workspace.
- Budget reservations lock all ancestor accounts in deterministic order before checking limits.
- Concurrent reservations cannot push any ancestor balance above its hard limit.
- Actual usage is never discarded. If actual usage exceeds its reservation, the overage is journaled, affected accounts become overdrawn and new reservations are denied until limits or balances are reconciled.
- Policy decisions, authorization-ticket events, approval-grant events, budget events and quota events are append-only.
- Existing migrations `0012_tokens_policies_and_approvals.sql` and `0013_audit_and_redaction.sql` are never edited.
- H2 migrations start at `0023` and are created once.
- H2 does not implement model calls, tool execution, sandboxing, user-facing approval channels, scheduling fairness or trigger execution. It provides governing contracts consumed by those plans.
- CI must not require an external provider, Rig, MCP server or network access.
- Branch: `feat/h2-policy-approval-budget-core`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  security/authorization.rs
  security/policy.rs
  security/approval_grant.rs
  security/mod.rs
  budget/mod.rs
  budget/dimension.rs
  budget/limit.rs
  budget/account.rs
  budget/reservation.rs
  budget/quota.rs
  run/event.rs
  run/checkpoint.rs

crates/vestrace-application/src/
  policy/mod.rs
  policy/ports.rs
  policy/evaluator.rs
  policy/authorization.rs
  approval/mod.rs
  approval/ports.rs
  approval/service.rs
  budget/mod.rs
  budget/commands.rs
  budget/ports.rs
  budget/service.rs
  budget/quota.rs
  guard/mod.rs
  guard/service.rs

crates/vestrace-application/tests/
  policy_evaluator.rs
  policy_authorization.rs
  approval_grants.rs
  budget_service.rs
  action_guard.rs

crates/vestrace-infrastructure/src/postgres/
  policy/mod.rs
  policy/bundle_repository.rs
  policy/decision_repository.rs
  policy/ticket_repository.rs
  policy/approval_grant_repository.rs
  policy/action_guard.rs
  budget/mod.rs
  budget/account_repository.rs
  budget/reservation_repository.rs
  budget/quota_repository.rs
  budget/snapshot_repository.rs

migrations/
  0023_policy_bundles_and_snapshots.sql
  0024_policy_decisions_tickets_and_approval_grants.sql
  0025_resource_budget_accounts_and_limits.sql
  0026_budget_reservations_events_allocations_quotas.sql
  0027_policy_budget_rls_and_run_bindings.sql

tests/
  policy_persistence.rs
  authorization_tickets.rs
  approval_consumption.rs
  budget_accounts.rs
  budget_reservations.rs
  budget_allocations.rs
  workspace_quotas.rs
  policy_budget_rls.rs
  h2_acceptance.rs
```

## Normative contracts

### Base capability ceiling

The v0.1 capability evaluator is exposed through this adapter:

```rust
#[async_trait::async_trait]
pub trait CapabilityAuthorizationPort: Send + Sync {
    async fn authorize_capabilities(
        &self,
        context: &RequestContext,
        request: &AuthorizationRequest,
    ) -> Result<BaseAuthorizationDecision, ApplicationError>;
}

pub enum BaseAuthorizationDecision {
    Permit { matched_binding_ids: Vec<PolicyId> },
    Deny { reason: BaseDenyReason },
}
```

`BaseAuthorizationDecision::Deny` is final. H2 never converts it to permit.

### Evaluation and recorded authorization

Evaluation and persistence are intentionally separate:

```rust
#[async_trait::async_trait]
pub trait PolicyEvaluationPort: Send + Sync {
    async fn evaluate(
        &self,
        context: &RequestContext,
        request: &AuthorizationRequest,
    ) -> Result<AuthorizationDecisionDraft, ApplicationError>;
}

#[async_trait::async_trait]
pub trait PolicyEnginePort: Send + Sync {
    async fn authorize(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationDecision, ApplicationError>;
}
```

`PolicyEnginePort::authorize` evaluates and journals a decision exactly once, but does not issue an execution ticket. Protected execution must use `ActionGuardService`, which evaluates through `PolicyEvaluationPort` and atomically persists the decision with its ticket and reservation. This prevents duplicate decision rows.

### Operation identity

```rust
pub struct AuthorizationRequest {
    pub principal_id: PrincipalId,
    pub acting_agent_snapshot_id: Option<AgentRuntimeSnapshotId>,
    pub run_id: Option<AgentRunId>,
    pub step_id: Option<RunStepId>,
    pub action: ActionId,
    pub resource: ResourceRef,
    pub normalized_arguments: CanonicalArguments,
    pub data_classification: DataClassification,
    pub risk: Option<RiskLevel>,
    pub execution_environment: Option<ExecutionEnvironmentClass>,
    pub delegation_lineage: Vec<AgentRunId>,
    pub approval_grant_ids: Vec<ApprovalGrantId>,
    pub budget_state: BudgetStateSummary,
    pub requested_at: Timestamp,
}

pub enum OperationFingerprintSchema {
    V1,
}

pub struct OperationFingerprint {
    pub schema: OperationFingerprintSchema,
    pub hash: [u8; 32],
}
```

The V1 canonical byte sequence contains:

```text
vestrace-operation/v1
workspace UUID
principal UUID
acting agent snapshot UUID or nil
run UUID or nil
step UUID or nil
action UTF-8
resource kind UTF-8
resource UUID
canonical arguments JSON bytes
```

JSON canonicalization recursively sorts object keys by UTF-8 byte order, preserves array order, rejects non-finite numbers and serializes without insignificant whitespace. The same canonicalizer is used by policy decisions, approvals, idempotency checks and authorization tickets.

### Decision lattice

Final decision precedence is:

```text
Deny
> PrepareOnly
> RequireApproval
> PermitWithObligations
> Permit
```

- explicit `Deny` always wins;
- `PrepareOnly` denies the requested execute/commit action but does not deny a separately authorized prepare action;
- `RequireApproval` becomes `PermitWithObligations` only when matching valid grants are present;
- obligations are unioned and de-duplicated by canonical representation;
- no matching H2 rule inherits the permitted base capability unless an applicable bundle has `unmatched = Deny`;
- a lower policy layer cannot remove a restriction introduced by an upper layer.

### Policy layers

```text
System
Deployment
Workspace
Principal
Agent
Workflow
Run
Delegation
Tool
```

Snapshots store exact immutable bundle revision IDs in this order and include a content hash. Current policy is evaluated for every new protected action; an old Run does not retain stale authority merely because it began under a previous snapshot.

### Budget units

Every amount is an unsigned integer in a dimension-specific base unit:

```rust
pub enum BudgetDimension {
    MoneyMicrounits { currency: CurrencyCode },
    InputTokens,
    OutputTokens,
    ModelInvocations,
    ToolInvocations,
    RunSteps,
    WallClockMillis,
    SandboxCpuMillis,
    SandboxMemoryBytes,
    StorageBytes,
    NetworkBytes,
    ArtifactCount,
    SubrunCount,
    ParallelismSlots,
}

#[serde(transparent)]
pub struct ResourceAmount(u64);
```

No floating-point values are stored or compared in the budget core.

### Budget limits

```rust
pub struct ResourceLimit {
    pub soft: Option<ResourceAmount>,
    pub approval: Option<ResourceAmount>,
    pub hard: Option<ResourceAmount>,
}
```

For every defined pair: `soft <= approval <= hard`. Missing thresholds are skipped. Crossing soft emits a warning. Crossing approval requires a matching approval grant. Crossing hard is always denied until the account limit is revised; an approval cannot bypass hard.

### Atomic action guard

```rust
pub struct GuardedAction {
    pub authorization_ticket_id: AuthorizationTicketId,
    pub policy_decision_id: PolicyDecisionId,
    pub policy_snapshot_id: PolicySnapshotId,
    pub budget_reservation_id: Option<BudgetReservationId>,
    pub operation_fingerprint: OperationFingerprint,
    pub obligations: Vec<PolicyObligation>,
    pub expires_at: Timestamp,
}
```

`ActionGuardService::prepare` evaluates policy and creates any required reservation before issuing the ticket. `consume` atomically fences ticket use and approval-grant use before the enforcement point starts work. `complete` reconciles actual usage and records obligation results. `abort` releases unconsumed reservations. A consumed ticket is never reusable beyond its configured use count.

---

### Task 1: Add authorization, resource and obligation domain types

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/security/authorization.rs`
- Modify: `crates/vestrace-domain/src/security/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline unit and property tests

**Interfaces:**
- Adds IDs `PolicyBundleId`, `PolicyBundleRevisionId`, `PolicySnapshotId`, `PolicyDecisionId`, `AuthorizationTicketId`, `ApprovalGrantId`, `BudgetAccountId`, `BudgetReservationId`, `BudgetEventId`, `BudgetAllocationId`, `QuotaLeaseId`, `BudgetIncreaseRequestId`, `TriggerDefinitionId` and `AgentProfileId`.
- Produces `ActionId`, `ResourceKind`, `ResourceRef`, `DataClassification`, `DataLabel`, `RiskLevel`, `ExecutionEnvironmentClass`, `CanonicalArguments`, `OperationFingerprint`, `PolicyObligation`, `AuthorizationRequest` and `AuthorizationDecision`.

- [ ] **Step 1: Write failing action and canonicalization tests**

```rust
#[test]
fn action_id_requires_dotted_lowercase_segments() {
    assert!("tool.invoke".parse::<ActionId>().is_ok());
    assert!("Tool Invoke".parse::<ActionId>().is_err());
    assert!("tool..invoke".parse::<ActionId>().is_err());
}

#[test]
fn canonical_arguments_ignore_object_insertion_order() {
    let left = CanonicalArguments::new(serde_json::json!({"b": 2, "a": 1})).unwrap();
    let right = CanonicalArguments::new(serde_json::json!({"a": 1, "b": 2})).unwrap();
    assert_eq!(left.bytes(), right.bytes());
    assert_eq!(left.hash(), right.hash());
}
```

Run:

```bash
cargo test -p vestrace-domain security::authorization
```

Expected: FAIL because the types do not exist.

- [ ] **Step 2: Implement validated action and resource identities**

`ActionId` accepts lowercase ASCII segments separated by one dot, each segment 1–64 bytes, total length at most 255 bytes. `ResourceRef::new` requires a non-nil resource UUID and authoritative workspace ID.

- [ ] **Step 3: Implement canonical arguments and fingerprints**

Add `sha2` to workspace dependencies. Implement recursive key sorting and deterministic serialization without provider request types. Unit tests cover nested objects, arrays, Unicode strings, integer boundaries and rejection of non-finite values introduced through custom deserialization.

- [ ] **Step 4: Implement classification and obligations**

```rust
pub struct DataClassification {
    pub sensitivity: Sensitivity,
    pub labels: std::collections::BTreeSet<DataLabel>,
}

pub enum PolicyObligation {
    RedactLabels { labels: std::collections::BTreeSet<DataLabel> },
    RequireSandbox { class: ExecutionEnvironmentClass },
    RestrictProviderClass { class: String },
    RetainAuditRecord,
    VerifyExternalState,
    DeleteTemporaryArtifacts,
    LimitExternalRecipients { maximum: u32 },
    ConsumeApprovalGrant { grant_id: ApprovalGrantId },
}
```

Reject blank provider classes and zero recipient limits.

- [ ] **Step 5: Implement final decision types**

```rust
pub enum AuthorizationDecisionKind {
    Permit,
    PermitWithObligations,
    PrepareOnly,
    RequireApproval,
    Deny,
}

pub struct AuthorizationDecision {
    pub id: PolicyDecisionId,
    pub request_fingerprint: OperationFingerprint,
    pub snapshot_id: PolicySnapshotId,
    pub kind: AuthorizationDecisionKind,
    pub obligations: Vec<PolicyObligation>,
    pub matched_rule_ids: Vec<String>,
    pub explanation_code: String,
    pub explanation: String,
    pub valid_until: Timestamp,
}
```

Explanation code is a stable dotted identifier; text is bounded to 8 KiB. Deny and unsatisfied RequireApproval decisions never issue an authorization ticket.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain security::authorization
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/vestrace-domain
git commit -m "feat(policy): add authorization domain contracts"
```

---

### Task 2: Implement immutable policy bundles, snapshots and pure evaluation

**Files:**
- Create: `crates/vestrace-domain/src/security/policy.rs`
- Create: `crates/vestrace-application/src/policy/mod.rs`
- Create: `crates/vestrace-application/src/policy/evaluator.rs`
- Create: `crates/vestrace-application/tests/policy_evaluator.rs`
- Modify: `crates/vestrace-domain/src/security/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces `PolicyLayer`, `PolicyOwnerRef`, `PolicyBundle`, `PolicyBundleRevision`, `PolicyRule`, `PolicyEffect`, `PolicyCondition`, `UnmatchedPolicy`, `PolicySnapshot` and pure `evaluate_policy`.

- [ ] **Step 1: Write failing restrictive-lattice tests**

Cover:

```rust
#[test]
fn explicit_deny_wins_over_lower_layer_permit() { /* system deny + run permit => deny */ }

#[test]
fn missing_contextual_match_inherits_base_permit() { /* unmatched inherit => permit */ }

#[test]
fn unmatched_deny_restricts_base_permit() { /* workspace unmatched deny => deny */ }

#[test]
fn obligations_are_canonicalized_and_deduplicated() { /* duplicate audit obligation => one */ }
```

Run:

```bash
cargo test -p vestrace-application --test policy_evaluator
```

Expected: FAIL because `evaluate_policy` is undefined.

- [ ] **Step 2: Implement bundle and revision types**

```rust
pub struct PolicyBundleRevision {
    pub id: PolicyBundleRevisionId,
    pub bundle_id: PolicyBundleId,
    pub revision: u32,
    pub layer: PolicyLayer,
    pub owner: PolicyOwnerRef,
    pub unmatched: UnmatchedPolicy,
    pub rules: Vec<PolicyRule>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Rules are immutable, carry stable IDs unique within a revision and are sorted by explicit priority then rule ID. Reject duplicate IDs, revision zero and content-hash mismatches.

- [ ] **Step 3: Implement typed matching conditions**

Support exact/prefix action patterns, resource kind/ID, principal/agent/run/step bindings, maximum sensitivity, required/forbidden labels, risk range, execution environment and UTC time windows. Do not execute policy scripts, model-supplied regular expressions or arbitrary expressions.

- [ ] **Step 4: Implement the pure evaluator**

```rust
pub fn evaluate_policy(
    base: BaseAuthorizationDecision,
    snapshot: &PolicySnapshot,
    request: &AuthorizationRequest,
    valid_approval_grants: &[ApprovalGrant],
    now: Timestamp,
) -> Result<AuthorizationDecisionDraft, ApplicationError>;
```

It validates snapshot hash/order, applies every applicable revision, uses the normative lattice, validates but does not consume approvals, de-duplicates obligations and returns matched rule IDs plus stable explanation code/text.

- [ ] **Step 5: Add property tests**

Use `proptest` to prove adding a deny rule cannot make a result less restrictive, reordering equivalent rules does not change the result and duplicate obligations do not change the canonical decision hash.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test policy_evaluator
cargo test -p vestrace-domain security::policy
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(policy): evaluate restrictive policy bundles"
```

---

### Task 3: Add operation-bound approval grants

**Files:**
- Create: `crates/vestrace-domain/src/security/approval_grant.rs`
- Create: `crates/vestrace-application/src/approval/mod.rs`
- Create: `crates/vestrace-application/src/approval/ports.rs`
- Create: `crates/vestrace-application/src/approval/service.rs`
- Create: `crates/vestrace-application/tests/approval_grants.rs`
- Modify: `crates/vestrace-domain/src/security/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces `ApprovalMode::{Once, ForCurrentRun}`, `ArgumentConstraint`, `OperationBinding`, `ApprovalGrant`, `ApprovalGrantStatus`, `IssueApprovalGrant`, `RevokeApprovalGrant`, `ApprovalGrantPort` and `ApprovalService`.
- `ApprovalGrant` references the existing v0.1 `ApprovalRecordId` that proves a human or administrator approved the request.

- [ ] **Step 1: Write failing binding and lifecycle tests**

```rust
#[test]
fn approval_for_one_resource_cannot_authorize_another() {
    let grant = grant_fixture(resource_a());
    assert!(!grant.matches(&request_fixture(resource_b()), now()));
}

#[test]
fn once_grant_has_exactly_one_available_use() {
    let grant = once_grant_fixture();
    assert_eq!(grant.maximum_executions(), 1);
}
```

Also test expiry, revocation, wrong Run, wrong action and mismatched canonical arguments.

- [ ] **Step 2: Implement typed argument constraints**

```rust
pub enum ArgumentConstraint {
    ExactArgumentsHash([u8; 32]),
    FieldEquals { json_pointer: String, value_hash: [u8; 32] },
    FieldIn { json_pointer: String, allowed_value_hashes: Vec<[u8; 32]> },
}
```

JSON pointers must be RFC 6901 absolute pointers; allowed sets are non-empty and duplicate hashes are rejected.

- [ ] **Step 3: Implement immutable operation binding**

`OperationBinding` contains action, resource selector, argument constraints, optional Run binding and optional step binding. Its canonical hash is stored in the grant. `Once` enforces maximum executions `1`; `ForCurrentRun` requires a Run ID and a configured maximum from `1..=10_000`.

- [ ] **Step 4: Implement grant issuance service**

`ApprovalService::issue` loads an approved and unexpired v0.1 `ApprovalRecord`, verifies its canonical payload hash equals the requested `OperationBinding`, then creates the H2 grant. Rejected, expired or superseded records cannot issue grants.

- [ ] **Step 5: Define atomic usage port**

```rust
#[async_trait::async_trait]
pub trait ApprovalGrantPort: Send + Sync {
    async fn load_many(
        &self,
        context: &RequestContext,
        ids: &[ApprovalGrantId],
    ) -> Result<Vec<ApprovalGrant>, ApplicationError>;

    async fn consume(
        &self,
        context: &RequestContext,
        grant_id: ApprovalGrantId,
        expected_binding_hash: [u8; 32],
        at: Timestamp,
    ) -> Result<ApprovalGrant, ApplicationError>;

    async fn revoke(
        &self,
        context: &RequestContext,
        command: RevokeApprovalGrant,
    ) -> Result<ApprovalGrant, ApplicationError>;
}
```

Stable errors are `approval_exhausted`, `approval_expired`, `approval_revoked` and `approval_mismatch`.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test approval_grants
cargo test -p vestrace-domain security::approval_grant
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(approval): add operation-bound approval grants"
```

---

### Task 4: Add hierarchical budget domain and decisions

**Files:**
- Create: `crates/vestrace-domain/src/budget/mod.rs`
- Create: `crates/vestrace-domain/src/budget/dimension.rs`
- Create: `crates/vestrace-domain/src/budget/limit.rs`
- Create: `crates/vestrace-domain/src/budget/account.rs`
- Create: `crates/vestrace-domain/src/budget/reservation.rs`
- Create: `crates/vestrace-domain/src/budget/quota.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline unit and property tests

**Interfaces:**
- Produces `CurrencyCode`, `BudgetDimension`, `ResourceAmount`, `ResourceLimit`, `BudgetOwner`, `BudgetAccount`, `BudgetBalance`, `BudgetReservation`, `BudgetReservationLine`, `BudgetAllocation`, `BudgetLimitDecision`, `WorkspaceQuotaKind` and `QuotaLimit`.

- [ ] **Step 1: Write failing limit-order and overflow tests**

```rust
#[test]
fn resource_limit_rejects_inverted_thresholds() {
    assert!(ResourceLimit::new(
        Some(ResourceAmount::new(100)),
        Some(ResourceAmount::new(90)),
        Some(ResourceAmount::new(200)),
    ).is_err());
}

#[test]
fn amount_addition_rejects_overflow() {
    let max = ResourceAmount::new(u64::MAX);
    assert!(max.checked_add(ResourceAmount::new(1)).is_err());
}
```

- [ ] **Step 2: Implement dimensions and base units**

`CurrencyCode` accepts exactly three uppercase ASCII letters. Each `BudgetDimension` has a stable storage key; money keys include currency, for example `money_microunits:USD`.

- [ ] **Step 3: Implement account hierarchy**

```rust
pub enum BudgetOwner {
    Deployment,
    Workspace(WorkspaceId),
    Principal(PrincipalId),
    Trigger(TriggerDefinitionId),
    AgentProfile(AgentProfileId),
    Run(AgentRunId),
    Step(RunStepId),
    SubRun(AgentRunId),
}
```

`BudgetAccount` has one optional parent, immutable owner, status `Active | Frozen | Overdrawn | Closed`, revision and per-dimension limits.

- [ ] **Step 4: Implement deterministic limit evaluation**

```rust
pub enum BudgetLimitDecision {
    Allowed,
    AllowedWithSoftWarning { dimensions: Vec<BudgetDimension> },
    ApprovalRequired { dimensions: Vec<BudgetDimension> },
    HardLimitExceeded { dimensions: Vec<BudgetDimension> },
}
```

Evaluate proposed `consumed + reserved + request`; sort returned dimensions by stable key.

- [ ] **Step 5: Implement reservation and allocation lifecycles**

Reservation states: `Prepared`, `Active`, `Consumed`, `Reconciled`, `Released`, `Expired`, `Cancelled`, `Overrun`. Allocation states: `Active`, `Returned`, `Exhausted`, `Cancelled`. Terminal states do not reopen. Duplicate dimensions are merged with checked addition.

- [ ] **Step 6: Add property tests and commit**

Prove increasing a request cannot improve its decision, releasing never increases reserved balance and an allocation cannot exceed its parent reservation.

```bash
cargo test -p vestrace-domain budget
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(budget): add hierarchical budget domain"
```

---

### Task 5: Define policy, ticket, budget and guard application boundaries

**Files:**
- Create: `crates/vestrace-application/src/policy/ports.rs`
- Create: `crates/vestrace-application/src/policy/authorization.rs`
- Create: `crates/vestrace-application/src/budget/mod.rs`
- Create: `crates/vestrace-application/src/budget/commands.rs`
- Create: `crates/vestrace-application/src/budget/ports.rs`
- Create: `crates/vestrace-application/src/guard/mod.rs`
- Create: `crates/vestrace-application/src/guard/service.rs`
- Create: `crates/vestrace-application/tests/policy_authorization.rs`
- Create: `crates/vestrace-application/tests/action_guard.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces `PolicySnapshotPort`, `PolicyDecisionJournalPort`, `PolicyEvaluationPort`, `PolicyEnginePort`, `BudgetAccountPort`, `BudgetReservationPort`, `BudgetSnapshotPort`, `ActionGuardCommitPort`, `PolicyAuthorizationService` and `ActionGuardService`.

- [ ] **Step 1: Define policy snapshot and decision journals**

```rust
#[async_trait::async_trait]
pub trait PolicySnapshotPort: Send + Sync {
    async fn load_current(
        &self,
        context: &RequestContext,
        request: &AuthorizationRequest,
    ) -> Result<PolicySnapshot, ApplicationError>;
}

#[async_trait::async_trait]
pub trait PolicyDecisionJournalPort: Send + Sync {
    async fn append(
        &self,
        context: &RequestContext,
        record: PolicyDecisionRecord,
    ) -> Result<(), ApplicationError>;
}
```

Decision records store hashes, classifications and typed references, never secret argument values.

- [ ] **Step 2: Define authorization tickets**

```rust
pub struct AuthorizationTicket {
    pub id: AuthorizationTicketId,
    pub workspace_id: WorkspaceId,
    pub policy_decision_id: PolicyDecisionId,
    pub request_fingerprint: OperationFingerprint,
    pub approval_grant_ids: Vec<ApprovalGrantId>,
    pub budget_reservation_id: Option<BudgetReservationId>,
    pub obligations: Vec<PolicyObligation>,
    pub maximum_uses: u32,
    pub uses: u32,
    pub status: AuthorizationTicketStatus,
    pub expires_at: Timestamp,
}

pub enum AuthorizationTicketStatus {
    Issued,
    Consumed,
    Completed,
    Aborted,
    Expired,
    Violated,
}
```

Tickets default to one use. More uses require policy and every attached grant to allow the same count.

- [ ] **Step 3: Define budget ports**

```rust
#[async_trait::async_trait]
pub trait BudgetReservationPort: Send + Sync {
    async fn preview(
        &self,
        context: &RequestContext,
        request: &ReserveBudget,
    ) -> Result<BudgetPreview, ApplicationError>;

    async fn reserve(
        &self,
        context: &RequestContext,
        request: ReserveBudget,
    ) -> Result<BudgetReservation, ApplicationError>;

    async fn reconcile(
        &self,
        context: &RequestContext,
        command: ReconcileBudget,
    ) -> Result<BudgetReservation, ApplicationError>;

    async fn release(
        &self,
        context: &RequestContext,
        command: ReleaseBudget,
    ) -> Result<BudgetReservation, ApplicationError>;
}
```

All commands include idempotency keys. Requests include leaf account, Run/step, line items, expiry and optional approval grants.

- [ ] **Step 4: Implement evaluation and recorded authorization with fakes**

`PolicyEvaluationService` obtains base authorization, snapshot and candidate grants, then returns a draft without persistence. `PolicyAuthorizationService` uses it and appends exactly one final decision. Tests prove base deny cannot be overridden, expired grants do not satisfy approval and every direct authorization is journaled once.

- [ ] **Step 5: Define atomic guard port**

```rust
#[async_trait::async_trait]
pub trait ActionGuardCommitPort: Send + Sync {
    async fn issue(
        &self,
        context: &RequestContext,
        command: IssueGuardedAction,
    ) -> Result<GuardedAction, ApplicationError>;

    async fn record_rejection(
        &self,
        context: &RequestContext,
        decision: AuthorizationDecision,
    ) -> Result<(), ApplicationError>;

    async fn consume(
        &self,
        context: &RequestContext,
        command: ConsumeGuardedAction,
    ) -> Result<ConsumedActionGuard, ApplicationError>;

    async fn complete(
        &self,
        context: &RequestContext,
        command: CompleteGuardedAction,
    ) -> Result<(), ApplicationError>;

    async fn abort(
        &self,
        context: &RequestContext,
        command: AbortGuardedAction,
    ) -> Result<(), ApplicationError>;
}
```

`issue` persists one decision, optional reservation and ticket atomically. `record_rejection` persists a non-permit decision without ticket/reservation. `consume` increments ticket/grant uses and activates a reservation atomically.

- [ ] **Step 6: Implement ActionGuard orchestration with fakes**

Deny, PrepareOnly and unsatisfied RequireApproval call `record_rejection`. Permit decisions call `issue`. Duplicate issue idempotency returns the same ticket; duplicate consume cannot double-consume a Once grant.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test policy_authorization --test action_guard
cargo clippy -p vestrace-application --all-targets -- -D warnings
git add crates/vestrace-application
git commit -m "feat(guard): define policy budget and action guard ports"
```

---

### Task 6: Persist policy bundles, decisions, tickets and grants

**Files:**
- Create: `migrations/0023_policy_bundles_and_snapshots.sql`
- Create: `migrations/0024_policy_decisions_tickets_and_approval_grants.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/policy/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/policy/bundle_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/policy/decision_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/policy/ticket_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/policy/approval_grant_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/policy_persistence.rs`
- Create: `tests/authorization_tickets.rs`
- Create: `tests/approval_consumption.rs`

**Interfaces:**
- Produces `harness_policy_bundles`, `harness_policy_bundle_revisions`, `harness_policy_activations`, `policy_snapshots`, `policy_snapshot_revisions`, `policy_decisions`, `authorization_tickets`, `authorization_ticket_events`, `approval_grants` and `approval_grant_events`.

- [ ] **Step 1: Write failing immutable revision and snapshot tests**

Test duplicate revision, content-hash mismatch, update/delete rejection, invalid layer order, cross-workspace owner and activation of nonexistent revision.

- [ ] **Step 2: Create migration `0023`**

Bundles store stable identity, workspace, layer, owner and lifecycle status. Revisions store immutable rules JSON, unmatched mode, revision, SHA-256 hash and timestamps. Activations store exact revision/effective interval. Snapshots store ordered revision IDs and aggregate hash. Deferred triggers validate owner workspace and layer order.

- [ ] **Step 3: Write failing ticket and grant-use tests**

Test fingerprint mismatch, expired/revoked ticket, concurrent Once consumption, atomic rollback of ticket/grant counters and rejection of a grant referencing an unapproved v0.1 ApprovalRecord.

- [ ] **Step 4: Create migration `0024`**

Decisions and event tables are append-only. Ticket/grant rows contain mutable status and use counters guarded by expected counts; each mutation appends an event in the same transaction. Store only hashes and typed references for arguments.

- [ ] **Step 5: Implement repositories and atomic approval consumption**

Lock ticket and grant rows ordered by UUID, validate binding/expiry/status, increment counters and append events. Map failures to stable codes after loading current state.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test policy_persistence --test authorization_tickets --test approval_consumption
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add migrations/0023_policy_bundles_and_snapshots.sql migrations/0024_policy_decisions_tickets_and_approval_grants.sql crates/vestrace-infrastructure tests
git commit -m "feat(policy): persist decisions tickets and approval grants"
```

---

### Task 7: Persist budget accounts, limits and hierarchy

**Files:**
- Create: `migrations/0025_resource_budget_accounts_and_limits.sql`
- Create: `crates/vestrace-application/src/budget/service.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/account_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-application/tests/budget_service.rs`
- Create: `tests/budget_accounts.rs`

**Interfaces:**
- Produces `budget_accounts`, `budget_account_limits` and `budget_balances` plus `BudgetAccountPort` and hierarchy loading.

- [ ] **Step 1: Write failing hierarchy tests**

Test parent cycle, cross-workspace parent, duplicate owner binding, invalid threshold order, child hard limit above parent allocation and concurrent account revision conflict.

- [ ] **Step 2: Create migration `0025`**

Accounts contain owner kind/ID, parent, status, revision and timestamps. Limits use stable dimension key plus nullable soft/approval/hard BIGINT values with nonnegative/order checks. Balances contain consumed/reserved and revision. A deferred recursive trigger rejects cycles and invalid owner hierarchy.

- [ ] **Step 3: Implement account repository**

Provide create, load, load ancestor chain, revise limits with expected revision, freeze, mark overdrawn and close. Ancestor order returned to application is root-to-leaf; SQL lock order used later is UUID order.

- [ ] **Step 4: Implement pure budget preview service**

Load account chain and balances, merge request lines, evaluate every ancestor and return the most restrictive `BudgetLimitDecision` plus sorted warnings. Preview does not reserve or mutate.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test budget_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test budget_accounts
git add migrations/0025_resource_budget_accounts_and_limits.sql crates tests/budget_accounts.rs
git commit -m "feat(budget): persist hierarchical budget accounts"
```

---

### Task 8: Add atomic reservations, allocations, journals and quotas

**Files:**
- Create: `migrations/0026_budget_reservations_events_allocations_quotas.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/reservation_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/quota_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/snapshot_repository.rs`
- Create: `crates/vestrace-application/src/budget/quota.rs`
- Create: `tests/budget_reservations.rs`
- Create: `tests/budget_allocations.rs`
- Create: `tests/workspace_quotas.rs`

**Interfaces:**
- Produces `budget_reservations`, `budget_reservation_lines`, `budget_reservation_impacts`, `budget_allocations`, `budget_events`, `budget_snapshots`, `workspace_quotas`, `quota_counters`, `quota_leases` and `quota_events`.
- Produces PostgreSQL reservation, allocation, snapshot and quota ports.

- [ ] **Step 1: Write failing concurrent reservation tests**

Build Deployment → Workspace → Run accounts with hard money 10_000_000 microunits. Concurrently reserve 6_000_000 twice. Exactly one succeeds; the other returns `budget_hard_limit_exceeded`; no balance exceeds the limit.

Also test idempotent duplicate reservation, rollback when one ancestor fails and deterministic merge of duplicate dimensions.

- [ ] **Step 2: Create reservation and journal tables**

Reservation rows contain leaf account, Run/step, status, idempotency key, expiry and timestamps. Lines store dimension/amount. Impact rows record every ancestor affected. Budget events and snapshots are append-only; snapshots contain sorted balances and account revisions.

- [ ] **Step 3: Implement ancestor locking and reserve**

Load full chain, sort account UUIDs and acquire `FOR UPDATE` locks in that order. Re-read limits/balances, evaluate checked sums, insert reservation/lines/impacts, update balances, append event and create snapshot in one transaction.

- [ ] **Step 4: Implement reconciliation**

If actual <= reserved, move reserved to consumed and release difference. If actual > reserved, record actual, mark reservation Overrun, mark affected accounts Overdrawn, append `budget.estimate_exceeded` and deny new reservations. Never truncate actual usage.

- [ ] **Step 5: Implement allocations**

Allocation creates a parent reservation, child account and immutable allocation row in one transaction. Child hard limits equal allocated lines. Returning closes the child, reconciles consumed usage into the parent allocation and releases only unused remainder.

- [ ] **Step 6: Implement quota leases**

```rust
pub enum WorkspaceQuotaKind {
    ConcurrentRuns,
    QueuedRuns,
    ActiveSandboxes,
    ModelRequestsPerMinute,
    StorageBytes,
    DailySpendMicrounits { currency: CurrencyCode },
    TriggerExecutions,
    ExternalNotifications,
}
```

Lock counter, expire stale leases, check capacity, create lease and increment counter atomically. Release uses holder ID and generation fencing. Two concurrent acquisitions against limit one yield one success.

- [ ] **Step 7: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test budget_reservations --test budget_allocations --test workspace_quotas
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add migrations/0026_budget_reservations_events_allocations_quotas.sql crates tests
git commit -m "feat(budget): add reservations allocations and quotas"
```

---

### Task 9: Integrate policy and budget references with durable Runs

**Files:**
- Create: `migrations/0027_policy_budget_rls_and_run_bindings.sql`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/checkpoint.rs`
- Modify: `crates/vestrace-application/src/run/coordinator.rs`
- Create: `tests/policy_budget_rls.rs`
- Modify: `tests/run_replay.rs`

**Interfaces:**
- Adds `RunCheckpointPayload::V2` while retaining V1 decoding.
- Adds logical Run events for budget binding, authorization waiting and applied budget exhaustion.
- Adds Run-to-budget-account binding and forced RLS for every H2 table.

- [ ] **Step 1: Write failing checkpoint compatibility test**

Deserialize an H1 `run-checkpoint/v1`, replay unchanged, then create V2:

```rust
pub struct RunCheckpointPayloadV2 {
    pub state: RunCheckpointPayloadV1,
    pub last_policy_snapshot_id: Option<PolicySnapshotId>,
    pub budget_snapshot_id: Option<BudgetSnapshotId>,
    pub active_authorization_ticket_ids: Vec<AuthorizationTicketId>,
    pub active_budget_reservation_ids: Vec<BudgetReservationId>,
}
```

Reject duplicate ticket/reservation IDs.

- [ ] **Step 2: Extend logical Run events**

```rust
RunBudgetBound { account_id: BudgetAccountId },
AuthorizationWaitEntered { decision_id: PolicyDecisionId },
BudgetExhaustionApplied { snapshot_id: BudgetSnapshotId, disposition: BudgetExhaustionDisposition },
```

These are emitted only with a logical Run mutation. Routine policy evaluations, reservations, heartbeats and accounting changes stay in their own journals and do not increment RunVersion.

- [ ] **Step 3: Add Run budget binding**

Migration creates `run_budget_bindings(workspace_id, run_id, budget_account_id, created_at, closed_at)` with one active binding per Run. A deferred trigger verifies owner `BudgetOwner::Run(run_id)` and same workspace.

- [ ] **Step 4: Add forced RLS, indexes and append-only protection**

Apply forced RLS to every H2 workspace table. Index active-policy lookup, ticket/grant expiry, reservation expiry, account owner, budget timeline and quota lease expiry. Application roles cannot mutate immutable revisions or append-only journals.

- [ ] **Step 5: Integrate explicit Run dispositions**

Unsatisfied approval may transition a nonterminal Run to `WaitingForApproval` with `AuthorizationWaitEntered`. Budget exhaustion is applied only through an explicit command selecting `Pause`, `WaitingForInput` or `Partial`; `Partial` requires a nonblank `RunTerminalResult` listing completed and unmet work. Resume reevaluates current policy rather than trusting `last_policy_snapshot_id`.

- [ ] **Step 6: Verify RLS and replay**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test policy_budget_rls --test run_replay
git add migrations/0027_policy_budget_rls_and_run_bindings.sql crates tests
git commit -m "feat(run): bind policy and budgets to durable runs"
```

---

### Task 10: Implement atomic PostgreSQL ActionGuard commits

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/policy/action_guard.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/policy/mod.rs`
- Create: `tests/h2_acceptance.rs`

**Interfaces:**
- Produces `PostgresActionGuardStore` implementing `ActionGuardCommitPort`.
- Coordinates decisions, tickets, approval usage and budget reservations without executing the protected action.

- [ ] **Step 1: Write failing issue rollback test**

A permit decision plus reservation crossing an ancestor hard limit must leave no ticket, decision, approval use, reservation or balance change.

- [ ] **Step 2: Write failing concurrent consume test**

Issue one ticket bound to a Once grant. Consume concurrently from two workers. Exactly one succeeds; ticket use, grant use and reservation activation each equal one.

- [ ] **Step 3: Implement atomic issue**

Within one scoped transaction: verify idempotency, insert one decision, lock budget ancestors, create Prepared reservation if requested, insert ticket, append ticket/budget events, persist idempotent result and commit. Duplicate idempotency returns the original `GuardedAction`.

- [ ] **Step 4: Implement rejection recording**

`record_rejection` appends the decision exactly once using decision idempotency. It never creates ticket, grant use or reservation.

- [ ] **Step 5: Implement atomic consume**

Lock ticket, grants and reservation in deterministic table/UUID order. Validate fingerprint, expiry, status and remaining uses. Increment ticket/grant counts, move Prepared reservation to Active, append events and commit. No external action runs inside the transaction.

- [ ] **Step 6: Implement complete and abort**

`complete` stores typed obligation receipts, reconciles usage and marks ticket Completed or Violated. `abort` only accepts Issued tickets, releases Prepared reservation and marks Aborted. A consumed ticket cannot be aborted.

- [ ] **Step 7: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test h2_acceptance
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add crates/vestrace-infrastructure tests/h2_acceptance.rs
git commit -m "feat(guard): atomically issue and consume action permits"
```

---

### Task 11: Add H0-RIG fixtures and complete H2 acceptance

**Files:**
- Create: `crates/vestrace-application/src/guard/testing.rs`
- Create: `crates/vestrace-application/src/budget/testing.rs`
- Modify: `crates/vestrace-application/src/guard/mod.rs`
- Modify: `crates/vestrace-application/src/budget/mod.rs`
- Modify: `tests/h2_acceptance.rs`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md` only when contract names need to replace shorthand wording

**Interfaces:**
- Produces deterministic `TestPolicyEvaluation`, `TestApprovalGrantStore`, `TestBudgetReservationStore`, `TestActionGuardStore` and `DeterministicProtectedAction` fixtures for H0-RIG and downstream tests.
- Fixtures are available only under tests or explicit `test-support`; they contain no production bypass.

- [ ] **Step 1: Implement deterministic policy fixtures**

Provide Permit, Deny, PrepareOnly and RequireApproval snapshots with fixed IDs/timestamps. Fixtures use real fingerprints and evaluator logic.

- [ ] **Step 2: Implement deterministic budget fixtures**

Provide in-memory ancestor locking under a Tokio mutex, fixed thresholds and event capture. The fixture reproduces concurrency behavior rather than always permitting.

- [ ] **Step 3: Implement protected-action proof fixture**

`DeterministicProtectedAction` increments an atomic counter only after guard consume succeeds. Tests prove no ticket, wrong fingerprint, missing approval and hard budget failure leave counter zero; valid consume makes it one; repeat consume leaves it one.

- [ ] **Step 4: Complete the end-to-end H2 scenario**

Create a Run account under a workspace account. Authorize a protected action requiring capability, obligation, Once approval and money/token reservations. Consume, execute, complete with lower actual usage and verify exact policy snapshot/rules, one approval use, released unused amount, obligation receipt, V2 checkpoint reload, denied second execution and cross-workspace isolation.

- [ ] **Step 5: Run full verification**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test policy_persistence \
             --test authorization_tickets \
             --test approval_consumption \
             --test budget_accounts \
             --test budget_reservations \
             --test budget_allocations \
             --test workspace_quotas \
             --test policy_budget_rls \
             --test h2_acceptance
```

Expected: every command exits `0` without AI provider, Rig or network access.

- [ ] **Step 6: Commit**

```bash
git add crates/vestrace-application tests docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md
git commit -m "test(harness): complete H2 policy and budget acceptance"
```

---

## Migration sequence

```text
0023 policy bundles and snapshots
0024 policy decisions, tickets and approval grants
0025 resource budget accounts and limits
0026 budget reservations, events, allocations and quotas
0027 policy/budget RLS, indexes and Run bindings
```

Each migration is owned by one task and is created once. No task edits a migration after that migration's task commit.

## H2 exit gate

H2 is complete only when a deterministic protected action demonstrates:

1. missing base capability cannot be overridden by policy or approval;
2. explicit deny cannot be overridden by a lower layer;
3. RequireApproval accepts only a matching valid operation-bound grant;
4. enforcement cannot execute without consuming the exact ticket;
5. concurrent consumers cannot reuse a Once ticket or grant;
6. concurrent reservations cannot exceed an ancestor hard limit;
7. a SubRun cannot access unallocated parent budget;
8. actual usage is reconciled and unused reservation released;
9. overrun is recorded and freezes new spending instead of hiding usage;
10. forced RLS prevents cross-workspace policy, approval and budget access;
11. checkpoint V1 remains readable and V2 stores policy/budget references;
12. all decisions and accounting changes have append-only journals;
13. explicit budget exhaustion can produce a durable Partial outcome;
14. H0-RIG can use stable policy, budget, checkpoint and protected-action fixtures without infrastructure types.

## Documentation-only status

This plan is a documentation artifact. Creating or revising it does not start implementation, create a feature branch, modify Rust code or apply migrations. Implementation begins only after an explicit future instruction from the user.
