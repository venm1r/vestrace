use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, MemoryRepository};
use vestrace_domain::{
    Confidence, Importance, Memory, MemoryKind, MemoryRevision, MemoryStatus, StructuredMemory,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
};

pub struct PgMemoryRepository {
    pool: PgPool,
}

impl PgMemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn kind_from_str(value: &str) -> Result<MemoryKind, ApplicationError> {
    match value {
        "fact" => Ok(MemoryKind::Fact),
        "preference" => Ok(MemoryKind::Preference),
        "constraint" => Ok(MemoryKind::Constraint),
        "decision" => Ok(MemoryKind::Decision),
        "task" => Ok(MemoryKind::Task),
        "procedure" => Ok(MemoryKind::Procedure),
        "observation" => Ok(MemoryKind::Observation),
        "outcome" => Ok(MemoryKind::Outcome),
        "summary" => Ok(MemoryKind::Summary),
        _ => Err(ApplicationError::Storage(format!(
            "stored memory kind '{value}' is not supported"
        ))),
    }
}

fn status_from_str(value: &str) -> Result<MemoryStatus, ApplicationError> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "active" => Ok(MemoryStatus::Active),
        "superseded" => Ok(MemoryStatus::Superseded),
        "rejected" => Ok(MemoryStatus::Rejected),
        "expired" => Ok(MemoryStatus::Expired),
        "deleted" => Ok(MemoryStatus::Deleted),
        _ => Err(ApplicationError::Storage(format!(
            "stored memory status '{value}' is not supported"
        ))),
    }
}

#[async_trait]
impl MemoryRepository for PgMemoryRepository {
    async fn save_memory(&self, memory: &Memory) -> Result<(), ApplicationError> {
        let status_str = match memory.status {
            MemoryStatus::Candidate => "candidate",
            MemoryStatus::Active => "active",
            MemoryStatus::Superseded => "superseded",
            MemoryStatus::Rejected => "rejected",
            MemoryStatus::Expired => "expired",
            MemoryStatus::Deleted => "deleted",
        };

        let kind_str = match memory.kind {
            MemoryKind::Fact => "fact",
            MemoryKind::Preference => "preference",
            MemoryKind::Constraint => "constraint",
            MemoryKind::Decision => "decision",
            MemoryKind::Task => "task",
            MemoryKind::Procedure => "procedure",
            MemoryKind::Observation => "observation",
            MemoryKind::Outcome => "outcome",
            MemoryKind::Summary => "summary",
        };

        sqlx::query(
            r#"
            INSERT INTO memories (id, workspace_id, kind, status, active_revision_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                active_revision_id = EXCLUDED.active_revision_id,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(memory.id.as_uuid())
        .bind(memory.workspace_id.as_uuid())
        .bind(kind_str)
        .bind(status_str)
        .bind(memory.active_revision_id.map(|r| r.as_uuid()))
        .bind(memory.created_at)
        .bind(memory.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn save_revision(&self, revision: &MemoryRevision) -> Result<(), ApplicationError> {
        sqlx::query(
            r#"
            INSERT INTO memory_revisions (id, memory_id, workspace_id, revision_number, content, structured, confidence, importance, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(revision.id.as_uuid())
        .bind(revision.memory_id.as_uuid())
        .bind(revision.workspace_id.as_uuid())
        .bind(revision.revision_number as i32)
        .bind(&revision.content)
        .bind(revision.structured.as_ref().map(|s| serde_json::to_value(s).unwrap_or_default()))
        .bind(revision.confidence.value())
        .bind(revision.importance.value())
        .bind(revision.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn find_memory_by_id(&self, id: MemoryId) -> Result<Option<Memory>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, kind, status, active_revision_id, created_at, updated_at
            FROM memories
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(parse_memory_row).transpose()
    }

    async fn find_revision_by_id(
        &self,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT id, memory_id, workspace_id, revision_number, content, structured, confidence, importance, created_at
            FROM memory_revisions
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(parse_revision_row).transpose()
    }
}

fn parse_memory_row(row: sqlx::postgres::PgRow) -> Result<Memory, ApplicationError> {
    let id: uuid::Uuid = row.try_get("id").map_err(storage_error)?;
    let workspace_id: uuid::Uuid = row.try_get("workspace_id").map_err(storage_error)?;
    let kind_str: String = row.try_get("kind").map_err(storage_error)?;
    let status_str: String = row.try_get("status").map_err(storage_error)?;
    let active_revision_id: Option<uuid::Uuid> =
        row.try_get("active_revision_id").map_err(storage_error)?;
    let created_at = row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        .map_err(storage_error)?;
    let updated_at = row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
        .map_err(storage_error)?;

    Ok(Memory {
        id: MemoryId::from_uuid(id),
        workspace_id: WorkspaceId::from_uuid(workspace_id),
        kind: kind_from_str(&kind_str)?,
        status: status_from_str(&status_str)?,
        active_revision_id: active_revision_id.map(MemoryRevisionId::from_uuid),
        created_at,
        updated_at,
    })
}

fn parse_revision_row(row: sqlx::postgres::PgRow) -> Result<MemoryRevision, ApplicationError> {
    let id: uuid::Uuid = row.try_get("id").map_err(storage_error)?;
    let memory_id: uuid::Uuid = row.try_get("memory_id").map_err(storage_error)?;
    let workspace_id: uuid::Uuid = row.try_get("workspace_id").map_err(storage_error)?;
    let revision_number: i32 = row.try_get("revision_number").map_err(storage_error)?;
    let content: String = row.try_get("content").map_err(storage_error)?;
    let structured_value: Option<serde_json::Value> =
        row.try_get("structured").map_err(storage_error)?;
    let confidence_f32: f32 = row.try_get("confidence").map_err(storage_error)?;
    let importance_f32: f32 = row.try_get("importance").map_err(storage_error)?;
    let created_at = row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        .map_err(storage_error)?;

    let structured = structured_value
        .map(serde_json::from_value::<StructuredMemory>)
        .transpose()
        .map_err(storage_error)?;

    Ok(MemoryRevision {
        id: MemoryRevisionId::from_uuid(id),
        memory_id: MemoryId::from_uuid(memory_id),
        workspace_id: WorkspaceId::from_uuid(workspace_id),
        revision_number: u32::try_from(revision_number)
            .map_err(|e| ApplicationError::Storage(e.to_string()))?,
        content,
        structured,
        confidence: Confidence::new(confidence_f32)
            .map_err(|e| ApplicationError::Storage(e.to_string()))?,
        importance: Importance::new(importance_f32)
            .map_err(|e| ApplicationError::Storage(e.to_string()))?,
        created_at,
    })
}
