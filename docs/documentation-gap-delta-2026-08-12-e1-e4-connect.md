# Documentation Gap Delta — E1–E4 Connect External Effects

**Date:** 2026-08-12

**Scope:** v0.6 Connect, E1–E4

**Authority:** External Effects Contract, normative `EXT-*`/`REC-*` invariants, qualification specification, and the 36-PR execution matrix.

## Implemented bounded contract

- E1 adds immutable `ExternalEffectIntent`, explicit preconditions, argument digest, capability/policy/budget references, delivery semantics, idempotency profile, reversibility and dry-run declarations.
- E2 adds `ExternalEffectAdapter`, shared application `ExternalEffectService`, authorization binding, final precondition recheck, immutable `ExternalEffectReceipt`, lifecycle states and first-class `UNKNOWN` with no automatic retry.
- E3 adds strongest-evidence `reconcile_effect` and `ExternalReconciliation`; compensation creates a new effect identity and preserves `compensates_effect_id` without mutating the original effect.
- E4 adds deterministic fault points/observations, unsafe-retry detection, adapter descriptor validation, a v0.6 `ConnectGate` for `EXT-001..018` plus `QUAL-008`, and a traceable evidence fixture.

## Evidence

- focused integration suite: `tests/e1_e4_connect.rs`;
- domain contract: `crates/vestrace-domain/src/external_effects.rs`;
- shared authorization/dispatch application boundary: `crates/vestrace-application/src/external_effects.rs`;
- machine-readable fixture: `tests/fixtures/qualification/e1-e4-connect.json`;
- execution plan: `docs/superpowers/plans/2026-08-12-e1-e4-connect.md`.

## Explicit non-claims

This delta is additive and in-memory. It does not add durable intent/receipt/reconciliation persistence, provider/network adapters, crash injection, secret/key governance, migration changes, or a deployed v0.6 qualification bundle. It does not retrofit the legacy `ToolInvocation` projection into the new external-effect owner, and it does not claim universal exactly-once delivery.

## Verification recorded for this slice

- E1–E4 integration and fixture: 6 passed;
- workspace compilation and full tests are the final gate for this slice;
- existing unrelated warnings remain outside this delta.
