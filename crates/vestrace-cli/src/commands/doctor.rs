use std::sync::Arc;

use anyhow::{Context, anyhow};
use secrecy::ExposeSecret;
use vestrace_application::{DoctorService, RequestContext};
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::{AppConfig, PgDiagnosticsRepository, PgStore};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let url = config.database.url.expose_secret();
    let redacted = redact_url(url);

    print!("Connecting to database at {redacted} ... ");
    let store = PgStore::connect(&config.database)
        .await
        .context("database is unavailable")?;
    println!("OK");

    let pool = store.pool().clone();
    let repo = Arc::new(PgDiagnosticsRepository::new(pool));
    let doctor = DoctorService::new(repo);

    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let ctx = RequestContext::new(workspace_id, principal_id);

    print!("Running diagnostics ... ");
    let report = doctor
        .run_checks(&ctx)
        .await
        .context("diagnostics execution failed")?;
    println!("done");

    if report.is_clean() {
        println!("Doctor: all checks passed");
        return Ok(());
    }

    for finding in &report.findings {
        let severity_label = match finding.severity {
            vestrace_domain::diagnostics::DiagnosticSeverity::Error => "ERROR",
            vestrace_domain::diagnostics::DiagnosticSeverity::Warning => "WARN ",
            vestrace_domain::diagnostics::DiagnosticSeverity::Info => "INFO ",
        };
        println!(
            "[{severity_label}] {code}: {message}",
            code = finding.code,
            message = finding.message
        );
        if !finding.remediation.is_empty() {
            println!(
                "         remediation: {remedy}",
                remedy = finding.remediation
            );
        }
    }

    if report.has_errors() {
        let error_count = report.errors().len();
        let warning_count = report.warnings().len();
        Err(anyhow!(
            "doctor: {error_count} error(s), {warning_count} warning(s) found"
        ))
    } else {
        let warning_count = report.warnings().len();
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
