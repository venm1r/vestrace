pub mod checkpoint;
mod command;
mod decision;
mod error;
mod event;
mod legacy_event;
mod reducer;
mod state;
pub mod status;
pub mod step;
pub mod work;

use crate::DomainError;
use serde::{Deserialize, Serialize};

pub use checkpoint::{RunCheckpoint, RunCheckpointPayload, RunCheckpointPayloadV1};
pub use command::{RunCommand, RunCommandEnvelope};
pub use decision::decide;
pub use error::{RunDecisionError, RunReduceError, RunReplayError};
pub use event::{RunEvent, RunEventPayload};
pub use legacy_event::{LegacyRunEvent, LegacyRunEventEnvelope, PendingRunEvent};
pub use reducer::{apply, replay};
pub use state::{RunActor, RunCompletion, RunState, RunStepState, RunWait, StallReason};
pub use status::{RunExecutionMode, RunStatus, RunStepStatus};
pub use step::{
    AgentRun, NewAgentRun, NewRunStep, ParentRunLink, RunActorRef, RunCurrentStepChange,
    RunFailure, RunPlanChange, RunReference, RunReferenceKind, RunStatusChange, RunStep,
    RunTerminalResult,
};
pub use work::{RunWorkPayload, WorkItem, WorkItemKind, WorkItemStatus};

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

    pub fn previous(self) -> Result<Self, DomainError> {
        if self.0 <= 1 {
            Err(DomainError::InvalidArgument(
                "run version has no previous".into(),
            ))
        } else {
            Ok(Self(self.0 - 1))
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct ResumeCursor(u64);

impl ResumeCursor {
    pub const BEFORE_FIRST: Self = Self(0);

    pub const fn from_version(version: RunVersion) -> Self {
        Self(version.value())
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}
