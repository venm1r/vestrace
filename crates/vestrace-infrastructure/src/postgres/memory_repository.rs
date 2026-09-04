use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{ApplicationError, MemoryRepository, RequestContext};
use vestrace_domain::{
    Confidence, Importance, Memory, MemoryKind, MemoryRevision, MemorySource, MemoryStatus,
    StructuredMemory,
    id::{MemoryId, MemoryRevisionId, WorkspaceId},
};

use super::PgStore;

pub struct PgMemoryRepository {
    store: PgStore,
}

impl PgMemoryRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
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
    async fn save_memory_with_revision(
        &self,
        context: &RequestContext,
        memory: &Memory,
        revision: &MemoryRevision,
        source: &MemorySource,
    ) -> Result<(), ApplicationError> {
        for workspace in [
            memory.workspace_id,
            revision.workspace_id,
            source.workspace_id,
        ] {
            if workspace != context.workspace_id {
                return Err(ApplicationError::Policy(
                    "a memory, its revision and its source must all belong to the caller's \
                     workspace"
                        .into(),
                ));
            }
        }
        if revision.memory_id != memory.id || source.memory_id != memory.id {
            return Err(ApplicationError::Policy(
                "a memory's first revision and source must belong to that memory".into(),
            ));
        }
        revision
            .validate_temporal_range()
            .map_err(ApplicationError::from)?;
        MemoryRevision::validate_classification_shape(revision.classification.as_deref())
            .map_err(ApplicationError::from)?;

        // One transaction for all three.
        //
        // `tr_active_memory_has_source` is `DEFERRABLE INITIALLY DEFERRED`, so
        // it evaluates at commit. Written separately, the memory commits alone
        // and the deferred check runs against a database that has no source
        // yet — which is why creating a memory had never succeeded. Inside one
        // transaction the order stops mattering, which is what deferring the
        // trigger was for.
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        if revision.revision_number == 1 {
            sqlx::query(
                r#"
                INSERT INTO memories
                    (id, workspace_id, kind, status, active_revision_id, state_revision, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (id) DO UPDATE SET
                    status = EXCLUDED.status,
                    active_revision_id = EXCLUDED.active_revision_id,
                    state_revision = EXCLUDED.state_revision,
                    updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(memory.id.as_uuid())
            .bind(memory.workspace_id.as_uuid())
            .bind(super::memory_encoding::kind_str(memory.kind))
            .bind(super::memory_encoding::status_str(memory.status))
            .bind(memory.active_revision_id.map(|r| r.as_uuid()))
            .bind(memory.state_revision as i32)
            .bind(memory.created_at)
            .bind(memory.updated_at)
            .execute(scoped.connection())
            .await
            .map_err(storage_error)?;
        } else {
            let expected_revision = revision.revision_number - 1;
            let expected_state_revision =
                memory.state_revision.checked_sub(1).ok_or_else(|| {
                    ApplicationError::Storage(
                        "memories.state_revision cannot precede a revision compare-and-set".into(),
                    )
                })?;
            let updated = sqlx::query(
                r#"
                UPDATE memories AS memory
                SET status = $3,
                    active_revision_id = $4,
                    state_revision = $5,
                    updated_at = $6
                WHERE memory.id = $1
                  AND memory.workspace_id = $2
                  AND memory.state_revision = $7
                  AND EXISTS (
                      SELECT 1
                      FROM memory_revisions AS active
                      WHERE active.id = memory.active_revision_id
                        AND active.workspace_id = memory.workspace_id
                        AND active.revision_number = $8
                  )
                "#,
            )
            .bind(memory.id.as_uuid())
            .bind(memory.workspace_id.as_uuid())
            .bind(super::memory_encoding::status_str(memory.status))
            .bind(memory.active_revision_id.map(|r| r.as_uuid()))
            .bind(memory.state_revision as i32)
            .bind(memory.updated_at)
            .bind(expected_state_revision as i32)
            .bind(expected_revision as i32)
            .execute(scoped.connection())
            .await
            .map_err(storage_error)?;

            if updated.rows_affected() != 1 {
                let current: Option<i32> = sqlx::query_scalar(
                    r#"
                    SELECT active.revision_number
                    FROM memories AS memory
                    JOIN memory_revisions AS active ON active.id = memory.active_revision_id
                    WHERE memory.id = $1 AND memory.workspace_id = $2
                    "#,
                )
                .bind(memory.id.as_uuid())
                .bind(memory.workspace_id.as_uuid())
                .fetch_optional(scoped.connection())
                .await
                .map_err(storage_error)?;
                let current = current.ok_or_else(|| {
                    ApplicationError::Storage(format!(
                        "memories.active_revision_id for memory {} disappeared during revision \
                         compare-and-set",
                        memory.id
                    ))
                })?;
                return Err(ApplicationError::from(
                    vestrace_domain::DomainError::RevisionConflict {
                        expected: u64::from(expected_revision),
                        current: u64::try_from(current).map_err(|_| {
                            ApplicationError::Storage(format!(
                                "memory_revisions.revision_number {current} is negative"
                            ))
                        })?,
                    },
                ));
            }
        }

        sqlx::query(
            r#"
            INSERT INTO memory_revisions
                (id, memory_id, workspace_id, revision_number, content, structured,
                 confidence, importance, created_at, valid_from, valid_until,
                 change_reason, canonical_hash, classification)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
        )
        .bind(revision.id.as_uuid())
        .bind(revision.memory_id.as_uuid())
        .bind(revision.workspace_id.as_uuid())
        .bind(revision.revision_number as i32)
        .bind(&revision.content)
        .bind(
            revision
                .structured
                .as_ref()
                .map(|s| serde_json::to_value(s).unwrap_or_default()),
        )
        .bind(revision.confidence.value())
        .bind(revision.importance.value())
        .bind(revision.created_at)
        .bind(revision.valid_from)
        .bind(revision.valid_until)
        .bind(&revision.change_reason)
        .bind(&revision.canonical_hash)
        .bind(&revision.classification)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        let evidence_ref_json = source
            .evidence_ref
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO memory_sources (id, memory_id, workspace_id, event_id, role, derivation_id, created_at, evidence_ref)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(source.id.as_uuid())
        .bind(source.memory_id.as_uuid())
        .bind(source.workspace_id.as_uuid())
        .bind(source.event_id.as_uuid())
        .bind(super::memory_encoding::evidence_role_str(source.role))
        .bind(source.derivation_id.map(|d| d.as_uuid()))
        .bind(source.created_at)
        .bind(evidence_ref_json)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        // The search index is a projection of the active revision, so it moves
        // in this transaction rather than after it. `search_documents` is the
        // only table the text channel reads, and nothing had ever written it —
        // every retrieval returned nothing, successfully.
        sqlx::query(
            r#"
            INSERT INTO search_documents (id, memory_id, workspace_id, title, content, created_at, updated_at)
            VALUES ($1, $2, $3, '', $4, $5, $5)
            ON CONFLICT (workspace_id, memory_id) DO UPDATE SET
                content = EXCLUDED.content,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(uuid::Uuid::now_v7())
        .bind(memory.id.as_uuid())
        .bind(memory.workspace_id.as_uuid())
        .bind(&revision.content)
        .bind(revision.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        // Both deferred checks run here, with the revision and the source
        // present.
        scoped.commit().await.map_err(storage_error)
    }

    async fn save_memory(
        &self,
        context: &RequestContext,
        memory: &Memory,
    ) -> Result<(), ApplicationError> {
        if memory.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "a memory cannot be written into another workspace".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let status_str = super::memory_encoding::status_str(memory.status);
        let kind_str = super::memory_encoding::kind_str(memory.kind);

        sqlx::query(
            r#"
            INSERT INTO memories
                (id, workspace_id, kind, status, active_revision_id, state_revision, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                active_revision_id = EXCLUDED.active_revision_id,
                state_revision = EXCLUDED.state_revision,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(memory.id.as_uuid())
        .bind(memory.workspace_id.as_uuid())
        .bind(kind_str)
        .bind(status_str)
        .bind(memory.active_revision_id.map(|r| r.as_uuid()))
        .bind(memory.state_revision as i32)
        .bind(memory.created_at)
        .bind(memory.updated_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn save_revision(
        &self,
        context: &RequestContext,
        revision: &MemoryRevision,
    ) -> Result<(), ApplicationError> {
        if revision.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "a memory revision cannot be written into another workspace".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        revision
            .validate_temporal_range()
            .map_err(ApplicationError::from)?;
        MemoryRevision::validate_classification_shape(revision.classification.as_deref())
            .map_err(ApplicationError::from)?;

        sqlx::query(
            r#"
            INSERT INTO memory_revisions
                (id, memory_id, workspace_id, revision_number, content, structured,
                 confidence, importance, created_at, valid_from, valid_until,
                 change_reason, canonical_hash, classification)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
        )
        .bind(revision.id.as_uuid())
        .bind(revision.memory_id.as_uuid())
        .bind(revision.workspace_id.as_uuid())
        .bind(revision.revision_number as i32)
        .bind(&revision.content)
        .bind(
            revision
                .structured
                .as_ref()
                .map(|s| serde_json::to_value(s).unwrap_or_default()),
        )
        .bind(revision.confidence.value())
        .bind(revision.importance.value())
        .bind(revision.created_at)
        .bind(revision.valid_from)
        .bind(revision.valid_until)
        .bind(&revision.change_reason)
        .bind(&revision.canonical_hash)
        .bind(&revision.classification)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn find_memory_by_id(
        &self,
        context: &RequestContext,
        id: MemoryId,
    ) -> Result<Option<Memory>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The workspace predicate is new. This selected on `id` alone, so a
        // memory id from any tenant returned that tenant's memory.
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, kind, status, active_revision_id, state_revision,
                   created_at, updated_at
            FROM memories
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        row.map(parse_memory_row).transpose()
    }

    async fn find_revision_by_id(
        &self,
        context: &RequestContext,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT id, memory_id, workspace_id, revision_number, content, structured,
                   confidence, importance, created_at, valid_from, valid_until,
                   change_reason, canonical_hash, classification
            FROM memory_revisions
            WHERE workspace_id = $1 AND id = $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

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
        state_revision: row.try_get::<i32, _>("state_revision").unwrap_or(0) as u32,
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

    let revision = MemoryRevision {
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
        valid_from: row.try_get("valid_from").ok(),
        valid_until: row.try_get("valid_until").ok(),
        change_reason: row.try_get("change_reason").ok(),
        canonical_hash: row.try_get("canonical_hash").ok(),
        classification: row.try_get("classification").map_err(storage_error)?,
    };
    MemoryRevision::validate_classification_shape(revision.classification.as_deref()).map_err(
        |error| {
            ApplicationError::Storage(format!(
                "stored memory_revisions.classification is invalid: {error}"
            ))
        },
    )?;
    Ok(revision)
}

#[cfg(test)]
mod tests {
    use super::parse_revision_row;

    #[sqlx::test]
    async fn a_type_mismatched_classification_column_fails_closed(pool: sqlx::PgPool) {
        let row = sqlx::query(
            r#"
            SELECT '10000000-0000-0000-0000-000000000001'::uuid AS id,
                   '10000000-0000-0000-0000-000000000002'::uuid AS memory_id,
                   '10000000-0000-0000-0000-000000000003'::uuid AS workspace_id,
                   1::integer AS revision_number,
                   'content'::text AS content,
                   NULL::jsonb AS structured,
                   0.9::real AS confidence,
                   0.7::real AS importance,
                   NOW() AS created_at,
                   NULL::timestamptz AS valid_from,
                   NULL::timestamptz AS valid_until,
                   NULL::text AS change_reason,
                   NULL::text AS canonical_hash,
                   42::integer AS classification
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let message = parse_revision_row(row).unwrap_err().to_string();

        assert!(message.contains("classification"), "{message}");
    }

    #[sqlx::test]
    async fn a_blank_or_untrimmed_classification_column_fails_closed(pool: sqlx::PgPool) {
        for classification in ["   ", " internal", "internal "] {
            let row = sqlx::query(
                r#"
                SELECT '10000000-0000-0000-0000-000000000001'::uuid AS id,
                       '10000000-0000-0000-0000-000000000002'::uuid AS memory_id,
                       '10000000-0000-0000-0000-000000000003'::uuid AS workspace_id,
                       1::integer AS revision_number,
                       'content'::text AS content,
                       NULL::jsonb AS structured,
                       0.9::real AS confidence,
                       0.7::real AS importance,
                       NOW() AS created_at,
                       NULL::timestamptz AS valid_from,
                       NULL::timestamptz AS valid_until,
                       NULL::text AS change_reason,
                       NULL::text AS canonical_hash,
                       $1::text AS classification
                "#,
            )
            .bind(classification)
            .fetch_one(&pool)
            .await
            .unwrap();

            let message = parse_revision_row(row).unwrap_err().to_string();
            assert!(
                message.contains("memory_revisions.classification"),
                "{message}"
            );
        }
    }
}
