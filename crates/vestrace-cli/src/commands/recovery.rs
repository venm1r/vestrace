use std::sync::Arc;

use anyhow::anyhow;
use vestrace_application::{
    RequestContext, RunRecoveryService, StartupRecoveryOutcome, StartupRecoveryService,
};
use vestrace_domain::id::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::{PgRunRecoveryStore, PgStartupRecoverySource, PgStore};

/// Sweep the given workspaces for runs a previous process left mid-flight.
///
/// This runs before the process starts accepting work. It is fail-closed: a
/// failing sweep aborts startup rather than letting the process serve traffic
/// on top of unrecovered state.
pub async fn run_startup_recovery(
    workspaces: &[uuid::Uuid],
    store: &PgStore,
) -> anyhow::Result<()> {
    let recovery_store = Arc::new(PgRunRecoveryStore::new(store.clone()));
    let source = Arc::new(PgStartupRecoverySource::new(store.clone()));
    let service = StartupRecoveryService::with_candidate_source(
        Arc::new(RunRecoveryService::new(recovery_store)),
        source,
    );

    for workspace in workspaces {
        let context = RequestContext::new(
            WorkspaceId::from_uuid(*workspace),
            PrincipalId::from_uuid(*workspace),
        );
        let report = service
            .run_discovered(&context)
            .await
            .map_err(|error| anyhow!("startup recovery failed for {workspace}: {error}"))?;

        for record in report.records() {
            match record.outcome {
                StartupRecoveryOutcome::Restored | StartupRecoveryOutcome::RetryReady => {
                    tracing::info!(
                        workspace = %workspace,
                        run_id = %record.run_id,
                        target = ?record.target,
                        outcome = ?record.outcome,
                        "startup recovery recovered run"
                    );
                }
                StartupRecoveryOutcome::ReconciliationRequired
                | StartupRecoveryOutcome::Aborted
                | StartupRecoveryOutcome::HumanReviewRequired => {
                    tracing::warn!(
                        workspace = %workspace,
                        run_id = %record.run_id,
                        target = ?record.target,
                        outcome = ?record.outcome,
                        "startup recovery left run for explicit handling"
                    );
                }
            }
        }

        tracing::info!(
            workspace = %workspace,
            candidates = report.records().len(),
            "startup recovery sweep complete"
        );
    }

    Ok(())
}
