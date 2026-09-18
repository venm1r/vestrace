use super::{
    LegacyRunEvent, LegacyRunEventEnvelope, RunCompletion, RunReduceError, RunReplayError,
    RunState, RunStatus, RunStepState, RunStepStatus, RunVersion, RunWait,
};

pub fn apply(
    state: Option<RunState>,
    event: &LegacyRunEventEnvelope,
) -> Result<RunState, RunReduceError> {
    validate_envelope(&state, event)?;

    match state {
        None => apply_first(event),
        Some(state) => apply_existing(state, event),
    }
}

pub fn replay(
    events: impl IntoIterator<Item = LegacyRunEventEnvelope>,
) -> Result<Option<RunState>, RunReplayError> {
    let mut state = None;
    for event in events {
        state = Some(apply(state, &event)?);
    }
    Ok(state)
}

fn apply_first(event: &LegacyRunEventEnvelope) -> Result<RunState, RunReduceError> {
    let LegacyRunEvent::Created {
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
    event: &LegacyRunEventEnvelope,
) -> Result<RunState, RunReduceError> {
    match &event.payload {
        LegacyRunEvent::Created { .. } => return Err(RunReduceError::DuplicateCreatedEvent),
        LegacyRunEvent::Prepared if state.status == RunStatus::Created => {
            state.status = RunStatus::Preparing;
        }
        LegacyRunEvent::Started if state.status == RunStatus::Preparing => {
            state.status = RunStatus::Running;
            state.started_at = Some(event.occurred_at);
        }
        LegacyRunEvent::StepStarted {
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
        LegacyRunEvent::StepSucceeded { step_id, .. }
            if state.status == RunStatus::Running && active_step_matches(&state, *step_id) =>
        {
            state.active_step = None;
        }
        LegacyRunEvent::StepFailed { step_id, .. }
            if state.status == RunStatus::Running && active_step_matches(&state, *step_id) =>
        {
            state.active_step = None;
        }
        LegacyRunEvent::WaitingForInput { request_id, prompt }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            state.status = RunStatus::WaitingForInput;
            state.wait = Some(RunWait::Input {
                request_id: *request_id,
                prompt: prompt.clone(),
            });
        }
        LegacyRunEvent::WaitingForApproval { approval_id }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            state.status = RunStatus::WaitingForApproval;
            state.wait = Some(RunWait::Approval {
                approval_id: *approval_id,
            });
        }
        LegacyRunEvent::WaitingForDependency { dependency_run_id }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            state.status = RunStatus::WaitingForDependency;
            state.wait = Some(RunWait::Event {
                event_type: "run.succeeded".to_owned(),
                correlation_id: crate::id::CorrelationId::from_uuid(dependency_run_id.as_uuid()),
            });
        }
        LegacyRunEvent::ApprovalGranted { .. } if state.status == RunStatus::WaitingForApproval => {
            state.status = RunStatus::Running;
            state.wait = None;
        }
        LegacyRunEvent::Resumed if state.status.is_waiting() || state.status.can_resume() => {
            state.status = RunStatus::Running;
            state.wait = None;
        }
        LegacyRunEvent::Succeeded { summary }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            finish(
                &mut state,
                RunStatus::Succeeded,
                RunCompletion::Succeeded {
                    summary: summary.clone(),
                },
                event.occurred_at,
            );
        }
        LegacyRunEvent::SucceededWithWarnings { summary, warnings }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            finish(
                &mut state,
                RunStatus::SucceededWithWarnings,
                RunCompletion::SucceededWithWarnings {
                    summary: summary.clone(),
                    warnings: warnings.clone(),
                },
                event.occurred_at,
            );
        }
        LegacyRunEvent::PartialCompleted {
            summary,
            remaining_work,
        } if state.status == RunStatus::Running && state.active_step.is_none() => {
            finish(
                &mut state,
                RunStatus::Partial,
                RunCompletion::Partial {
                    summary: summary.clone(),
                    remaining_work: remaining_work.clone(),
                },
                event.occurred_at,
            );
        }
        LegacyRunEvent::Failed {
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
        LegacyRunEvent::Cancelled { reason } => {
            finish(
                &mut state,
                RunStatus::Cancelled,
                RunCompletion::Cancelled {
                    reason: reason.clone(),
                },
                event.occurred_at,
            );
        }
        LegacyRunEvent::Paused if state.status.can_pause() => {
            state.status = RunStatus::Paused;
        }
        LegacyRunEvent::Expired if state.status.is_waiting() => {
            finish(
                &mut state,
                RunStatus::Expired,
                RunCompletion::Expired,
                event.occurred_at,
            );
        }
        // Deliberately unguarded, unlike every arm above.
        //
        // A reconciliation settles long after the dispatch, and often after the
        // run has finished — that late answer is the whole reason the event
        // exists. Guarding it on `Running` would make the one case it is for
        // the one case it rejects, and replay of a real stream would fail with
        // `InvalidTransition` on a fact that is simply true.
        //
        // It changes no state: it is an observation, not a transition. Whether a
        // `NotApplied` effect should reopen a succeeded run is a policy question
        // nobody has answered, and answering it here by silently mutating status
        // would be the wrong way to ask.
        LegacyRunEvent::ExternalEffectSettled { .. } => {}
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
    event: &LegacyRunEventEnvelope,
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
