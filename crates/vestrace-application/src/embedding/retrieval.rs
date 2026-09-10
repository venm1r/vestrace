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
}

impl EmbeddingRetrievalDegradation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingLocalIndex => "missing_local_index",
            Self::LegacyAdoptionPending => "legacy_adoption_pending",
            Self::TransitionNotReady => "transition_not_ready",
            Self::GenerationNotReady => "generation_not_ready",
            Self::GenerationChanged(_) => "retrieval_generation_changed",
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
    pub successor_job_id: EmbeddingJobId,
    pub successor_request_id: RetrievalRunId,
    pub idempotency_key: String,
}

#[async_trait]
pub trait EmbeddingRetrievalRepository: Send + Sync {
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
