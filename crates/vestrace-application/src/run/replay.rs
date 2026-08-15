use std::collections::HashMap;

use vestrace_domain::{
    DomainError,
    id::{
        AgentRunId, AgentRuntimeSnapshotId, PlanRevisionId, RunCheckpointId, RunStepId, WorkspaceId,
    },
    run::{
        ParentRunLink, RunEvent, RunEventPayload, RunExecutionMode, RunStatus, RunStep,
        RunTerminalResult, RunVersion,
    },
    time::Timestamp,
};

use crate::ApplicationError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunProjection {
    pub run_id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub status: RunStatus,
    pub version: RunVersion,
    pub objective: String,
    pub execution_mode: RunExecutionMode,
    pub coordinator_snapshot_id: AgentRuntimeSnapshotId,
    pub parent: Option<ParentRunLink>,
    pub root_run_id: AgentRunId,
    pub active_plan_revision_id: Option<PlanRevisionId>,
    pub current_step_id: Option<RunStepId>,
    pub checkpoint_id: Option<RunCheckpointId>,
    pub known_steps: HashMap<RunStepId, RunStep>,
    pub result: Option<RunTerminalResult>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

pub fn replay_run(events: &[RunEvent]) -> Result<RunProjection, ApplicationError> {
    if events.is_empty() {
        return Err(DomainError::InvalidArgument("event stream is empty".into()).into());
    }

    let first = &events[0];
    let RunEventPayload::RunCreated {
        objective,
        execution_mode,
        coordinator_snapshot_id,
        parent,
    } = &first.payload
    else {
        return Err(DomainError::InvalidArgument("first event must be RunCreated".into()).into());
    };

    if first.run_version != RunVersion::INITIAL {
        return Err(DomainError::InvalidArgument("RunCreated must have version 1".into()).into());
    }

    let run_id = first.run_id;
    let workspace_id = first.workspace_id;
    let root_run_id = run_id;

    let mut projection = RunProjection {
        run_id,
        workspace_id,
        status: RunStatus::Created,
        version: first.run_version,
        objective: objective.clone(),
        execution_mode: *execution_mode,
        coordinator_snapshot_id: *coordinator_snapshot_id,
        parent: *parent,
        root_run_id,
        active_plan_revision_id: None,
        current_step_id: None,
        checkpoint_id: None,
        known_steps: HashMap::new(),
        result: None,
        created_at: first.occurred_at,
        updated_at: first.occurred_at,
        finished_at: None,
    };

    let mut expected_version = RunVersion::INITIAL.next().map_err(ApplicationError::from)?;

    for event in &events[1..] {
        if event.run_id != run_id {
            return Err(DomainError::InvalidArgument(
                "all events must belong to the same run".into(),
            )
            .into());
        }
        if event.workspace_id != workspace_id {
            return Err(DomainError::InvalidArgument(
                "all events must belong to the same workspace".into(),
            )
            .into());
        }
        if event.run_version != expected_version {
            return Err(DomainError::InvalidArgument(format!(
                "event version {} does not match expected version {}",
                event.run_version.value(),
                expected_version.value()
            ))
            .into());
        }

        apply_event(&mut projection, event)?;
        projection.version = event.run_version;
        projection.updated_at = event.occurred_at;
        expected_version = expected_version.next().map_err(ApplicationError::from)?;
    }

    Ok(projection)
}

fn apply_event(projection: &mut RunProjection, event: &RunEvent) -> Result<(), ApplicationError> {
    match &event.payload {
        RunEventPayload::RunCreated { .. } => {
            return Err(DomainError::InvalidArgument("duplicate RunCreated event".into()).into());
        }
        RunEventPayload::RunStatusChanged { from, to, result } => {
            if projection.status != *from {
                return Err(DomainError::InvalidArgument(format!(
                    "status change from {from:?} does not match current status {:?}",
                    projection.status
                ))
                .into());
            }
            from.transition_to(*to).map_err(ApplicationError::from)?;
            projection.status = *to;
            if to.is_terminal() {
                projection.result = result.clone();
                projection.finished_at = Some(event.occurred_at);
            }
        }
        RunEventPayload::PlanAttached { previous, current } => {
            if projection.active_plan_revision_id != *previous {
                return Err(DomainError::InvalidArgument(
                    "plan attached previous does not match current active plan".into(),
                )
                .into());
            }
            projection.active_plan_revision_id = Some(*current);
        }
        RunEventPayload::StepsAdded { steps } => {
            for step in steps {
                if projection.known_steps.contains_key(&step.id) {
                    return Err(DomainError::InvalidArgument(
                        "duplicate step id in StepsAdded event".to_string(),
                    )
                    .into());
                }
                projection.known_steps.insert(step.id, step.clone());
            }
        }
        RunEventPayload::StepStatusChanged {
            step_id,
            from,
            to,
            attempt,
        } => {
            let step = projection
                .known_steps
                .get_mut(step_id)
                .ok_or_else(|| {
                    DomainError::InvalidArgument("StepStatusChanged for unknown step".into())
                })
                .map_err(ApplicationError::from)?;
            if step.status != *from {
                return Err(DomainError::InvalidArgument(format!(
                    "step status change from {from:?} does not match current status {:?}",
                    step.status
                ))
                .into());
            }
            from.transition_to(*to).map_err(ApplicationError::from)?;
            step.status = *to;
            step.attempt = *attempt;
            if *to == vestrace_domain::run::RunStepStatus::Running && step.started_at.is_none() {
                step.started_at = Some(event.occurred_at);
            }
            if to.is_terminal() {
                step.finished_at = Some(event.occurred_at);
            }
        }
        RunEventPayload::CurrentStepChanged { previous, current } => {
            if projection.current_step_id != *previous {
                return Err(DomainError::InvalidArgument(
                    "CurrentStepChanged previous does not match current step".into(),
                )
                .into());
            }
            projection.current_step_id = *current;
        }
        RunEventPayload::CheckpointCreated {
            checkpoint_id,
            resume_cursor: _,
        } => {
            projection.checkpoint_id = Some(*checkpoint_id);
        }
        RunEventPayload::ApprovalGranted { .. } => {
            // Carries no state a projection derives: the run's return to
            // `Running` is expressed by the accompanying status change, and
            // this event exists to record *who* approved. Replay must not
            // reject it, and must not double-apply the transition.
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vestrace_domain::{
        id::{AgentRuntimeSnapshotId, CorrelationId},
        run::{ResumeCursor, RunActorRef},
    };

    fn now() -> Timestamp {
        use chrono::TimeZone;
        chrono::Utc
            .with_ymd_and_hms(2026, 8, 8, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn run_id() -> AgentRunId {
        AgentRunId::new()
    }

    fn workspace_id() -> WorkspaceId {
        WorkspaceId::new()
    }

    fn correlation_id() -> CorrelationId {
        CorrelationId::new()
    }

    fn make_event(
        version: RunVersion,
        run_id: AgentRunId,
        workspace_id: WorkspaceId,
        payload: RunEventPayload,
    ) -> RunEvent {
        RunEvent::new(
            run_id,
            workspace_id,
            version,
            ResumeCursor::from_version(version),
            RunActorRef::System,
            payload,
            correlation_id(),
            None,
            now(),
        )
        .unwrap()
    }

    fn scripted_events() -> Vec<RunEvent> {
        let run_id = run_id();
        let ws = workspace_id();

        let created = make_event(
            RunVersion::INITIAL,
            run_id,
            ws,
            RunEventPayload::RunCreated {
                objective: "Test objective".into(),
                execution_mode: RunExecutionMode::Autopilot,
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                parent: None,
            },
        );

        let preparing = make_event(
            RunVersion::new(2).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Created,
                to: RunStatus::Preparing,
                result: None,
            },
        );

        let running = make_event(
            RunVersion::new(3).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Preparing,
                to: RunStatus::Running,
                result: None,
            },
        );

        let waiting = make_event(
            RunVersion::new(4).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Running,
                to: RunStatus::WaitingForInput,
                result: None,
            },
        );

        let resumed = make_event(
            RunVersion::new(5).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::WaitingForInput,
                to: RunStatus::Running,
                result: None,
            },
        );

        let paused = make_event(
            RunVersion::new(6).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Running,
                to: RunStatus::Paused,
                result: None,
            },
        );

        let resumed_again = make_event(
            RunVersion::new(7).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Paused,
                to: RunStatus::Running,
                result: None,
            },
        );

        vec![
            created,
            preparing,
            running,
            waiting,
            resumed,
            paused,
            resumed_again,
        ]
    }

    #[test]
    fn replay_reconstructs_projection() {
        let events = scripted_events();
        let projection = replay_run(&events).unwrap();
        assert_eq!(projection.status, RunStatus::Running);
        assert_eq!(projection.version, RunVersion::new(7).unwrap());
    }

    #[test]
    fn empty_events_rejected() {
        let result = replay_run(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn first_event_must_be_creation() {
        let run_id = run_id();
        let ws = workspace_id();
        let event = make_event(
            RunVersion::new(2).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Created,
                to: RunStatus::Preparing,
                result: None,
            },
        );
        let result = replay_run(&[event]);
        assert!(result.is_err());
    }

    #[test]
    fn non_contiguous_versions_rejected() {
        let run_id = run_id();
        let ws = workspace_id();

        let created = make_event(
            RunVersion::INITIAL,
            run_id,
            ws,
            RunEventPayload::RunCreated {
                objective: "Test".into(),
                execution_mode: RunExecutionMode::Autopilot,
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                parent: None,
            },
        );

        let mut skip_event = make_event(
            RunVersion::new(3).unwrap(),
            run_id,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Created,
                to: RunStatus::Preparing,
                result: None,
            },
        );
        skip_event.run_version = RunVersion::new(3).unwrap();
        skip_event.sequence = ResumeCursor::from_version(RunVersion::new(3).unwrap());

        let result = replay_run(&[created, skip_event]);
        assert!(result.is_err());
    }

    #[test]
    fn mixed_run_ids_rejected() {
        let run_id_1 = run_id();
        let run_id_2 = run_id();
        let ws = workspace_id();

        let created = make_event(
            RunVersion::INITIAL,
            run_id_1,
            ws,
            RunEventPayload::RunCreated {
                objective: "Test".into(),
                execution_mode: RunExecutionMode::Autopilot,
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                parent: None,
            },
        );

        let mut wrong_run = make_event(
            RunVersion::new(2).unwrap(),
            run_id_1,
            ws,
            RunEventPayload::RunStatusChanged {
                from: RunStatus::Created,
                to: RunStatus::Preparing,
                result: None,
            },
        );
        wrong_run.run_id = run_id_2;

        let result = replay_run(&[created, wrong_run]);
        assert!(result.is_err());
    }
}
