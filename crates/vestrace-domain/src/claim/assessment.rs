use crate::{
    Confidence,
    id::{ClaimAssessmentId, ClaimId, PrincipalId, WorkspaceId},
    time::Timestamp,
};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentKind {
    ModelHeuristic,
    PolicyEvaluation,
    HumanJudgment,
    ConsensusVote,
    AutomatedCheck,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ClaimAssessment {
    pub assessment_id: ClaimAssessmentId,
    pub claim_id: ClaimId,
    pub workspace_id: WorkspaceId,
    pub assessment_kind: AssessmentKind,
    pub confidence: Confidence,
    pub basis_refs: Vec<crate::EvidenceRef>,
    pub policy_version: String,
    pub assessor: PrincipalId,
    pub created_at: Timestamp,
}

impl ClaimAssessment {
    pub fn new(
        assessment_id: ClaimAssessmentId,
        claim_id: ClaimId,
        workspace_id: WorkspaceId,
        assessment_kind: AssessmentKind,
        confidence: Confidence,
        policy_version: String,
        assessor: PrincipalId,
        at: Timestamp,
    ) -> Self {
        Self {
            assessment_id,
            claim_id,
            workspace_id,
            assessment_kind,
            confidence,
            basis_refs: Vec::new(),
            policy_version,
            assessor,
            created_at: at,
        }
    }
}
