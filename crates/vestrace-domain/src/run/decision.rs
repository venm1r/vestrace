use super::{
    PendingRunEvent, RunCommand, RunCommandEnvelope, RunDecisionError, RunEvent, RunState,
    RunStatus, RunVersion,
};

pub fn decide(
    state: Option<&RunState>,
    command: &RunCommandEnvelope,
) -> Result<Vec<PendingRunEvent>, RunDecisionError> {
    let actual_version = state.map_or(RunVersion::ZERO, |state| state.version);
    if command.expected_version != actual_version {
        return Err(RunDecisionError::VersionConflict {
            expected: command.expected_version,
            actual: actual_version,
        });
    }

    match (state, &command.command) {
        (
            None,
            RunCommand::Create {
                principal_id,
                title,
            },
        ) => Ok(one(
            RunEvent::Created {
                principal_id: *principal_id,
                title: required_text(title, "run title")?,
            },
            command,
        )),
        (Some(_), RunCommand::Create { .. }) => Err(RunDecisionError::AlreadyExists),
        (None, _) => Err(RunDecisionError::MissingState),
        (Some(state), run_command) => decide_existing(state, run_command, command),
    }
}

fn decide_existing(
    state: &RunState,
    run_command: &RunCommand,
    envelope: &RunCommandEnvelope,
) -> Result<Vec<PendingRunEvent>, RunDecisionError> {
    if state.workspace_id != envelope.workspace_id {
        return Err(RunDecisionError::InvalidCommand(
            "command workspace does not match run".to_owned(),
        ));
    }
    if state.id != envelope.run_id {
        return Err(RunDecisionError::InvalidCommand(
            "command run id does not match aggregate".to_owned(),
        ));
    }
    if state.status.is_terminal() {
        return invalid_transition(state);
    }

    let event = match run_command {
        RunCommand::MarkReady if state.status == RunStatus::Created => RunEvent::MarkedReady,
        RunCommand::Start if state.status == RunStatus::Ready => RunEvent::Started,
        RunCommand::StartStep {
            step_id,
            kind,
            label,
        } if state.status == RunStatus::Running && state.active_step.is_none() => {
            RunEvent::StepStarted {
                step_id: *step_id,
                kind: required_text(kind, "step kind")?,
                label: optional_text(label),
            }
        }
        RunCommand::CompleteStep {
            step_id,
            output_references,
        } if state.status == RunStatus::Running => {
            require_active_step(state, *step_id)?;
            RunEvent::StepCompleted {
                step_id: *step_id,
                output_references: output_references.clone(),
            }
        }
        RunCommand::FailStep {
            step_id,
            code,
            message,
            retryable,
        } if state.status == RunStatus::Running => {
            require_active_step(state, *step_id)?;
            RunEvent::StepFailed {
                step_id: *step_id,
                code: required_text(code, "step failure code")?,
                message: required_text(message, "step failure message")?,
                retryable: *retryable,
            }
        }
        RunCommand::WaitForInput { request_id, prompt }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            RunEvent::WaitingForInput {
                request_id: *request_id,
                prompt: required_text(prompt, "input prompt")?,
            }
        }
        RunCommand::WaitForApproval { approval_id }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            RunEvent::WaitingForApproval {
                approval_id: *approval_id,
            }
        }
        RunCommand::Resume if state.status.is_waiting() => RunEvent::Resumed,
        RunCommand::Complete { summary }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            RunEvent::Completed {
                summary: optional_text(summary),
            }
        }
        RunCommand::Fail {
            code,
            message,
            retryable,
        } => RunEvent::Failed {
            code: required_text(code, "run failure code")?,
            message: required_text(message, "run failure message")?,
            retryable: *retryable,
        },
        RunCommand::Cancel { reason } => RunEvent::Cancelled {
            reason: optional_text(reason),
        },
        RunCommand::MarkStalled { reason } => RunEvent::Stalled {
            reason: reason.clone(),
        },
        _ => return invalid_transition(state),
    };

    Ok(one(event, envelope))
}

fn one(event: RunEvent, command: &RunCommandEnvelope) -> Vec<PendingRunEvent> {
    vec![PendingRunEvent {
        event,
        occurred_at: command.issued_at,
    }]
}

fn required_text(value: &str, field: &str) -> Result<String, RunDecisionError> {
    let value = value.trim();
    if value.is_empty() {
        Err(RunDecisionError::InvalidCommand(format!(
            "{field} is required"
        )))
    } else {
        Ok(value.to_owned())
    }
}

fn optional_text(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn require_active_step(
    state: &RunState,
    requested_step: crate::id::RunStepId,
) -> Result<(), RunDecisionError> {
    let active_step = state
        .active_step
        .as_ref()
        .ok_or(RunDecisionError::InvalidTransition {
            status: state.status,
        })?;
    if active_step.id != requested_step {
        return Err(RunDecisionError::InvalidCommand(
            "step id does not match the active step".to_owned(),
        ));
    }
    Ok(())
}

fn invalid_transition<T>(state: &RunState) -> Result<T, RunDecisionError> {
    Err(RunDecisionError::InvalidTransition {
        status: state.status,
    })
}
