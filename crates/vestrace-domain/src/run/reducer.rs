use super::{
    RunEvent, RunEventEnvelope, RunReduceError, RunReplayError, RunState, RunStatus, RunVersion,
};

pub fn apply(
    state: Option<RunState>,
    event: &RunEventEnvelope,
) -> Result<RunState, RunReduceError> {
    validate_envelope(&state, event)?;

    match (state, &event.payload) {
        (
            None,
            RunEvent::Created {
                principal_id,
                title,
            },
        ) => Ok(RunState {
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
        }),
        (None, _) => Err(RunReduceError::MissingCreatedEvent),
        (Some(_), RunEvent::Created { .. }) => Err(RunReduceError::DuplicateCreatedEvent),
        (Some(state), _) => Err(RunReduceError::InvalidTransition {
            status: state.status,
        }),
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
