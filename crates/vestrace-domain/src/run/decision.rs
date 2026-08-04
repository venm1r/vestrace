use super::{
    PendingRunEvent, RunCommand, RunCommandEnvelope, RunDecisionError, RunEvent, RunState,
    RunVersion,
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
        ) => {
            let title = title.trim();
            if title.is_empty() {
                return Err(RunDecisionError::InvalidCommand(
                    "run title is required".to_owned(),
                ));
            }

            Ok(vec![PendingRunEvent {
                event: RunEvent::Created {
                    principal_id: *principal_id,
                    title: title.to_owned(),
                },
                occurred_at: command.issued_at,
            }])
        }
        (Some(_), RunCommand::Create { .. }) => Err(RunDecisionError::AlreadyExists),
        (None, _) => Err(RunDecisionError::MissingState),
        (Some(state), _) => Err(RunDecisionError::InvalidTransition {
            status: state.status,
        }),
    }
}
