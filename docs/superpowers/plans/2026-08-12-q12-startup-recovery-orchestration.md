# Q12 — Startup Recovery Orchestration

## Goal

Apply the existing domain recovery matrix to startup candidates and restore only projections whose recovery class is safe.

## Tasks

- [x] Add RED tests for safe recovery, barriers, duplicate candidates, and operation failure.
- [x] Add `StartupRecoveryCandidate`, typed outcomes, and report records.
- [x] Add fail-closed `StartupRecoveryService` over `RunRecoveryOperations`.
- [x] Document the application boundary and explicit non-claims.
- [x] Durable run-candidate discovery added as `StartupRecoveryCandidateSource` with `PgStartupRecoverySource`, classifying from lease state only (R1).
- [x] Production server/worker wiring added behind an explicit `recovery.workspaces` scope; a failing sweep aborts startup (R1). See [`2026-08-12-open-follow-up-gates.md`](2026-08-12-open-follow-up-gates.md).

## Verification

- `cargo test --test startup_recovery -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
