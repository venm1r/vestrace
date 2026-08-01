use crate::{
    id::{DerivationId, EventId, MemoryId, MemorySourceId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole {
    DirectSource,
    SupportingContext,
    ContradictingEvidence,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DerivationMethod {
    LlmExtraction { model: String, prompt_version: String },
    RuleBased { rule_id: String },
    ManualConsolidation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Derivation {
    pub id: DerivationId,
    pub workspace_id: WorkspaceId,
    pub method: DerivationMethod,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemorySource {
    pub id: MemorySourceId,
    pub memory_id: MemoryId,
    pub workspace_id: WorkspaceId,
    pub event_id: EventId,
    pub role: EvidenceRole,
    pub derivation_id: Option<DerivationId>,
    pub created_at: Timestamp,
}

impl MemorySource {
    pub fn new_direct(
        id: MemorySourceId,
        memory_id: MemoryId,
        workspace_id: WorkspaceId,
        event_id: EventId,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            memory_id,
            workspace_id,
            event_id,
            role: EvidenceRole::DirectSource,
            derivation_id: None,
            created_at: at,
        }
    }

    pub fn new_derived(
        id: MemorySourceId,
        memory_id: MemoryId,
        workspace_id: WorkspaceId,
        event_id: EventId,
        role: EvidenceRole,
        derivation_id: DerivationId,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            memory_id,
            workspace_id,
            event_id,
            role,
            derivation_id: Some(derivation_id),
            created_at: at,
        }
    }
}
