use super::{
    LegacyRunEvent, PendingRunEvent, RunCommand, RunCommandEnvelope, RunDecisionError, RunState,
    RunStatus, RunVersion,
};

pub fn decide(
    state: Option<&RunState>,
    command: &RunCommandEnvelope,
) -> Result<Vec<PendingRunEvent>, RunDecisionError> {
    if let RunCommand::Create {
        principal_id,
        title,
    } = &command.command
    {
        if state.is_some() {
            return Err(RunDecisionError::AlreadyExists);
        }
        if command.expected_version != RunVersion::ZERO {
            return Err(RunDecisionError::VersionConflict {
                expected: command.expected_version,
                actual: RunVersion::ZERO,
            });
        }
        return Ok(one(
            LegacyRunEvent::Created {
                principal_id: *principal_id,
                title: required_text(title, "run title")?,
            },
            command,
        ));
    }

    let actual_version = state.map_or(RunVersion::ZERO, |state| state.version);
    if command.expected_version != actual_version {
        return Err(RunDecisionError::VersionConflict {
            expected: command.expected_version,
            actual: actual_version,
        });
    }

    match state {
        None => Err(RunDecisionError::MissingState),
        Some(state) => decide_existing(state, &command.command, command),
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
        RunCommand::Prepare if state.status == RunStatus::Created => LegacyRunEvent::Prepared,
        RunCommand::Start if state.status == RunStatus::Preparing => LegacyRunEvent::Started,
        RunCommand::StartStep {
            step_id,
            kind,
            label,
        } if state.status == RunStatus::Running && state.active_step.is_none() => {
            LegacyRunEvent::StepStarted {
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
            LegacyRunEvent::StepSucceeded {
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
            LegacyRunEvent::StepFailed {
                step_id: *step_id,
                code: required_text(code, "step failure code")?,
                message: required_text(message, "step failure message")?,
                retryable: *retryable,
            }
        }
        RunCommand::WaitForInput { request_id, prompt }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            LegacyRunEvent::WaitingForInput {
                request_id: *request_id,
                prompt: required_text(prompt, "input prompt")?,
            }
        }
        RunCommand::WaitForApproval { approval_id }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            LegacyRunEvent::WaitingForApproval {
                approval_id: *approval_id,
            }
        }
        RunCommand::WaitForDependency { dependency_run_id }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            LegacyRunEvent::WaitingForDependency {
                dependency_run_id: *dependency_run_id,
            }
        }
        // An approval is only meaningful where one was actually pending, so it
        // is accepted from `WaitingForApproval` and nowhere else.
        RunCommand::Approve { approver_id } if state.status == RunStatus::WaitingForApproval => {
            LegacyRunEvent::ApprovalGranted {
                approver_id: *approver_id,
            }
        }
        RunCommand::Resume if state.status.is_waiting() || state.status.can_resume() => {
            LegacyRunEvent::Resumed
        }
        RunCommand::Succeed { summary }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            LegacyRunEvent::Succeeded {
                summary: optional_text(summary),
            }
        }
        RunCommand::SucceedWithWarnings { summary, warnings }
            if state.status == RunStatus::Running && state.active_step.is_none() =>
        {
            LegacyRunEvent::SucceededWithWarnings {
                summary: required_text(summary, "summary")?,
                warnings: warnings.clone(),
            }
        }
        RunCommand::PartialComplete {
            summary,
            remaining_work,
        } if state.status == RunStatus::Running && state.active_step.is_none() => {
            LegacyRunEvent::PartialCompleted {
                summary: required_text(summary, "summary")?,
                remaining_work: remaining_work.clone(),
            }
        }
        RunCommand::Fail {
            code,
            message,
            retryable,
        } => LegacyRunEvent::Failed {
            code: required_text(code, "run failure code")?,
            message: required_text(message, "run failure message")?,
            retryable: *retryable,
        },
        RunCommand::Cancel { reason } => LegacyRunEvent::Cancelled {
            reason: optional_text(reason),
        },
        RunCommand::Pause if state.status.can_pause() => LegacyRunEvent::Paused,
        RunCommand::Expire if state.status.is_waiting() => LegacyRunEvent::Expired,
        RunCommand::Create { .. } => return Err(RunDecisionError::AlreadyExists),
        _ => return invalid_transition(state),
    };

    Ok(one(event, envelope))
}

fn one(event: LegacyRunEvent, command: &RunCommandEnvelope) -> Vec<PendingRunEvent> {
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
