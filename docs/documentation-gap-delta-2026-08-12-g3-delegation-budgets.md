# G3 Delegation Attenuation and Hierarchical Budgets Delta

Date: 2026-08-12

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: G3 follows the G2 universal authorization boundary and requires delegation/budget attenuation evidence at the Govern gate.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: CAP-005 requires child authority not to exceed parent authority; CAP-006 requires an explicit delegation permission/contract; CAP-007 requires bounded depth; CAP-013 calls for auditable budget reservations/accounting.
- `docs/specs/vestrace-trust-authority-model-v0.2.md`: delegation is an intersection of parent authority, explicit delegated subset, child constraints, and current policy; safe default depth is 1; a child receives only an allocated/reserved budget subset; hard exhaustion cannot be overridden by approval.
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`: the security-negative suite includes child authority escalation denial and the golden Delegation attenuation scenario.
- `docs/specs/vestrace-domain-model-v0.2.md`: `DelegatedCapability` is a derived/value-object authority whose child result is the intersection of parent effective authority, explicit delegation, and child policy constraints.

## Scope implemented

1. Added the stable `capability.delegate` capability identifier and `DELEGATION_OPERATION` constant.
2. Added `DelegationContract` with explicit capability/resource subset, maximum depth, maximum lifetime, risk ceiling, and child budget ceiling. Wildcard scopes and unbounded depth are rejected.
3. Added `DelegatedCapability::from_root` and `DelegatedCapability::delegate`. Issuance requires active parent and permission grants, a matching `capability.delegate` operation, distinct delegator/child subjects, matching workspace/issuer relationships, a non-broader capability/operation/resource selector, child validity inside the parent and contract interval, attenuated risk, and an explicit child budget.
4. Added `HierarchicalBudget` with root registration, parent-scoped child reservations, remaining allocation calculation, and charge rejection on overspend. Child reservations consume parent headroom before effect-time charging, so sibling allocations cannot bypass the parent/global hard limit.
5. Kept child authorization on the existing `PolicyDecision` path and added `BudgetPolicyEngine` as an explicit application adapter that charges the shared hierarchical ledger before returning an allow. No second allow path or production allow-all policy was introduced.
6. Added `tests/g3_delegation_budgets.rs` and `tests/fixtures/qualification/g3-governance.json` covering valid attenuation, capability/resource/risk/validity/budget broadening denial, condition preservation, delegation-permission expiry, missing explicit permission, self-delegation denial, bounded depth, hierarchical charge/reservation denial, atomic invalid-child handling, and shared policy evaluation.

## Exit evidence

- RED was observed before the G3 types and `capability.delegate` identifier existed: the focused test failed on unresolved public APIs and enum variant.
- `cargo test --test g3_delegation_budgets -- --nocapture`: 10 passed.
- `cargo test -p vestrace-domain --lib -- --nocapture`: 147 passed.
- `cargo test -p vestrace-application --lib -- --nocapture`: 53 passed.
- `cargo check --workspace`: passed.
- Scoped Rust formatting check and `git diff --check`: passed; existing CRLF conversion warnings and unrelated repository warnings remain non-failing.

## Explicit non-claims

This is an additive in-memory domain/application-boundary slice, not a complete Govern qualification. It does not claim:

- PostgreSQL persistence or immutable grant/delegation revisions;
- durable audit/evidence records for issue, delegate, reserve, charge, release, or correction;
- reservation release/correction, multi-dimensional/monetary budget accounting, or concurrency-safe storage transactions;
- universal enforcement for the generic job poller, federation adapters, or future external-effect paths;
- G4 cross-workspace source-grant plus target-mount acceptance/revocation;
- `QualificationBundle` or TRUSTED/GOVERNANCE profile qualification.

The next documented gate is G4: cross-workspace sharing v2 with source grant revision and target `MemoryMount` acceptance.
