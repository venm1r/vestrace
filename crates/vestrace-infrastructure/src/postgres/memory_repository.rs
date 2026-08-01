use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::{ApplicationError, MemoryRepository};
use vestrace_domain::{id::*, Memory, MemoryRevision};

pub struct PgMemoryRepository {
    pool: PgPool,
}

impl PgMemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MemoryRepository for PgMemoryRepository {
    async fn save_memory(&mut self, memory: &Memory) -> Result<(), ApplicationError> {
        let status_str = match memory.status {
            vestrace_domain::MemoryStatus::Candidate => "candidate",
            vestrace_domain::MemoryStatus::Active => "active",
            vestrace_domain::MemoryStatus::Superseded => "superseded",
            vestrace_domain::MemoryStatus::Rejected => "rejected",
            vestrace_domain::MemoryStatus::Expired => "expired",
            vestrace_domain::MemoryStatus::Deleted => "deleted",
        };

        let kind_str = match memory.kind {
            vestrace_domain::MemoryKind::Fact => "fact",
            vestrace_domain::MemoryKind::Preference => "preference",
            vestrace_domain::MemoryKind::Constraint => "constraint",
            vestrace_domain::MemoryKind::Decision => "decision",
            vestrace_domain::MemoryKind::Task => "task",
            vestrace_domain::MemoryKind::Procedure => "procedure",
            vestrace_domain::MemoryKind::Observation => "observation",
            vestrace_domain::MemoryKind::Outcome => "outcome",
            vestrace_domain::MemoryKind::Summary => "summary",
        };

        sqlx::query!(
            r#"
            INSERT INTO memories (id, workspace_id, kind, status, active_revision_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                active_revision_id = EXCLUDED.active_revision_id,
                updated_at = EXCLUDED.updated_at
            "#,
            memory.id.as_uuid(),
            memory.workspace_id.as_uuid(),
            kind_str,
            status_str,
            memory.active_revision_id.map(|r| r.as_uuid()),
            memory.created_at.as_datetime(),
            memory.updated_at.as_datetime()
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::StorageFailure(e.to_string()))?;

        Ok(())
    }

    async fn save_revision(&mut self, revision: &MemoryRevision) -> Result<(), ApplicationError> {
        sqlx::query!(
            r#"
            INSERT INTO memory_revisions (id, memory_id, workspace_id, revision_number, content, structured, confidence, importance, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            revision.id.as_uuid(),
            revision.memory_id.as_uuid(),
            revision.workspace_id.as_uuid(),
            revision.revision_number as i32,
            revision.content,
            revision.structured.as_ref().map(|s| serde_json::to_value(s).unwrap_or_default()),
            revision.confidence.value(),
            revision.importance.value(),
            revision.created_at.as_datetime()
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::StorageFailure(e.to_string()))?;

        Ok(())
    }

    async fn find_memory_by_id(&self, _id: MemoryId) -> Result<Option<Memory>, ApplicationError> {
        Ok(None)
    }

    async fn find_revision_by_id(&self, _id: MemoryRevisionId) -> Result<Option<MemoryRevision>, ApplicationError> {
        Ok(None)
    }
}
