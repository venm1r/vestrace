//! Job-owned retrieval: one fence per attempt, one terminal result, and one
//! authorized successor after a generation change.
//!
//! A `retrieval_query` job embeds a query and persists no vector.  Nothing in
//! this module carries query bytes or a query digest: the fence records which
//! generation the attempt was admitted against, and the result records only
//! safe references, ranks and scores.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use vestrace_domain::{
    EmbeddingJobId, RetrievalCandidate, embedding::RetrievalGenerationChangedReason,
    id::RetrievalRunId,
};
use zeroize::Zeroizing;

use super::super::NormalizedRetrievalRequest;
use crate::{ApplicationError, RequestContext};

/// The embedded query, owned for exactly as long as the search that uses it.
///
/// A query embedding is as sensitive as the text it was derived from, and
/// unlike a stored projection it is never encrypted at rest, because it is
/// never at rest. So the type carries no `Clone`, no `Debug`, no serialization
/// and no accessor that yields an owned copy: the components can only be
/// borrowed inside a callback, and the buffer is zeroized when the value drops
/// — on the success branch, on every degradation branch, and on unwind alike,
/// because `Drop` does not care which one it was.
///
/// This is what keeps the query vector out of `EmbeddingRetrievalOutcome`,
/// which carries references, ranks and scores only.
pub struct QueryEmbedding(Zeroizing<Vec<f32>>);

impl QueryEmbedding {
    /// Rejects an unusable vector before it can be searched with. A zero-length
    /// or non-finite query would otherwise reach the local index and be refused
    /// there, one layer past the point where the components are still known.
    pub fn new(values: Vec<f32>) -> Result<Self, ApplicationError> {
        if values.is_empty() {
            return Err(ApplicationError::Policy(
                "query embedding must not be empty".to_owned(),
            ));
        }
        if !values.iter().all(|value| value.is_finite()) {
            return Err(ApplicationError::Policy(
                "query embedding components must be finite".to_owned(),
            ));
        }
        Ok(Self(Zeroizing::new(values)))
    }

    pub fn dimensions(&self) -> usize {
        self.0.len()
    }

    /// The only way to read the components. Borrowed for the call and no
    /// longer, so a caller cannot keep them past the search.
    pub fn with_values<T>(&self, use_values: impl FnOnce(&[f32]) -> T) -> T {
        use_values(self.0.as_slice())
    }
}

/// Why an attempt produced no terminal result.  Closed: every degradation a
/// caller may observe is one of these, so no adapter invents a new one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingRetrievalDegradation {
    /// The process-local index for the pinned generation is not loaded.  The
    /// durable epoch is unchanged; a later attempt may succeed unaided.
    MissingLocalIndex,
    /// Legacy vectors have not finished governed adoption, so no canonical
    /// generation can answer yet.
    LegacyAdoptionPending,
    /// A transition owns the space and has not been activated.
    TransitionNotReady,
    /// The active space has no Ready current generation.
    GenerationNotReady,
    /// The pinned generation moved under the attempt.  This is the only
    /// degradation an authorized retry may follow.
    GenerationChanged(RetrievalGenerationChangedReason),
    /// The attempt was accepted and no worker reached a terminal outcome for it
    /// inside the request budget.  Nothing about the corpus is wrong; the
    /// answer simply is not ready, and asking again is an ordinary new request
    /// rather than the authorized retry a generation change earns.
    Pending,
}

impl EmbeddingRetrievalDegradation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingLocalIndex => "missing_local_index",
            Self::LegacyAdoptionPending => "legacy_adoption_pending",
            Self::TransitionNotReady => "transition_not_ready",
            Self::GenerationNotReady => "generation_not_ready",
            Self::GenerationChanged(_) => "retrieval_generation_changed",
            Self::Pending => "retrieval_pending",
        }
    }

    /// Only a generation change is retryable, and only once.
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::GenerationChanged(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmbeddingRetrievalOutcome {
    Completed(Vec<RetrievalCandidate>),
    Degraded(EmbeddingRetrievalDegradation),
}

/// The one client a retrieval request uses to reach the embedding side.  The
/// logical request identity is minted once by `RetrievalService::search`; no
/// HTTP, MCP, vector-adapter or worker layer creates another.
#[async_trait]
pub trait EmbeddingRetrievalJobClient: Send + Sync {
    async fn retrieve(
        &self,
        context: &RequestContext,
        request_id: RetrievalRunId,
        request: &NormalizedRetrievalRequest,
        deadline: DateTime<Utc>,
    ) -> Result<EmbeddingRetrievalOutcome, ApplicationError>;
}

pub type SharedEmbeddingRetrievalJobClient = Arc<dyn EmbeddingRetrievalJobClient>;

/// Admits one attempt against the generation current at acceptance, and binds
/// the fence one-to-one to the job that will answer it.
#[derive(Clone, Debug)]
pub struct AcceptRetrievalAttempt {
    pub job_id: EmbeddingJobId,
    pub request_id: RetrievalRunId,
    pub space_registration_id: uuid::Uuid,
    pub deadline: DateTime<Utc>,
}

/// What acceptance pinned.  These are the durable identifiers the fence row
/// actually holds; the full space key is not reconstructed here because the
/// caller never revalidates on its own.  SQL re-checks the exact pinned tuple
/// when the fence is cashed, which is the only check that can be trusted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalAttemptAdmission {
    pub fence_id: uuid::Uuid,
    pub generation_id: uuid::Uuid,
    pub generation_epoch: u64,
    pub guard_version: u64,
}

/// One safe reference in an ordered result.  No content, no vector, no digest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetrievalResultReference {
    pub ordinal: u32,
    pub memory_id: uuid::Uuid,
    pub revision_id: uuid::Uuid,
    pub rank: u32,
    pub score: f32,
}

#[derive(Clone, Debug)]
pub struct FinalizeRetrievalResult {
    pub job_id: EmbeddingJobId,
    pub fence_id: uuid::Uuid,
    pub references: Vec<RetrievalResultReference>,
}

#[derive(Clone, Debug)]
pub struct ObserveRetrievalGenerationChange {
    pub job_id: EmbeddingJobId,
    pub fence_id: uuid::Uuid,
    pub reason: RetrievalGenerationChangedReason,
}

/// The one authorized successor after a confirmed generation change.  A replay
/// under the same key returns the same successor; a different key against the
/// same predecessor conflicts.
#[derive(Clone, Debug)]
pub struct RetryRetrievalGenerationChanged {
    pub predecessor_job_id: EmbeddingJobId,
    /// What the caller believed the predecessor was when it decided to spend a
    /// further provider call. Checked rather than trusted: a predecessor that
    /// moved since the caller read it is a different decision, and the caller
    /// should make it again knowing what it is now.
    pub expected_predecessor_version: u64,
    pub successor_job_id: EmbeddingJobId,
    pub successor_request_id: RetrievalRunId,
    pub idempotency_key: String,
}

/// One member of a pinned generation, resolved to what a caller may be told
/// about it.
///
/// A local index hit names a projection and the material it was computed from,
/// which is the right identity inside the corpus and useless outside it. The
/// bridge is the material's owner: a governed embedding source is owned by the
/// memory revision it was materialized from, so that is what resolves a hit
/// into a reference a retrieval result may carry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalMemberReference {
    pub projection_id: uuid::Uuid,
    pub memory_id: uuid::Uuid,
    pub revision_id: uuid::Uuid,
}

#[async_trait]
pub trait EmbeddingRetrievalRepository: Send + Sync {
    /// The fence one job owns, and the exact generation it pinned, in the shape
    /// the local registry keys an index by.
    ///
    /// One read rather than two because the fence is one-to-one with the job:
    /// asking for them separately would let a caller pair a fence with a
    /// snapshot resolved a moment later. Read from the fence rather than
    /// resolved afresh, because the whole point of the fence is that the
    /// attempt answers from the generation it was admitted against even if the
    /// current one has since moved.
    async fn pinned_attempt(
        &self,
        _context: &RequestContext,
        _job_id: EmbeddingJobId,
    ) -> Result<
        (
            uuid::Uuid,
            vestrace_domain::embedding::CanonicalGenerationSnapshot,
        ),
        ApplicationError,
    > {
        Err(ApplicationError::Unavailable(
            "governed embedding retrieval snapshot lookup is not configured".to_owned(),
        ))
    }

    /// What each hit projection may be named as, in the order asked.
    ///
    /// A projection whose source can no longer be resolved is absent from the
    /// answer rather than guessed at, and the caller decides what that means.
    async fn resolve_members(
        &self,
        _context: &RequestContext,
        _projection_ids: &[uuid::Uuid],
    ) -> Result<Vec<RetrievalMemberReference>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding retrieval member resolution is not configured".to_owned(),
        ))
    }

    async fn accept_attempt(
        &self,
        _context: &RequestContext,
        _command: AcceptRetrievalAttempt,
    ) -> Result<RetrievalAttemptAdmission, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding retrieval acceptance is not configured".to_owned(),
        ))
    }

    async fn finalize_result(
        &self,
        _context: &RequestContext,
        _command: FinalizeRetrievalResult,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding retrieval finalization is not configured".to_owned(),
        ))
    }

    async fn observe_generation_change(
        &self,
        _context: &RequestContext,
        _command: ObserveRetrievalGenerationChange,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding retrieval observation is not configured".to_owned(),
        ))
    }

    async fn authorize_retry(
        &self,
        _context: &RequestContext,
        _command: RetryRetrievalGenerationChanged,
    ) -> Result<EmbeddingJobId, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding retrieval retry is not configured".to_owned(),
        ))
    }
}

pub type SharedEmbeddingRetrievalRepository = Arc<dyn EmbeddingRetrievalRepository>;

/// What a `retrieval_query` job's provider response is handed to.
///
/// This exists so the executor never holds a query vector for longer than the
/// call that uses it. The vector arrives as a `QueryEmbedding`, which cannot be
/// cloned, copied out or serialized, and is zeroized when this call returns --
/// on the success branch, on every degradation branch, and on unwind alike.
#[async_trait]
pub trait EmbeddingRetrievalSink: Send + Sync {
    async fn complete(
        &self,
        context: &RequestContext,
        job_id: EmbeddingJobId,
        query: QueryEmbedding,
    ) -> Result<(), ApplicationError>;
}

pub type SharedEmbeddingRetrievalSink = Arc<dyn EmbeddingRetrievalSink>;

/// Answers one accepted attempt from the generation it pinned.
///
/// It searches a process-local index and writes references, never vectors. The
/// index it searches is the one the fence names, loaded on demand if this
/// process has not built it yet: a cache miss is not an answer, and a query
/// that declined because an index happened to be cold would be indistinguishable
/// from one that found nothing.
pub struct EmbeddingRetrievalExecutionService<T, R, V, D, F, G> {
    repository: Arc<T>,
    index: Arc<crate::embedding::index::EmbeddingIndexService<R, V, D, F, G>>,
    owner: String,
    limit: usize,
}

impl<T, R, V, D, F, G> EmbeddingRetrievalExecutionService<T, R, V, D, F, G>
where
    T: EmbeddingRetrievalRepository,
    R: crate::embedding::index::EmbeddingIndexRepository,
    V: crate::MaterialKeyVault,
    D: crate::embedding::index::EmbeddingIndexDecoder,
    F: crate::embedding::index::EmbeddingIndexFactory,
    G: crate::embedding::index::EmbeddingIndexRegistryPort<F::Index>,
{
    pub fn new(
        repository: Arc<T>,
        index: Arc<crate::embedding::index::EmbeddingIndexService<R, V, D, F, G>>,
        owner: impl Into<String>,
        limit: usize,
    ) -> Result<Self, ApplicationError> {
        let owner = owner.into();
        if owner.trim().is_empty() || owner.len() > 128 || limit == 0 || limit > 1024 {
            return Err(ApplicationError::InvalidConfiguration(
                "embedding retrieval execution bounds are invalid".to_owned(),
            ));
        }
        Ok(Self {
            repository,
            index,
            owner,
            limit,
        })
    }
}

#[async_trait]
impl<T, R, V, D, F, G> EmbeddingRetrievalSink
    for EmbeddingRetrievalExecutionService<T, R, V, D, F, G>
where
    T: EmbeddingRetrievalRepository + 'static,
    R: crate::embedding::index::EmbeddingIndexRepository + 'static,
    V: crate::MaterialKeyVault + 'static,
    D: crate::embedding::index::EmbeddingIndexDecoder + 'static,
    F: crate::embedding::index::EmbeddingIndexFactory + 'static,
    G: crate::embedding::index::EmbeddingIndexRegistryPort<F::Index> + 'static,
{
    async fn complete(
        &self,
        context: &RequestContext,
        job_id: EmbeddingJobId,
        query: QueryEmbedding,
    ) -> Result<(), ApplicationError> {
        let (fence_id, snapshot) = self.repository.pinned_attempt(context, job_id).await?;
        let index = match self
            .index
            .ensure_loaded(context, &snapshot, &self.owner)
            .await
        {
            Ok(index) => index,
            Err(ApplicationError::Unavailable(reason))
                if reason == "embedding-generation-changed" =>
            {
                // The pinned generation moved under the attempt. This is the
                // one degradation an authorized retry may follow, so it is
                // recorded as the terminal fact it is rather than returned as
                // an error the worker would retry on its own budget.
                return self
                    .repository
                    .observe_generation_change(
                        context,
                        ObserveRetrievalGenerationChange {
                            job_id,
                            fence_id,
                            reason: RetrievalGenerationChangedReason::Replaced,
                        },
                    )
                    .await;
            }
            Err(error) => return Err(error),
        };

        // The only place the components are read, and they are borrowed for
        // exactly this call.
        use crate::embedding::index::LocalEmbeddingIndex;
        let hits = query.with_values(|values| index.search(values, self.limit))?;
        let projection_ids: Vec<uuid::Uuid> = hits.iter().map(|hit| hit.projection_id).collect();
        let resolved = self
            .repository
            .resolve_members(context, &projection_ids)
            .await?;

        // Rank follows the search order; a hit whose source no longer resolves
        // is dropped rather than reported as something it is not, and the
        // ordinals close over the gap because a result's ordinals must be
        // contiguous.
        let mut references = Vec::with_capacity(resolved.len());
        for (rank, hit) in hits.iter().enumerate() {
            let Some(member) = resolved
                .iter()
                .find(|member| member.projection_id == hit.projection_id)
            else {
                continue;
            };
            references.push(RetrievalResultReference {
                ordinal: u32::try_from(references.len()).map_err(|_| {
                    ApplicationError::Internal("retrieval result is larger than u32".to_owned())
                })?,
                memory_id: member.memory_id,
                revision_id: member.revision_id,
                rank: u32::try_from(rank).map_err(|_| {
                    ApplicationError::Internal("retrieval rank is larger than u32".to_owned())
                })?,
                // The stored score is the similarity a caller may see. The raw
                // distance is not it: a distance is a property of the vector
                // pair, and one of that pair is the query.
                score: 1.0_f32 - (hit.distance as f32),
            });
        }

        self.repository
            .finalize_result(
                context,
                FinalizeRetrievalResult {
                    job_id,
                    fence_id,
                    references,
                },
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_embedding_refuses_an_unusable_vector() {
        assert!(QueryEmbedding::new(Vec::new()).is_err());
        assert!(QueryEmbedding::new(vec![0.5, f32::NAN]).is_err());
        assert!(QueryEmbedding::new(vec![0.5, f32::INFINITY]).is_err());
        let embedding = QueryEmbedding::new(vec![0.25, -0.5]).expect("a finite vector is usable");
        assert_eq!(embedding.dimensions(), 2);
        assert_eq!(embedding.with_values(<[f32]>::to_vec), vec![0.25, -0.5]);
    }

    /// The degradation vocabulary is closed, and only one member is retryable.
    #[test]
    fn exactly_one_degradation_authorizes_a_successor() {
        let all = [
            EmbeddingRetrievalDegradation::MissingLocalIndex,
            EmbeddingRetrievalDegradation::LegacyAdoptionPending,
            EmbeddingRetrievalDegradation::TransitionNotReady,
            EmbeddingRetrievalDegradation::GenerationNotReady,
            EmbeddingRetrievalDegradation::GenerationChanged(
                RetrievalGenerationChangedReason::Replaced,
            ),
        ];
        assert_eq!(all.iter().filter(|value| value.is_retryable()).count(), 1);
        let reasons = all.map(|value| value.as_str());
        assert_eq!(
            reasons
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            reasons.len(),
            "each degradation must be distinguishable in a journal"
        );
    }
}
