use super::{RunStatus, RunVersion};
use crate::id::{AgentRunId, WorkspaceId};

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RunDecisionError {
    #[error("run already exists")]
    AlreadyExists,
    #[error("run does not exist")]
    MissingState,
    #[error("run version conflict: expected {expected:?}, actual {actual:?}")]
    VersionConflict {
        expected: RunVersion,
        actual: RunVersion,
    },
    #[error("invalid run command: {0}")]
    InvalidCommand(String),
    #[error("command is not valid while run is {status:?}")]
    InvalidTransition { status: RunStatus },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RunReduceError {
    #[error("the first run event must be run.created")]
    MissingCreatedEvent,
    #[error("run.created cannot be applied to an existing run")]
    DuplicateCreatedEvent,
    #[error("run version overflow")]
    VersionOverflow,
    #[error("run event sequence mismatch: expected {expected:?}, actual {actual:?}")]
    SequenceMismatch {
        expected: RunVersion,
        actual: RunVersion,
    },
    #[error("run event workspace mismatch: expected {expected}, actual {actual}")]
    WorkspaceMismatch {
        expected: WorkspaceId,
        actual: WorkspaceId,
    },
    #[error("run event aggregate mismatch: expected {expected}, actual {actual}")]
    RunMismatch {
        expected: AgentRunId,
        actual: AgentRunId,
    },
    #[error("run event type mismatch: expected {expected}, actual {actual}")]
    EventTypeMismatch { expected: String, actual: String },
    #[error("unsupported run event version {version} for {event_type}")]
    UnsupportedEventVersion { event_type: String, version: u16 },
    #[error("run event cannot be applied after terminal state {status:?}")]
    EventAfterTerminal { status: RunStatus },
    #[error("run event is invalid while run is {status:?}")]
    InvalidTransition { status: RunStatus },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RunReplayError {
    #[error(transparent)]
    Reduce(#[from] RunReduceError),
}
