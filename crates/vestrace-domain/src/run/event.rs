use crate::{
    DomainError,
    id::{
        AgentRunId, AgentRuntimeSnapshotId, CorrelationId, PlanRevisionId, RunCheckpointId,
        RunEventId, RunStepId, WorkspaceId,
    },
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

use super::status::{RunExecutionMode, RunStatus, RunStepStatus};
use super::step::{ParentRunLink, RunActorRef, RunStep, RunTerminalResult};
use super::{ResumeCursor, RunVersion};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RunEventPayload {
    RunCreated {
        objective: String,
        execution_mode: RunExecutionMode,
        coordinator_snapshot_id: AgentRuntimeSnapshotId,
        parent: Option<ParentRunLink>,
    },
    RunStatusChanged {
        from: RunStatus,
        to: RunStatus,
        result: Option<RunTerminalResult>,
    },
    PlanAttached {
        previous: Option<PlanRevisionId>,
        current: PlanRevisionId,
    },
    StepsAdded {
        steps: Vec<RunStep>,
    },
    StepStatusChanged {
        step_id: RunStepId,
        from: RunStepStatus,
        to: RunStepStatus,
        attempt: u32,
    },
    CurrentStepChanged {
        previous: Option<RunStepId>,
        current: Option<RunStepId>,
    },
    CheckpointCreated {
        checkpoint_id: RunCheckpointId,
        resume_cursor: ResumeCursor,
    },
    /// An approval was granted, naming who granted it.
    ///
    /// Distinct from a `RunStatusChanged` back to `Running` on purpose: an
    /// approval names its approver, and collapsing the two would erase that
    /// from the canonical history. The status change is emitted as well, so a
    /// reader reconstructing state does not need to interpret this event.
    ApprovalGranted {
        approver_id: crate::id::PrincipalId,
    },
}

impl RunEventPayload {
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::RunCreated { .. } => "run.created",
            Self::RunStatusChanged { .. } => "run.status_changed",
            Self::PlanAttached { .. } => "run.plan_attached",
            Self::StepsAdded { .. } => "run.steps_added",
            Self::StepStatusChanged { .. } => "run.step_status_changed",
            Self::CurrentStepChanged { .. } => "run.current_step_changed",
            Self::CheckpointCreated { .. } => "run.checkpoint_created",
            // Matches the legacy event's type string, so a reader of the
            // canonical log does not have to know which writer produced a
            // historical row.
            Self::ApprovalGranted { .. } => "run.approval_granted",
        }
    }

    pub fn is_creation(&self) -> bool {
        matches!(self, Self::RunCreated { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunEvent {
    pub id: RunEventId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub sequence: ResumeCursor,
    pub run_version: RunVersion,
    pub actor: RunActorRef,
    pub payload: RunEventPayload,
    pub correlation_id: CorrelationId,
    pub causation_event_id: Option<RunEventId>,
    pub occurred_at: Timestamp,
}

impl RunEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: AgentRunId,
        workspace_id: WorkspaceId,
        run_version: RunVersion,
        sequence: ResumeCursor,
        actor: RunActorRef,
        payload: RunEventPayload,
        correlation_id: CorrelationId,
        causation_event_id: Option<RunEventId>,
        occurred_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if sequence != ResumeCursor::from_version(run_version) {
            return Err(DomainError::InvalidArgument(format!(
                "event sequence {} must equal run version {}",
                sequence.value(),
                run_version.value()
            )));
        }
        if payload.is_creation() && run_version != RunVersion::INITIAL {
            return Err(DomainError::InvalidArgument(
                "RunCreated event must have version 1".into(),
            ));
        }
        if !payload.is_creation() && run_version == RunVersion::INITIAL {
            return Err(DomainError::InvalidArgument(
                "non-creation event must have version greater than 1".into(),
            ));
        }
        let id = RunEventId::new();
        if let Some(causation) = causation_event_id {
            if causation == id {
                return Err(DomainError::InvalidArgument(
                    "causation event id must not self-reference".into(),
                ));
            }
        }
        Ok(Self {
            id,
            run_id,
            workspace_id,
            sequence,
            run_version,
            actor,
            payload,
            correlation_id,
            causation_event_id,
            occurred_at,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_id(
        id: RunEventId,
        run_id: AgentRunId,
        workspace_id: WorkspaceId,
        run_version: RunVersion,
        sequence: ResumeCursor,
        actor: RunActorRef,
        payload: RunEventPayload,
        correlation_id: CorrelationId,
        causation_event_id: Option<RunEventId>,
        occurred_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if sequence != ResumeCursor::from_version(run_version) {
            return Err(DomainError::InvalidArgument(format!(
                "event sequence {} must equal run version {}",
                sequence.value(),
                run_version.value()
            )));
        }
        if payload.is_creation() && run_version != RunVersion::INITIAL {
            return Err(DomainError::InvalidArgument(
                "RunCreated event must have version 1".into(),
            ));
        }
        if !payload.is_creation() && run_version == RunVersion::INITIAL {
            return Err(DomainError::InvalidArgument(
                "non-creation event must have version greater than 1".into(),
            ));
        }
        if let Some(causation) = causation_event_id {
            if causation == id {
                return Err(DomainError::InvalidArgument(
                    "causation event id must not self-reference".into(),
                ));
            }
        }
        Ok(Self {
            id,
            run_id,
            workspace_id,
            sequence,
            run_version,
            actor,
            payload,
            correlation_id,
            causation_event_id,
            occurred_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::AgentRuntimeSnapshotId;

    fn run_id() -> AgentRunId {
        AgentRunId::new()
    }

    fn workspace_id() -> WorkspaceId {
        WorkspaceId::new()
    }

    fn correlation_id() -> CorrelationId {
        CorrelationId::new()
    }

    fn now() -> Timestamp {
        use chrono::TimeZone;
        chrono::Utc
            .with_ymd_and_hms(2026, 8, 8, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn creation_payload() -> RunEventPayload {
        RunEventPayload::RunCreated {
            objective: "Test objective".into(),
            execution_mode: RunExecutionMode::Autopilot,
            coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
            parent: None,
        }
    }

    fn status_change_payload() -> RunEventPayload {
        RunEventPayload::RunStatusChanged {
            from: RunStatus::Created,
            to: RunStatus::Preparing,
            result: None,
        }
    }

    #[test]
    fn creation_event_at_version_one_is_valid() {
        let event = RunEvent::new(
            run_id(),
            workspace_id(),
            RunVersion::INITIAL,
            ResumeCursor::from_version(RunVersion::INITIAL),
            RunActorRef::System,
            creation_payload(),
            correlation_id(),
            None,
            now(),
        );
        assert!(event.is_ok());
    }

    #[test]
    fn creation_event_at_version_two_is_invalid() {
        let v2 = RunVersion::new(2).unwrap();
        let event = RunEvent::new(
            run_id(),
            workspace_id(),
            v2,
            ResumeCursor::from_version(v2),
            RunActorRef::System,
            creation_payload(),
            correlation_id(),
            None,
            now(),
        );
        assert!(event.is_err());
    }

    #[test]
    fn non_creation_event_at_version_one_is_invalid() {
        let event = RunEvent::new(
            run_id(),
            workspace_id(),
            RunVersion::INITIAL,
            ResumeCursor::from_version(RunVersion::INITIAL),
            RunActorRef::System,
            status_change_payload(),
            correlation_id(),
            None,
            now(),
        );
        assert!(event.is_err());
    }

    #[test]
    fn sequence_must_equal_version() {
        let v3 = RunVersion::new(3).unwrap();
        let v2 = RunVersion::new(2).unwrap();
        let event = RunEvent::new(
            run_id(),
            workspace_id(),
            v3,
            ResumeCursor::from_version(v2),
            RunActorRef::System,
            status_change_payload(),
            correlation_id(),
            None,
            now(),
        );
        assert!(event.is_err());
    }

    #[test]
    fn causation_cannot_self_reference() {
        let id = RunEventId::new();
        let event = RunEvent::with_id(
            id,
            run_id(),
            workspace_id(),
            RunVersion::new(2).unwrap(),
            ResumeCursor::from_version(RunVersion::new(2).unwrap()),
            RunActorRef::System,
            status_change_payload(),
            correlation_id(),
            Some(id),
            now(),
        );
        assert!(event.is_err());
    }
}
