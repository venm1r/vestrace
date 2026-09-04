use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    AuditEvent, EmbeddingJobId, EmbeddingSpaceId, ExternalEffectIntent, ModelRequestEvidenceId,
    embedding::EmbeddingJobKind,
};

use crate::{
    ApplicationError, GovernedMutationReceipt, IdempotencyRecord, OutboxMessage, RequestContext,
};

/// One embedding job, accepted with every identity it will ever own.
///
/// Spec line 219: a job owns "exactly one immutable embedding-kind
/// `ModelBindingSnapshot`, one external-effect identity, and one
/// `ModelRequestEvidence` identity", and an ordinary job's snapshot is
/// "resolved from the current tuple at acceptance and never changed
/// thereafter". Every one of those is stated by the caller here and proved
/// consistent by the guarded function; none is resolved later.
///
/// The intent is carried rather than constructed inside the repository.
/// `ExternalEffectIntent::new` allocates a fresh id, so building one at the
/// storage boundary would give a same-key replay a different effect than the
/// job it is being accepted for — the same trap the Run-step path documents.
pub struct AcceptEmbeddingJob {
    pub job_id: EmbeddingJobId,
    pub space_registration_id: EmbeddingSpaceId,
    pub kind: EmbeddingJobKind,
    pub model_binding_snapshot_id: uuid::Uuid,
    pub intent: ExternalEffectIntent,
    pub model_request_evidence_id: ModelRequestEvidenceId,
    /// Spec line 257: only the authorized duplicate-charge acknowledgement sets
    /// this, and only against a terminal `InconclusiveUnknown` predecessor. An
    /// ordinary job retries nothing, and the guarded function refuses a
    /// predecessor in any other state rather than trusting the caller.
    pub retries_unknown_embedding_job_id: Option<EmbeddingJobId>,
    /// The predecessor version the acknowledging operator read.
    ///
    /// Line 257 requires the acknowledgement to be expected-version checked.
    /// Present exactly when a predecessor is named: the guarded function refuses
    /// a successor without one and refuses a version without a successor, so the
    /// pair cannot drift apart in either direction.
    pub expected_predecessor_version: Option<u64>,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// Accepts embedding jobs through the governed-mutation boundary.
///
/// This trait deliberately has one method. Everything an accepted job does
/// afterwards — rediscovery, dispatch, recovery — belongs to
/// `ProviderDispatchRepository`, because a second dispatch authority for
/// embeddings is exactly what P04 exists to avoid.
#[async_trait]
pub trait EmbeddingJobRepository: Send + Sync {
    /// The default refuses. An unconfigured authority that answered `Ok` would
    /// let a caller believe a job exists and then wait forever for a dispatch
    /// nothing will make.
    async fn accept_governed(
        &self,
        _context: RequestContext,
        _command: AcceptEmbeddingJob,
    ) -> Result<GovernedMutationReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding job acceptance is not configured".to_owned(),
        ))
    }
}

pub type SharedEmbeddingJobRepository = Arc<dyn EmbeddingJobRepository>;
