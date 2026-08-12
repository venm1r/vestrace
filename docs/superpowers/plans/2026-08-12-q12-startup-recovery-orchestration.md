# Q12 — Startup Recovery Orchestration

## Goal

Apply the existing domain recovery matrix to startup candidates and restore only projections whose recovery class is safe.

## Tasks

- [x] Add RED tests for safe recovery, barriers, duplicate candidates, and operation failure.
- [x] Add `StartupRecoveryCandidate`, typed outcomes, and report records.
- [x] Add fail-closed `StartupRecoveryService` over `RunRecoveryOperations`.
- [x] Document the application boundary and explicit non-claims.
- [ ] Add durable candidate discovery and production server/worker wiring in a later bounded gate.

## Verification

- `cargo test --test startup_recovery -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
