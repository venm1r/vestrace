use crate::{
    id::{AgentId, SkillId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Agent {
    pub id: AgentId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Skill {
    pub id: SkillId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub instructions: String,
    pub created_at: Timestamp,
}
