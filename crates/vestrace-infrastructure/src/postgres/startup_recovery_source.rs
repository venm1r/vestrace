use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, RequestContext, StartupRecoveryCandidate, StartupRecoveryCandidateSource,
};
use vestrace_domain::{AgentRunId, RecoveryTarget};

use super::PgStore;

/// Durable discovery of runs that a previous process left mid-flight.
///
/// The classification is derived from lease state only, never inferred from the
/// run payload:
///
/// - a non-terminal run whose lease has expired is a [`RecoveryTarget::StaleLease`],
///   which the domain classifies as safe to retry;
/// - a non-terminal run that was executing but holds no lease at all is a
///   [`RecoveryTarget::UnknownOutcome`], which the domain classifies as
///   must-reconcile. We cannot prove such a run stopped cleanly, so it is never
///   silently resumed;
/// - a run holding a live lease belongs to another worker and is not a candidate.
///
/// The query was already scoped to `context.workspace_id` in every branch,
/// including the `NOT EXISTS` — this is a sweep run at startup, and one that
/// crossed workspaces would hand another tenant's runs to this process. What it
/// lacked was the connection scope, so the policies on `agent_runs` and
/// `run_leases` saw no workspace.
#[derive(Clone, Debug)]
pub struct PgStartupRecoverySource {
    store: PgStore,
}

impl PgStartupRecoverySource {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

const TERMINAL_STATUSES: &str =
    "('succeeded','succeeded_with_warnings','partial','failed','cancelled','expired')";

const EXECUTING_STATUSES: &str = "('preparing','running')";

#[async_trait]
impl StartupRecoveryCandidateSource for PgStartupRecoverySource {
    async fn find_startup_recovery_candidates(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<StartupRecoveryCandidate>, ApplicationError> {
        let query = format!(
            r#"
            SELECT r.id AS run_id, 'stale_lease' AS target
            FROM agent_runs AS r
            JOIN run_leases AS l ON l.run_id = r.id
            WHERE r.workspace_id = $1
              AND l.workspace_id = $1
              AND r.status NOT IN {TERMINAL_STATUSES}
              AND l.lease_until < NOW()

            UNION ALL

            SELECT r.id AS run_id, 'unknown_outcome' AS target
            FROM agent_runs AS r
            WHERE r.workspace_id = $1
              AND r.status IN {EXECUTING_STATUSES}
              AND NOT EXISTS (
                  SELECT 1
                  FROM run_leases AS l
                  WHERE l.run_id = r.id
                    AND l.workspace_id = $1
              )

            ORDER BY run_id
            "#
        );

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(&query)
            .bind(context.workspace_id.as_uuid())
            .fetch_all(scoped.connection())
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;

        scoped.commit().await.map_err(storage_error)?;

        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let run_id: uuid::Uuid = row.try_get("run_id").map_err(storage_error)?;
            let target: String = row.try_get("target").map_err(storage_error)?;
            let target = match target.as_str() {
                "stale_lease" => RecoveryTarget::StaleLease,
                "unknown_outcome" => RecoveryTarget::UnknownOutcome,
                other => {
                    return Err(ApplicationError::Storage(format!(
                        "startup recovery discovery produced unknown target {other:?}"
                    )));
                }
            };
            candidates.push(StartupRecoveryCandidate::new(
                AgentRunId::from_uuid(run_id),
                target,
            ));
        }
        Ok(candidates)
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
