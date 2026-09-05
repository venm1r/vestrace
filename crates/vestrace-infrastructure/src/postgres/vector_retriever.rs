use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::retrieval::{
    CorpusGenerationResolver, EmbeddingSpace, ResolvedCorpusGeneration,
};
use vestrace_application::{
    ApplicationError, NormalizedRetrievalRequest, RequestContext, SharedGovernedEmbeddingProvider,
    VectorRetriever,
};
use vestrace_domain::embedding::EmbeddingSpaceKey;
use vestrace_domain::{
    CorpusGenerationId, MemoryKind, MemoryStatus, RetrievalCandidate, id::MemoryId,
};

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

pub struct PgCorpusGenerationResolver {
    store: PgStore,
}

impl PgCorpusGenerationResolver {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
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
        if request.embedding_space_key.workspace_id() != context.workspace_id
            || request.embedding_space_key.name() != self.space_name
            || request.embedding_space_key.model() != self.provider.model()
        {
            return Err(ApplicationError::Policy(
                "vector retriever configured embedding space disagrees with retrieval request"
                    .to_owned(),
            ));
        }
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
            "SELECT space_id AS id, dimensions FROM embedding_space_registrations
             WHERE workspace_id = $1 AND name = $2 AND model = $3 AND dimensions = $4",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(request.embedding_space_key.name())
        .bind(request.embedding_space_key.model())
        .bind(
            i32::try_from(request.embedding_space_key.dimensions())
                .map_err(|error| ApplicationError::Storage(error.to_string()))?,
        )
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

        let generation_state = sqlx::query_scalar::<_, String>(
            "SELECT generation.state
               FROM embedding_corpus_generations generation
               JOIN embedding_space_registrations registration
                 ON registration.id = generation.space_registration_id
                AND registration.workspace_id = generation.workspace_id
              WHERE generation.workspace_id = $1
                AND generation.id = $2
                AND registration.name = $3
                AND registration.model = $4",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(request.corpus_generation_id.as_uuid())
        .bind(request.embedding_space_key.name())
        .bind(request.embedding_space_key.model())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;
        match generation_state.as_deref() {
            Some("ready") => {}
            Some("stale") => {
                return Err(ApplicationError::Unavailable(format!(
                    "embedding corpus generation {} for space {} is stale",
                    request.corpus_generation_id,
                    request.embedding_space_key.name()
                )));
            }
            Some(_) => {
                return Err(ApplicationError::Unavailable(format!(
                    "embedding corpus generation {} for space {} is not ready",
                    request.corpus_generation_id,
                    request.embedding_space_key.name()
                )));
            }
            None => {
                return Err(ApplicationError::Unavailable(format!(
                    "embedding corpus generation {} does not exist for space {}",
                    request.corpus_generation_id,
                    request.embedding_space_key.name()
                )));
            }
        }

        let rows = sqlx::query(
            "SELECT e.memory_id, m.kind AS memory_kind, m.status AS memory_status,
                    m.state_revision AS source_generation, r.id AS revision_id,
                    r.revision_number, r.content, r.valid_from, r.valid_until,
                    r.created_at AS revision_created_at,
                    member.corpus_generation_id AS corpus_generation_id,
                    e.embedding <=> $3::vector AS distance
             FROM memory_embeddings e
             INNER JOIN embedding_corpus_generation_members member ON member.memory_embedding_id = e.id AND member.workspace_id = e.workspace_id
             INNER JOIN memories m ON m.id = e.memory_id AND m.workspace_id = e.workspace_id
             INNER JOIN memory_revisions r
                     ON r.id = m.active_revision_id AND r.workspace_id = m.workspace_id
             WHERE e.workspace_id = $1
               AND e.space_id = $2
               AND member.corpus_generation_id = $6
               AND m.status = ANY($4)
               AND (cardinality($5::text[]) = 0 OR m.kind = ANY($5))
             ORDER BY distance ASC
             LIMIT $7",
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
        .bind(request.corpus_generation_id.as_uuid())
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
                    classification: None,
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
                    corpus_generation_id: CorpusGenerationId::from_uuid(
                        row.try_get("corpus_generation_id").map_err(storage_error)?,
                    ),
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

#[async_trait]
impl CorpusGenerationResolver for PgCorpusGenerationResolver {
    async fn resolve(
        &self,
        context: &vestrace_application::RequestContext,
        space_name: &str,
        model: &str,
    ) -> Result<ResolvedCorpusGeneration, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows = sqlx::query(
            "SELECT registration.name, registration.model, registration.dimensions, generation.id
               FROM embedding_space_registrations registration
               JOIN embedding_corpus_generations generation
                 ON generation.workspace_id = registration.workspace_id
                AND generation.space_registration_id = registration.id
              WHERE registration.workspace_id = $1 AND registration.name = $2
                AND registration.model = $3 AND generation.state = 'ready'",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(space_name)
        .bind(model)
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;
        if rows.is_empty() {
            return Err(ApplicationError::Unavailable(format!(
                "embedding space {space_name} has no Ready generation"
            )));
        }
        if rows.len() != 1 {
            return Err(ApplicationError::Unavailable(format!(
                "embedding space {space_name} has ambiguous Ready generations"
            )));
        }
        let row = &rows[0];
        let dimensions: i32 = row.try_get("dimensions").map_err(storage_error)?;
        Ok(ResolvedCorpusGeneration {
            embedding_space_key: EmbeddingSpaceKey::new(
                context.workspace_id,
                space_name,
                model,
                u32::try_from(dimensions)
                    .map_err(|error| ApplicationError::Storage(error.to_string()))?,
            )
            .map_err(|error| ApplicationError::Storage(error.to_string()))?,
            corpus_generation_id: CorpusGenerationId::from_uuid(
                row.try_get("id").map_err(storage_error)?,
            ),
        })
    }
}
