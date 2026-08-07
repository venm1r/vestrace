use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, PurgeRepository};
use vestrace_domain::{MemoryId, PrincipalId, WorkspaceId};

pub struct PgPurgeRepository {
    pool: PgPool,
}

impl PgPurgeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl PurgeRepository for PgPurgeRepository {
    async fn purge_memory(
        &self,
        workspace_id: WorkspaceId,
        memory_id: MemoryId,
        principal_id: PrincipalId,
        reason: &str,
        approval_id: &str,
    ) -> Result<(), ApplicationError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;

        sqlx::query("DELETE FROM knowledge_relations WHERE source_memory_id = $1 AND workspace_id = $2 OR target_memory_id = $1 AND workspace_id = $2")
            .bind(memory_id.as_uuid())
            .bind(workspace_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        sqlx::query("DELETE FROM memory_sources WHERE memory_id = $1 AND workspace_id = $2")
            .bind(memory_id.as_uuid())
            .bind(workspace_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        sqlx::query("DELETE FROM memory_revisions WHERE memory_id = $1 AND workspace_id = $2")
            .bind(memory_id.as_uuid())
            .bind(workspace_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        sqlx::query("DELETE FROM memories WHERE id = $1 AND workspace_id = $2")
            .bind(memory_id.as_uuid())
            .bind(workspace_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO purge_audits (id, workspace_id, principal_id, target_type, target_id, reason, purged_at)
            VALUES ($1, $2, $3, 'memory', $4, $5, NOW())
            "#,
        )
        .bind(uuid::Uuid::now_v7())
        .bind(workspace_id.as_uuid())
        .bind(principal_id.as_uuid())
        .bind(memory_id.as_uuid())
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;

        tx.commit().await.map_err(storage_error)?;

        let _ = approval_id;
        Ok(())
    }
}
