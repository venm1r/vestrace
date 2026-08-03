use crate::{
    DomainError,
    id::{ContextPackId, MemoryId, RetrievalRunId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalIntent {
    SemanticRecall,
    CurrentState,
    DecisionRecall,
    Timeline,
    TaskResume,
    ProcedureLookup,
    UserPreferences,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RetrievalCandidate {
    pub memory_id: MemoryId,
    pub score: f32,
    pub explanation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextPack {
    pub id: ContextPackId,
    pub retrieval_run_id: RetrievalRunId,
    pub workspace_id: WorkspaceId,
    pub token_budget: u32,
    pub used_tokens: u32,
    pub candidate_ids: Vec<MemoryId>,
    pub created_at: Timestamp,
}

impl ContextPack {
    pub fn new(
        id: ContextPackId,
        retrieval_run_id: RetrievalRunId,
        workspace_id: WorkspaceId,
        token_budget: u32,
        used_tokens: u32,
        candidate_ids: Vec<MemoryId>,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if used_tokens > token_budget {
            return Err(DomainError::InvalidArgument(
                "used_tokens cannot exceed token_budget".into(),
            ));
        }
        Ok(Self {
            id,
            retrieval_run_id,
            workspace_id,
            token_budget,
            used_tokens,
            candidate_ids,
            created_at: at,
        })
    }
}
