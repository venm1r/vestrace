//! Guarded creation of immutable embedding-transition versions.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    EmbeddingJobId, TransitionBatchId,
    embedding::{
        EmbeddingSpaceTransitionState, TransitionInputOrdinal, TransitionRecipeOrdinal,
        TransitionVersion,
    },
};

use super::carry::AcknowledgeCarriedTransitionBatchAfterUnknown;
use crate::{ApplicationError, RequestContext};

/// One immutable recipe in the transition wire batch.
#[derive(Clone, Debug)]
pub struct TransitionPlanRecipe {
    pub identity: uuid::Uuid,
    pub ordinal: TransitionRecipeOrdinal,
    pub input_ordinals: Vec<TransitionInputOrdinal>,
}

/// The complete caller-stated tuple from which a transition version is planned.
#[derive(Clone, Debug)]
pub struct PlanEmbeddingTransitionVersion {
    pub transition_id: uuid::Uuid,
    pub plan_id: uuid::Uuid,
    pub version: TransitionVersion,
    pub source_connection_id: uuid::Uuid,
    pub source_connection_revision_id: uuid::Uuid,
    pub source_branch: TransitionAuthBinding,
    pub target_connection_id: uuid::Uuid,
    pub target_connection_revision_id: uuid::Uuid,
    pub target_connection_qualification_revision_id: uuid::Uuid,
    pub target_model_revision_id: uuid::Uuid,
    pub target_model_qualification_revision_id: uuid::Uuid,
    pub target_branch: TransitionAuthBinding,
    pub target_space_registration_id: uuid::Uuid,
    pub batch_id: TransitionBatchId,
    pub snapshot_id: uuid::Uuid,
    pub recipes: Vec<TransitionPlanRecipe>,
}

/// Binds one immutable transition recipe to one fresh physical embedding job.
/// The SQL authority derives the eventual satisfaction kind from terminal
/// facts; callers provide no kind selector.
#[derive(Clone, Debug)]
pub struct CreateEmbeddingTransitionBatchAttempt {
    pub plan_id: uuid::Uuid,
    pub batch_id: TransitionBatchId,
    pub attempt_id: uuid::Uuid,
    pub job_id: EmbeddingJobId,
    pub recipe_ordinal: TransitionRecipeOrdinal,
    pub old_projection_id: uuid::Uuid,
    pub target_input_ordinal: TransitionInputOrdinal,
    pub expected_job_version: u64,
}

/// Asks SQL to derive the single satisfier for one exactly bound recipe.
/// `None` selects only the Existing branch; a present attempt selects only the
/// terminal-result branch after its own version fence has been checked.
#[derive(Clone, Debug)]
pub struct ObserveEmbeddingTransitionAttempt {
    pub plan_id: uuid::Uuid,
    pub batch_id: TransitionBatchId,
    pub recipe_ordinal: TransitionRecipeOrdinal,
    pub attempt_id: Option<uuid::Uuid>,
    pub expected_job_version: Option<u64>,
}

/// The single authority that may expose an exact, fully satisfied batch as
/// ready for activation.
#[derive(Clone, Debug)]
pub struct ProveEmbeddingTransitionCompleteness {
    pub transition_id: uuid::Uuid,
    pub plan_id: uuid::Uuid,
    pub batch_id: TransitionBatchId,
    pub expected_transition_version: TransitionVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddingTransitionProgress {
    pub state: EmbeddingSpaceTransitionState,
}

/// The complete caller-stated tuple that asks SQL to activate one proven
/// transition.  Callers supply identities and expected versions only: the
/// allowed guard set, the credential lineage and every retirement event are
/// derived by the activation authority from durable facts.
#[derive(Clone, Debug)]
pub struct ActivateEmbeddingTransition {
    pub transition_id: uuid::Uuid,
    pub plan_id: uuid::Uuid,
    pub batch_id: TransitionBatchId,
    pub expected_transition_version: TransitionVersion,
    pub expected_qualification_head_version: u64,
    pub audit_event_id: uuid::Uuid,
}

/// The immutable record of one activation.  `source_credential_id` and
/// `target_credential_id` are absent on the matching no-auth branch, so a
/// no-auth-to-no-auth activation carries neither.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionActivationReceipt {
    pub receipt_id: uuid::Uuid,
    pub transition_id: uuid::Uuid,
    pub target_qualification_id: uuid::Uuid,
    pub target_space_id: uuid::Uuid,
    pub source_credential_id: Option<uuid::Uuid>,
    pub target_credential_id: Option<uuid::Uuid>,
    pub resulting_qualification_head_version: u64,
}

/// The auth-binding XOR carried by both source and target plan tuples.
#[derive(Clone, Debug)]
pub enum TransitionAuthBinding {
    Credential {
        credential_revision_id: uuid::Uuid,
        credential_slot_id: Option<uuid::Uuid>,
        credential_activation_guard_id: Option<uuid::Uuid>,
        expected_slot_version: Option<u64>,
    },
    NoAuth {
        binding_revision_id: uuid::Uuid,
    },
}

#[async_trait]
pub trait EmbeddingTransitionRepository: Send + Sync {
    async fn plan_version(
        &self,
        _context: RequestContext,
        _command: PlanEmbeddingTransitionVersion,
    ) -> Result<uuid::Uuid, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding transition planning is not configured".to_owned(),
        ))
    }

    async fn acknowledge_carried_batch_after_unknown(
        &self,
        _context: RequestContext,
        _command: AcknowledgeCarriedTransitionBatchAfterUnknown,
    ) -> Result<uuid::Uuid, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding transition carry acknowledgement is not configured".to_owned(),
        ))
    }

    async fn create_batch_attempt(
        &self,
        _context: RequestContext,
        _command: CreateEmbeddingTransitionBatchAttempt,
    ) -> Result<EmbeddingJobId, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding transition execution is not configured".to_owned(),
        ))
    }

    async fn observe_attempt(
        &self,
        _context: RequestContext,
        _command: ObserveEmbeddingTransitionAttempt,
    ) -> Result<EmbeddingTransitionProgress, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding transition execution is not configured".to_owned(),
        ))
    }

    async fn prove_completeness(
        &self,
        _context: RequestContext,
        _command: ProveEmbeddingTransitionCompleteness,
    ) -> Result<EmbeddingTransitionProgress, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding transition execution is not configured".to_owned(),
        ))
    }

    async fn activate(
        &self,
        _context: RequestContext,
        _command: ActivateEmbeddingTransition,
    ) -> Result<TransitionActivationReceipt, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "governed embedding transition activation is not configured".to_owned(),
        ))
    }
}

pub type SharedEmbeddingTransitionRepository = Arc<dyn EmbeddingTransitionRepository>;
