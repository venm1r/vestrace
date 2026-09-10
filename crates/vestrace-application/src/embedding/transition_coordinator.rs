//! Narrow application boundary for the transition-rebuild bijection.

use super::transition::{
    ActivateEmbeddingTransition, CreateEmbeddingTransitionBatchAttempt,
    EmbeddingTransitionProgress, ObserveEmbeddingTransitionAttempt,
    ProveEmbeddingTransitionCompleteness, SharedEmbeddingTransitionRepository,
    TransitionActivationReceipt,
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

    /// Activates one proven transition.  The coordinator contributes
    /// identities and expected versions only; the allowed guard set, the
    /// credential lineage and every retirement event are derived by SQL from
    /// durable facts.  No provider lease is held across this call.
    pub async fn activate(
        &self,
        context: RequestContext,
        command: ActivateEmbeddingTransition,
    ) -> Result<TransitionActivationReceipt, ApplicationError> {
        self.repository.activate(context, command).await
    }
}
