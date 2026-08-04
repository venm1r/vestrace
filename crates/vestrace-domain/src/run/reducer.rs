use super::{
    RunCompletion, RunEvent, RunEventEnvelope, RunReduceError, RunReplayError, RunState,
    RunStatus, RunStepState, RunStepStatus, RunVersion, RunWait,
};

pub fn apply(
    state: Option<RunState>,
    event: &RunEventEnvelope,
) -> Result<RunState, RunReduceError> {
    validate_envelope(&state, event)?;

    match state {
        None => apply_first(event),
        Some(state) => apply_existing(state, event),
    }
}

pub fn replay(
    events: impl IntoIterator<Item = RunEventEnvelope>,
) -> Result<Option<RunState>, RunReplayError> {
    let mut state = None;
    for event in events {
        state = Some(apply(state, &event)?);
    }
    Ok(state)
}

fn apply_first(event: &RunEventEnvelope) -> Result<RunState, RunReduceError> {
    let RunEvent::Created {
        principal_id,
        title,
    } = &event.payload
    else {
        return Err(RunReduceError::MissingCreatedEvent);
    };

    Ok(RunState {
        id: event.run_id,
        workspace_id: event.workspace_id,
        principal_id: *principal_id,
        title: title.clone(),
        status: RunStatus::Created,
        version: event.sequence,
        active_step: None,
        wait: None,
        completion: None,
        created_at: event.occurred_at,
        started_at: None,
        updated_at: event.occurred_at,
        finished_at: None,
    })
}

fn apply_existing(
    mut state: RunState,
    event: &RunEventEnvelope,
) -> Result<RunState, RunReduceError> {
    match &event.payload {
        RunEvent::Created { .. } => return Err(RunReduceError::DuplicateCreatedEvent),
        RunEvent::MarkedReady if state.status == RunStatus::Created => {
            state.status = RunStatus::Ready;
        }
        RunEvent::Started if state.status == RunStatus::Ready => {
            state.status = RunStatus::Running;
            state.started_at = Some(event.occurred_at);
        }
        RunEvent::StepStarted {
            step_id,
            kind,
            label,
        } if state.status == RunStatus::Running && state.active_step.is_none() => {
            state.active_step = Some(RunStepState {
                id: *step_id,
                kind: kind.clone(),
                label: label.clone(),
                status: RunStepStatus::Running,
                started_at: event.occurred_at,
                finished_at: None,
            });
        }
        RunEvent::StepCompleted { step_id, .. }
            if state.status == RunStatus::Running
                && active_step_matches(&state, *step_id) =>
        {
            state.active_step = None;
        }
        RunEvent::StepFailed { step_id, .. }
            if state.status == RunStatus::Running
                && active_step_matches(&state, *step_id) =>
        {
            state.active_step = None;
        }
        RunEvent::WaitingForInput { request_id, prompt }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            state.status = RunStatus::WaitingForInput;
            state.wait = Some(RunWait::Input {
                request_id: *request_id,
                prompt: prompt.clone(),
            });
        }
        RunEvent::WaitingForApproval { approval_id }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            state.status = RunStatus::WaitingForApproval;
            state.wait = Some(RunWait::Approval {
                approval_id: *approval_id,
            });
        }
        RunEvent::Resumed if state.status.is_waiting() => {
            state.status = RunStatus::Running;
            state.wait = None;
        }
        RunEvent::Completed { summary }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            finish(
                &mut state,
                RunStatus::Completed,
                RunCompletion::Completed {
                    summary: summary.clone(),
                },
                event.occurred_at,
            );
        }
        RunEvent::Failed {
            code,
            message,
            retryable,
        } => {
            finish(
                &mut state,
                RunStatus::Failed,
                RunCompletion::Failed {
                    code: code.clone(),
                    message: message.clone(),
                    retryable: *retryable,
                },
                event.occurred_at,
            );
        }
        RunEvent::Cancelled { reason } => {
            finish(
                &mut state,
                RunStatus::Cancelled,
                RunCompletion::Cancelled {
                    reason: reason.clone(),
                },
                event.occurred_at,
            );
        }
        RunEvent::Stalled { reason } => {
            finish(
                &mut state,
                RunStatus::Stalled,
                RunCompletion::Stalled {
                    reason: reason.clone(),
                },
                event.occurred_at,
            );
        }
        _ => {
            return Err(RunReduceError::InvalidTransition {
                status: state.status,
            });
        }
    }

    state.version = event.sequence;
    state.updated_at = event.occurred_at;
    Ok(state)
}

fn active_step_matches(state: &RunState, step_id: crate::id::RunStepId) -> bool {
    state
        .active_step
        .as_ref()
        .is_some_and(|active| active.id == step_id)
}

fn finish(
    state: &mut RunState,
    status: RunStatus,
    completion: RunCompletion,
    at: crate::time::Timestamp,
) {
    state.status = status;
    state.active_step = None;
    state.wait = None;
    state.completion = Some(completion);
    state.finished_at = Some(at);
}

fn validate_envelope(
    state: &Option<RunState>,
    event: &RunEventEnvelope,
) -> Result<(), RunReduceError> {
    let expected_type = event.payload.event_type();
    if event.event_type != expected_type {
        return Err(RunReduceError::EventTypeMismatch {
            expected: expected_type.to_owned(),
            actual: event.event_type.clone(),
        });
    }

    if event.event_version != event.payload.event_version() {
        return Err(RunReduceError::UnsupportedEventVersion {
            event_type: event.event_type.clone(),
            version: event.event_version,
        });
    }

    match state {
        None => {
            if event.sequence != RunVersion::INITIAL {
                return Err(RunReduceError::SequenceMismatch {
                    expected: RunVersion::INITIAL,
                    actual: event.sequence,
                });
            }
        }
        Some(state) => {
            if state.workspace_id != event.workspace_id {
                return Err(RunReduceError::WorkspaceMismatch {
                    expected: state.workspace_id,
                    actual: event.workspace_id,
                });
            }
            if state.id != event.run_id {
                return Err(RunReduceError::RunMismatch {
                    expected: state.id,
                    actual: event.run_id,
                });
            }
            if state.status.is_terminal() {
                return Err(RunReduceError::EventAfterTerminal {
                    status: state.status,
                });
            }

            let expected = state
                .version
                .next()
                .map_err(|_| RunReduceError::VersionOverflow)?;
            if event.sequence != expected {
                return Err(RunReduceError::SequenceMismatch {
                    expected,
                    actual: event.sequence,
                });
            }
        }
    }

    Ok(())
}
