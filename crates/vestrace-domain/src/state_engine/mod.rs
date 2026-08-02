use crate::{
    id::{AgentRunId, RunExportId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureProfile {
    Minimal,
    Operational,
    Reproducible,
    Forensic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SignedRunExport {
    pub id: RunExportId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub signature: String,
    pub profile: CaptureProfile,
    pub created_at: Timestamp,
}
