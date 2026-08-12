# Q13 — Runtime Reconciliation

## Goal

Make ambiguous external-effect recovery explicit, workspace-scoped, evidence-bearing, and non-retryable by default.

## Tasks

- [x] Add RED tests for UNKNOWN-effect reconciliation, workspace mismatch, and missing evidence.
- [x] Add `ExternalEffectReconciliationService` over the existing domain reconciler.
- [x] Preserve strongest-evidence selection and explicit reconciliation outcomes.
- [x] Keep automatic dispatch/retry outside the recovery path.
- [x] Document the boundary and explicit non-claims.
- [ ] Add durable effect/reconciliation persistence and provider read-back adapters in a later gate.

## Verification

- `cargo test --test recovery_reconciliation -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
