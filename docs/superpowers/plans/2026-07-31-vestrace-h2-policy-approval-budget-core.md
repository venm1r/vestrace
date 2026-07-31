# Vestrace H2 Policy, Approval and Budget Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the centralized, versioned policy decision layer, operation-bound approval grants, hierarchical resource budgets, atomic reservations, run/subrun allocations and workspace quotas required to govern every later Harness action.

**Architecture:** Extend the v0.1 capability and approval foundation instead of replacing it. Existing capability authorization remains the base ceiling; H2 policy bundles may deny, restrict to preparation, require an approval or add enforceable obligations, but can never grant a missing capability. A Vestrace-owned `ActionGuardService` combines a canonical operation fingerprint, a policy snapshot, an optional approval grant and an atomic budget reservation into a short-lived authorization ticket that an enforcement point must consume before protected work begins.

**Tech Stack:** Existing Vestrace v0.1 and H1 Rust workspace, Rust Edition 2024, Tokio, Serde, Schemars, SQLx, PostgreSQL 17, SHA-256, tracing, proptest and the repository PostgreSQL integration-test harness.

## Global Constraints

- Complete all five v0.1 plans and `2026-07-31-vestrace-h1-durable-run-core.md` before implementing H2.
- Existing v0.1 `Capability`, `Sensitivity`, policy bindings, `ApprovalRecord` and audit contracts remain valid and are extended rather than replaced.
- PostgreSQL is authoritative for policy revisions, snapshots, decisions, authorization tickets, approval grants, budget accounts, reservations, allocations, quota leases and accounting journals.
- Domain and application crates must not depend on SQLx, Axum, Rig, a model provider or a tool adapter.
- Authorization defaults to deny when base capability authorization fails, a policy explicitly denies, a required approval is absent, or an authorization ticket is invalid.
- H2 policy rules can only preserve or reduce authority granted by the v0.1 capability layer.
- An approval never overrides an explicit deny, a missing base capability or a hard budget limit.
- Every protected operation is bound to canonical action, resource, normalized arguments, workspace, Run and optional step identifiers.
- Authorization tickets and approval grants are never bearer permissions outside their bound operation and workspace.
- Budget reservations lock all ancestor accounts in deterministic order before checking limits.
- Concurrent reservations cannot push any ancestor balance above its hard limit.
- Actual usage is never discarded. If actual usage exceeds its reservation, the overage is journaled, the account becomes overdrawn, and new reservations are denied until limits or balances are reconciled.
- Policy decisions, policy ticket events, approval grant events and budget events are append-only.
- Existing migrations `0012_tokens_policies_and_approvals.sql` and `0013_audit_and_redaction.sql` are never edited.
- H2 migrations start at `0023` and are created once.
- H2 does not implement model calls, tool execution, sandboxing, user-facing approval channels, scheduling fairness or trigger execution. It provides the governing contracts consumed by those plans.
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
  policy/ticket.rs
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
  budget/mod.rs
  budget/account_repository.rs
  budget/reservation_repository.rs
  budget/quota_repository.rs
  budget/snapshot_repository.rs

migrations/
  0023_policy_bundles_and_snapshots.sql
  0024_policy_decisions_tickets_and_approval_grants.sql
  0025_resource_budget_accounts_and_limits.sql
  0026_budget_reservations_events_and_quotas.sql
  0027_policy_budget_rls_and_run_bindings.sql

tests/
  policy_persistence.rs
  authorization_tickets.rs
  approval_consumption.rs
  budget_reservations.rs
  budget_allocations.rs
  workspace_quotas.rs
  policy_budget_rls.rs
  h2_acceptance.rs
```

## Normative contracts

### Base authorization and final policy decision

The v0.1 capability evaluator is exposed to H2 through this adapter boundary:

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

H2 exposes the final downstream boundary:

```rust
#[async_trait::async_trait]
pub trait PolicyEnginePort: Send + Sync {
    async fn authorize(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationDecision, ApplicationError>;
}
```

A `BaseAuthorizationDecision::Deny` is final. H2 never converts it to permit.

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

pub struct OperationFingerprint {
    pub schema: &'static str,
    pub hash: [u8; 32],
}
```

The fingerprint schema is `vestrace-operation/v1`. Its canonical byte sequence is:

```text
schema marker
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

JSON canonicalization recursively sorts object keys by UTF-8 byte order, preserves array order, rejects non-finite numbers and serializes without insignificant whitespace. The same canonicalizer is used by policy decisions, approvals, idempotency checks and action tickets.

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
- `RequireApproval` becomes `PermitWithObligations` only when a matching valid grant is present;
- obligations are unioned and de-duplicated by their canonical representation;
- no matching H2 contextual rule inherits the permitted base capability unless an applicable bundle has `unmatched = Deny`;
- a lower policy layer cannot remove a restriction introduced by an upper layer.

### Policy layer order

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

Snapshots store exact immutable bundle revision IDs in this order and include a content hash. Current policy is evaluated for each new protected action; an old Run does not retain stale authority merely because it began under a previous snapshot.

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

### Budget limit semantics

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
- Adds IDs `PolicyBundleId`, `PolicyBundleRevisionId`, `PolicySnapshotId`, `PolicyDecisionId`, `AuthorizationTicketId`, `ApprovalGrantId`, `BudgetAccountId`, `BudgetReservationId`, `BudgetEventId`, `BudgetAllocationId`, `QuotaLeaseId` and `BudgetIncreaseRequestId`.
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

`ActionId` accepts lowercase ASCII segments separated by one dot, each segment 1–64 bytes, total length at most 255 bytes. `ResourceRef` includes the authoritative workspace ID, typed resource kind and UUID; constructors reject nil IDs and cross-workspace construction helpers.

- [ ] **Step 3: Implement canonical arguments and operation fingerprinting**

Add `sha2` to workspace dependencies. Implement recursive key sorting and deterministic serialization without using a provider-specific request type. Unit tests cover nested objects, arrays, Unicode strings, integer boundaries and rejection of non-finite values introduced through custom deserialization.

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

Obligation constructors reject blank provider classes and zero recipient limits.

- [ ] **Step 5: Implement final decision variants**

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
    pub explanation: String,
    pub valid_until: Timestamp,
}
```

Explanation is bounded to 8 KiB. Deny and RequireApproval decisions never issue an authorization ticket.

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
- Produces `PolicyLayer`, `PolicyBundle`, `PolicyBundleRevision`, `PolicyRule`, `PolicyEffect`, `PolicyCondition`, `UnmatchedPolicy`, `PolicySnapshot` and pure `evaluate_policy`.

- [ ] **Step 1: Write failing restrictive-lattice tests**

Cover all of these cases:

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

Rules are immutable, carry stable rule IDs unique inside a bundle revision and are sorted by explicit priority then rule ID. Reject duplicate IDs, empty rule sets with `unmatched = Inherit`, and content-hash mismatches.

- [ ] **Step 3: Implement typed matching conditions**

Support exact or prefix action patterns, resource kinds and IDs, principal/agent/run/step bindings, maximum sensitivity, required/forbidden labels, risk range, execution environment and UTC time windows. Do not execute scripts, regular expressions supplied by a model or arbitrary expressions.

- [ ] **Step 4: Implement pure deterministic evaluator**

```rust
pub fn evaluate_policy(
    base: BaseAuthorizationDecision,
    snapshot: &PolicySnapshot,
    request: &AuthorizationRequest,
    valid_approval_grants: &[ApprovalGrant],
    now: Timestamp,
) -> Result<AuthorizationDecisionDraft, ApplicationError>;
```

The evaluator:

1. returns deny immediately for base deny;
2. validates snapshot hash and layer order;
3. evaluates every applicable bundle revision;
4. applies the normative decision lattice;
5. validates supplied approval grants but does not consume them;
6. unions obligations deterministically;
7. returns matched rule IDs and a stable explanation code plus bounded human text.

- [ ] **Step 5: Add property tests**

Use `proptest` to prove adding a new deny rule cannot make a decision less restrictive, reordering equivalent rules does not change the result, and duplicate obligations do not change the canonical decision hash.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test policy_evaluator
cargo test -p vestrace-domain security::policy
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(policy): evaluate versioned restrictive policy bundles"
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
- `ApprovalGrant` references the existing v0.1 `ApprovalRecordId` that proved a human or administrator approved the request.

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

JSON pointers must be RFC 6901 absolute pointers, allowed sets are non-empty and duplicate hashes are rejected.

- [ ] **Step 3: Implement immutable operation binding**

`OperationBinding` contains action, resource selector, argument constraints, optional Run binding and optional step binding. Its canonical hash is stored in the grant. `Once` enforces maximum executions `1`; `ForCurrentRun` requires a Run ID and a configured maximum from `1..=10_000`.

- [ ] **Step 4: Implement grant issuance service**

`ApprovalService::issue` loads an existing approved and unexpired v0.1 `ApprovalRecord`, verifies its canonical payload hash equals the requested `OperationBinding`, then creates the H2 grant. Rejected, expired or already superseded approval records cannot issue grants.

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

`consume` uses optimistic usage count and returns `approval_exhausted`, `approval_expired`, `approval_revoked` or `approval_mismatch` as stable application codes.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test approval_grants
cargo test -p vestrace-domain security::approval_grant
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(approval): add operation-bound approval grants"
```

---

### Task 4: Add hierarchical budget domain and limit decisions

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
    assert!(ResourceLimit::new(Some(100), Some(90), Some(200)).is_err());
}

#[test]
fn amount_addition_rejects_overflow() {
    let max = ResourceAmount::new(u64::MAX);
    assert!(max.checked_add(ResourceAmount::new(1)).is_err());
}
```

- [ ] **Step 2: Implement dimensions and base units**

`CurrencyCode` accepts exactly three uppercase ASCII letters. `BudgetDimension` has a stable lowercase storage key; money keys include the currency, for example `money_microunits:USD`.

- [ ] **Step 3: Implement account hierarchy**

```rust
pub enum BudgetOwner {
    Deployment,
    Workspace(WorkspaceId),
    Principal(PrincipalId),
    Trigger(RunReferenceId),
    AgentSnapshot(AgentRuntimeSnapshotId),
    Run(AgentRunId),
    Step(RunStepId),
    SubRun(AgentRunId),
}
```

`BudgetAccount` has one optional parent, immutable owner, status `Active | Frozen | Overdrawn | Closed`, revision and per-dimension limits. A child account cannot define a hard limit greater than the effective remaining allocation granted by its parent.

- [ ] **Step 4: Implement deterministic limit evaluation**

```rust
pub enum BudgetLimitDecision {
    Allowed,
    AllowedWithSoftWarning { dimensions: Vec<BudgetDimension> },
    ApprovalRequired { dimensions: Vec<BudgetDimension> },
    HardLimitExceeded { dimensions: Vec<BudgetDimension> },
}
```

Evaluate proposed `consumed + reserved + request` for each dimension. Sort returned dimensions by stable key.

- [ ] **Step 5: Implement reservation and allocation lifecycles**

Reservation states: `Prepared`, `Active`, `Consumed`, `Reconciled`, `Released`, `Expired`, `Cancelled`, `Overrun`. Allocation states: `Active`, `Returned`, `Exhausted`, `Cancelled`. Terminal states do not reopen. Duplicate dimensions in a request are merged with checked addition.

- [ ] **Step 6: Add property tests and commit**

Prove that increasing requested amount cannot improve a limit decision, releasing a reservation never increases reserved balance, and an allocation cannot exceed its parent reservation.

```bash
cargo test -p vestrace-domain budget
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(budget): add hierarchical budget domain"
```

---

### Task 5: Define policy, ticket, budget and guard application ports

**Files:**
- Create: `crates/vestrace-application/src/policy/ports.rs`
- Create: `crates/vestrace-application/src/policy/authorization.rs`
- Create: `crates/vestrace-application/src/policy/ticket.rs`
- Create: `crates/vestrace-application/src/budget/mod.rs`
- Create: `crates/vestrace-application/src/budget/commands.rs`
- Create: `crates/vestrace-application/src/budget/ports.rs`
- Create: `crates/vestrace-application/src/guard/mod.rs`
- Create: `crates/vestrace-application/src/guard/service.rs`
- Create: `crates/vestrace-application/tests/policy_authorization.rs`
- Create: `crates/vestrace-application/tests/action_guard.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Produces `PolicySnapshotPort`, `PolicyDecisionJournalPort`, `AuthorizationTicketPort`, `BudgetAccountPort`, `BudgetReservationPort`, `BudgetSnapshotPort`, `ActionGuardCommitPort`, `PolicyAuthorizationService` and `ActionGuardService`.

- [ ] **Step 1: Define exact policy source and journal ports**

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

A decision record stores request hash, decision, matched rules, obligations, exact bundle revisions, explanation code, timestamp and expiry without storing secret argument values.

- [ ] **Step 2: Define authorization ticket lifecycle**

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

Tickets default to one use. H2 permits more than one use only when all attached approval grants and the policy decision explicitly permit it.

- [ ] **Step 3: Define budget reservation ports**

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

All commands include an idempotency key. Reservation requests include leaf account, Run/step, line items, expiry and optional approval grant IDs.

- [ ] **Step 4: Implement `PolicyAuthorizationService` with fakes**

Service flow:

1. verifies request principal/workspace against `RequestContext`;
2. obtains base capability decision;
3. loads current policy snapshot;
4. loads supplied approval grants without consuming them;
5. runs pure evaluator;
6. appends decision record;
7. returns decision.

Tests prove a base deny cannot be overridden, an expired grant does not satisfy RequireApproval and every decision is journaled once.

- [ ] **Step 5: Define atomic guard commit port**

```rust
#[async_trait::async_trait]
pub trait ActionGuardCommitPort: Send + Sync {
    async fn issue(
        &self,
        context: &RequestContext,
        command: IssueGuardedAction,
    ) -> Result<GuardedAction, ApplicationError>;

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

`issue` atomically stores the policy decision, budget reservation and ticket. `consume` atomically increments ticket/grant usage and changes reservation from Prepared to Active. `complete` records obligation receipts and reconciles usage. `abort` releases an unconsumed reservation.

- [ ] **Step 6: Implement `ActionGuardService` orchestration with in-memory fakes**

Deny, PrepareOnly and unsatisfied RequireApproval return stable errors without a ticket. A permitted request with zero resource estimate may issue a ticket without a reservation. Tests prove duplicate `issue` idempotency returns the same ticket and duplicate `consume` cannot double-consume a Once grant.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test policy_authorization --test action_guard
cargo clippy -p vestrace-application --all-targets -- -D warnings
git add crates/vestrace-application
git commit -m "feat(guard): define policy budget and action guard ports"
```

---

### Task 6: Persist policy bundles, snapshots, decisions, tickets and grants

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
- Produces tables `harness_policy_bundles`, `harness_policy_bundle_revisions`, `harness_policy_activations`, `policy_snapshots`, `policy_snapshot_revisions`, `policy_decisions`, `authorization_tickets`, `authorization_ticket_events`, `approval_grants` and `approval_grant_events`.
- Produces PostgreSQL implementations of policy, ticket and approval ports.

- [ ] **Step 1: Write failing immutable-revision and snapshot tests**

Test duplicate revision number, content hash mismatch, update/delete rejection for revisions and snapshots, invalid layer order, cross-workspace owner binding and activation of a nonexistent revision.

- [ ] **Step 2: Create `0023_policy_bundles_and_snapshots.sql`**

`harness_policy_bundles` stores stable identity, workspace, layer, owner kind/ID and lifecycle status. Revisions store immutable `rules JSONB`, `unmatched`, revision number, SHA-256 content hash and timestamps. Activations store effective interval and exact revision. Snapshots store ordered revision IDs and aggregate hash. Deferred triggers validate owner workspace and revision order.

- [ ] **Step 3: Write failing ticket and approval-use tests**

Test that:

- ticket fingerprint cannot differ from its decision;
- expired/revoked ticket cannot be consumed;
- Once grant cannot be consumed twice under concurrency;
- ticket and approval use counts roll back together when either predicate fails;
- approval grant cannot reference an unapproved v0.1 `ApprovalRecord`.

- [ ] **Step 4: Create `0024_policy_decisions_tickets_and_approval_grants.sql`**

`policy_decisions` and both event tables are append-only. Ticket and grant rows contain mutable status/use counters guarded by expected counts; every mutation appends an event in the same transaction. Argument values are not stored in decision rows—only canonical hashes, classifications and typed references.

- [ ] **Step 5: Implement repository mapping and atomic consumption**

Use one transaction to lock the ticket and all grant rows ordered by UUID, validate fingerprint/binding/expiry/status, increment counts and append events. Zero affected rows maps to the exact stable application error after reloading current state.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test policy_persistence --test authorization_tickets --test approval_consumption
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add migrations/0023_policy_bundles_and_snapshots.sql migrations/0024_policy_decisions_tickets_and_approval_grants.sql crates/vestrace-infrastructure tests
git commit -m "feat(policy): persist decisions tickets and approval grants"
```

---

### Task 7: Persist hierarchical budgets and atomic reservations

**Files:**
- Create: `migrations/0025_resource_budget_accounts_and_limits.sql`
- Create: `migrations/0026_budget_reservations_events_and_quotas.sql`
- Create: `crates/vestrace-application/src/budget/service.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/account_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/reservation_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/snapshot_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-application/tests/budget_service.rs`
- Create: `tests/budget_reservations.rs`

**Interfaces:**
- Produces tables `budget_accounts`, `budget_account_limits`, `budget_balances`, `budget_reservations`, `budget_reservation_lines`, `budget_reservation_impacts`, `budget_events` and `budget_snapshots`.
- Produces `BudgetService` and PostgreSQL implementations of budget account, reservation and snapshot ports.

- [ ] **Step 1: Write failing hierarchy and concurrency tests**

Build Deployment → Workspace → Run accounts with a hard money limit of 10_000_000 microunits. Concurrently reserve 6_000_000 twice. Assert exactly one succeeds, the other returns `budget_hard_limit_exceeded`, and no balance exceeds 10_000_000.

Also test parent-cycle rejection, cross-workspace parent rejection, duplicate dimension merge, idempotent duplicate reservation and rollback when one ancestor fails.

- [ ] **Step 2: Create `0025_resource_budget_accounts_and_limits.sql`**

Accounts contain owner kind/ID, parent, status, revision and timestamps. Limits use stable dimension key plus nullable soft/approval/hard `BIGINT` values with nonnegative and ordering checks. Balances contain consumed/reserved values and optimistic revision. A deferred recursive check rejects cycles and invalid owner hierarchy.

- [ ] **Step 3: Create reservation and event tables in `0026`**

Reservations contain leaf account, Run/step, status, idempotency key, expiry and usage timestamps. Each line stores dimension/amount. Impact rows record every ancestor account affected by each line. `budget_events` and `budget_snapshots` are append-only. Snapshot payload contains sorted balances and exact account revisions.

- [ ] **Step 4: Implement deterministic ancestor locking**

Load the full ancestor chain, sort account UUIDs and acquire `SELECT ... FOR UPDATE` locks in that order. Re-read limits and balances under lock. For each line calculate proposed reserved plus consumed using checked integer arithmetic. Insert reservation, impact rows, update every balance and append one event in a single transaction.

- [ ] **Step 5: Implement reconciliation semantics**

If actual <= reserved: move reserved to consumed and release the difference. If actual > reserved: record actual, mark reservation `Overrun`, mark affected accounts `Overdrawn`, append `budget.estimate_exceeded`, and deny future reservations. Never truncate actual usage to the reservation estimate.

- [ ] **Step 6: Implement soft/approval behavior**

Soft crossing succeeds with sorted warnings. Approval crossing requires a supplied valid approval grant bound to action `budget.reserve`, the leaf account and canonical line items. Hard crossing always fails. `BudgetService` returns a `BudgetSnapshotId` after every successful reserve/reconcile/release.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test budget_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test budget_reservations
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add migrations/0025_resource_budget_accounts_and_limits.sql migrations/0026_budget_reservations_events_and_quotas.sql crates tests/budget_reservations.rs
git commit -m "feat(budget): persist atomic hierarchical reservations"
```

---

### Task 8: Add Run/SubRun budget allocations and workspace quotas

**Files:**
- Create: `crates/vestrace-application/src/budget/quota.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/budget/quota_repository.rs`
- Create: `tests/budget_allocations.rs`
- Create: `tests/workspace_quotas.rs`
- Modify: `migrations/0026_budget_reservations_events_and_quotas.sql` before its task commit only

**Interfaces:**
- Produces `AllocateBudget`, `ReturnBudgetAllocation`, `BudgetAllocationPort`, `WorkspaceQuota`, `QuotaLease`, `QuotaPort` and PostgreSQL implementations.
- Adds tables `budget_allocations`, `workspace_quotas`, `quota_counters`, `quota_leases` and `quota_events` to migration `0026` before that migration is first committed.

- [ ] **Step 1: Write failing parent-allocation tests**

Test that a Run with 10_000 tokens can allocate 4_000 and 5_000 to two SubRuns, cannot allocate a further 2_000, and receives 1_000 back when the first SubRun closes after consuming 3_000.

- [ ] **Step 2: Implement allocation transaction**

Allocation creates a parent reservation, a child budget account and immutable allocation row in one transaction. The child hard limits equal the allocated lines. Returning closes the child account, reconciles its consumed usage into the parent allocation and releases only the unused remainder.

- [ ] **Step 3: Define quota kinds and leases**

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

A quota lease has workspace, kind, amount, holder reference, expiry and generation. Polling or heartbeat does not change RunVersion.

- [ ] **Step 4: Implement atomic quota acquisition**

Lock the quota counter row, expire stale leases, check `current + request <= limit`, create a lease and increment the counter in one transaction. Release uses holder ID and generation fencing. Tests use two concurrent Run acquisitions against limit one and assert exactly one succeeds.

- [ ] **Step 5: Add trigger/agent budget owner placeholders without behavior**

Persist owner kinds for Trigger and AgentSnapshot accounts so later plans can attach ceilings. H2 does not create triggers or profiles and does not invent their execution behavior.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test budget_allocations --test workspace_quotas
git add crates/vestrace-application crates/vestrace-infrastructure migrations/0026_budget_reservations_events_and_quotas.sql tests
git commit -m "feat(budget): add allocations and workspace quotas"
```

---

### Task 9: Integrate policy and budget references with durable Runs

**Files:**
- Create: `migrations/0027_policy_budget_rls_and_run_bindings.sql`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/checkpoint.rs`
- Create: `crates/vestrace-application/src/budget/commands.rs`
- Modify: `crates/vestrace-application/src/run/coordinator.rs`
- Create: `tests/policy_budget_rls.rs`
- Modify: `tests/run_replay.rs`

**Interfaces:**
- Adds `RunCheckpointPayload::V2` while retaining `V1` decoding.
- Adds canonical Run events for policy/budget attachment and limit-driven waiting.
- Adds Run-to-budget-account binding and forced RLS for every H2 table.

- [ ] **Step 1: Write failing checkpoint backward-compatibility test**

Deserialize a stored H1 `run-checkpoint/v1`, replay it unchanged, then create V2 containing:

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

- [ ] **Step 2: Extend Run event payload without changing H1 semantics**

Add:

```rust
PolicySnapshotEvaluated { snapshot_id: PolicySnapshotId, decision_id: PolicyDecisionId },
BudgetSnapshotAttached { snapshot_id: BudgetSnapshotId },
AuthorizationRequired { decision_id: PolicyDecisionId },
BudgetLimitReached { snapshot_id: BudgetSnapshotId, hard: bool },
```

These events are emitted only when the Run's logical state changes. Routine authorization checks, reservations, heartbeats and accounting events remain in their own journals and do not increment RunVersion.

- [ ] **Step 3: Add Run budget binding**

Migration creates `run_budget_bindings(workspace_id, run_id, budget_account_id, created_at, closed_at)` with one active binding per Run. A deferred trigger verifies owner `BudgetOwner::Run(run_id)` and same workspace.

- [ ] **Step 4: Add forced RLS and append-only protections**

Apply forced RLS to every H2 workspace table. Add indexes for active policy lookup, ticket/grant expiry, reservation expiry, account owner, budget event timeline and quota lease expiry. Application roles cannot update/delete append-only journals or immutable revisions.

- [ ] **Step 5: Integrate Run state changes**

When `ActionGuardService` returns unsatisfied RequireApproval, caller can atomically transition a nonterminal Run to `WaitingForApproval` with `AuthorizationRequired`. Hard budget exhaustion can transition to `Partial`, `WaitingForInput` or `Paused` according to an explicit caller command; H2 does not choose silently. Resume always reevaluates current policy rather than trusting `last_policy_snapshot_id`.

- [ ] **Step 6: Verify RLS and replay**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test policy_budget_rls --test run_replay
git add migrations/0027_policy_budget_rls_and_run_bindings.sql crates tests
git commit -m "feat(run): bind policy and budgets to durable runs"
```

---

### Task 10: Implement the PostgreSQL atomic `ActionGuardCommitPort`

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/policy/action_guard.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/policy/mod.rs`
- Create: `tests/h2_acceptance.rs`

**Interfaces:**
- Produces `PostgresActionGuardStore` implementing the H2 guard commit boundary.
- Atomically coordinates policy decisions, authorization tickets, approval usage and budget reservations without executing the protected action itself.

- [ ] **Step 1: Write failing rollback test**

Prepare a permit decision plus a reservation that conflicts with a hard ancestor limit. `issue` must fail and leave:

- no authorization ticket;
- no new policy decision row;
- no approval usage;
- no reservation or balance change.

- [ ] **Step 2: Write failing concurrent consumption test**

Issue one ticket bound to a Once grant. Consume concurrently from two workers. Exactly one succeeds; the other receives `authorization_ticket_exhausted`. Ticket use, grant use and reservation activation each equal one.

- [ ] **Step 3: Implement atomic issue transaction**

Inside one scoped transaction:

1. verify idempotency key;
2. insert the append-only policy decision;
3. lock budget ancestors and create Prepared reservation when requested;
4. insert ticket bound to decision, grant IDs and reservation;
5. append ticket and budget events;
6. persist idempotent result;
7. commit.

A duplicate idempotency key returns the original `GuardedAction`.

- [ ] **Step 4: Implement atomic consume transaction**

Lock ticket, approval grants and reservation rows in deterministic table/UUID order. Validate fingerprint, expiry, status and remaining uses. Increment ticket and grant counts, move reservation Prepared → Active, append all events and commit. No external action is invoked inside this transaction.

- [ ] **Step 5: Implement complete and abort**

`complete` verifies the consumed ticket, stores typed obligation receipts, reconciles budget usage and marks ticket Completed or Violated. `abort` only accepts Issued tickets, releases Prepared reservation and marks ticket Aborted. A consumed ticket cannot be aborted.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h2_acceptance
cargo clippy -p vestrace-infrastructure --all-targets -- -D warnings
git add crates/vestrace-infrastructure tests/h2_acceptance.rs
git commit -m "feat(guard): atomically issue and consume action permits"
```

---

### Task 11: Add H0-RIG spike fixtures and complete H2 acceptance

**Files:**
- Create: `crates/vestrace-application/src/guard/testing.rs`
- Create: `crates/vestrace-application/src/budget/testing.rs`
- Modify: `crates/vestrace-application/src/guard/mod.rs`
- Modify: `crates/vestrace-application/src/budget/mod.rs`
- Modify: `tests/h2_acceptance.rs`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md` only to mark the H2 contract names as normative if they differ from shorthand wording

**Interfaces:**
- Produces deterministic `TestPolicyEngine`, `TestApprovalGrantStore`, `TestBudgetReservationStore`, `TestActionGuardStore` and `DeterministicProtectedAction` fixtures for H0-RIG and downstream plan tests.
- Fixtures are enabled only for tests or an explicit `test-support` feature and contain no production bypass.

- [ ] **Step 1: Implement deterministic policy fixtures**

Provide constructors for Permit, Deny, PrepareOnly and RequireApproval snapshots with fixed IDs and timestamps. Fixture decisions still use real canonical fingerprints and evaluator code.

- [ ] **Step 2: Implement deterministic budget fixtures**

Provide in-memory ancestor locking semantics under a Tokio mutex, fixed soft/approval/hard limits and event capture. The fixture must reproduce concurrency behavior, not simply always permit.

- [ ] **Step 3: Implement protected-action proof fixture**

`DeterministicProtectedAction` increments an atomic counter only after `ActionGuardCommitPort::consume` succeeds. Tests prove:

1. no ticket → counter stays zero;
2. wrong fingerprint → counter stays zero;
3. missing required approval → no ticket is issued;
4. hard budget failure → no ticket is issued;
5. valid ticket/grant/reservation → counter becomes one;
6. repeated consume → counter remains one.

- [ ] **Step 4: Complete H2 end-to-end acceptance scenario**

Create a Run budget account under a workspace account. Authorize a protected action requiring one capability, a policy obligation, a Once approval and money/token reservations. Consume, execute deterministic action, complete with lower actual usage and verify:

- exact policy snapshot and matched rules are journaled;
- approval use is one;
- reservation is reconciled and unused amount released;
- obligation receipt is recorded;
- Run can checkpoint V2 and reload;
- a second execution is denied;
- another workspace cannot read any H2 row.

- [ ] **Step 5: Run full verification**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test policy_persistence \
             --test authorization_tickets \
             --test approval_consumption \
             --test budget_reservations \
             --test budget_allocations \
             --test workspace_quotas \
             --test policy_budget_rls \
             --test h2_acceptance
```

Expected: every command exits `0` without AI-provider, Rig or network access.

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

Each migration is created once and never edited after its task commit. Task 8 additions to `0026` must be completed before the Task 7/8 migration commit boundary is finalized; implementers may combine Tasks 7 and 8 into one review branch commit only if the migration has not previously been applied or merged.

## H2 exit gate

H2 is complete only when a deterministic protected action demonstrates all of the following:

1. a missing base capability cannot be overridden by a policy or approval;
2. an explicit policy deny cannot be overridden by a lower layer;
3. a RequireApproval decision accepts only a matching, valid operation-bound grant;
4. an enforcement point cannot execute without consuming the exact authorization ticket;
5. concurrent consumers cannot reuse a Once ticket or grant;
6. concurrent reservations cannot exceed any ancestor hard limit;
7. a SubRun allocation cannot access the parent's unallocated budget;
8. actual usage is reconciled and unused reservation is released;
9. an overrun is recorded and freezes new spending instead of hiding actual usage;
10. forced RLS prevents cross-workspace policy, approval and budget access;
11. Run checkpoint V1 remains readable and V2 stores policy/budget references;
12. all decisions and accounting changes have append-only, source-backed journals;
13. the H0-RIG spike can use stable policy, budget, checkpoint and deterministic protected-action fixtures without importing infrastructure types.

## Documentation-only status

This plan is a documentation artifact. Creating it does not start implementation, create a feature branch, modify Rust code or apply migrations. Implementation begins only after an explicit future instruction from the user.
