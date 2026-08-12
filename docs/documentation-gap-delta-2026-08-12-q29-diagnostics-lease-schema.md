# Documentation Gap Delta — Q29 Diagnostics Lease Schema Correction

**Status:** implemented and verified as a bounded runtime correction; not a v1.0 qualification claim

## Finding

The PostgreSQL diagnostics repository queried `run_leases.expires_at`, while
migration `0021_run_leases_and_work_items.sql` and the run lease repository use
the canonical `run_leases.lease_until` column. This caused the read-only
`doctor` command to fail against a live deployment even when the restricted
runtime role and database connectivity were valid.

## Correction

The expired-lease diagnostic now compares `lease_until < NOW()`. A focused
schema-contract test rejects the old column spelling and requires the migration
column name.

## Non-claims

This correction does not alter data, run lease semantics, migrations, or the
release evidence contract. A fresh Docker deployment must still be rebuilt and
audited for exact build/configuration/environment identity before v1.0 can be
claimed.
