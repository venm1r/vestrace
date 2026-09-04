//! Guarded creation of immutable embedding-transition versions.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    TransitionBatchId,
    embedding::{TransitionInputOrdinal, TransitionRecipeOrdinal, TransitionVersion},
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
}

pub type SharedEmbeddingTransitionRepository = Arc<dyn EmbeddingTransitionRepository>;
