# Backup, upgrade, and recovery acceptance

**Scope:** Requirements for P05/P12 verification, not an already executed disaster-recovery procedure. No unverified command here promises restoration of a live installation.

## Recovery set

A consistent recovery set includes the canonical database and required WAL, material keys, fingerprints, and secret dependencies of the specific implementation. Arbitrary independent volume copies may be inconsistent. Establish the freeze/drain boundary and exact receipts before backup.

## Controlled experiment

1. In isolation, create sources, revisions, Runs, and permitted materials through normal APIs.
2. Record a safe inventory of expected identities, queries, and permissions, without keys.
3. Execute the operator-approved backup procedure and record durable boundaries.
4. Restore to a separate environment, never over the original.
5. Check history, isolation, key usability, recovery of unfinished work, and continued operation.
6. Repeat with a required component missing: refusal must identify the dependency rather than fabricate a new empty state.

## Upgrade

Test migration against both a populated supported baseline and an empty database. Parsing SQL or starting successfully is insufficient. Verify owners/grants, constrained writes, replay, and rollback of complete business transactions on error.

Do not claim downgrade support without an inverse contract. A forward-only migration may require restoration of a compatible environment from a tested backup, not arbitrary reverse SQL. The exact strategy belongs in the operator plan.

## Report

Record the actual procedure, target, restored data, and checks not run. RPO/RTO are measured outcomes, not invented service guarantees. MW knowledge JSON round-trip does not replace installation restore testing.

**Sources:** [Compose](../../docker-compose.yml), [frozen release program](../superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [governance](../specs/en/vestrace-crypto-data-governance-contract-v0.2.md).
