use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::retrieval::EmbeddingSpace;
use vestrace_application::{
    ApplicationError, NormalizedRetrievalRequest, RequestContext, SharedGovernedEmbeddingProvider,
    VectorRetriever,
};
use vestrace_domain::{MemoryKind, MemoryStatus, RetrievalCandidate, id::MemoryId};

use super::PgStore;

/// The vector retrieval channel.
///
/// # Why there was none
///
/// `RetrievalService::with_channels` has taken an optional vector retriever since
/// it was written, and nothing ever supplied one. So reciprocal rank fusion — the
/// step whose determinism has its own conformance case — has been fusing a single
/// channel with itself, and the "channel degraded" path could not occur because
/// there was only ever one channel to degrade.
///
/// # Distance, and why the score is inverted
///
/// pgvector's `<=>` is cosine *distance*: 0 is identical, 2 is opposite. A
/// retrieval candidate carries a *score*, where higher is better, so the
/// distance is turned into `1 - distance`. Storing the distance in a field named
/// score would leave every consumer — fusion, ranking, the context pack — sorting
/// the wrong way while looking correct.
///
/// ```compile_fail
/// use vestrace_application::retrieval::SharedEmbeddingProvider;
/// use vestrace_infrastructure::{PgStore, PgVectorRetriever};
///
/// let store: PgStore = todo!();
/// let raw: SharedEmbeddingProvider = todo!();
/// let _ = PgVectorRetriever::new(store, raw, "space");
/// ```
pub struct PgVectorRetriever {
    store: PgStore,
    provider: SharedGovernedEmbeddingProvider,
    space_name: String,
}

impl PgVectorRetriever {
    pub fn new(
        store: PgStore,
        provider: SharedGovernedEmbeddingProvider,
        space_name: impl Into<String>,
    ) -> Self {
        Self {
            store,
            provider,
            space_name: space_name.into(),
        }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

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

fn memory_kind_from_str(value: &str) -> Result<MemoryKind, ApplicationError> {
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
        other => Err(ApplicationError::Storage(format!(
            "stored memory kind '{other}' is not supported"
        ))),
    }
}

fn memory_status_from_str(value: &str) -> Result<MemoryStatus, ApplicationError> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "active" => Ok(MemoryStatus::Active),
        "superseded" => Ok(MemoryStatus::Superseded),
        "rejected" => Ok(MemoryStatus::Rejected),
        "expired" => Ok(MemoryStatus::Expired),
        "deleted" => Ok(MemoryStatus::Deleted),
        other => Err(ApplicationError::Storage(format!(
            "stored memory status '{other}' is not supported"
        ))),
    }
}

#[async_trait]
impl VectorRetriever for PgVectorRetriever {
    async fn search(
        &self,
        context: &RequestContext,
        request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
        // The query is embedded by the same provider that produced the stored
        // vectors. Comparing a query embedded by one model against vectors from
        // another gives distances that are arithmetically valid and meaningless.
        let embedded = self
            .provider
            .embed_retrieval_query(request.request_id, &request.query)
            .await?;
        let query_vector = embedded.into_iter().next().ok_or_else(|| {
            ApplicationError::Unavailable("the embedding provider returned nothing".into())
        })?;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let space = sqlx::query(
            "SELECT id, dimensions FROM embedding_spaces WHERE workspace_id = $1 AND name = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(&self.space_name)
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        let Some(space) = space else {
            // No space means nothing has been embedded in this workspace yet.
            // An empty result is the truthful answer; the caller records the
            // channel as consulted, which is how "nothing indexed" stays
            // distinguishable from "not configured".
            scoped.commit().await.map_err(storage_error)?;
            return Ok(Vec::new());
        };

        let space = EmbeddingSpace {
            id: vestrace_domain::id::EmbeddingSpaceId::from_uuid(
                space.try_get("id").map_err(storage_error)?,
            ),
            name: self.space_name.clone(),
            model: self.provider.model().to_string(),
            dimensions: u32::try_from(
                space
                    .try_get::<i32, _>("dimensions")
                    .map_err(storage_error)?,
            )
            .map_err(|_| ApplicationError::Storage("embedding space width is negative".into()))?,
        };

        if query_vector.len() != space.dimensions as usize {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "the configured embedding model produces {} dimensions while space {} holds \
                 {}; searching across the two would compare incomparable geometries",
                query_vector.len(),
                space.name,
                space.dimensions
            )));
        }

        let rows = sqlx::query(
            "SELECT e.memory_id, m.kind AS memory_kind, m.status AS memory_status,
                    m.state_revision AS source_generation, r.id AS revision_id,
                    r.revision_number, r.content, r.valid_from, r.valid_until,
                    r.created_at AS revision_created_at,
                    e.embedding <=> $3::vector AS distance
             FROM memory_embeddings e
             INNER JOIN memories m ON m.id = e.memory_id AND m.workspace_id = e.workspace_id
             INNER JOIN memory_revisions r
                     ON r.id = m.active_revision_id AND r.workspace_id = m.workspace_id
             WHERE e.workspace_id = $1
               AND e.space_id = $2
               AND m.status = ANY($4)
               AND (cardinality($5::text[]) = 0 OR m.kind = ANY($5))
             ORDER BY distance ASC
             LIMIT $6",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(space.id.as_uuid())
        .bind(vector_literal(&query_vector))
        .bind(
            request
                .allowed_statuses
                .iter()
                .map(|status| match status {
                    MemoryStatus::Active => "active",
                    MemoryStatus::Candidate => "candidate",
                    MemoryStatus::Superseded => "superseded",
                    MemoryStatus::Rejected => "rejected",
                    MemoryStatus::Expired => "expired",
                    MemoryStatus::Deleted => "deleted",
                })
                .collect::<Vec<&str>>(),
        )
        .bind(
            request
                .allowed_kinds
                .iter()
                .map(|kind| match kind {
                    MemoryKind::Fact => "fact",
                    MemoryKind::Preference => "preference",
                    MemoryKind::Constraint => "constraint",
                    MemoryKind::Decision => "decision",
                    MemoryKind::Task => "task",
                    MemoryKind::Procedure => "procedure",
                    MemoryKind::Observation => "observation",
                    MemoryKind::Outcome => "outcome",
                    MemoryKind::Summary => "summary",
                })
                .collect::<Vec<&str>>(),
        )
        .bind(i64::from(request.channel_limit))
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .enumerate()
            .map(|(index, row)| {
                let distance: f64 = row.try_get("distance").map_err(storage_error)?;
                Ok(RetrievalCandidate {
                    memory_id: MemoryId::from_uuid(
                        row.try_get("memory_id").map_err(storage_error)?,
                    ),
                    revision_id: vestrace_domain::id::MemoryRevisionId::from_uuid(
                        row.try_get("revision_id").map_err(storage_error)?,
                    ),
                    kind: memory_kind_from_str(
                        row.try_get::<String, _>("memory_kind")
                            .map_err(storage_error)?
                            .as_str(),
                    )?,
                    memory_status: memory_status_from_str(
                        row.try_get::<String, _>("memory_status")
                            .map_err(storage_error)?
                            .as_str(),
                    )?,
                    revision_number: u32::try_from(
                        row.try_get::<i32, _>("revision_number")
                            .map_err(storage_error)?,
                    )
                    .map_err(|error| ApplicationError::Storage(error.to_string()))?,
                    content: row.try_get("content").map_err(storage_error)?,
                    valid_from: row.try_get("valid_from").map_err(storage_error)?,
                    valid_until: row.try_get("valid_until").map_err(storage_error)?,
                    revision_created_at: row
                        .try_get("revision_created_at")
                        .map_err(storage_error)?,
                    source_generation: u32::try_from(
                        row.try_get::<i32, _>("source_generation")
                            .map_err(storage_error)?,
                    )
                    .map_err(|error| ApplicationError::Storage(error.to_string()))?,
                    // Cosine distance inverted into a score, so higher is better
                    // here as it is in every other channel.
                    score: (1.0 - distance) as f32,
                    channel_rank: u32::try_from(index + 1).unwrap_or(u32::MAX),
                    channel: "vector".to_string(),
                    explanation: format!("cosine distance {distance:.4} in space {}", space.name),
                    conflict_ids: Vec::new(),
                })
            })
            .collect()
    }
}
