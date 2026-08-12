const DIAGNOSTICS_REPOSITORY_SOURCE: &str =
    include_str!("../src/postgres/diagnostics_repository.rs");
const PGCRYPTO_MIGRATION_SOURCE: &str =
    include_str!("../../../migrations/0130_pgcrypto_extension.sql");

#[test]
fn expired_lease_diagnostics_match_run_leases_schema() {
    assert!(DIAGNOSTICS_REPOSITORY_SOURCE.contains("AND lease_until < NOW()"));
    assert!(!DIAGNOSTICS_REPOSITORY_SOURCE.contains("AND expires_at < NOW()"));
}

#[test]
fn migrations_install_extensions_required_by_diagnostics() {
    assert!(PGCRYPTO_MIGRATION_SOURCE.contains("CREATE EXTENSION IF NOT EXISTS pgcrypto"));
}
