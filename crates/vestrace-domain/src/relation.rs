use crate::{
    Confidence, DomainError,
    id::{MemoryId, RelationId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    Supports,
    Contradicts,
    Extends,
    Refines,
    DerivedFrom,
    RelatesTo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct KnowledgeRelation {
    pub id: RelationId,
    pub workspace_id: WorkspaceId,
    pub source_memory_id: MemoryId,
    pub target_memory_id: MemoryId,
    pub relation_type: RelationType,
    pub confidence: Confidence,
    pub created_at: Timestamp,
}

impl KnowledgeRelation {
    pub fn new(
        id: RelationId,
        workspace_id: WorkspaceId,
        source_memory_id: MemoryId,
        target_memory_id: MemoryId,
        relation_type: RelationType,
        confidence: Confidence,
        at: Timestamp,
    ) -> Result<Self, DomainError> {
        if source_memory_id == target_memory_id {
            return Err(DomainError::InvalidArgument(
                "relation source and target memory must be distinct".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            source_memory_id,
            target_memory_id,
            relation_type,
            confidence,
            created_at: at,
        })
    }
}
