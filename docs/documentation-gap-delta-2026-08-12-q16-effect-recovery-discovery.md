# Documentation Gap Delta — Q16 Durable Effect Recovery Discovery

**Date:** 2026-08-12
**Scope:** durable UNKNOWN-effect discovery and provider read-back boundary
**Status:** implemented as a bounded additive slice; not a v1.0 qualification claim

## Implemented

- `ExternalEffectRepository::find_reconciliation_candidates` discovers only
  UNKNOWN receipts for the requested workspace that do not already have a
  reconciliation fact.
- Migration `0128_external_effect_recovery_indexes.sql` adds indexed
  `effect_id`/`receipt_id` fields to `external_reconciliations`, backfills them
  from the authoritative JSONB payload, and adds discovery indexes.
- `PgExternalEffectRepository` validates the new indexed reconciliation
  identity fields and the full candidate payload before returning recovery
  candidates.
- `ExternalEffectReadBackAdapter` is an injected application boundary for
  provider observations.
- `ExternalEffectRecoveryService` discovers candidates, requests read-back,
  delegates strongest-evidence selection to
  `ExternalEffectReconciliationService`, and persists the resulting fact.

## Evidence

- Focused application recovery tests: 2/2 passed.
- Docker PostgreSQL repository tests: 4/4 passed.
- Workspace library tests: 240/240 passed.
- Workspace compile-all-tests: passed.
- `cargo fmt -- --check`: passed.

## Explicit non-claims

This slice does not wire the recovery service into server/worker startup, ship
provider-specific read-back adapters, execute fault injection in production,
make the read-back provider authoritative without evidence, perform dispatch
or retry, restore trust, or qualify a release profile.
