# Q14 — External Effect Persistence

## Goal

Persist immutable external-effect intent, receipt, and reconciliation evidence so UNKNOWN outcomes remain auditable across restart.

## Tasks

- [x] Add RED SQLx repository tests for round-trip, idempotent retry, and conflicting immutable identity.
- [x] Add the application `ExternalEffectRepository` port.
- [x] Add migration `0127_external_effect_reconciliation_records.sql`.
- [x] Add `PgExternalEffectRepository` with payload/index integrity checks.
- [x] Document the persistence boundary and explicit non-claims.
- [x] Durable **external-effect** candidate discovery delivered by Q16 (`ExternalEffectRecoveryService`); the `StartupRecoveryService` contract was delivered by Q12, but run-candidate discovery was not.
- [x] Provider read-back (R2) and production startup wiring (R1) delivered; see [`2026-08-12-open-follow-up-gates.md`](2026-08-12-open-follow-up-gates.md).

## Verification

- `cargo test -p vestrace-infrastructure --test external_effect_repository -- --nocapture` (requires `DATABASE_URL`; Docker gate below)
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- Docker PostgreSQL migration smoke for migration 0127
