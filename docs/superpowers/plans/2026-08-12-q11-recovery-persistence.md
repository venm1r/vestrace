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
- [ ] Keep startup recovery orchestration and production fault execution as the next gate.

## Verification

- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- Docker PostgreSQL 17 migration smoke for migration 0126
- `cargo test -p vestrace-infrastructure --test recovery_repository -- --nocapture` remains blocked without host `DATABASE_URL`
