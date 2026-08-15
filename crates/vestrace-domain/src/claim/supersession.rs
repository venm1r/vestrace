use crate::{
    id::{ClaimId, MemoryId, MemoryRevisionId, SupersessionLinkId, WorkspaceId},
    time::Timestamp,
};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionTargetKind {
    Claim,
    MemoryRevision,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SupersessionLink {
    pub link_id: SupersessionLinkId,
    pub workspace_id: WorkspaceId,
    pub target_kind: SupersessionTargetKind,
    pub superseded_claim_id: Option<ClaimId>,
    pub superseded_memory_id: Option<MemoryId>,
    pub superseded_revision_id: Option<MemoryRevisionId>,
    pub replacement_claim_id: Option<ClaimId>,
    pub replacement_memory_id: Option<MemoryId>,
    pub replacement_revision_id: Option<MemoryRevisionId>,
    pub reason: String,
    pub created_at: Timestamp,
}

impl SupersessionLink {
    pub fn for_claim(
        link_id: SupersessionLinkId,
        workspace_id: WorkspaceId,
        superseded: ClaimId,
        replacement: ClaimId,
        reason: String,
        at: Timestamp,
    ) -> Self {
        Self {
            link_id,
            workspace_id,
            target_kind: SupersessionTargetKind::Claim,
            superseded_claim_id: Some(superseded),
            superseded_memory_id: None,
            superseded_revision_id: None,
            replacement_claim_id: Some(replacement),
            replacement_memory_id: None,
            replacement_revision_id: None,
            reason,
            created_at: at,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn for_memory_revision(
        link_id: SupersessionLinkId,
        workspace_id: WorkspaceId,
        superseded_memory: MemoryId,
        superseded_revision: MemoryRevisionId,
        replacement_memory: MemoryId,
        replacement_revision: MemoryRevisionId,
        reason: String,
        at: Timestamp,
    ) -> Self {
        Self {
            link_id,
            workspace_id,
            target_kind: SupersessionTargetKind::MemoryRevision,
            superseded_claim_id: None,
            superseded_memory_id: Some(superseded_memory),
            superseded_revision_id: Some(superseded_revision),
            replacement_claim_id: None,
            replacement_memory_id: Some(replacement_memory),
            replacement_revision_id: Some(replacement_revision),
            reason,
            created_at: at,
        }
    }
}
