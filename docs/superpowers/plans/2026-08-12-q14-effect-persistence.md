# Q14 — External Effect Persistence

## Goal

Persist immutable external-effect intent, receipt, and reconciliation evidence so UNKNOWN outcomes remain auditable across restart.

## Tasks

- [x] Add RED SQLx repository tests for round-trip, idempotent retry, and conflicting immutable identity.
- [x] Add the application `ExternalEffectRepository` port.
- [x] Add migration `0127_external_effect_reconciliation_records.sql`.
- [x] Add `PgExternalEffectRepository` with payload/index integrity checks.
- [x] Document the persistence boundary and explicit non-claims.
- [ ] Wire durable candidate discovery/read-back adapters and startup orchestration in later gates.

## Verification

- `cargo test -p vestrace-infrastructure --test external_effect_repository -- --nocapture` (requires `DATABASE_URL`; Docker gate below)
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- Docker PostgreSQL migration smoke for migration 0127
