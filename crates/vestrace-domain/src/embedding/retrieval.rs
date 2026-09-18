//! Attempt-scoped retrieval metadata; never contains a query or stored vector.
use super::{CanonicalGenerationSnapshot, EmbeddingGenerationError};
use crate::{EmbeddingJobId, RetrievalRunId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalGenerationFence {
    pub job_id: EmbeddingJobId,
    pub request_id: RetrievalRunId,
    pub snapshot: CanonicalGenerationSnapshot,
}
impl RetrievalGenerationFence {
    pub fn new(
        job_id: EmbeddingJobId,
        request_id: RetrievalRunId,
        snapshot: CanonicalGenerationSnapshot,
    ) -> Result<Self, EmbeddingGenerationError> {
        let value = Self {
            job_id,
            request_id,
            snapshot,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), EmbeddingGenerationError> {
        if self.job_id.as_uuid().is_nil() || self.request_id.as_uuid().is_nil() {
            return Err(EmbeddingGenerationError::NilRetrievalIdentity);
        }
        self.snapshot.validate()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RetrievalGenerationChangedReason {
    Stale,
    Revoked,
    Replaced,
    CorpusChanged,
    MemberUnavailable,
}

impl RetrievalGenerationChangedReason {
    pub const ALL: [Self; 5] = [
        Self::Stale,
        Self::Revoked,
        Self::Replaced,
        Self::CorpusChanged,
        Self::MemberUnavailable,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stale => "stale",
            Self::Revoked => "revoked",
            Self::Replaced => "replaced",
            Self::CorpusChanged => "corpus_changed",
            Self::MemberUnavailable => "member_unavailable",
        }
    }
}
