# Documentation Gap Delta — Q11 Recovery and Trust Persistence

**Date:** 2026-08-12
**Scope:** durable Incident/RevalidationRun/TrustState/RecoveryPoint evidence after Q10
**Authority:** Health / Repair / Incident Contract, Qualification / Conformance Specification, and v0.2 → v1.0 roadmap.

## Implemented bounded contract

- Added application `RecoveryRepository` and shared repository port for incident, revalidation, trust-state, and recovery-point records.
- Added migration `0126_incident_recovery_trust_records.sql` with payload-preserving JSONB storage and indexed metadata.
- Added `PgRecoveryRepository` with read-back integrity checks for all indexed fields.
- Immutable records are idempotent on exact retry and reject conflicting reuse of the same identity; trust-state records are append-oriented and support latest-by-scope lookup.
- Added focused SQLx integration tests covering round-trip/idempotent persistence and conflicting immutable identity.

## Explicit boundary

This slice persists evidence but does not yet scan/recover unfinished runtime work on startup, execute reconciliation, drive Incident lifecycle transitions, or restore trust. The Docker migration smoke passed against PostgreSQL 17; the repository SQLx integration test could not start in the host environment because `DATABASE_URL` is absent, and the Docker Rust test container hit the bounded compile timeout before tests began.
