//! Governed embedding job acceptance.
//!
//! Dispatch itself is not here and must not move here: an embedding job reaches
//! the provider through `ProviderDispatchRepository`, the same trait a Run step
//! uses, and this module exists only for the step before that — turning a
//! request to embed something into a durable job with its identities fixed.

pub mod adoption;
pub mod barrier;
pub mod carry;
pub mod executor;
pub mod finalization;
pub mod index;
pub mod job;
pub mod keys;
pub mod result;
pub mod retrieval;
pub mod transition;
pub mod transition_coordinator;
pub mod work;

pub use adoption::{
    EmbeddingLegacyAdoptionRepository, EmbeddingLegacyAdoptionService, LegacyAdoptionBlocker,
    LegacyAdoptionBlockerRecord, LegacyAdoptionMember, LegacyAdoptionMemberState,
    LegacyAdoptionProgress, LegacyAdoptionRebuildFactory, LegacyAdoptionSourceMaterializer,
    MaterializedSource, SharedEmbeddingLegacyAdoptionRepository, StartLegacyAdoption,
};
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
pub use retrieval::{
    AcceptRetrievalAttempt, EmbeddingRetrievalDegradation, EmbeddingRetrievalJobClient,
    EmbeddingRetrievalOutcome, EmbeddingRetrievalRepository, FinalizeRetrievalResult,
    ObserveRetrievalGenerationChange, QueryEmbedding, RetrievalAttemptAdmission,
    RetrievalResultReference, RetryRetrievalGenerationChanged, SharedEmbeddingRetrievalJobClient,
    SharedEmbeddingRetrievalRepository,
};
pub use transition::{
    ActivateEmbeddingTransition, CreateEmbeddingTransitionBatchAttempt,
    EmbeddingTransitionProgress, EmbeddingTransitionRepository, ObserveEmbeddingTransitionAttempt,
    PlanEmbeddingTransitionVersion, ProveEmbeddingTransitionCompleteness,
    SharedEmbeddingTransitionRepository, TransitionActivationReceipt, TransitionAuthBinding,
    TransitionPlanRecipe,
};
pub use transition_coordinator::EmbeddingTransitionCoordinator;
pub use work::{
    EmbeddingWorkClaim, EmbeddingWorkKind, EmbeddingWorkOutcome, EmbeddingWorkRepository,
    SharedEmbeddingWorkRepository,
};
