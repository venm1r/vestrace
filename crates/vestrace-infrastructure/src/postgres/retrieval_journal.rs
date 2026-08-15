use async_trait::async_trait;
use vestrace_application::retrieval::RetrievalRunRecord;
use vestrace_application::{ApplicationError, RequestContext, RetrievalJournal};
use vestrace_domain::{
    WorkspaceId,
    id::{ContextPackId, RetrievalRunId},
};

use super::PgStore;

/// The retrieval journal.
///
/// # Why this holds a store and not a pool
///
/// It wrote through a bare `PgPool`, so no `vestrace.workspace_id` was ever set
/// for the connection. `retrieval_runs` and `context_packs` had row level
/// security enabled but not **forced**, and the runtime role owns both tables —
/// so the policy was inert for exactly the writer that mattered, and the journal
/// recorded whatever workspace id it was handed with nothing checking it.
///
/// Migration 0138 forces the policy on both tables, which turned that from a
/// latent hole into an immediate failure. The write now runs inside a scoped
/// transaction like every other store, so the policy has something to check
/// against.
pub struct PgRetrievalJournal {
    store: PgStore,
}

impl PgRetrievalJournal {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl RetrievalJournal for PgRetrievalJournal {
    async fn record_run(
        &self,
        context: &RequestContext,
        record: &RetrievalRunRecord,
    ) -> Result<(), ApplicationError> {
        // Serialised here rather than in the port so the journal's storage
        // shape stays an infrastructure concern; the record carries typed
        // channel outcomes, not JSON.
        let channels = serde_json::to_value(&record.channels)
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO retrieval_runs
                (id, workspace_id, query_text, intent, candidate_count,
                 execution_time_ms, parameters, channels)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(record.run_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(&record.query)
        .bind(&record.intent)
        .bind(record.candidate_count as i32)
        .bind(record.execution_time_ms)
        .bind(&record.parameters)
        .bind(&channels)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn record_context_pack(
        &self,
        context: &RequestContext,
        pack_id: ContextPackId,
        retrieval_run_id: RetrievalRunId,
        workspace_id: WorkspaceId,
        token_budget: u32,
        used_tokens: u32,
        items: &serde_json::Value,
    ) -> Result<(), ApplicationError> {
        // Refused rather than written into the caller's scope: a pack recorded
        // under a workspace the caller is not in would attribute one tenant's
        // retrieval to another. This parameter used to be taken on trust, and
        // the unforced policy meant nothing downstream checked it either.
        if workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "a context pack cannot be journaled into another workspace".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO context_packs (id, retrieval_run_id, workspace_id, token_budget, used_tokens, items)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(pack_id.as_uuid())
        .bind(retrieval_run_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(token_budget as i32)
        .bind(used_tokens as i32)
        .bind(items)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }
}
