//! Bounded, durable claims for embedding-worker cycles.
//!
//! A claim only identifies work.  It deliberately carries neither a dispatch
//! authority nor any routing or provider material, so acquiring one cannot
//! authorize a provider call.

use async_trait::async_trait;
use std::sync::Arc;
use vestrace_domain::EmbeddingJobId;

use crate::{ApplicationError, RequestContext};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingWorkKind {
    Dispatch,
    ReconcileKeys,
    FinalizeResult,
    BuildIndex,
    CoordinateTransition,
    PropagateErasure,
}

impl EmbeddingWorkKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dispatch => "dispatch",
            Self::ReconcileKeys => "reconcile_keys",
            Self::FinalizeResult => "finalize_result",
            Self::BuildIndex => "build_index",
            Self::CoordinateTransition => "coordinate_transition",
            Self::PropagateErasure => "propagate_erasure",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingWorkClaim {
    pub job_id: EmbeddingJobId,
    pub kind: EmbeddingWorkKind,
    pub owner: String,
    pub claim_deadline: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingWorkOutcome {
    Completed,
    RetryableFailure,
    DefiniteFailure,
}

#[async_trait]
pub trait EmbeddingWorkRepository: Send + Sync {
    async fn claim(
        &self,
        context: &RequestContext,
        kind: EmbeddingWorkKind,
        owner: &str,
        limit: u32,
    ) -> Result<Vec<EmbeddingWorkClaim>, ApplicationError>;

    async fn finish(
        &self,
        context: &RequestContext,
        claim: &EmbeddingWorkClaim,
        outcome: EmbeddingWorkOutcome,
    ) -> Result<(), ApplicationError>;
}

pub type SharedEmbeddingWorkRepository = Arc<dyn EmbeddingWorkRepository>;
