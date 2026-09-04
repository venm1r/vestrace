use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::retrieval::{EmbeddingSpace, EmbeddingStore, PendingEmbedding};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::MemoryRevision;
use vestrace_domain::id::{EmbeddingSpaceId, MemoryId};

use super::PgStore;

/// Embeddings, and the spaces that give them meaning.
///
/// # Why the width is checked here
///
/// Migration 0148 made `memory_embeddings.embedding` dimension-free, because the
/// column previously insisted on 1536 while `embedding_spaces` already carried a
/// `dimensions` field — two statements about the same thing, and the column won,
/// which excluded every model that does not emit exactly that width.
///
/// The constraint moved rather than vanished: a vector is checked against its
/// space's declared width before it is stored. Two widths in one space would
/// make distances between them arithmetic nonsense rather than an error.
pub struct PgEmbeddingStore {
    store: PgStore,
}

impl PgEmbeddingStore {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

/// pgvector accepts its literal form as `[1,2,3]`.
fn vector_literal(values: &[f32]) -> String {
    let mut out = String::with_capacity(values.len() * 8 + 2);
    out.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

#[async_trait]
impl EmbeddingStore for PgEmbeddingStore {
    async fn ensure_space(
        &self,
        context: &RequestContext,
        name: &str,
        model: &str,
        dimensions: u32,
    ) -> Result<EmbeddingSpace, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let existing = sqlx::query(
            "SELECT id, name, model, dimensions FROM embedding_spaces
             WHERE workspace_id = $1 AND name = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(name)
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        let space = match existing {
            Some(row) => {
                let stored_model: String = row.try_get("model").map_err(storage_error)?;
                let stored_dimensions: i32 = row.try_get("dimensions").map_err(storage_error)?;
                // A space whose model changed is not the same space. Silently
                // writing new vectors beside old ones would leave one index
                // holding two incomparable geometries.
                if stored_model != model || stored_dimensions != dimensions as i32 {
                    return Err(ApplicationError::Conflict(format!(
                        "embedding space {name} holds {stored_model} at {stored_dimensions} \
                         dimensions and cannot accept {model} at {dimensions}; create a new \
                         space instead"
                    )));
                }
                EmbeddingSpace {
                    id: EmbeddingSpaceId::from_uuid(row.try_get("id").map_err(storage_error)?),
                    name: row.try_get("name").map_err(storage_error)?,
                    model: stored_model,
                    dimensions,
                }
            }
            None => {
                let id = EmbeddingSpaceId::new();
                sqlx::query(
                    "INSERT INTO embedding_spaces (id, workspace_id, name, dimensions, model)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(id.as_uuid())
                .bind(context.workspace_id.as_uuid())
                .bind(name)
                .bind(dimensions as i32)
                .bind(model)
                .execute(scoped.connection())
                .await
                .map_err(storage_error)?;

                EmbeddingSpace {
                    id,
                    name: name.to_string(),
                    model: model.to_string(),
                    dimensions,
                }
            }
        };

        scoped.commit().await.map_err(storage_error)?;
        Ok(space)
    }

    async fn memories_without_embedding(
        &self,
        context: &RequestContext,
        space: &EmbeddingSpace,
        limit: u32,
    ) -> Result<Vec<PendingEmbedding>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The text embedded is the active revision's content, which is what the
        // search document holds too — one memory, one meaning, whichever channel
        // is asked.
        let rows = sqlx::query(
            "SELECT m.id AS memory_id, r.content, r.classification
             FROM memories m
             JOIN memory_revisions r ON r.id = m.active_revision_id
             LEFT JOIN memory_embeddings e
               ON e.memory_id = m.id
              AND e.workspace_id = m.workspace_id
              AND e.space_id = $2
             WHERE m.workspace_id = $1
               AND m.status = 'active'
               AND e.id IS NULL
             ORDER BY m.created_at ASC
             LIMIT $3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(space.id.as_uuid())
        .bind(i64::from(limit))
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let classification: Option<String> =
                    row.try_get("classification").map_err(storage_error)?;
                MemoryRevision::validate_classification_shape(classification.as_deref()).map_err(
                    |error| {
                        ApplicationError::Storage(format!(
                            "stored memory_revisions.classification is invalid: {error}"
                        ))
                    },
                )?;
                Ok(PendingEmbedding {
                    memory_id: MemoryId::from_uuid(
                        row.try_get("memory_id").map_err(storage_error)?,
                    ),
                    content: row.try_get("content").map_err(storage_error)?,
                    classification,
                })
            })
            .collect()
    }

    async fn upsert(
        &self,
        context: &RequestContext,
        space: &EmbeddingSpace,
        memory_id: MemoryId,
        embedding: &[f32],
    ) -> Result<(), ApplicationError> {
        if embedding.len() != space.dimensions as usize {
            return Err(ApplicationError::Policy(format!(
                "a {}-dimension vector cannot be stored in space {} which holds {}",
                embedding.len(),
                space.name,
                space.dimensions
            )));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            "INSERT INTO memory_embeddings (id, memory_id, workspace_id, space_id, embedding)
             VALUES ($1, $2, $3, $4, $5::vector)
             ON CONFLICT (workspace_id, memory_id, space_id) DO UPDATE SET
                 embedding = EXCLUDED.embedding",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(space.id.as_uuid())
        .bind(vector_literal(embedding))
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn missing_count(
        &self,
        context: &RequestContext,
        space: &EmbeddingSpace,
    ) -> Result<i64, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            "SELECT count(*) AS missing
             FROM memories m
             LEFT JOIN memory_embeddings e
               ON e.memory_id = m.id
              AND e.workspace_id = m.workspace_id
              AND e.space_id = $2
             WHERE m.workspace_id = $1 AND m.status = 'active' AND e.id IS NULL",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(space.id.as_uuid())
        .fetch_one(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;
        row.try_get("missing").map_err(storage_error)
    }
}
