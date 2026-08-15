use async_trait::async_trait;
use vestrace_application::{ApplicationError, PurgeOutcome, PurgeRepository, RequestContext};
use vestrace_domain::MemoryId;

use super::PgStore;

/// Irreversible removal of a memory and everything that points at it.
///
/// # What was wrong with this
///
/// It ran on a bare pool. Every table it deletes from — `memories`,
/// `memory_revisions`, `memory_sources`, `knowledge_relations` — has forced
/// row-level security, so on a connection where `vestrace.workspace_id` was
/// never set, `vestrace_current_workspace_id()` is NULL, the policy matches no
/// row, and each `DELETE` removed nothing while succeeding. The audit row then
/// inserted without complaint, because `purge_audits` was one of the tables
/// whose policy had been enabled and never forced.
///
/// So the operation reported success, destroyed nothing, and wrote down that it
/// had. Demonstrated against the deployed database before it was fixed:
///
/// ```text
/// SELECT count(*) FROM memories;   →  0        (nothing visible, unscoped)
/// DELETE FROM memories WHERE …     →  DELETE 0
/// INSERT INTO purge_audits …       →  INSERT 0 1
/// ```
pub struct PgPurgeRepository {
    store: PgStore,
}

impl PgPurgeRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl PurgeRepository for PgPurgeRepository {
    async fn purge_memory(
        &self,
        context: &RequestContext,
        memory_id: MemoryId,
        reason: &str,
        approval_id: &str,
    ) -> Result<PurgeOutcome, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let mut outcome = PurgeOutcome::default();

        // Relations first, then the rows that reference the memory, then the
        // memory: the reverse of the order they were written, so no statement
        // is left pointing at something already gone.
        //
        // The parentheses are not decoration. This read
        // `source = $1 AND workspace = $2 OR target = $1 AND workspace = $2`,
        // which is correct only because `AND` binds tighter than `OR` — one
        // edit away from deleting another workspace's relations, on the single
        // statement in this system that cannot be undone.
        let relations = sqlx::query(
            "DELETE FROM knowledge_relations
             WHERE workspace_id = $2
               AND (source_memory_id = $1 OR target_memory_id = $1)",
        )
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;
        outcome
            .removed
            .insert("knowledge_relations".to_string(), relations.rows_affected());

        for (table, statement) in [
            (
                "memory_embeddings",
                "DELETE FROM memory_embeddings WHERE memory_id = $1 AND workspace_id = $2",
            ),
            (
                "search_documents",
                "DELETE FROM search_documents WHERE memory_id = $1 AND workspace_id = $2",
            ),
            (
                "memory_sources",
                "DELETE FROM memory_sources WHERE memory_id = $1 AND workspace_id = $2",
            ),
            (
                "memory_revisions",
                "DELETE FROM memory_revisions WHERE memory_id = $1 AND workspace_id = $2",
            ),
        ] {
            let removed = sqlx::query(statement)
                .bind(memory_id.as_uuid())
                .bind(context.workspace_id.as_uuid())
                .execute(scoped.connection())
                .await
                .map_err(storage_error)?;
            outcome
                .removed
                .insert(table.to_string(), removed.rows_affected());
        }

        // The memory last, and its own count is the one that decides whether a
        // purge happened at all.
        let memories = sqlx::query("DELETE FROM memories WHERE id = $1 AND workspace_id = $2")
            .bind(memory_id.as_uuid())
            .bind(context.workspace_id.as_uuid())
            .execute(scoped.connection())
            .await
            .map_err(storage_error)?;
        outcome
            .removed
            .insert("memories".to_string(), memories.rows_affected());

        // The audit is written in the same transaction as the deletion, so
        // there is no state where one exists without the other — and it now
        // carries the approval that permitted the purge and the counts of what
        // it removed. The approval reference used to end at `let _ =
        // approval_id;`.
        sqlx::query(
            "INSERT INTO purge_audits
                (id, workspace_id, principal_id, target_type, target_id, reason,
                 approval_id, removed_counts, purged_at)
             VALUES ($1, $2, $3, 'memory', $4, $5, $6, $7, NOW())",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(context.workspace_id.as_uuid())
        .bind(context.principal_id.as_uuid())
        .bind(memory_id.as_uuid())
        .bind(reason)
        .bind(approval_id)
        .bind(serde_json::to_value(&outcome.removed).map_err(storage_error)?)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        Ok(outcome)
    }
}
