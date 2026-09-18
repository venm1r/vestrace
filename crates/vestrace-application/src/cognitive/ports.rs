use std::sync::Arc;

use async_trait::async_trait;

use crate::{ApplicationError, RequestContext};
use vestrace_domain::{
    id::{AgentId, SkillId, WorkspaceId},
    time::Timestamp,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AgentRecord {
    pub id: AgentId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkillRecord {
    pub id: SkillId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub instructions: String,
    pub created_at: Timestamp,
}

#[async_trait]
pub trait AgentRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        agent: &AgentRecord,
    ) -> Result<(), ApplicationError>;
    async fn list(&self, context: &RequestContext) -> Result<Vec<AgentRecord>, ApplicationError>;
    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: AgentId,
    ) -> Result<Option<AgentRecord>, ApplicationError>;
}

#[async_trait]
pub trait SkillRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        skill: &SkillRecord,
    ) -> Result<(), ApplicationError>;
    async fn list(&self, context: &RequestContext) -> Result<Vec<SkillRecord>, ApplicationError>;
}

pub type SharedAgentRepository = Arc<dyn AgentRepository>;
pub type SharedSkillRepository = Arc<dyn SkillRepository>;
