use std::sync::Arc;

use anyhow::{Context, anyhow};
use secrecy::ExposeSecret;
use vestrace_application::{
    HealthInspectionService, HealthMonitorService, ModelRevisionRepository, MonitoredFinding,
    RequestContext, standard_invariants,
};
use vestrace_domain::embedding::EmbeddingReadinessReason;
use vestrace_domain::health::HealthSeverity;
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::{
    AppConfig, PgHealthFindingRepository, PgInvariantObserver, PgModelRevisionRepository, PgStore,
};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let url = config.database.url.expose_secret();
    let redacted = crate::commands::redact_url(url);

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

    let models = PgModelRevisionRepository::new(store.clone());

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

        // Before the `continue` below, not after it. Embedding readiness is not
        // an invariant violation, so a workspace where every invariant holds is
        // exactly a workspace whose adoption state an operator still needs to
        // see -- and reporting it only when something else was already wrong
        // would hide it precisely when it is the only thing to report.
        report_embedding_readiness(&models, &ctx).await?;

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

/// What is standing in the way of this workspace answering from embeddings.
///
/// Read from the governed Models projection rather than computed here, so an
/// operator running `doctor` and an operator reading `GET /v1/models` are told
/// the same thing by the same authority. A second derivation would eventually
/// disagree with the first, and the operator would have no way to know which
/// was right.
///
/// `embedding-index-not-loaded` never appears, and cannot: `doctor` holds a
/// database connection and nothing else, and no database transaction can see
/// whether some worker process has an index in memory. A doctor that printed it
/// would send an operator to rebuild an index that may already be loaded.
async fn report_embedding_readiness(
    models: &PgModelRevisionRepository,
    context: &RequestContext,
) -> anyhow::Result<()> {
    let projections = models
        .list_safe_models(context)
        .await
        .context("reading the governed model projection failed")?;

    let mut reported = 0usize;
    for model in &projections {
        let reasons: Vec<EmbeddingReadinessReason> = model
            .blockers
            .iter()
            .filter_map(|blocker| blocker.parse().ok())
            .collect();
        if reasons.is_empty() {
            continue;
        }
        reported += 1;
        for reason in reasons {
            println!(
                "[EMBED] model {model_id}: {reason}",
                model_id = model.id.as_uuid()
            );
            println!(
                "         remediation: {remedy}",
                remedy = remediation(reason)
            );
        }
    }
    if reported == 0 && !projections.is_empty() {
        println!("  embedding readiness: nothing blocking");
    }
    // Not counted as an error or a warning. These are standing conditions of an
    // installation mid-adoption, not invariant violations, and an exit code
    // that failed a deployment for being mid-transition would make `doctor`
    // unusable during exactly the work it exists to supervise.
    Ok(())
}

/// What to do about each. Stated here rather than beside the vocabulary because
/// a remedy is an operational instruction and the vocabulary is a protocol.
const fn remediation(reason: EmbeddingReadinessReason) -> &'static str {
    match reason {
        EmbeddingReadinessReason::LegacyAdoptionRequired => {
            "run governed legacy adoption; the plaintext corpus cannot be read directly"
        }
        EmbeddingReadinessReason::IndexNotLoaded => {
            "unreachable from a database read; see the retrieval degradation instead"
        }
        EmbeddingReadinessReason::GenerationNotReady => {
            "publish a generation: capture and publish through the governed builder"
        }
        EmbeddingReadinessReason::QualificationTransitionNotReady => {
            "finish or abandon the transition that owns this space before retrieval resumes"
        }
        EmbeddingReadinessReason::ErasurePending => {
            "run the erasure reconciler; projections of an erased source are still drawable"
        }
        EmbeddingReadinessReason::IndexBuildFailed => {
            "read the failed build attempt's safe reason, then rebuild the current generation"
        }
    }
}
