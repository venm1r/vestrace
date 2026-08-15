# G3 Delegation Attenuation and Hierarchical Budgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a fail-closed domain kernel for explicit capability delegation, bounded attenuation, and hierarchical budget reservation/charging.

**Architecture:** Keep existing direct `CapabilityGrant` construction backward-compatible. Add a `DelegationContract`, `DelegatedCapability`, and `HierarchicalBudget` value-object layer in the domain security module. Delegation creates a child grant only when a parent capability, an explicit `capability.delegate` grant, validity/risk/scope/depth limits, and a reserved budget subset all pass; existing application policy engines can evaluate the resulting child grant without a second authorization path.

**Tech Stack:** Rust 2024, serde/schemars domain types, chrono timestamps, existing `DomainError`, cargo test, JSON qualification fixtures.

## Global Constraints

- Default authorization remains deny and approval cannot override a hard capability ceiling or exhausted budget.
- Child authority is the intersection of parent authority, explicit delegation contract, child constraints, and current policy.
- Delegation depth is bounded; the implementation hard ceiling is `MAX_DELEGATION_DEPTH = 8` and every contract declares a positive lower/equal depth limit.
- A delegated child must use an explicit budget subset; reservations are checked before child issuance and charges cannot exceed the reservation.
- Scope selectors are fail-closed: only exact scope or a slash-delimited descendant of the parent scope is narrower; opaque/wildcard broadening is rejected.
- This slice is additive and in-memory. Durable grant revisions, audit persistence, cross-workspace grant+mount sharing, and G4/G5 federation behavior are not claimed.
- Preserve all unrelated dirty work. Do not commit, reset, rebase, or broad-format the checkout.

---

### Task 1: G3 RED qualification contract

**Files:**
- Create: `tests/g3_delegation_budgets.rs`
- Create: `tests/fixtures/qualification/g3-governance.json`

**Interfaces:**
- Consumes: planned public domain exports `CapabilityDelegate`, `DelegationContract`, `DelegatedCapability`, `HierarchicalBudget`.
- Produces: executable cases for CAP-005, CAP-006, CAP-007, CAP-013 and the G3 qualification manifest.

- [x] **Step 1: Write failing tests**

  Cover: attenuated child issuance; broader capability, resource, risk, validity, budget, missing delegation permission, self-delegation, and depth overflow denial; hierarchical reservation/charge exhaustion; child grant evaluation through `GrantPolicyEngine`.

- [x] **Step 2: Run the focused test before implementation**

  Run: `cargo test --test g3_delegation_budgets -- --nocapture`

  Expected: FAIL because the G3 public types and capability identifier do not exist yet.

### Task 2: Domain delegation and budget kernel

**Files:**
- Modify: `crates/vestrace-domain/src/security/mod.rs`
- Create: `crates/vestrace-domain/src/security/delegation.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Consumes: existing `CapabilityGrant`, `CapabilityGrantSpec`, `BudgetConstraint`, `RiskCategory`, `Timestamp`, and `DomainError`.
- Produces: `MAX_DELEGATION_DEPTH`, `DELEGATION_OPERATION`, `DelegationContract::new`, `DelegatedCapability::from_root`, `DelegatedCapability::delegate`, `HierarchicalBudget::new`, `register_root`, `reserve_child`, `charge`, and `remaining`.

- [x] **Step 1: Add the explicit delegation capability identifier and module exports**
- [x] **Step 2: Implement fail-closed selector, validity, risk, depth, subject, and budget checks**
- [x] **Step 3: Implement root/child reservations and ancestor-safe charging**
- [x] **Step 4: Run the focused G3 test suite**

  Run: `cargo test --test g3_delegation_budgets -- --nocapture`

  Expected: PASS.

### Task 3: Existing policy boundary integration evidence

**Files:**
- Modify: `crates/vestrace-application/src/lib.rs` only if a new public security export is required.
- Modify: `tests/g3_delegation_budgets.rs`.

**Interfaces:**
- Consumes: `DelegatedCapability.grant` and existing `GrantPolicyEngine`.
- Produces: proof that a valid delegated child is evaluated through the same existing policy decision path and that no allow-all production engine is introduced.

- [x] **Step 1: Add the application-level child evaluation case**
- [x] **Step 2: Run application and G3 focused tests**

  Run: `cargo test -p vestrace-application --lib -- --nocapture` and `cargo test --test g3_delegation_budgets -- --nocapture`.

### Task 4: Verification, independent review, and documentation delta

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-g3-delegation-budgets.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/requirement-coverage-v0.2.md`
- Modify: `tests/fixtures/qualification/g3-governance.json` if review requires manifest corrections.

**Interfaces:**
- Consumes: passing G3 tests, exact diff checks, and independent review.
- Produces: evidence-backed G3 status with explicit in-memory/audit/persistence limitations.

- [x] **Step 1: Run rustfmt, focused tests, workspace check/test, and `git diff --check`**
- [x] **Step 2: Request fresh independent review of only the G3 diff**
- [x] **Step 3: Apply only actionable review findings and rerun affected verification**
- [x] **Step 4: Update documentation status/coverage without claiming G4+ behavior**
