use std::sync::Arc;

use anyhow::{Context, anyhow};
use secrecy::ExposeSecret;
use vestrace_application::{
    HealthInspectionService, HealthMonitorService, MonitoredFinding, RequestContext,
    standard_invariants,
};
use vestrace_domain::health::HealthSeverity;
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::{AppConfig, PgHealthFindingRepository, PgInvariantObserver, PgStore};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let url = config.database.url.expose_secret();
    let redacted = redact_url(url);

    print!("Connecting to database at {redacted} ... ");
    let store = PgStore::connect(&config.database)
        .await
        .context("database is unavailable")?;
    println!("OK");

    // The observer measures and the registry decides what a measurement means.
    // Before this the adapter did both, so there was no list of what the doctor
    // claims to check and no version on any check.
    let inspection = HealthInspectionService::new(standard_invariants());
    println!(
        "Checking {} registered invariants",
        inspection.registry().definitions().count()
    );
    let monitor = HealthMonitorService::new(
        Arc::new(PgInvariantObserver::new(store.clone())),
        Arc::new(PgHealthFindingRepository::new(store.clone())),
        inspection,
    );

    // Seven of the nine checks count rows in a tenant's tables. This command
    // used to build its context from `WorkspaceId::new()` — a fresh random id
    // belonging to no workspace — so those seven asked about a workspace that
    // does not exist and could only ever answer "nothing wrong". A clean
    // doctor report meant nothing had been examined.
    //
    // The workspaces come from the same explicit top-level list the worker and
    // startup recovery poll. There is no cross-workspace scan to fall back on.
    if config.workspaces.is_empty() {
        return Err(anyhow!(
            "doctor: no workspaces are configured; set `workspaces` so the \
             tenant checks have something to examine"
        ));
    }

    let principal_id = PrincipalId::new();
    let mut error_count = 0usize;
    let mut warning_count = 0usize;

    for workspace in &config.workspaces {
        let workspace_id = WorkspaceId::from_uuid(*workspace);
        let ctx = RequestContext::new(workspace_id, principal_id);

        print!("Running diagnostics for workspace {workspace} ... ");
        let findings = monitor
            .inspect(&ctx, vestrace_domain::time::now())
            .await
            .context("invariant inspection failed")?;
        println!("done");

        if findings.is_empty() {
            println!("  every invariant holds");
            continue;
        }

        for entry in &findings {
            let severity_label = if entry.is_silenced() {
                // A silenced finding still prints. Hiding it would make an
                // operator's decision to look away indistinguishable from the
                // problem having gone.
                "SILNT"
            } else {
                match entry.severity() {
                    HealthSeverity::Critical => "CRIT ",
                    HealthSeverity::Error => "ERROR",
                    HealthSeverity::Warning => "WARN ",
                    HealthSeverity::Info => "INFO ",
                }
            };
            let seen = if entry.observed_on_this_run {
                format!("seen {}x", entry.finding.occurrence_count())
            } else {
                format!(
                    "seen {}x, not on this run",
                    entry.finding.occurrence_count()
                )
            };
            println!(
                "[{severity_label}] {invariant}@{version} ({seen}, since {since}): {detail}",
                invariant = entry.finding.invariant_id(),
                version = entry.finding.invariant_version(),
                since = entry.finding.first_seen_at(),
                detail = entry.detail
            );
            println!(
                "         remediation: {remedy}",
                remedy = entry.definition.remediation()
            );
        }

        error_count += findings.iter().filter(|entry| entry.is_error()).count();
        warning_count += findings
            .iter()
            .filter(|entry| !MonitoredFinding::is_error(entry))
            .count();
    }

    if error_count > 0 {
        Err(anyhow!(
            "doctor: {error_count} error(s), {warning_count} warning(s) found"
        ))
    } else {
        println!("Doctor: {warning_count} warning(s) found, no errors");
        Ok(())
    }
}

fn redact_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(scheme_end) = url.find("://") {
            let scheme = &url[..scheme_end + 3];
            let host = &url[at_pos + 1..];
            return format!("{scheme}***@{host}");
        }
    }
    url.to_string()
}
