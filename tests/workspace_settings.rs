//! Workspace settings: the values an operator can actually change.
//!
//! Only settings the runtime genuinely consults belong here. Facts about the
//! environment — whether RLS is enforced, the database pool size, the log
//! filter — are observed and reported read-only, never stored as if an operator
//! could flip them.

use vestrace_domain::{DomainError, LogLevel, WorkspaceId, WorkspaceSettings};

#[test]
fn defaults_are_conservative_and_versioned() {
    let settings = WorkspaceSettings::defaults(WorkspaceId::new());

    assert_eq!(settings.max_concurrent_runs(), 1);
    assert_eq!(settings.run_budget_cap_micros(), 0);
    assert_eq!(settings.log_level(), LogLevel::Info);
    assert_eq!(settings.version(), 1);
}

#[test]
fn concurrency_must_be_positive_and_bounded() {
    let workspace = WorkspaceId::new();

    assert!(matches!(
        WorkspaceSettings::new(workspace, 0, 0, LogLevel::Info, 1),
        Err(DomainError::InvalidArgument(message)) if message.contains("max_concurrent_runs")
    ));
    assert!(matches!(
        WorkspaceSettings::new(workspace, 10_001, 0, LogLevel::Info, 1),
        Err(DomainError::InvalidArgument(message)) if message.contains("max_concurrent_runs")
    ));
    assert!(WorkspaceSettings::new(workspace, 1, 0, LogLevel::Info, 1).is_ok());
    assert!(WorkspaceSettings::new(workspace, 10_000, 0, LogLevel::Info, 1).is_ok());
}

/// The version is an optimistic-concurrency token, so it must always be a real
/// revision rather than an unset zero.
#[test]
fn version_must_be_positive() {
    assert!(matches!(
        WorkspaceSettings::new(WorkspaceId::new(), 1, 0, LogLevel::Info, 0),
        Err(DomainError::InvalidArgument(message)) if message.contains("version")
    ));
}

/// A budget cap of zero means "no cap"; a negative cap is not representable, so
/// the type keeps it unsigned and the meaning explicit.
#[test]
fn a_zero_budget_cap_means_unlimited() {
    let settings = WorkspaceSettings::new(WorkspaceId::new(), 4, 0, LogLevel::Warn, 3).unwrap();

    assert!(settings.budget_is_unlimited());

    let capped =
        WorkspaceSettings::new(WorkspaceId::new(), 4, 25_000_000, LogLevel::Warn, 3).unwrap();

    assert!(!capped.budget_is_unlimited());
    assert_eq!(capped.run_budget_cap_micros(), 25_000_000);
}

#[test]
fn applying_changes_bumps_the_version_and_keeps_identity() {
    let workspace = WorkspaceId::new();
    let settings = WorkspaceSettings::defaults(workspace);

    let updated = settings
        .clone()
        .apply(8, 5_000_000, LogLevel::Debug)
        .unwrap();

    assert_eq!(updated.workspace_id(), workspace);
    assert_eq!(updated.max_concurrent_runs(), 8);
    assert_eq!(updated.run_budget_cap_micros(), 5_000_000);
    assert_eq!(updated.log_level(), LogLevel::Debug);
    assert_eq!(updated.version(), settings.version() + 1);
}

#[test]
fn applying_invalid_changes_leaves_no_partial_update() {
    let settings = WorkspaceSettings::defaults(WorkspaceId::new());

    let error = settings.clone().apply(0, 0, LogLevel::Info).unwrap_err();

    assert!(matches!(error, DomainError::InvalidArgument(_)));
    assert_eq!(settings.max_concurrent_runs(), 1);
    assert_eq!(settings.version(), 1);
}

#[test]
fn log_levels_round_trip_through_their_wire_names() {
    for (level, name) in [
        (LogLevel::Error, "error"),
        (LogLevel::Warn, "warn"),
        (LogLevel::Info, "info"),
        (LogLevel::Debug, "debug"),
        (LogLevel::Trace, "trace"),
    ] {
        assert_eq!(level.as_str(), name);
        assert_eq!(name.parse::<LogLevel>().unwrap(), level);
    }

    assert!("verbose".parse::<LogLevel>().is_err());
}
