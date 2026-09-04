//! The one authorized acknowledgement of a carried transition ambiguity.

use crate::RequestContext;
use vestrace_domain::embedding::{
    TransitionInputOrdinal, TransitionRecipeOrdinal, TransitionVersion,
};

#[derive(Clone, Debug)]
pub struct CarryRecipeMapping {
    pub old_recipe: TransitionRecipeOrdinal,
    pub old_input: TransitionInputOrdinal,
    pub new_recipe: TransitionRecipeOrdinal,
    pub new_input: TransitionInputOrdinal,
}

#[derive(Clone, Debug)]
pub struct AcknowledgeCarriedTransitionBatchAfterUnknown {
    pub transition_id: uuid::Uuid,
    pub carry_id: uuid::Uuid,
    pub predecessor_embedding_job_id: uuid::Uuid,
    pub predecessor_version: TransitionVersion,
    pub predecessor_transition_batch_id: uuid::Uuid,
    pub successor_transition_batch_id: uuid::Uuid,
    pub successor_embedding_job_id: uuid::Uuid,
    pub space_registration_id: uuid::Uuid,
    pub kind: String,
    pub model_binding_snapshot_id: uuid::Uuid,
    pub external_effect_id: uuid::Uuid,
    pub model_request_evidence_id: uuid::Uuid,
    pub mappings: Vec<CarryRecipeMapping>,
}

impl AcknowledgeCarriedTransitionBatchAfterUnknown {
    pub fn context_tuple(&self, _context: &RequestContext) -> (&uuid::Uuid, &uuid::Uuid) {
        (&self.transition_id, &self.carry_id)
    }
}
