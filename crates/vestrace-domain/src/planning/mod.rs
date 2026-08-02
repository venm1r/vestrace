use crate::{
    id::{AgentRunId, ExecutionPlanId, ExecutionPlanRevisionId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanningMode {
    Direct,
    Guided,
    Workflow,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecutionPlan {
    pub id: ExecutionPlanId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub title: String,
    pub mode: PlanningMode,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecutionPlanRevision {
    pub id: ExecutionPlanRevisionId,
    pub plan_id: ExecutionPlanId,
    pub workspace_id: WorkspaceId,
    pub revision_number: u32,
    pub content_hash: String,
    pub steps: Vec<serde_json::Value>,
    pub created_at: Timestamp,
}
