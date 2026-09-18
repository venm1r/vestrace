use crate::{
    id::{ModelExecutionAttemptId, ModelExecutionId, ProviderId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Success,
    Failed,
    FallbackTriggered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelExecutionAttempt {
    pub id: ModelExecutionAttemptId,
    pub model_execution_id: ModelExecutionId,
    pub workspace_id: WorkspaceId,
    pub attempt_number: u32,
    pub provider_id: ProviderId,
    pub status: AttemptStatus,
    pub error_message: Option<String>,
    pub created_at: Timestamp,
}
