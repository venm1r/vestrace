const STARTUP_RECOVERY_SOURCE: &str = include_str!("../src/postgres/startup_recovery_source.rs");

/// Discovery must stay workspace-scoped: every run/lease query in this
/// repository is filtered by the caller's workspace, and startup recovery is no
/// exception.
#[test]
fn startup_recovery_discovery_is_workspace_scoped() {
    assert!(STARTUP_RECOVERY_SOURCE.contains("workspace_id = $1"));
}

/// A run whose lease has expired is the one case the domain models exactly
/// (`RecoveryTarget::StaleLease`), so it must be selected through `lease_until`
/// and not through the column name the diagnostics path once used.
#[test]
fn expired_lease_candidates_match_run_leases_schema() {
    assert!(STARTUP_RECOVERY_SOURCE.contains("lease_until < NOW()"));
    assert!(!STARTUP_RECOVERY_SOURCE.contains("expires_at"));
}

/// A live lease belongs to another running worker; recovering it would race
/// that worker. The leased branch therefore admits expired leases only, and the
/// unleased branch admits only runs with no lease row at all.
#[test]
fn live_leases_are_excluded_from_startup_candidates() {
    assert!(STARTUP_RECOVERY_SOURCE.contains("JOIN run_leases"));
    assert!(STARTUP_RECOVERY_SOURCE.contains("AND l.lease_until < NOW()"));
    assert!(STARTUP_RECOVERY_SOURCE.contains("NOT EXISTS"));
}

/// A run that was executing without any lease cannot be proven to have stopped
/// cleanly, so it must reconcile rather than silently resume.
#[test]
fn unleased_executing_runs_are_classified_as_unknown_outcome() {
    assert!(STARTUP_RECOVERY_SOURCE.contains("'unknown_outcome'"));
    assert!(STARTUP_RECOVERY_SOURCE.contains("RecoveryTarget::UnknownOutcome"));
}

/// Terminal runs are finished work and must not be recovered.
#[test]
fn terminal_runs_are_excluded_from_startup_candidates() {
    for terminal in [
        "succeeded",
        "succeeded_with_warnings",
        "partial",
        "failed",
        "cancelled",
        "expired",
    ] {
        assert!(
            STARTUP_RECOVERY_SOURCE.contains(&format!("'{terminal}'")),
            "terminal status {terminal} is not excluded"
        );
    }
}
