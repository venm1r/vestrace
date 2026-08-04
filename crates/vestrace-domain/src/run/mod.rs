mod command;
mod decision;
mod error;
mod event;
mod reducer;
mod state;

use crate::{
    DomainError,
    id::{AgentRunId, PrincipalId, WorkspaceId},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

pub use command::{RunCommand, RunCommandEnvelope};
pub use decision::decide;
pub use error::{RunDecisionError, RunReduceError, RunReplayError};
pub use event::{PendingRunEvent, RunEvent, RunEventEnvelope};
pub use reducer::{apply, replay};
pub use state::{
    RunActor, RunCompletion, RunState, RunStepState, RunStepStatus, RunWait, StallReason,
};

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct RunVersion(u64);

impl RunVersion {
    pub const ZERO: Self = Self(0);
    pub const INITIAL: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            Err(DomainError::InvalidArgument(
                "run version must be positive".into(),
            ))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Result<Self, DomainError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or_else(|| DomainError::InvalidArgument("run version overflow".into()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Running,
    WaitingForInput,
    WaitingForApproval,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentRun {
    pub id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub title: String,
    pub status: RunStatus,
    pub version: RunVersion,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl AgentRun {
    pub fn new(
        id: AgentRunId,
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        title: impl Into<String>,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            workspace_id,
            principal_id,
            title: title.into(),
            status: RunStatus::Created,
            version: RunVersion::INITIAL,
            created_at: at,
            updated_at: at,
        }
    }
}
