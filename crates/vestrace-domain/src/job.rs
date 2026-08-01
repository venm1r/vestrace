use crate::{
    id::{JobId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Leased,
    Completed,
    Failed,
    DeadLetter,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Job {
    pub id: JobId,
    pub workspace_id: WorkspaceId,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub state: JobState,
    pub attempts: u32,
    pub max_attempts: u32,
    pub run_at: Timestamp,
    pub leased_until: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
