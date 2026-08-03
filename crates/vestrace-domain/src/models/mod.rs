use crate::{
    DomainError,
    id::{ModelId, ProviderId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

pub mod runtime;

pub use runtime::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderLocality {
    Local,
    Remote,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelCostProfile {
    pub input_cost_per_mtoken: f32,
    pub output_cost_per_mtoken: f32,
}

impl ModelCostProfile {
    pub fn new(input_cost: f32, output_cost: f32) -> Result<Self, DomainError> {
        if input_cost < 0.0 || output_cost < 0.0 {
            return Err(DomainError::InvalidArgument(
                "costs per million tokens cannot be negative".into(),
            ));
        }
        Ok(Self {
            input_cost_per_mtoken: input_cost,
            output_cost_per_mtoken: output_cost,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelProfile {
    pub id: ModelId,
    pub provider_id: ProviderId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub context_window: u32,
    pub cost: ModelCostProfile,
    pub created_at: Timestamp,
}
