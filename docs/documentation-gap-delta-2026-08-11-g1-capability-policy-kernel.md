# G1 Capability / Policy Decision Kernel Delta

Date: 2026-08-11

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: G1 is the additive CAP gate after L3; G2 is the later universal boundary and G3 is delegation/budget attenuation.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: `CAP-001..004`, `CAP-008..009`, and `CAP-011` define the in-scope runtime-authority, deny, constraint, expiry, risk, and decision-evidence semantics.
- `docs/specs/vestrace-domain-model-v0.2.md`: `CapabilityGrant` contains selectors, constraints, validity, budget, risk and conditions; `PolicyDecision` retains policy version, input state and result.
- `docs/adr/0003-capabilities-are-runtime-authority.md`: default policy is deny; roles are issuance templates, not runtime authority.

## Scope implemented

1. Added typed `CapabilityGrantId` and `PolicyDecisionId` identifiers.
2. Added the domain `CapabilityGrant` kernel with exact operation/tool and resource selectors, validity interval, revocation, budget ceiling, risk ceiling, and typed conditions.
3. Added `RiskCategory` (`low`, `medium`, `high`, `critical`) and contextual risk elevation through `AuthorizationRequest::effective_risk`.
4. Added `PolicyDecision` and typed `PolicyInputState`; every evaluated result retains the exact policy version, workspace/principal identity, requested/effective risk, budget, selectors, conditions, result, reason, timestamp, and matched grant when applicable.
5. Added a fail-closed `evaluate_capability_grants` function and application `GrantPolicyEngine` adapter. `DenyAllPolicyEngine` makes default deny explicit for application callers.
6. Added `tests/g1_capability_policy.rs` and `tests/fixtures/qualification/g1-governance.json` with deterministic positive/negative cases for grant validation, scope, identity, expiry, revocation, budget, risk, conditions, evidence, and default deny.

## Exit evidence

- Focused G1 suite passes: 9 tests.
- RED was observed before the domain/application implementation: the new test compiled only after the typed APIs and adapters were added.
- Scoped Rust formatting passes for all changed Rust files.

## Explicit non-claims

This is a G1 kernel and evidence fixture, not a `QualificationBundle` and not a qualified `GOVERNANCE` profile. It does not claim:

- universal HTTP, MCP, worker, or internal entry-point enforcement; that is G2;
- delegated-child attenuation, delegation depth, or hierarchical effect-time budgets; those are G3;
- durable PostgreSQL grant/decision persistence or runtime audit/event integration;
- approval override semantics, material intent hash invalidation, or agent-facing self-escalation proof;
- replacement of the existing coarse policy helpers in all callers.
