use crate::{
    EvidenceRef, EvidenceRole,
    id::{ClaimEvidenceLinkId, ClaimId, PrincipalId, WorkspaceId},
    time::Timestamp,
};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceClassification {
    Direct,
    Inferred,
    Imported,
    Federated,
    HumanAuthored,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClaimEvidenceLink {
    pub link_id: ClaimEvidenceLinkId,
    pub claim_id: ClaimId,
    pub workspace_id: WorkspaceId,
    pub evidence_ref: EvidenceRef,
    pub role: EvidenceRole,
    pub evidence_weight_metadata: Option<serde_json::Value>,
    pub source_classification: SourceClassification,
    pub added_by: PrincipalId,
    pub created_at: Timestamp,
}

impl ClaimEvidenceLink {
    pub fn new(
        link_id: ClaimEvidenceLinkId,
        claim_id: ClaimId,
        workspace_id: WorkspaceId,
        evidence_ref: EvidenceRef,
        role: EvidenceRole,
        source_classification: SourceClassification,
        added_by: PrincipalId,
        at: Timestamp,
    ) -> Self {
        Self {
            link_id,
            claim_id,
            workspace_id,
            evidence_ref,
            role,
            evidence_weight_metadata: None,
            source_classification,
            added_by,
            created_at: at,
        }
    }
}
