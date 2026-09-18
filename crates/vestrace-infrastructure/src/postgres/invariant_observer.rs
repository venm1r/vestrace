use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, InvariantObservation, InvariantObserver, RequestContext,
};
use vestrace_domain::health::{HealthScope, HealthState};

use super::PgStore;

/// Measures the standard invariants against PostgreSQL.
///
/// # What this deliberately does not do
///
/// It reports what it saw and nothing about what it means. No severity, no
/// remediation, no code of its own: those come from the registry in
/// `vestrace_application::health`, keyed by the invariant id each observation
/// names. The predecessor of this adapter — `PgDiagnosticsRepository` — decided
/// all of them per query, which is why there was no list of what the system
/// checks and no version on any check.
///
/// Two of these invariants are properties of the deployment rather than of a
/// tenant (`database.migrations_applied`, `database.extensions_installed`).
/// They are still observed inside the scoped transaction so there is one code
/// path, and their scope is the workspace that asked, because a finding has to
/// belong somewhere and a deployment-wide finding attached to no workspace could
/// not be read back under row level security.
pub struct PgInvariantObserver {
    store: PgStore,
}

impl PgInvariantObserver {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl InvariantObserver for PgInvariantObserver {
    async fn observe(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<InvariantObservation>, ApplicationError> {
        let scope = HealthScope::workspace(context.workspace_id);
        let workspace = context.workspace_id.as_uuid();
        let mut observations = Vec::new();

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // database.migrations_applied
        let row = sqlx::query(
            "SELECT count(*) FILTER (WHERE success = false) AS failed_count,
                    count(*) AS applied_count
             FROM _sqlx_migrations",
        )
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        let failed: i64 = row.try_get("failed_count").map_err(storage_error)?;
        let applied: i64 = row.try_get("applied_count").map_err(storage_error)?;
        if failed > 0 {
            observations.push(InvariantObservation {
                invariant_id: "database.migrations_applied".to_string(),
                scope: scope.clone(),
                fingerprint: "database.migrations_applied:failed".to_string(),
                state: HealthState::Unhealthy,
                detail: format!("{failed} of {applied} migrations failed to apply"),
                evidence_refs: vec!["table:_sqlx_migrations".to_string()],
            });
        }

        // database.extensions_installed
        let rows = sqlx::query(
            "SELECT extname FROM pg_extension WHERE extname IN ('pgcrypto', 'vector', 'pg_trgm')",
        )
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;
        let installed: Vec<String> = rows
            .into_iter()
            .map(|row| row.get::<String, _>("extname"))
            .collect();
        let missing: Vec<&str> = ["pgcrypto", "vector", "pg_trgm"]
            .into_iter()
            .filter(|required| !installed.iter().any(|name| name == required))
            .collect();
        if !missing.is_empty() {
            observations.push(InvariantObservation {
                invariant_id: "database.extensions_installed".to_string(),
                scope: scope.clone(),
                // The missing set is part of the identity: losing `vector` and
                // losing `pg_trgm` are different problems with different fixes.
                fingerprint: format!("database.extensions_installed:{}", missing.join(",")),
                state: HealthState::Unhealthy,
                detail: format!(
                    "required extensions are not installed: {}",
                    missing.join(", ")
                ),
                evidence_refs: vec!["catalog:pg_extension".to_string()],
            });
        }

        // memory.active_memory_has_source
        let row = sqlx::query(
            "SELECT count(*) AS unsourced
             FROM memories m
             LEFT JOIN memory_sources ms ON ms.memory_id = m.id
             WHERE m.workspace_id = $1 AND m.status = 'active' AND ms.id IS NULL",
        )
        .bind(workspace)
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        let unsourced: i64 = row.try_get("unsourced").map_err(storage_error)?;
        if unsourced > 0 {
            observations.push(InvariantObservation {
                invariant_id: "memory.active_memory_has_source".to_string(),
                scope: scope.clone(),
                fingerprint: format!("memory.active_memory_has_source:{workspace}"),
                state: HealthState::Degraded,
                detail: format!("{unsourced} active memories have no provenance source"),
                evidence_refs: vec!["table:memory_sources".to_string()],
            });
        }

        // memory.active_memory_is_indexed
        let row = sqlx::query(
            "SELECT count(*) AS unindexed
             FROM memories m
             LEFT JOIN search_documents sd
               ON sd.memory_id = m.id AND sd.workspace_id = m.workspace_id
             WHERE m.workspace_id = $1 AND m.status = 'active' AND sd.id IS NULL",
        )
        .bind(workspace)
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        let unindexed: i64 = row.try_get("unindexed").map_err(storage_error)?;
        if unindexed > 0 {
            observations.push(InvariantObservation {
                invariant_id: "memory.active_memory_is_indexed".to_string(),
                scope: scope.clone(),
                fingerprint: format!("memory.active_memory_is_indexed:{workspace}"),
                state: HealthState::Degraded,
                detail: format!(
                    "{unindexed} active memories have no search document, so retrieval \
                     cannot return them"
                ),
                evidence_refs: vec!["table:search_documents".to_string()],
            });
        }

        // memory.active_memory_is_embedded
        //
        // Counted across every space: a workspace with no space configured has
        // no vector channel, and the count is then simply zero rather than
        // every memory being reported unembedded.
        let row = sqlx::query(
            "SELECT count(*) AS unembedded
             FROM memories m
             WHERE m.workspace_id = $1
               AND m.status = 'active'
               AND EXISTS (SELECT 1 FROM embedding_spaces s WHERE s.workspace_id = $1)
               AND NOT EXISTS (
                   SELECT 1 FROM memory_embeddings e
                   WHERE e.memory_id = m.id AND e.workspace_id = m.workspace_id
               )",
        )
        .bind(workspace)
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        let unembedded: i64 = row.try_get("unembedded").map_err(storage_error)?;
        if unembedded > 0 {
            observations.push(InvariantObservation {
                invariant_id: "memory.active_memory_is_embedded".to_string(),
                scope: scope.clone(),
                fingerprint: format!("memory.active_memory_is_embedded:{workspace}"),
                state: HealthState::Degraded,
                detail: format!(
                    "{unembedded} active memories have no embedding, so the vector channel                      cannot return them"
                ),
                evidence_refs: vec!["table:memory_embeddings".to_string()],
            });
        }

        // outbox.backlog_within_budget
        //
        // Grouped by topic rather than counted flat. A backlog has two very
        // different causes — a drain that is behind, and a topic no handler
        // claims — and a bare number cannot tell them apart. The dispatcher
        // counts unhandled messages but leaves them pending on purpose, so the
        // topic is the whole diagnosis.
        //
        // Dead letters are excluded. They are not waiting for anything, and
        // counting them here would report the same message twice under two
        // invariants and leave a backlog that can never fall to zero.
        let rows = sqlx::query(
            "SELECT topic, count(*) AS pending
             FROM outbox
             WHERE workspace_id = $1
               AND processed_at IS NULL
               AND dead_lettered_at IS NULL
               AND created_at < NOW() - INTERVAL '60 seconds'
             GROUP BY topic
             ORDER BY count(*) DESC, topic ASC",
        )
        .bind(workspace)
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;
        let mut pending: i64 = 0;
        let mut by_topic: Vec<String> = Vec::new();
        for row in &rows {
            let topic: String = row.try_get("topic").map_err(storage_error)?;
            let count: i64 = row.try_get("pending").map_err(storage_error)?;
            pending += count;
            by_topic.push(format!("{topic}={count}"));
        }
        if pending > 0 {
            observations.push(InvariantObservation {
                invariant_id: "outbox.backlog_within_budget".to_string(),
                scope: scope.clone(),
                fingerprint: format!("outbox.backlog_within_budget:{workspace}"),
                state: HealthState::Degraded,
                detail: format!(
                    "{pending} outbox messages have waited more than 60 seconds ({})",
                    by_topic.join(", ")
                ),
                evidence_refs: vec!["table:outbox".to_string()],
            });
        }

        // outbox.no_dead_letters
        //
        // Reported apart from the backlog, and more severely: a backlog is
        // delivery running late, and a dead letter is delivery abandoned. The
        // last error is carried into the detail because that is the whole
        // reason it is stored on the row — the log line that produced it has
        // rotated by the time anyone reads this.
        let rows = sqlx::query(
            "SELECT topic, count(*) AS abandoned, max(last_error) AS last_error
             FROM outbox
             WHERE workspace_id = $1 AND dead_lettered_at IS NOT NULL
             GROUP BY topic
             ORDER BY count(*) DESC, topic ASC",
        )
        .bind(workspace)
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;
        let mut abandoned: i64 = 0;
        let mut causes: Vec<String> = Vec::new();
        for row in &rows {
            let topic: String = row.try_get("topic").map_err(storage_error)?;
            let count: i64 = row.try_get("abandoned").map_err(storage_error)?;
            let last_error: Option<String> = row.try_get("last_error").map_err(storage_error)?;
            abandoned += count;
            causes.push(format!(
                "{topic}={count} ({})",
                last_error
                    .unwrap_or_else(|| "no reason recorded".to_string())
                    .chars()
                    .take(160)
                    .collect::<String>()
            ));
        }
        if abandoned > 0 {
            observations.push(InvariantObservation {
                invariant_id: "outbox.no_dead_letters".to_string(),
                scope: scope.clone(),
                fingerprint: format!("outbox.no_dead_letters:{workspace}"),
                state: HealthState::Unhealthy,
                detail: format!(
                    "{abandoned} outbox messages were abandoned after exhausting their delivery \
                     attempts: {}",
                    causes.join("; ")
                ),
                evidence_refs: vec!["table:outbox".to_string()],
            });
        }

        // model.recent_failures_within_budget
        let row = sqlx::query(
            "SELECT count(*) AS failures
             FROM model_executions
             WHERE workspace_id = $1
               AND status = 'failed'
               AND created_at > NOW() - INTERVAL '24 hours'",
        )
        .bind(workspace)
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        let failures: i64 = row.try_get("failures").map_err(storage_error)?;
        if failures > 5 {
            observations.push(InvariantObservation {
                invariant_id: "model.recent_failures_within_budget".to_string(),
                scope: scope.clone(),
                fingerprint: format!("model.recent_failures_within_budget:{workspace}"),
                state: HealthState::Degraded,
                detail: format!("{failures} model executions failed in the last 24 hours"),
                evidence_refs: vec!["table:model_executions".to_string()],
            });
        }

        // run.lease_not_expired
        let row = sqlx::query(
            "SELECT count(*) AS expired
             FROM run_leases
             WHERE workspace_id = $1 AND lease_until < NOW()",
        )
        .bind(workspace)
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        let expired: i64 = row.try_get("expired").map_err(storage_error)?;
        if expired > 0 {
            observations.push(InvariantObservation {
                invariant_id: "run.lease_not_expired".to_string(),
                scope: scope.clone(),
                fingerprint: format!("run.lease_not_expired:{workspace}"),
                state: HealthState::Degraded,
                detail: format!("{expired} run leases have expired"),
                evidence_refs: vec!["table:run_leases".to_string()],
            });
        }

        // jobs.no_dead_letters
        let row = sqlx::query(
            "SELECT count(*) AS dead_lettered
             FROM jobs
             WHERE workspace_id = $1 AND state = 'dead_lettered'",
        )
        .bind(workspace)
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;
        let dead_lettered: i64 = row.try_get("dead_lettered").map_err(storage_error)?;
        if dead_lettered > 0 {
            observations.push(InvariantObservation {
                invariant_id: "jobs.no_dead_letters".to_string(),
                scope,
                fingerprint: format!("jobs.no_dead_letters:{workspace}"),
                state: HealthState::Degraded,
                detail: format!("{dead_lettered} jobs are in the dead-letter state"),
                evidence_refs: vec!["table:jobs".to_string()],
            });
        }

        scoped.commit().await.map_err(storage_error)?;
        Ok(observations)
    }
}
