//! Narrow application boundary for the transition-rebuild bijection.

use super::transition::{
    CreateEmbeddingTransitionBatchAttempt, EmbeddingTransitionProgress,
    ObserveEmbeddingTransitionAttempt, ProveEmbeddingTransitionCompleteness,
    SharedEmbeddingTransitionRepository,
};
use crate::{ApplicationError, RequestContext};
use vestrace_domain::EmbeddingJobId;

/// Coordinates the three SQL-authorized transition execution steps.  It does
/// not calculate a satisfaction kind: the repository derives that from the
/// terminal job, publication, and Live projection facts.
pub struct EmbeddingTransitionCoordinator {
    repository: SharedEmbeddingTransitionRepository,
}

impl EmbeddingTransitionCoordinator {
    pub fn new(repository: SharedEmbeddingTransitionRepository) -> Self {
        Self { repository }
    }

    pub async fn create_attempt(
        &self,
        context: RequestContext,
        command: CreateEmbeddingTransitionBatchAttempt,
    ) -> Result<EmbeddingJobId, ApplicationError> {
        self.repository.create_batch_attempt(context, command).await
    }

    pub async fn observe_attempt(
        &self,
        context: RequestContext,
        command: ObserveEmbeddingTransitionAttempt,
    ) -> Result<EmbeddingTransitionProgress, ApplicationError> {
        self.repository.observe_attempt(context, command).await
    }

    pub async fn prove_completeness(
        &self,
        context: RequestContext,
        command: ProveEmbeddingTransitionCompleteness,
    ) -> Result<EmbeddingTransitionProgress, ApplicationError> {
        self.repository.prove_completeness(context, command).await
    }
}
