# G1 Capability Policy Kernel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Implement the documented G1 kernel for constrained capability grants and evidence-bearing policy decisions while keeping universal entry-point enforcement in the subsequent G2 boundary.

**Architecture:** Add a focused domain security module containing validated `CapabilityGrant`, typed selectors/constraints, risk evaluation, and `PolicyDecision` evidence. Add an application `GrantPolicyEngine` adapter that binds the domain evaluator to the trusted `RequestContext`; retain legacy coarse policy helpers for existing callers, but provide explicit default-deny behavior and do not claim that HTTP/MCP/worker wiring is complete.

**Tech Stack:** Rust 2024, `serde`, `schemars`, `chrono`, `async-trait`, Cargo workspace tests, JSON evidence fixture, Markdown implementation deltas.

## Global Constraints

- Runtime authority is effective capability/policy, not role name (`CAP-001`).
- Default policy is deny (`CAP-002`).
- Grants must constrain operation/tool, resource/scope, validity, budget, risk, and conditions (`CAP-003`).
- Expired grants are rejected (`CAP-004`).
- Risk categories include `low`, `medium`, `high`, and `critical`; context may raise effective risk (`CAP-008..009`).
- Decisions retain exact policy version and relevant input state (`CAP-011`).
- G1 is a kernel/evidence slice; G2 universal HTTP/MCP/worker/internal authorization and G3 delegation attenuation/hierarchical budgets remain explicit non-claims.
- Preserve all unrelated dirty work and do not commit, reset, rebase, or broad-format the checkout.

---

### Task 1: Establish the failing G1 contract tests

**Files:**
- Create: `tests/g1_capability_policy.rs`
- Create: `tests/fixtures/qualification/g1-governance.json`

**Interfaces:**
- Consumes the desired domain API: `CapabilityGrantSpec`, `CapabilityGrant`, `AuthorizationRequest`, `GrantCondition`, `BudgetConstraint`, `RiskCategory`, `PolicyDecisionResult`, and `evaluate_capability_grants`.
- Produces executable RED coverage for grant validation, expiry/revocation, scope/risk/budget/condition matching, default deny, decision evidence, and a non-qualification fixture manifest.

- [x] **Step 1: Write tests for the contract**

  The test module will construct a fixed UTC timestamp and a valid grant with exact capability, operation, resource scope, validity interval, budget ceiling, risk ceiling, and condition. It will assert that:

  - malformed operation/resource/condition and inverted validity are rejected;
  - an active matching grant allows the request;
  - wrong operation, resource, subject/workspace, capability, unsatisfied condition, over-budget, elevated-risk, revoked, and expired requests deny;
  - the denial is fail-closed and the decision stores the exact policy version, input state, and decision time;
  - the fixture manifest covers G1’s in-scope CAP requirements while retaining blocked G2/G3/full-qualification limitations.

- [x] **Step 2: Run the focused tests and verify the expected RED failure**

  Run: `cargo test --test g1_capability_policy -- --nocapture`

  Expected: compilation fails because the new typed kernel API is not yet defined. Fix only test typos if necessary; do not add production code before this RED checkpoint.

---

### Task 2: Implement the domain capability and decision kernel

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/security/capability.rs`
- Modify: `crates/vestrace-domain/src/security/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Consumes the RED tests from Task 1 and the normative Capability/PolicyDecision model.
- Produces serializable, schema-visible `CapabilityGrantId`, `PolicyDecisionId`, `RiskCategory`, `BudgetConstraint`, `GrantCondition`, `CapabilityGrantSpec`, `CapabilityGrant`, `AuthorizationRequest`, `PolicyInputState`, `PolicyDecisionResult`, `PolicyDecisionReason`, `PolicyDecision`, and `evaluate_capability_grants`.

- [x] **Step 1: Add typed IDs and the module exports**

  Add `CapabilityGrantId` and `PolicyDecisionId` through the existing `domain_id!` macro. Export the capability module through `security` and the domain crate root without changing existing `Capability` or approval APIs.

- [x] **Step 2: Implement validated grant construction**

  `CapabilityGrant::issue(spec, created_at)` must reject blank operation/resource selectors and conditions, reject `valid_until <= valid_from`, and create an active grant. `CapabilityGrant::revoke(at)` must return a revoked copy. `is_active_at` must fail closed for revoked grants, before-start timestamps, and timestamps at or after `valid_until`.

- [x] **Step 3: Implement matching and risk/budget/condition constraints**

  Matching is exact for the G1 operation and resource selectors. A request is allowed only when workspace, subject, capability, operation, resource, validity, budget, effective risk, and every grant condition match. `AuthorizationRequest::effective_risk` must be the maximum of requested and context risk, making contextual elevation visible to policy.

- [x] **Step 4: Implement deterministic evidence-bearing decisions**

  `evaluate_capability_grants` must default to deny, return an allow only for a matching active grant, preserve the exact policy version and typed `PolicyInputState`, and report a stable reason for the first failed constraint. `PolicyDecision::is_allowed` must be true only for `Allow`; no approval type is an override in this kernel.

- [x] **Step 5: Run the focused tests and verify GREEN**

  Run: `cargo test --test g1_capability_policy -- --nocapture`

  Expected: all domain contract tests pass. Then run the domain library tests: `cargo test -p vestrace-domain --lib`.

---

### Task 3: Add the application policy-decision adapter

**Files:**
- Modify: `crates/vestrace-application/src/security/policy_engine.rs`
- Modify: `crates/vestrace-application/src/security/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Consumes `RequestContext` and the domain `evaluate_capability_grants` function.
- Produces `PolicyDecisionEngine`, `GrantPolicyEngine`, and `DenyAllPolicyEngine`. `GrantPolicyEngine::decide` binds workspace/principal identity from `RequestContext` and returns the domain `PolicyDecision`; coarse `PolicyEngine` remains available for existing services until G2 wiring.

- [x] **Step 1: Add RED adapter assertions**

  Extend the G1 test module with an async adapter test that creates a `GrantPolicyEngine` at policy version `g1-test-v1`, evaluates a matching request, and asserts that the decision is allowed and evidence identity comes from `RequestContext`. Add a default-deny assertion for `DenyAllPolicyEngine`.

- [x] **Step 2: Run the adapter RED test**

  Run: `cargo test --test g1_capability_policy application_grant_policy_engine -- --nocapture`

  Expected: compilation fails because the decision-engine trait and adapters are not yet defined.

- [x] **Step 3: Implement the minimal async adapter**

  Validate a nonblank policy version at construction. `GrantPolicyEngine` stores its grants and version, delegates evaluation to the domain kernel, and never trusts subject/workspace values supplied by the request. `DenyAllPolicyEngine` produces an explicit deny decision for every request. Keep the legacy `AllowAllPolicyEngine` available only to unit tests; it is not a production API or a G1 authorization boundary.

- [x] **Step 4: Run application and focused tests**

  Run: `cargo test --test g1_capability_policy -- --nocapture` and `cargo test -p vestrace-application --lib`.

  Expected: all G1 tests and existing application policy tests pass.

---

### Task 4: Record the bounded fixture and documentation delta

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-11-g1-capability-policy-kernel.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/requirement-coverage-v0.2.md`

**Interfaces:**
- Consumes the implemented tests and the G1 matrix/exit criteria.
- Produces traceable documentation that marks only the implemented CAP kernel as evidence-backed and keeps G2, G3, runtime persistence, and profile qualification open.

- [x] **Step 1: Document files, behavior, and evidence**

  Record exact source/test paths, the fail-closed semantics, policy-version/input-state evidence, and the fact that the new fixture is `evidence_fixture_only`.

- [x] **Step 2: Update the implementation and coverage snapshots**

  Change only the CAP rows and current-delta navigation needed to reflect G1. Do not change the pinned historical baseline or claim universal entry-point enforcement.

- [x] **Step 3: Validate documentation references**

  Run focused `rg` checks for `CapabilityGrant`, `PolicyDecision`, G1, and the explicit G2/G3 non-claims; ensure every cited file exists.

---

### Task 5: Execute the multi-block verification gate and independent review

**Files:**
- No additional production files; inspect the exact G1 diff and preserve unrelated changes.

**Interfaces:**
- Consumes Tasks 1–4 and the repository’s existing workspace gates.
- Produces fresh test/build/diff evidence and a reviewer disposition for this bounded G1 slice.

- [x] **Step 1: Run scoped formatting and diff checks**

  Run rustfmt only on changed Rust files, then `git -c safe.directory=* -C E:/Soft/vestrace diff --check`. Do not run repository-wide formatting.

- [x] **Step 2: Run the full non-database gates**

  Run `cargo check --workspace`, `cargo test --workspace --lib`, and `cargo test --workspace --no-run`, plus the focused G1 test. Report any PostgreSQL runtime gate separately if `DATABASE_URL` is unavailable.

- [x] **Step 3: Request an independent review**

  Dispatch a fresh reviewer with the G1 requirements, exact changed paths, and the pre-change `HEAD` as comparison context. Fix Critical/Important findings before handoff; record Minor findings without broadening scope.

- [x] **Step 4: Update the progress ledger and hand off**

  Mark only G1 kernel evidence complete, state that G2 universal authorization and G3 delegation/budget attenuation remain next gates, and include the exact verification commands and any environment blocker.
