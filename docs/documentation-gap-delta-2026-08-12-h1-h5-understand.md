# Documentation Gap Delta — H1–H5 Understand Health and Repair

**Date:** 2026-08-12

**Scope:** v0.5 Understand, H1–H5
**Authority:** normative health/repair invariants, domain model, qualification specification, and the 36-PR execution matrix.

## Implemented bounded contract

The checkout now contains an additive health/repair contract in `vestrace-domain`:

- H1 adds `InvariantDefinition`, unique `InvariantRegistry`, `HealthFinding`, and separate `HealthOccurrence` values with stable invariant/version, scope, evidence, severity, repair risk, repairability, fingerprint, lifecycle and disposition metadata.
- H1 also adds dependency-aware `HealthProjection`; it is explicitly a projection, only follows declared dependencies, and returns `Unknown` when no authoritative observation exists.
- H2 adds immutable `RepairPlan`, explicit `RepairAuthorization`, stale/expired/unauthorized execution rejection, `RepairExecution`, and `VerificationRun`. Execution success does not close a finding; only verification can resolve it, while failed or inconclusive verification reopens it.
- H3 adds recurrence assessment, alternating-state flapping detection, attempt budget/cooldown, and governed `Suppressed`/`AcceptedRisk` dispositions. Disposition metadata includes actor, reason, expiry, policy version and audit reference; disposition does not rewrite observed integrity state.
- H4 adds application `HealthOperatorService` inspect/plan/repair phases and CLI `plan`/`repair` contract output. The service derives plans from findings and accepts only an exact plan plus authorization/current-state preconditions. The CLI contract does not start execution or access the database.
- H5 adds `UnderstandGate` for HLT-001..HLT-019 evidence. Understand remains a milestone label and is not added to `QualificationProfile`, consistent with ADR-0010.

## Evidence

- focused integration coverage: `tests/h1_h5_understand.rs`;
- application stale-plan coverage in `crates/vestrace-application/src/operator.rs`;
- CLI contract coverage in `crates/vestrace-cli/tests/cli.rs` and `crates/vestrace-cli/tests/command_contract.rs`;
- machine-readable fixture: `tests/fixtures/qualification/h1-h5-understand.json`;
- execution plan: `docs/superpowers/plans/2026-08-12-h1-h5-understand.md`.

## Explicit non-claims

This delta does not add migrations, durable invariant/finding/occurrence/plan/execution/verification stores, a database-backed plan loader, or an execution adapter. It does not retrofit the legacy SQL `rebuild` command into the new protocol. The CLI output is a contract surface only, and the fixture is evidence-only; neither qualifies v0.5 or a release profile.

## Verification recorded for this slice

- H1–H5 integration: 5 passed;
- application operator unit: 1 passed;
- CLI help/contract tests: 5 + 6 passed;
- workspace compilation and full tests remain the final gate for this slice.
