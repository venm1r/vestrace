# Documentation Gap Delta — Q14 External Effect Persistence

**Date:** 2026-08-12
**Scope:** durable external-effect intent, receipt, and reconciliation evidence after Q13
**Authority:** External Effects Contract, Health / Repair / Incident Contract, Qualification / Conformance Specification, and v0.2 → v1.0 roadmap.

## Implemented bounded contract

- Added the application `ExternalEffectRepository` port and shared repository type.
- Added migration `0127_external_effect_reconciliation_records.sql`.
- Added `PgExternalEffectRepository` for intents, receipts, and reconciliations.
- JSONB payloads remain authoritative and indexed identity/status fields are checked on read.
- Immutable records accept exact retries and reject conflicting reuse of an identity.
- Receipt storage retains the intent foreign-key boundary, while reconciliation payloads preserve the full effect/receipt relationship for later read-back and audit.
- Added SQLx tests for round-trip/idempotent persistence, conflicting intent identity, and confirmed reconciliation fixture construction.

## Explicit boundary

This slice does not discover unfinished effects during server/worker startup, provide provider read-back adapters, execute deterministic crash/fault injection, automatically retry an UNKNOWN effect, restore trust/capabilities, or qualify a v1.0 deployment.
