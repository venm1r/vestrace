use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use vestrace_domain::{
    DomainError,
    id::RunEventId,
    now,
    run::{
        AgentRun, RunActor, RunCommand, RunCommandEnvelope, RunDecisionError, RunEventEnvelope,
        RunState, apply, decide, replay,
    },
};

use crate::{ApplicationError, RequestContext};

use super::{
    RunCommandCommitOutcome, RunCommandExecutor, RunCommandReceipt, RunCommandResult,
    SharedRunCommandCommitter, SharedRunEventStore,
};

pub struct RunCommandService {
    event_store: SharedRunEventStore,
    committer: SharedRunCommandCommitter,
}

impl RunCommandService {
    pub fn new(event_store: SharedRunEventStore, committer: SharedRunCommandCommitter) -> Self {
        Self {
            event_store,
            committer,
        }
    }
}

#[async_trait]
impl RunCommandExecutor for RunCommandService {
    async fn execute(
        &self,
        context: &RequestContext,
        command: RunCommandEnvelope,
    ) -> Result<RunCommandResult, ApplicationError> {
        validate_command_context(context, &command)?;
        let receipt = RunCommandReceipt {
            command_id: command.command_id,
            workspace_id: command.workspace_id,
            principal_id: context.principal_id,
            command_type: command_type(&command.command).to_owned(),
            idempotency_key: command.idempotency_key.clone(),
            request_payload: serde_json::to_value(&command.command).map_err(storage_error)?,
            run_id: command.run_id,
        };

        let stored_events = self
            .event_store
            .load_stream(context, command.run_id)
            .await?;
        let initial_state = replay(stored_events).map_err(replay_error)?;
        let pending_events = decide(initial_state.as_ref(), &command).map_err(decision_error)?;
        if pending_events.is_empty() {
            return Err(ApplicationError::Internal(
                "run decision produced no events".to_owned(),
            ));
        }

        let recorded_at = database_timestamp(now());
        let mut sequence = command.expected_version;
        let mut envelopes = Vec::with_capacity(pending_events.len());
        for pending in pending_events {
            sequence = sequence.next()?;
            let payload = pending.event;
            envelopes.push(RunEventEnvelope {
                event_id: RunEventId::new(),
                workspace_id: command.workspace_id,
                run_id: command.run_id,
                sequence,
                event_type: payload.event_type().to_owned(),
                event_version: payload.event_version(),
                actor: command.actor.clone(),
                causation_id: command.command_id,
                correlation_id: command.correlation_id,
                payload,
                occurred_at: database_timestamp(pending.occurred_at),
                recorded_at,
            });
        }

        let mut resulting_state = initial_state;
        for event in &envelopes {
            resulting_state = Some(apply(resulting_state, event).map_err(reduce_error)?);
        }
        let resulting_state = resulting_state.ok_or_else(|| {
            ApplicationError::Internal("run command did not produce state".to_owned())
        })?;
        let projection = project_run(&resulting_state);

        match self
            .committer
            .commit(
                context,
                command.run_id,
                command.expected_version,
                &envelopes,
                &projection,
                &receipt,
            )
            .await?
        {
            RunCommandCommitOutcome::Committed(committed_version) => {
                if committed_version != projection.version {
                    return Err(ApplicationError::Internal(format!(
                        "committer returned run version {}, expected {}",
                        committed_version.value(),
                        projection.version.value()
                    )));
                }
                Ok(RunCommandResult {
                    run: projection,
                    events: envelopes,
                    replayed: false,
                })
            }
            RunCommandCommitOutcome::Replayed(run) => Ok(RunCommandResult {
                run,
                events: Vec::new(),
                replayed: true,
            }),
        }
    }
}

fn validate_command_context(
    context: &RequestContext,
    command: &RunCommandEnvelope,
) -> Result<(), ApplicationError> {
    if command.workspace_id != context.workspace_id {
        return Err(ApplicationError::Policy(
            "run command workspace does not match request context".to_owned(),
        ));
    }
    if let RunActor::Principal(principal_id) = &command.actor {
        if *principal_id != context.principal_id {
            return Err(ApplicationError::Policy(
                "run command principal does not match request context".to_owned(),
            ));
        }
    }
    Ok(())
}

fn command_type(command: &RunCommand) -> &'static str {
    match command {
        RunCommand::Create { .. } => "run.create",
        RunCommand::MarkReady => "run.mark_ready",
        RunCommand::Start => "run.start",
        RunCommand::StartStep { .. } => "run.start_step",
        RunCommand::CompleteStep { .. } => "run.complete_step",
        RunCommand::FailStep { .. } => "run.fail_step",
        RunCommand::WaitForInput { .. } => "run.wait_for_input",
        RunCommand::WaitForApproval { .. } => "run.wait_for_approval",
        RunCommand::Resume => "run.resume",
        RunCommand::Complete { .. } => "run.complete",
        RunCommand::Fail { .. } => "run.fail",
        RunCommand::Cancel { .. } => "run.cancel",
        RunCommand::MarkStalled { .. } => "run.mark_stalled",
    }
}

fn project_run(state: &RunState) -> AgentRun {
    AgentRun {
        id: state.id,
        workspace_id: state.workspace_id,
        principal_id: state.principal_id,
        title: state.title.clone(),
        status: state.status,
        version: state.version,
        created_at: state.created_at,
        updated_at: state.updated_at,
    }
}

fn database_timestamp(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    let submicrosecond_nanos = i64::from(timestamp.timestamp_subsec_nanos() % 1_000);
    timestamp - Duration::nanoseconds(submicrosecond_nanos)
}

fn decision_error(error: RunDecisionError) -> ApplicationError {
    match error {
        RunDecisionError::AlreadyExists => {
            ApplicationError::Conflict("run already exists".to_owned())
        }
        RunDecisionError::MissingState => {
            DomainError::NotFound("run does not exist".to_owned()).into()
        }
        RunDecisionError::VersionConflict { expected, actual } => {
            ApplicationError::Conflict(format!(
                "run version conflict: expected {}, actual {}",
                expected.value(),
                actual.value()
            ))
        }
        RunDecisionError::InvalidCommand(message) => DomainError::InvalidArgument(message).into(),
        RunDecisionError::InvalidTransition { status } => {
            DomainError::InvalidArgument(format!("command is not valid while run is {status:?}"))
                .into()
        }
    }
}

fn replay_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(format!("run event replay failed: {error}"))
}

fn reduce_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(format!("run event reduction failed: {error}"))
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
