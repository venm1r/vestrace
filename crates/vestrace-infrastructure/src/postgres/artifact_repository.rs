use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    ApplicationError, ArtifactContent, ArtifactListing, ArtifactRepository, RequestContext,
    StoredArtifact,
};
use vestrace_domain::artifact::{Artifact, ArtifactRevision, ArtifactStatus};

use super::PgStore;

/// Reads and writes the artifact registry from migration 0042 and the content
/// storage added by migration 0134.
///
/// A revision may still pin content that is not held here — the registry
/// behaviour predates content storage and remains valid — so `fetch_content`
/// distinguishes "not stored" from "failed".
#[derive(Clone, Debug)]
pub struct PgArtifactRepository {
    store: PgStore,
}

impl PgArtifactRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn parse_status(value: &str) -> Result<ArtifactStatus, ApplicationError> {
    match value {
        "quarantined" => Ok(ArtifactStatus::Quarantined),
        "active" => Ok(ArtifactStatus::Active),
        "archived" => Ok(ArtifactStatus::Archived),
        "purged" => Ok(ArtifactStatus::Purged),
        other => Err(ApplicationError::Storage(format!(
            "unknown artifact status {other:?}"
        ))),
    }
}

#[async_trait]
impl ArtifactRepository for PgArtifactRepository {
    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<ArtifactListing>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The latest revision per artifact carries the facts a reader needs, so
        // it is joined here rather than left to N follow-up queries.
        let rows = sqlx::query(
            r#"
            SELECT a.id, a.name, a.status, a.created_at,
                   r.id AS revision_id, r.revision_number, r.media_type,
                   r.content_hash, r.byte_size, r.created_at AS revision_created_at
            FROM artifacts AS a
            LEFT JOIN LATERAL (
                SELECT id, revision_number, media_type, content_hash, byte_size, created_at
                FROM artifact_revisions
                WHERE artifact_id = a.id AND workspace_id = $1
                ORDER BY revision_number DESC
                LIMIT 1
            ) AS r ON TRUE
            WHERE a.workspace_id = $1
            ORDER BY a.created_at DESC, a.id DESC
            LIMIT $2
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let artifact = Artifact {
                    id: vestrace_domain::id::ArtifactId::from_uuid(
                        row.try_get("id").map_err(storage_error)?,
                    ),
                    workspace_id: context.workspace_id,
                    name: row.try_get("name").map_err(storage_error)?,
                    status: parse_status(
                        &row.try_get::<String, _>("status").map_err(storage_error)?,
                    )?,
                    created_at: row.try_get("created_at").map_err(storage_error)?,
                };
                let revision_id: Option<uuid::Uuid> =
                    row.try_get("revision_id").map_err(storage_error)?;
                let latest_revision = match revision_id {
                    None => None,
                    Some(revision_id) => {
                        let revision_number: i32 =
                            row.try_get("revision_number").map_err(storage_error)?;
                        let byte_size: i64 = row.try_get("byte_size").map_err(storage_error)?;
                        Some(ArtifactRevision {
                            id: vestrace_domain::id::ArtifactRevisionId::from_uuid(revision_id),
                            artifact_id: artifact.id,
                            workspace_id: context.workspace_id,
                            revision_number: u32::try_from(revision_number)
                                .map_err(storage_error)?,
                            media_type: row.try_get("media_type").map_err(storage_error)?,
                            content_hash: row.try_get("content_hash").map_err(storage_error)?,
                            byte_size: u64::try_from(byte_size).map_err(storage_error)?,
                            created_at: row
                                .try_get("revision_created_at")
                                .map_err(storage_error)?,
                        })
                    }
                };
                Ok(ArtifactListing {
                    artifact,
                    latest_revision,
                })
            })
            .collect()
    }

    async fn store(
        &self,
        context: &RequestContext,
        name: &str,
        content: &ArtifactContent,
    ) -> Result<StoredArtifact, ApplicationError> {
        let workspace_id = context.workspace_id.as_uuid();
        let content_hash = content.content_hash();
        let byte_size = i64::try_from(content.bytes.len()).map_err(storage_error)?;

        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // Content-addressed, so identical bytes collapse to one row. `DO
        // NOTHING` rather than `DO UPDATE`: if the digest matches, the stored
        // bytes are already the same bytes, and rewriting them would only risk
        // replacing good content with a differing payload under a colliding
        // hash.
        sqlx::query(
            r#"
            INSERT INTO artifact_blobs
                (workspace_id, content_hash, media_type, bytes, byte_size)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (workspace_id, content_hash) DO NOTHING
            "#,
        )
        .bind(workspace_id)
        .bind(&content_hash)
        .bind(&content.media_type)
        .bind(content.bytes.as_slice())
        .bind(byte_size)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        let artifact_id = uuid::Uuid::now_v7();
        // `active`, not the table default of `quarantined`: this artifact was
        // produced by the system itself in this transaction, not ingested from
        // an untrusted source that still needs review.
        sqlx::query(
            "INSERT INTO artifacts (id, workspace_id, name, status) VALUES ($1, $2, $3, 'active')",
        )
        .bind(artifact_id)
        .bind(workspace_id)
        .bind(name)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        let revision_id = uuid::Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO artifact_revisions
                (id, artifact_id, workspace_id, revision_number, media_type, content_hash, byte_size)
            VALUES ($1, $2, $3, 1, $4, $5, $6)
            "#,
        )
        .bind(revision_id)
        .bind(artifact_id)
        .bind(workspace_id)
        .bind(&content.media_type)
        .bind(&content_hash)
        .bind(byte_size)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;

        Ok(StoredArtifact {
            artifact_id: vestrace_domain::id::ArtifactId::from_uuid(artifact_id),
            revision_id: vestrace_domain::id::ArtifactRevisionId::from_uuid(revision_id),
            content_hash,
            byte_size: content.bytes.len() as u64,
        })
    }

    async fn fetch_content(
        &self,
        context: &RequestContext,
        content_hash: &str,
    ) -> Result<Option<ArtifactContent>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            "SELECT media_type, bytes FROM artifact_blobs WHERE workspace_id = $1 AND content_hash = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(content_hash)
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        row.map(|row| {
            Ok(ArtifactContent {
                media_type: row.try_get("media_type").map_err(storage_error)?,
                bytes: row.try_get("bytes").map_err(storage_error)?,
            })
        })
        .transpose()
    }
}
