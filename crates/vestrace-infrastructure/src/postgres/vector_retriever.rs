use super::PgStore;
use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::retrieval::{CorpusGenerationResolver, ResolvedCorpusGeneration};
use vestrace_application::{
    ApplicationError, NormalizedRetrievalRequest, RequestContext, SharedGovernedEmbeddingProvider,
    VectorRetriever,
};
use vestrace_domain::embedding::{
    CanonicalEmbeddingSpace, CanonicalGenerationSnapshot, EmbeddingSpaceKey,
};
use vestrace_domain::{
    CorpusGenerationId, ModelQualificationRevisionId, ModelRevisionId, RetrievalCandidate,
};

/// The retired plaintext vector channel fails before provider or database work.
///
/// ```compile_fail
/// use vestrace_application::retrieval::SharedEmbeddingProvider;
/// use vestrace_infrastructure::{PgStore, PgVectorRetriever};
/// let store: PgStore = todo!();
/// let raw: SharedEmbeddingProvider = todo!();
/// let _ = PgVectorRetriever::new(store, raw, "space");
/// ```
pub struct PgVectorRetriever;
impl PgVectorRetriever {
    pub fn new(
        _store: PgStore,
        _provider: SharedGovernedEmbeddingProvider,
        _space_name: impl Into<String>,
    ) -> Self {
        Self
    }
}
#[async_trait]
impl VectorRetriever for PgVectorRetriever {
    async fn search(
        &self,
        _context: &RequestContext,
        _request: &NormalizedRetrievalRequest,
    ) -> Result<Vec<RetrievalCandidate>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "embedding-legacy-adoption-required".into(),
        ))
    }
}

pub struct PgCorpusGenerationResolver {
    store: PgStore,
}
impl PgCorpusGenerationResolver {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}
fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl CorpusGenerationResolver for PgCorpusGenerationResolver {
    async fn resolve(
        &self,
        context: &RequestContext,
        space_name: &str,
        model: &str,
    ) -> Result<ResolvedCorpusGeneration, ApplicationError> {
        let snapshot = self.resolve_canonical(context, space_name, model).await?;
        Ok(ResolvedCorpusGeneration {
            embedding_space_key: snapshot.space,
            corpus_generation_id: snapshot.generation_id,
        })
    }
    async fn resolve_canonical(
        &self,
        context: &RequestContext,
        space_name: &str,
        model: &str,
    ) -> Result<CanonicalGenerationSnapshot, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let rows=sqlx::query("SELECT s.name,s.dimensions,s.model_revision_id,s.model_qualification_revision_id,s.adapter_profile_revision,s.request_shape_revision_id,s.returned_model,s.encoding_format,g.id,g.generation_epoch,guard.guard_version,g.corpus_revision,g.built_through_projection_ordinal,g.member_count FROM model_qualification_heads h JOIN embedding_space_registrations s ON s.workspace_id=h.workspace_id AND s.id=h.active_space_registration_id AND s.model_revision_id=h.model_revision_id AND s.model_qualification_revision_id=h.current_qualification_revision_id JOIN embedding_index_generation_guards guard ON guard.workspace_id=s.workspace_id AND guard.space_registration_id=s.id JOIN embedding_corpus_generations g ON g.workspace_id=guard.workspace_id AND g.id=guard.current_generation_id AND g.space_registration_id=s.id JOIN embedding_space_corpus_states c ON c.workspace_id=s.workspace_id AND c.space_registration_id=s.id WHERE s.workspace_id=$1 AND s.name=$2 AND s.returned_model=$3 AND s.registration_kind='canonical' AND g.member_representation='encrypted_projection' AND g.state='ready' AND g.generation_epoch=guard.generation_epoch AND g.captured_guard_version+1=guard.guard_version AND g.corpus_revision=c.corpus_revision AND g.member_count=c.live_member_count")
            .bind(context.workspace_id.as_uuid()).bind(space_name).bind(model).fetch_all(scoped.connection()).await.map_err(storage_error)?;
        if rows.len() != 1 {
            let active_canonical:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM model_qualification_heads h JOIN embedding_space_registrations s ON s.workspace_id=h.workspace_id AND s.id=h.active_space_registration_id AND s.model_revision_id=h.model_revision_id AND s.model_qualification_revision_id=h.current_qualification_revision_id WHERE s.workspace_id=$1 AND s.name=$2 AND s.returned_model=$3 AND s.registration_kind='canonical')")
                .bind(context.workspace_id.as_uuid()).bind(space_name).bind(model).fetch_one(scoped.connection()).await.map_err(storage_error)?;
            let legacy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM embedding_space_registrations WHERE workspace_id=$1 AND name=$2 AND model=$3 AND registration_kind='legacy_upgrade')")
                .bind(context.workspace_id.as_uuid()).bind(space_name).bind(model).fetch_one(scoped.connection()).await.map_err(storage_error)?;
            scoped.commit().await.map_err(storage_error)?;
            return Err(ApplicationError::Unavailable(
                if legacy && !active_canonical {
                    "embedding-legacy-adoption-required"
                } else {
                    "embedding-canonical-generation-required"
                }
                .into(),
            ));
        }
        let row = &rows[0];
        let dimensions: i32 = row.try_get("dimensions").map_err(storage_error)?;
        let space = EmbeddingSpaceKey::canonical(
            context.workspace_id,
            space_name,
            CanonicalEmbeddingSpace {
                model_revision_id: ModelRevisionId::from_uuid(
                    row.try_get("model_revision_id").map_err(storage_error)?,
                ),
                model_qualification_revision_id: ModelQualificationRevisionId::from_uuid(
                    row.try_get("model_qualification_revision_id")
                        .map_err(storage_error)?,
                ),
                adapter_profile_revision: row
                    .try_get("adapter_profile_revision")
                    .map_err(storage_error)?,
                request_shape_revision_id: row
                    .try_get("request_shape_revision_id")
                    .map_err(storage_error)?,
                returned_model: row.try_get("returned_model").map_err(storage_error)?,
                encoding_format: row.try_get("encoding_format").map_err(storage_error)?,
                dimensions: u32::try_from(dimensions).map_err(storage_error)?,
            },
        )
        .map_err(storage_error)?;
        let value = |name: &str| -> Result<u64, ApplicationError> {
            u64::try_from(row.try_get::<i64, _>(name).map_err(storage_error)?)
                .map_err(storage_error)
        };
        let snapshot = CanonicalGenerationSnapshot::new(
            space,
            CorpusGenerationId::from_uuid(row.try_get("id").map_err(storage_error)?),
            value("generation_epoch")?,
            value("guard_version")?,
            value("corpus_revision")?,
            value("built_through_projection_ordinal")?,
            value("member_count")?,
        )
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;
        Ok(snapshot)
    }
}
