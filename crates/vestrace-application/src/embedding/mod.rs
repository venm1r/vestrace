//! Governed embedding job acceptance.
//!
//! Dispatch itself is not here and must not move here: an embedding job reaches
//! the provider through `ProviderDispatchRepository`, the same trait a Run step
//! uses, and this module exists only for the step before that — turning a
//! request to embed something into a durable job with its identities fixed.

pub mod barrier;
pub mod carry;
pub mod executor;
pub mod finalization;
pub mod index;
pub mod job;
pub mod keys;
pub mod result;
pub mod transition;
pub mod work;

pub use barrier::{BarrierBatchDispatch, EmbeddingTransitionBarrierRepository};
pub use carry::{AcknowledgeCarriedTransitionBatchAfterUnknown, CarryRecipeMapping};
pub use executor::{EMBEDDING_DISPATCH_TTL_SECONDS, EmbeddingExecutionOutcome, EmbeddingExecutor};
pub use finalization::{
    EmbeddingOutputCommitment, EmbeddingResultBoundOutput, EmbeddingResultCommitter,
    EmbeddingResultFinalizationAuthority, EmbeddingResultFinalizationProgress,
    EmbeddingResultFinalizationRepository, EmbeddingResultFinalizationService,
    EmbeddingResultPublication,
};
pub use job::{
    AcceptEmbeddingJob, EmbeddingJobRepository, EmbeddingJobTerminationReceipt,
    EmbeddingJobTerminationService, PreDispatchTerminalState, PreDispatchTerminationEvidence,
    SharedEmbeddingJobRepository, TerminateEmbeddingJobPreDispatch,
};
pub use keys::{
    AcceptDeliveryOutputs, DeliveryOutputAcceptanceReceipt, DeliveryOutputIdentity,
    EmbeddingOutputKeyPlan, EmbeddingOutputKeyProgress, EmbeddingOutputKeyRepository,
    EmbeddingOutputKeyService, RequestEmbeddingOutputRetirement,
    SharedEmbeddingOutputKeyRepository,
};
pub use result::{
    EMBEDDING_RESULT_CONFLICT, EmbeddingResultDispatchAuthority, EmbeddingResultEligibility,
    EmbeddingResultEligibilityPlan, EmbeddingResultOutputPlan, EmbeddingResultPreparationId,
    EmbeddingResultPreparationIdentities, EmbeddingResultPreparationOutcome,
    EmbeddingResultPreparationService, EmbeddingResultPreparedAttachment,
    EmbeddingResultRepository, EmbeddingResultSealer, SealedEmbeddingResultOutput,
};
pub use transition::{
    EmbeddingTransitionRepository, PlanEmbeddingTransitionVersion,
    SharedEmbeddingTransitionRepository, TransitionAuthBinding, TransitionPlanRecipe,
};
pub use work::{
    EmbeddingWorkClaim, EmbeddingWorkKind, EmbeddingWorkOutcome, EmbeddingWorkRepository,
    SharedEmbeddingWorkRepository,
};
