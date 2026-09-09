//! Durable index authority and bounded process-local construction ports.
use crate::{
    ApplicationError, EmbeddingOutputKeyBinding, EmbeddingResultPreparationId, RequestContext,
};
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;
use vestrace_domain::{
    ContentMaterialId, CorpusChangeEventId, IndexBuildAttemptId, MaterialKeyBindingReceipt,
    ZeroizingDek, embedding::CanonicalGenerationSnapshot,
};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexLimits {
    pub max_members: usize,
    pub max_bytes: usize,
}
/// Plaintext exists only in the zeroizing decoder-to-builder handoff.
pub struct IndexVector {
    pub projection_id: Uuid,
    pub projection_ordinal: u64,
    pub material_id: ContentMaterialId,
    pub values: Zeroizing<Vec<f32>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct IndexHit {
    pub projection_id: Uuid,
    pub projection_ordinal: u64,
    pub material_id: ContentMaterialId,
    pub distance: f64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexBuildPurpose {
    CorpusChange,
    Startup,
    LazyLoad,
}
#[derive(Clone, Debug)]
pub struct EmbeddingIndexBuildPlan {
    pub attempt_id: IndexBuildAttemptId,
    pub space_registration_id: Uuid,
    pub snapshot: CanonicalGenerationSnapshot,
    pub purpose: IndexBuildPurpose,
    pub event_id: Option<CorpusChangeEventId>,
    pub owner: String,
}
pub struct EncryptedIndexProjection {
    pub projection_id: Uuid,
    pub projection_ordinal: u64,
    pub binding: EmbeddingOutputKeyBinding,
    pub preparation_id: EmbeddingResultPreparationId,
    pub receipt: MaterialKeyBindingReceipt,
    pub ciphertext: Vec<u8>,
}
pub struct EncryptedProjectionChunk {
    pub projections: Vec<EncryptedIndexProjection>,
    pub complete: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationPublicationOutcome {
    Published,
    Discarded,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurrentGenerationValidation {
    Current { space_registration_id: Uuid },
    Changed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexFailureReason {
    InvalidVector,
    MemoryLimit,
    MaterialUnavailable,
    GenerationChanged,
    StorageUnavailable,
}
impl IndexFailureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidVector => "invalid_vector",
            Self::MemoryLimit => "memory_limit",
            Self::MaterialUnavailable => "material_unavailable",
            Self::GenerationChanged => "generation_changed",
            Self::StorageUnavailable => "storage_unavailable",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexBuildOutcome {
    Published,
    Loaded,
    Discarded,
    Failed(IndexFailureReason),
}
#[async_trait]
pub trait EmbeddingIndexRepository: Send + Sync {
    async fn claim_next_build(
        &self,
        context: &RequestContext,
        owner: &str,
        limit: u32,
    ) -> Result<Option<EmbeddingIndexBuildPlan>, ApplicationError>;
    async fn claim_local_load(
        &self,
        context: &RequestContext,
        snapshot: &CanonicalGenerationSnapshot,
        owner: &str,
    ) -> Result<EmbeddingIndexBuildPlan, ApplicationError>;
    async fn load_chunk(
        &self,
        context: &RequestContext,
        plan: &EmbeddingIndexBuildPlan,
        after_ordinal: Option<u64>,
        limit: u32,
    ) -> Result<EncryptedProjectionChunk, ApplicationError>;
    async fn publish_ready(
        &self,
        context: &RequestContext,
        plan: &EmbeddingIndexBuildPlan,
    ) -> Result<GenerationPublicationOutcome, ApplicationError>;
    async fn validate_current(
        &self,
        context: &RequestContext,
        snapshot: &CanonicalGenerationSnapshot,
    ) -> Result<CurrentGenerationValidation, ApplicationError>;
    async fn finish_attempt(
        &self,
        context: &RequestContext,
        attempt_id: IndexBuildAttemptId,
        owner: &str,
        outcome: IndexBuildOutcome,
    ) -> Result<(), ApplicationError>;
    async fn append_attempt_observation(
        &self,
        context: &RequestContext,
        attempt_id: IndexBuildAttemptId,
        owner: &str,
        reason: IndexFailureReason,
    ) -> Result<(), ApplicationError>;
}
/// No method exposes stored vector components. Query callers still need DB authority.
pub trait LocalEmbeddingIndex: Send + Sync {
    fn snapshot(&self) -> &CanonicalGenerationSnapshot;
    fn space_registration_id(&self) -> Uuid;
    fn allocated_bytes(&self) -> usize;
    fn search(&self, query: &[f32], limit: usize) -> Result<Vec<IndexHit>, ApplicationError>;
}
pub trait EmbeddingIndexBuilder: Send {
    type Index: LocalEmbeddingIndex;
    fn push(&mut self, vector: IndexVector) -> Result<(), ApplicationError>;
    fn finish(self) -> Result<Self::Index, ApplicationError>;
}
pub trait EmbeddingIndexFactory: Send + Sync {
    type Index: LocalEmbeddingIndex;
    type Builder: EmbeddingIndexBuilder<Index = Self::Index>;
    fn begin(
        &self,
        snapshot: CanonicalGenerationSnapshot,
        registration: Uuid,
        limits: IndexLimits,
    ) -> Result<Self::Builder, ApplicationError>;
}
pub trait EmbeddingIndexDecoder: Send + Sync {
    fn decode_into(
        &self,
        context: &RequestContext,
        projection: &EncryptedIndexProjection,
        dek: &ZeroizingDek,
        dimensions: u32,
        consume: &mut dyn FnMut(Zeroizing<Vec<f32>>) -> Result<(), ApplicationError>,
    ) -> Result<(), ApplicationError>;
}
pub trait EmbeddingIndexRegistryPort<I: LocalEmbeddingIndex>: Send + Sync {
    fn get(&self, snapshot: &CanonicalGenerationSnapshot, registration: Uuid) -> Option<Arc<I>>;
    /// Caller has committed publication/invalidation and validated this snapshot.
    fn install(&self, index: Arc<I>) -> Result<(), ApplicationError>;
    fn remove_exact(&self, snapshot: &CanonicalGenerationSnapshot, registration: Uuid);
    /// Call only with a committed invalidation epoch; equal/newer entries survive.
    fn remove_space_before_epoch(
        &self,
        workspace: vestrace_domain::WorkspaceId,
        registration: Uuid,
        committed_epoch: u64,
    );
}

/// Builds outside SQL transactions and installs only after committed authority.
pub struct EmbeddingIndexService<R, V, D, F, G> {
    repository: Arc<R>,
    vault: Arc<V>,
    decoder: Arc<D>,
    factory: Arc<F>,
    registry: Arc<G>,
    limits: IndexLimits,
    chunk_size: u32,
}
fn failure_reason(error: &ApplicationError) -> IndexFailureReason {
    match error {
        ApplicationError::Unavailable(message) if message == "embedding-index-memory-limit" => {
            IndexFailureReason::MemoryLimit
        }
        ApplicationError::Unavailable(message)
            if message == "embedding-index-material-unavailable" =>
        {
            IndexFailureReason::MaterialUnavailable
        }
        ApplicationError::Unavailable(message) if message == "embedding-generation-changed" => {
            IndexFailureReason::GenerationChanged
        }
        ApplicationError::Policy(_) => IndexFailureReason::InvalidVector,
        _ => IndexFailureReason::StorageUnavailable,
    }
}
fn changed() -> ApplicationError {
    ApplicationError::Unavailable("embedding-generation-changed".into())
}
impl<R, V, D, F, G> EmbeddingIndexService<R, V, D, F, G>
where
    R: EmbeddingIndexRepository,
    V: crate::MaterialKeyVault,
    D: EmbeddingIndexDecoder,
    F: EmbeddingIndexFactory,
    G: EmbeddingIndexRegistryPort<F::Index>,
{
    pub fn new(
        repository: Arc<R>,
        vault: Arc<V>,
        decoder: Arc<D>,
        factory: Arc<F>,
        registry: Arc<G>,
        limits: IndexLimits,
        chunk_size: u32,
    ) -> Self {
        Self {
            repository,
            vault,
            decoder,
            factory,
            registry,
            limits,
            chunk_size,
        }
    }
    async fn build(
        &self,
        context: &RequestContext,
        plan: &EmbeddingIndexBuildPlan,
    ) -> Result<F::Index, ApplicationError> {
        if plan.snapshot.workspace_id != context.workspace_id
            || plan.space_registration_id.is_nil()
            || self.chunk_size == 0
            || self.chunk_size > 64
        {
            return Err(changed());
        }
        plan.snapshot.validate().map_err(|_| changed())?;
        let mut builder = self.factory.begin(
            plan.snapshot.clone(),
            plan.space_registration_id,
            self.limits,
        )?;
        let mut after = None;
        let mut count = 0u64;
        loop {
            let chunk = self
                .repository
                .load_chunk(context, plan, after, self.chunk_size)
                .await?;
            if chunk.projections.len() > self.chunk_size as usize
                || (!chunk.complete && chunk.projections.is_empty())
            {
                return Err(changed());
            }
            for projection in chunk.projections {
                if projection.binding.workspace_id != context.workspace_id
                    || projection.projection_ordinal == 0
                    || after.is_some_and(|prior| projection.projection_ordinal <= prior)
                {
                    return Err(changed());
                }
                let mut decoded = None;
                let mut calls = 0usize;
                let mut consumed = 0usize;
                self.vault
                    .with_bound_embedding_output_key(
                        &projection.binding,
                        projection.preparation_id,
                        projection.receipt,
                        &mut |dek| {
                            calls += 1;
                            if calls == 1 {
                                decoded = Some(self.decoder.decode_into(
                                    context,
                                    &projection,
                                    dek,
                                    plan.snapshot.space.dimensions(),
                                    &mut |values| {
                                        consumed += 1;
                                        if consumed != 1 {
                                            return Err(changed());
                                        }
                                        builder.push(IndexVector {
                                            projection_id: projection.projection_id,
                                            projection_ordinal: projection.projection_ordinal,
                                            material_id: projection.binding.material_id,
                                            values,
                                        })
                                    },
                                ));
                            }
                        },
                    )
                    .map_err(|_| {
                        ApplicationError::Unavailable("embedding-index-material-unavailable".into())
                    })?;
                if calls != 1 {
                    return Err(changed());
                }
                decoded.ok_or_else(changed)??;
                if consumed != 1 {
                    return Err(changed());
                }
                after = Some(projection.projection_ordinal);
                count = count.checked_add(1).ok_or_else(changed)?;
                if count > plan.snapshot.member_count {
                    return Err(changed());
                }
            }
            if chunk.complete {
                break;
            }
        }
        if count != plan.snapshot.member_count {
            return Err(changed());
        }
        let index = builder.finish()?;
        if index.snapshot() != &plan.snapshot
            || index.space_registration_id() != plan.space_registration_id
        {
            return Err(changed());
        }
        Ok(index)
    }
    async fn is_current(
        &self,
        context: &RequestContext,
        plan: &EmbeddingIndexBuildPlan,
    ) -> Result<bool, ApplicationError> {
        Ok(self
            .repository
            .validate_current(context, &plan.snapshot)
            .await?
            == CurrentGenerationValidation::Current {
                space_registration_id: plan.space_registration_id,
            })
    }
    async fn install(
        &self,
        context: &RequestContext,
        plan: &EmbeddingIndexBuildPlan,
        index: F::Index,
        published: bool,
    ) -> Result<Arc<F::Index>, ApplicationError> {
        let index = Arc::new(index);
        let before = self.is_current(context, plan).await;
        if !matches!(before, Ok(true)) {
            drop(index);
            self.record_discard(
                context,
                plan,
                published,
                before
                    .as_ref()
                    .err()
                    .map(failure_reason)
                    .unwrap_or(IndexFailureReason::GenerationChanged),
            )
            .await?;
            return Err(before.err().unwrap_or_else(changed));
        }
        if let Err(error) = self.registry.install(index.clone()) {
            drop(index);
            self.record_discard(context, plan, published, failure_reason(&error))
                .await?;
            return Err(error);
        }
        let after = self.is_current(context, plan).await;
        if !matches!(after, Ok(true)) {
            self.registry
                .remove_exact(&plan.snapshot, plan.space_registration_id);
            drop(index);
            self.record_discard(
                context,
                plan,
                published,
                after
                    .as_ref()
                    .err()
                    .map(failure_reason)
                    .unwrap_or(IndexFailureReason::GenerationChanged),
            )
            .await?;
            return Err(after.err().unwrap_or_else(changed));
        }
        if !published {
            if let Err(error) = self
                .repository
                .finish_attempt(
                    context,
                    plan.attempt_id,
                    &plan.owner,
                    IndexBuildOutcome::Loaded,
                )
                .await
            {
                self.registry
                    .remove_exact(&plan.snapshot, plan.space_registration_id);
                drop(index);
                return Err(error);
            }
        }
        Ok(index)
    }
    async fn record_discard(
        &self,
        context: &RequestContext,
        plan: &EmbeddingIndexBuildPlan,
        published: bool,
        reason: IndexFailureReason,
    ) -> Result<(), ApplicationError> {
        if published {
            self.repository
                .append_attempt_observation(context, plan.attempt_id, &plan.owner, reason)
                .await
        } else {
            self.repository
                .finish_attempt(
                    context,
                    plan.attempt_id,
                    &plan.owner,
                    if reason == IndexFailureReason::GenerationChanged {
                        IndexBuildOutcome::Discarded
                    } else {
                        IndexBuildOutcome::Failed(reason)
                    },
                )
                .await
        }
    }
    async fn build_or_finish(
        &self,
        context: &RequestContext,
        plan: &EmbeddingIndexBuildPlan,
    ) -> Result<F::Index, ApplicationError> {
        match self.build(context, plan).await {
            Ok(index) => Ok(index),
            Err(error) => {
                let reason = failure_reason(&error);
                self.repository
                    .finish_attempt(
                        context,
                        plan.attempt_id,
                        &plan.owner,
                        if reason == IndexFailureReason::GenerationChanged {
                            IndexBuildOutcome::Discarded
                        } else {
                            IndexBuildOutcome::Failed(reason)
                        },
                    )
                    .await?;
                Err(error)
            }
        }
    }
    pub async fn reconcile_one(
        &self,
        context: &RequestContext,
        owner: &str,
    ) -> Result<Option<Arc<F::Index>>, ApplicationError> {
        let limit = self.limits.max_members.clamp(1, i32::MAX as usize) as u32;
        let Some(plan) = self
            .repository
            .claim_next_build(context, owner, limit)
            .await?
        else {
            return Ok(None);
        };
        // A foreign/malformed claim is not ours to terminalize. Its bounded SQL
        // deadline makes it recoverable without changing another owner's attempt.
        if plan.owner != owner || plan.purpose == IndexBuildPurpose::LazyLoad {
            return Err(changed());
        }
        let candidate = self.build_or_finish(context, &plan).await?;
        match self.repository.publish_ready(context, &plan).await {
            Ok(GenerationPublicationOutcome::Published) => self
                .install(context, &plan, candidate, true)
                .await
                .map(Some),
            Ok(GenerationPublicationOutcome::Discarded) => {
                drop(candidate);
                self.repository
                    .finish_attempt(
                        context,
                        plan.attempt_id,
                        &plan.owner,
                        IndexBuildOutcome::Discarded,
                    )
                    .await?;
                Err(changed())
            }
            Err(error) => {
                drop(candidate);
                Err(error)
            }
        }
    }
    pub async fn ensure_loaded(
        &self,
        context: &RequestContext,
        snapshot: &CanonicalGenerationSnapshot,
        owner: &str,
    ) -> Result<Arc<F::Index>, ApplicationError> {
        if snapshot.workspace_id != context.workspace_id {
            return Err(changed());
        }
        snapshot.validate().map_err(|_| changed())?;
        let CurrentGenerationValidation::Current {
            space_registration_id,
        } = self.repository.validate_current(context, snapshot).await?
        else {
            return Err(changed());
        };
        if let Some(index) = self.registry.get(snapshot, space_registration_id) {
            if self.repository.validate_current(context, snapshot).await?
                == (CurrentGenerationValidation::Current {
                    space_registration_id,
                })
            {
                return Ok(index);
            }
            self.registry.remove_exact(snapshot, space_registration_id);
            return Err(changed());
        }
        let plan = self
            .repository
            .claim_local_load(context, snapshot, owner)
            .await?;
        if plan.snapshot != *snapshot
            || plan.owner != owner
            || plan.space_registration_id != space_registration_id
            || plan.purpose != IndexBuildPurpose::LazyLoad
        {
            return Err(changed());
        }
        let candidate = self.build_or_finish(context, &plan).await?;
        self.install(context, &plan, candidate, false).await
    }
}
