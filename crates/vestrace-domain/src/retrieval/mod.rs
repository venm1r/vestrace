use crate::{
    DomainError,
    id::{ContextPackId, MemoryId, MemoryRevisionId, RetrievalRunId, WorkspaceId},
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
    ErrorRecovery,
    ModelSelection,
    WorkflowContext,
    Exploration,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimePerspective {
    Current,
    AsOf(Timestamp),
    Timeline,
    AllHistory,
}

impl Default for TimePerspective {
    fn default() -> Self {
        Self::Current
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RetrievalCandidate {
    pub memory_id: MemoryId,
    pub revision_id: Option<MemoryRevisionId>,
    pub score: f32,
    pub channel_rank: u32,
    pub channel: String,
    pub explanation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreComponents {
    pub fused_rank: f32,
    pub scope_match: f32,
    pub kind_match: f32,
    pub importance: f32,
    pub confidence: f32,
    pub recency: f32,
    pub provenance_quality: f32,
    pub redundancy_penalty: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RepresentationLevel {
    Full,
    Summary,
    Atomic,
    Reference,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextItem {
    pub memory_id: MemoryId,
    pub revision_id: Option<MemoryRevisionId>,
    pub representation: RepresentationLevel,
    pub rendered_text: String,
    pub accounted_tokens: u32,
    pub source_ids: Vec<String>,
    pub inclusion_explanation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextSection {
    pub label: String,
    pub items: Vec<ContextItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextPack {
    pub id: ContextPackId,
    pub retrieval_run_id: RetrievalRunId,
    pub workspace_id: WorkspaceId,
    pub token_budget: u32,
    pub used_tokens: u32,
    pub candidate_ids: Vec<MemoryId>,
    pub sections: Vec<ContextSection>,
    pub degraded: bool,
    pub warnings: Vec<String>,
    pub created_at: Timestamp,
}

impl ContextPack {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ContextPackId,
        retrieval_run_id: RetrievalRunId,
        workspace_id: WorkspaceId,
        token_budget: u32,
        used_tokens: u32,
        candidate_ids: Vec<MemoryId>,
        sections: Vec<ContextSection>,
        degraded: bool,
        warnings: Vec<String>,
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
            sections,
            degraded,
            warnings,
            created_at: at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_pack_rejects_used_tokens_above_budget() {
        let result = ContextPack::new(
            ContextPackId::new(),
            RetrievalRunId::new(),
            WorkspaceId::new(),
            100,
            101,
            Vec::new(),
            Vec::new(),
            false,
            Vec::new(),
            crate::now(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn context_pack_accepts_used_tokens_equal_to_budget() {
        let result = ContextPack::new(
            ContextPackId::new(),
            RetrievalRunId::new(),
            WorkspaceId::new(),
            100,
            100,
            Vec::new(),
            Vec::new(),
            false,
            Vec::new(),
            crate::now(),
        );
        assert!(result.is_ok());
    }
}
