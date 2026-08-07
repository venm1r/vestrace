use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, RequestContext, RetrievalJournal};
use vestrace_domain::{
    WorkspaceId,
    id::{ContextPackId, RetrievalRunId},
};

pub struct PgRetrievalJournal {
    pool: PgPool,
}

impl PgRetrievalJournal {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RetrievalJournal for PgRetrievalJournal {
    async fn record_run(
        &self,
        context: &RequestContext,
        run_id: RetrievalRunId,
        query: &str,
        intent: &str,
        candidate_count: usize,
        execution_time_ms: i32,
    ) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO retrieval_runs (id, workspace_id, query_text, intent, candidate_count, execution_time_ms)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(run_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(query)
        .bind(intent)
        .bind(candidate_count as i32)
        .bind(execution_time_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn record_context_pack(
        &self,
        _context: &RequestContext,
        pack_id: ContextPackId,
        retrieval_run_id: RetrievalRunId,
        workspace_id: WorkspaceId,
        token_budget: u32,
        used_tokens: u32,
        items: &serde_json::Value,
    ) -> Result<(), ApplicationError> {
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
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }
}
