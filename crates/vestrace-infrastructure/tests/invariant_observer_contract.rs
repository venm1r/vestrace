//! Source-level guards on the invariant observer's SQL.
//!
//! These predate `PgInvariantObserver` and were written against
//! `PgDiagnosticsRepository`, the hand-written check path it replaced. They are
//! kept because the defect they guard against was real: the lease check once
//! selected on `expires_at`, a column `run_leases` does not have, so the check
//! could never have run. A source assertion is a blunt instrument, and it is the
//! only one that catches a column name that is wrong in a query no test
//! exercises.

const INVARIANT_OBSERVER_SOURCE: &str = include_str!("../src/postgres/invariant_observer.rs");
const PGCRYPTO_MIGRATION_SOURCE: &str =
    include_str!("../../../migrations/0130_pgcrypto_extension.sql");

#[test]
fn expired_lease_observation_matches_the_run_leases_schema() {
    assert!(INVARIANT_OBSERVER_SOURCE.contains("lease_until < NOW()"));
    assert!(!INVARIANT_OBSERVER_SOURCE.contains("expires_at < NOW()"));
}

#[test]
fn migrations_install_extensions_the_observer_checks_for() {
    assert!(PGCRYPTO_MIGRATION_SOURCE.contains("CREATE EXTENSION IF NOT EXISTS pgcrypto"));
    // The observer must ask about the extensions the schema actually needs; a
    // list that drifts from the migrations would report a healthy database that
    // cannot run.
    for extension in ["pgcrypto", "vector", "pg_trgm"] {
        assert!(
            INVARIANT_OBSERVER_SOURCE.contains(extension),
            "the observer does not check for {extension}"
        );
    }
}
