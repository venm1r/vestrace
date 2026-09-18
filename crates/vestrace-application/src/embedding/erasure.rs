//! What erasing a source does to the corpora computed from it.
//!
//! Erasing a content material an embedding corpus was computed from is not the
//! ordinary two-phase erasure with an extra step. The corpus has to stop
//! answering from those vectors *before* the ciphertext can be destroyed, and
//! both facts have to be one transaction: a corpus advanced without its
//! generation revoked would keep serving content that is about to stop
//! existing, and a blocker terminalized without the corpus advanced would let
//! phase two delete ciphertext a live generation still points at.
//!
//! That transaction is `vestrace_propagate_embedding_source_erasure`, and this
//! module is the only thing in the product that calls it. It deliberately does
//! not reimplement the ordering in Rust: the database holds every lock the
//! ordering needs, and a Rust sequence of separate statements could not.

use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::embedding::index::{EmbeddingIndexRegistryPort, LocalEmbeddingIndex};
use crate::material::erasure::{EmbeddingSourceErasurePropagator, PropagatedErasure};
use crate::{ApplicationError, MaterialErasurePreparation, RequestContext};
use vestrace_domain::ContentMaterialId;

/// What one propagation did, beside preparing the erasure it delegates to.
///
/// The counts are kept because after phase two they cannot be recomputed: the
/// projections are gone, and an operator asking what erasing this source
/// removed from the corpus has nothing else to read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingErasurePropagation {
    propagation_id: Uuid,
    preparation: MaterialErasurePreparation,
    retired_vector_materials: Vec<ContentMaterialId>,
    dependent_projection_count: u64,
    revoked_generation_count: u64,
    staled_transition_count: u64,
}

impl EmbeddingErasurePropagation {
    pub const fn new(
        propagation_id: Uuid,
        preparation: MaterialErasurePreparation,
        retired_vector_materials: Vec<ContentMaterialId>,
        dependent_projection_count: u64,
        revoked_generation_count: u64,
        staled_transition_count: u64,
    ) -> Self {
        Self {
            propagation_id,
            preparation,
            retired_vector_materials,
            dependent_projection_count,
            revoked_generation_count,
            staled_transition_count,
        }
    }

    pub const fn propagation_id(&self) -> Uuid {
        self.propagation_id
    }

    /// The preparation the ordinary two-phase erasure continues from. It is the
    /// same preparation `prepare_content` would have returned, produced by the
    /// same function, inside the propagating transaction.
    pub const fn preparation(&self) -> MaterialErasurePreparation {
        self.preparation
    }

    /// One ciphertext per retired projection.
    ///
    /// The caller must erase these as well as the source. A vector computed
    /// from erased content is that content in another representation, and the
    /// propagation retired the projections that named them, so nothing else
    /// will ever come back for them.
    pub fn retired_vector_materials(&self) -> &[ContentMaterialId] {
        &self.retired_vector_materials
    }

    pub const fn dependent_projection_count(&self) -> u64 {
        self.dependent_projection_count
    }

    pub const fn revoked_generation_count(&self) -> u64 {
        self.revoked_generation_count
    }

    pub const fn staled_transition_count(&self) -> u64 {
        self.staled_transition_count
    }
}

/// One committed corpus-change event caused by an erasure: which space changed,
/// and the epoch it changed to.
///
/// Read after that transaction committed and never from inside it. An
/// invalidation observed before its commit could drop a local index for a
/// change that then rolled back, and the next query would rebuild it from a
/// corpus that never changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedEmbeddingInvalidation {
    pub space_registration_id: Uuid,
    pub committed_epoch: u64,
}

#[async_trait]
pub trait EmbeddingErasureRepository: Send + Sync {
    /// Phase one for a source a corpus may have been computed from.
    ///
    /// `Ok(None)` means nothing embedded depends on this material, so the
    /// ordinary two-phase authority owns it and this module must not interpose.
    /// That decision is the repository's because only it can make it under the
    /// material's own lock.
    async fn propagate_source_erasure(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<Option<EmbeddingErasurePropagation>, ApplicationError>;

    /// The highest committed erasure-caused epoch per space in this workspace,
    /// newest first, bounded by `limit`.
    async fn committed_invalidations(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<CommittedEmbeddingInvalidation>, ApplicationError>;
}

pub type SharedEmbeddingErasureRepository = Arc<dyn EmbeddingErasureRepository>;

/// A reconciliation pass reads at most this many spaces. Bounded because the
/// pass is idempotent and can simply run again, and an unbounded read would
/// make one worker cycle's cost depend on how much a workspace has ever erased.
pub const DEFAULT_ERASURE_RECONCILE_LIMIT: u32 = 64;

/// Drives erasure propagation and the local-index invalidation that follows it.
///
/// It holds no database transaction across the registry call, and the registry
/// call happens only after the propagation committed. The other order would
/// drop an index for a change that could still roll back.
pub struct EmbeddingErasureService<R, G, I> {
    repository: Arc<R>,
    registry: Arc<G>,
    reconcile_limit: u32,
    index: PhantomData<fn() -> I>,
}

impl<R, G, I> EmbeddingErasureService<R, G, I>
where
    R: EmbeddingErasureRepository,
    I: LocalEmbeddingIndex,
    G: EmbeddingIndexRegistryPort<I>,
{
    pub fn new(repository: Arc<R>, registry: Arc<G>) -> Self {
        Self {
            repository,
            registry,
            reconcile_limit: DEFAULT_ERASURE_RECONCILE_LIMIT,
            index: PhantomData,
        }
    }

    pub fn with_reconcile_limit(mut self, limit: u32) -> Result<Self, ApplicationError> {
        if limit == 0 || limit > 1024 {
            return Err(ApplicationError::InvalidConfiguration(
                "embedding erasure reconcile limit must be between 1 and 1024".into(),
            ));
        }
        self.reconcile_limit = limit;
        Ok(self)
    }

    /// Phase one for a source, propagated through every corpus computed from it.
    ///
    /// The local-index drop is attempted immediately after, because the caller
    /// is about to destroy the ciphertext and the shortest possible window is
    /// the point. It is attempted, not required: a missed drop is safe, since
    /// query still validates the database guard and a stale cached index fails
    /// closed there.
    pub async fn prepare_source(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<Option<EmbeddingErasurePropagation>, ApplicationError> {
        let propagated = self
            .repository
            .propagate_source_erasure(context, material_id)
            .await?;
        if propagated.is_some() {
            self.reconcile_one(context).await?;
        }
        Ok(propagated)
    }

    /// One bounded reconciliation pass: drop every local index older than a
    /// committed invalidation epoch.
    ///
    /// "One" is one pass rather than one event because there is no durable
    /// consumer cursor to advance, and none is needed:
    /// `remove_space_before_epoch` keeps equal and newer entries, so replaying
    /// a pass removes nothing a later build installed. Returns how many spaces
    /// the pass carried an epoch for.
    pub async fn reconcile_one(&self, context: &RequestContext) -> Result<usize, ApplicationError> {
        let invalidations = self
            .repository
            .committed_invalidations(context, self.reconcile_limit)
            .await?;
        for invalidation in &invalidations {
            self.registry.remove_space_before_epoch(
                context.workspace_id,
                invalidation.space_registration_id,
                invalidation.committed_epoch,
            );
        }
        Ok(invalidations.len())
    }
}

#[async_trait]
impl<R, G, I> EmbeddingSourceErasurePropagator for EmbeddingErasureService<R, G, I>
where
    R: EmbeddingErasureRepository + 'static,
    I: LocalEmbeddingIndex + 'static,
    G: EmbeddingIndexRegistryPort<I> + 'static,
{
    async fn propagate(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<Option<PropagatedErasure>, ApplicationError> {
        Ok(self
            .prepare_source(context, material_id)
            .await?
            .map(|propagated| PropagatedErasure {
                vectors: propagated.retired_vector_materials().to_vec(),
                source: propagated.preparation(),
            }))
    }
}
