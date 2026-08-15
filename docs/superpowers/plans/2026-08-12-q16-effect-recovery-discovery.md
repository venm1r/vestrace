# Q16 — Durable External-Effect Recovery Discovery

## Goal

Make UNKNOWN external effects discoverable after restart and provide a narrow
provider read-back boundary that persists reconciliation evidence without
dispatching or retrying the original effect.

## Tasks

- [x] Add RED tests for workspace-scoped discovery, already-reconciled exclusion, and empty read-back fail-closed behavior.
- [x] Extend `ExternalEffectRepository` with durable reconciliation-candidate discovery.
- [x] Add indexed reconciliation identity columns and migration `0128` with payload backfill.
- [x] Add `ExternalEffectReadBackAdapter` and `ExternalEffectRecoveryService`.
- [x] Persist each successful reconciliation through the existing immutable repository.
- [x] Verify application behavior and PostgreSQL behavior in Docker.
- [x] Document the boundary and explicit non-claims.
- [x] Production server/worker startup wiring (R1) and an HTTP read-back adapter (R2) delivered; see [`2026-08-12-open-follow-up-gates.md`](2026-08-12-open-follow-up-gates.md).

## Safety boundary

The service reads only `UNKNOWN` receipts belonging to the requested
workspace. It asks an injected provider read-back adapter for observations,
delegates outcome selection to the domain reconciler, and writes a new
reconciliation fact. It never calls an effect dispatch path and never turns an
ambiguous receipt into an automatic retry.

## Verification

- `cargo test --test effect_recovery -- --nocapture`
- `cargo test -p vestrace-infrastructure --test external_effect_repository -- --nocapture` (Docker PostgreSQL)
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
