# Documentation Gap Delta — Q17 Fault-Suite Evidence Persistence

**Date:** 2026-08-12
**Scope:** durable target-bound deterministic effect fault evidence
**Status:** implemented as a bounded additive slice; not a v1.0 qualification claim

## Implemented

- `ExternalEffectFaultSuiteEvidence` snapshots the exact target digest, every
  required fault observation, pass/fail decision, failure reasons, immutable
  ID, and creation timestamp.
- Failed decisions and unsafe-retry observations remain visible in the stored
  artifact; persistence does not promote them to a passing result.
- `FaultSuiteEvidenceRepository` defines the application persistence port.
- Migration `0129_external_effect_fault_suite_evidence.sql` adds durable JSONB
  storage with indexed target/pass/time metadata and payload-object checks.
- `PgFaultSuiteEvidenceRepository` provides idempotent exact retries, rejects
  conflicting immutable IDs, and validates indexed metadata against the
  authoritative payload on read.

## Evidence

- Snapshot tests: 3/3 passed.
- Docker PostgreSQL repository tests: 7/7 passed.
- Workspace library tests: 240/240 passed.
- Workspace compile-all-tests: passed.
- `cargo fmt -- --check`: passed.

## Explicit non-claims

This slice does not inject faults into production adapters, execute destructive
or chaos scenarios, automatically consume evidence in a qualification bundle,
restore trust, or qualify a release profile. Production wiring and release
approval remain open.
