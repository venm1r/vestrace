# Q13 — Runtime Reconciliation

## Goal

Make ambiguous external-effect recovery explicit, workspace-scoped, evidence-bearing, and non-retryable by default.

## Tasks

- [x] Add RED tests for UNKNOWN-effect reconciliation, workspace mismatch, and missing evidence.
- [x] Add `ExternalEffectReconciliationService` over the existing domain reconciler.
- [x] Preserve strongest-evidence selection and explicit reconciliation outcomes.
- [x] Keep automatic dispatch/retry outside the recovery path.
- [x] Document the boundary and explicit non-claims.
- [x] Durable effect/reconciliation persistence delivered by Q14 (`PgExternalEffectRepository`, migration `0127`).
- [x] Provider read-back delivered as `HttpExternalEffectReadBackAdapter`, fail-closed on transport failure (R2).

## Verification

- `cargo test --test recovery_reconciliation -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
