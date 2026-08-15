# Q11 — Recovery and Trust Persistence

## Goal

Persist incident/revalidation/trust/recovery evidence without losing typed payloads or allowing indexed metadata drift.

## Tasks

- [x] Add RED repository contract tests.
- [x] Add domain accessors needed for persistence integrity checks.
- [x] Add `RecoveryRepository` application port.
- [x] Add migration `0126_incident_recovery_trust_records.sql`.
- [x] Add `PgRecoveryRepository` with idempotent immutable inserts and latest trust-state lookup.
- [x] Add host compile/library verification and Docker PostgreSQL migration smoke.
- [x] Startup recovery orchestration contract delivered by Q12 and production fault execution by Q21/Q23. Run-candidate discovery and startup wiring remain open as R1 in [`2026-08-12-open-follow-up-gates.md`](2026-08-12-open-follow-up-gates.md).

## Verification

- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- Docker PostgreSQL 17 migration smoke for migration 0126
- `cargo test -p vestrace-infrastructure --test recovery_repository -- --nocapture` remains blocked without host `DATABASE_URL`
