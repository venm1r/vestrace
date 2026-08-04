use chrono::{TimeZone, Utc};
use vestrace_domain::{
    id::{
        AgentRunId, ApprovalRecordId, CorrelationId, OperationId, PrincipalId, RequestId,
        RunEventId, RunStepId, WorkspaceId,
    },
    run::{
        PendingRunEvent, RunActor, RunCommand, RunCommandEnvelope, RunCompletion, RunDecisionError,
        RunEventEnvelope, RunReduceError, RunState, RunStatus, RunStepStatus, RunVersion, RunWait,
        apply, decide, replay,
    },
    time::Timestamp,
};

struct Fixture {
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
    run_id: AgentRunId,
    correlation_id: CorrelationId,
    at: Timestamp,
}

impl Fixture {
    fn new() -> Self {
        Self {
            workspace_id: WorkspaceId::new(),
            principal_id: PrincipalId::new(),
            run_id: AgentRunId::new(),
            correlation_id: CorrelationId::new(),
            at: Utc.with_ymd_and_hms(2026, 8, 4, 8, 0, 0).single().unwrap(),
        }
    }

    fn command(&self, expected_version: RunVersion, command: RunCommand) -> RunCommandEnvelope {
        RunCommandEnvelope {
            command_id: OperationId::new(),
            idempotency_key: None,
            workspace_id: self.workspace_id,
            run_id: self.run_id,
            actor: RunActor::Principal(self.principal_id),
            expected_version,
            correlation_id: self.correlation_id,
            issued_at: self.at,
            command,
        }
    }

    fn create_command(&self, title: &str) -> RunCommandEnvelope {
        self.command(
            RunVersion::ZERO,
            RunCommand::Create {
                principal_id: self.principal_id,
                title: title.to_owned(),
            },
        )
    }

    fn envelope(
        &self,
        command: &RunCommandEnvelope,
        sequence: RunVersion,
        pending: PendingRunEvent,
    ) -> RunEventEnvelope {
        let event_type = pending.event.event_type().to_owned();
        let event_version = pending.event.event_version();

        RunEventEnvelope {
            event_id: RunEventId::new(),
            workspace_id: self.workspace_id,
            run_id: self.run_id,
            sequence,
            event_type,
            event_version,
            actor: command.actor.clone(),
            causation_id: command.command_id,
            correlation_id: command.correlation_id,
            payload: pending.event,
            occurred_at: pending.occurred_at,
            recorded_at: self.at,
        }
    }

    fn execute(
        &self,
        state: Option<RunState>,
        command: RunCommandEnvelope,
    ) -> (RunState, RunEventEnvelope) {
        let pending = decide(state.as_ref(), &command).unwrap();
        assert_eq!(pending.len(), 1);
        let sequence = command.expected_version.next().unwrap();
        let event = self.envelope(&command, sequence, pending[0].clone());
        let state = apply(state, &event).unwrap();
        (state, event)
    }

    fn created(&self) -> (RunState, RunEventEnvelope) {
        self.execute(None, self.create_command("First run"))
    }

    fn running(&self) -> (RunState, Vec<RunEventEnvelope>) {
        let (created, created_event) = self.created();
        let mark_ready = self.command(created.version, RunCommand::MarkReady);
        let (ready, ready_event) = self.execute(Some(created), mark_ready);
        let start = self.command(ready.version, RunCommand::Start);
        let (running, started_event) = self.execute(Some(ready), start);
        (running, vec![created_event, ready_event, started_event])
    }
}

#[test]
fn create_event_builds_version_one_state_and_replays_deterministically() {
    let fixture = Fixture::new();
    let command = fixture.create_command("  First run  ");
    let pending = decide(None, &command).unwrap();
    assert_eq!(pending.len(), 1);

    let event = fixture.envelope(&command, RunVersion::INITIAL, pending[0].clone());
    let state = apply(None, &event).unwrap();

    assert_eq!(state.title, "First run");
    assert_eq!(state.status, RunStatus::Created);
    assert_eq!(state.version, RunVersion::INITIAL);
    assert_eq!(replay([event.clone()]).unwrap(), Some(state.clone()));
    assert_eq!(replay([event]).unwrap(), Some(state));
}

#[test]
fn create_rejects_blank_title_and_stale_expected_version() {
    let fixture = Fixture::new();
    let blank = decide(None, &fixture.create_command("   ")).unwrap_err();
    assert_eq!(
        blank,
        RunDecisionError::InvalidCommand("run title is required".to_owned())
    );

    let (state, _) = fixture.created();
    let stale = fixture.command(RunVersion::ZERO, RunCommand::MarkReady);
    assert_eq!(
        decide(Some(&state), &stale).unwrap_err(),
        RunDecisionError::VersionConflict {
            expected: RunVersion::ZERO,
            actual: RunVersion::INITIAL,
        }
    );
}

#[test]
fn reducer_rejects_stream_identity_sequence_and_schema_corruption() {
    let fixture = Fixture::new();
    let (state, event) = fixture.created();

    let mut wrong_sequence = event.clone();
    wrong_sequence.sequence = RunVersion::new(2).unwrap();
    assert_eq!(
        apply(None, &wrong_sequence).unwrap_err(),
        RunReduceError::SequenceMismatch {
            expected: RunVersion::INITIAL,
            actual: RunVersion::new(2).unwrap(),
        }
    );

    let mut next_event = event;
    next_event.sequence = state.version.next().unwrap();
    next_event.workspace_id = WorkspaceId::new();
    assert!(matches!(
        apply(Some(state.clone()), &next_event),
        Err(RunReduceError::WorkspaceMismatch { .. })
    ));

    next_event.workspace_id = fixture.workspace_id;
    next_event.run_id = AgentRunId::new();
    assert!(matches!(
        apply(Some(state.clone()), &next_event),
        Err(RunReduceError::RunMismatch { .. })
    ));

    next_event.run_id = fixture.run_id;
    next_event.event_type = "run.unknown".to_owned();
    assert!(matches!(
        apply(Some(state.clone()), &next_event),
        Err(RunReduceError::EventTypeMismatch { .. })
    ));

    next_event.event_type = next_event.payload.event_type().to_owned();
    next_event.event_version = 2;
    assert!(matches!(
        apply(Some(state), &next_event),
        Err(RunReduceError::UnsupportedEventVersion { .. })
    ));
}

#[test]
fn lifecycle_supports_ready_running_wait_resume_and_completion() {
    let fixture = Fixture::new();
    let (running, mut events) = fixture.running();
    assert_eq!(running.status, RunStatus::Running);
    assert_eq!(running.started_at, Some(fixture.at));

    let request_id = RequestId::new();
    let wait = fixture.command(
        running.version,
        RunCommand::WaitForInput {
            request_id,
            prompt: "Provide the target".to_owned(),
        },
    );
    let (waiting, wait_event) = fixture.execute(Some(running), wait);
    events.push(wait_event);
    assert_eq!(waiting.status, RunStatus::WaitingForInput);
    assert_eq!(
        waiting.wait,
        Some(RunWait::Input {
            request_id,
            prompt: "Provide the target".to_owned(),
        })
    );

    let resume = fixture.command(waiting.version, RunCommand::Resume);
    let (resumed, resumed_event) = fixture.execute(Some(waiting), resume);
    events.push(resumed_event);
    assert_eq!(resumed.status, RunStatus::Running);
    assert_eq!(resumed.wait, None);

    let complete = fixture.command(
        resumed.version,
        RunCommand::Complete {
            summary: Some("Done".to_owned()),
        },
    );
    let (completed, completed_event) = fixture.execute(Some(resumed), complete);
    events.push(completed_event);
    assert_eq!(completed.status, RunStatus::Completed);
    assert_eq!(
        completed.completion,
        Some(RunCompletion::Completed {
            summary: Some("Done".to_owned()),
        })
    );
    assert_eq!(completed.finished_at, Some(fixture.at));
    assert_eq!(replay(events).unwrap(), Some(completed.clone()));

    let after_terminal = fixture.command(completed.version, RunCommand::Resume);
    assert!(matches!(
        decide(Some(&completed), &after_terminal),
        Err(RunDecisionError::InvalidTransition {
            status: RunStatus::Completed
        })
    ));
}

#[test]
fn approval_wait_and_terminal_fail_cancel_stalled_states_are_typed() {
    let fixture = Fixture::new();
    let (running, _) = fixture.running();
    let approval_id = ApprovalRecordId::new();
    let wait = fixture.command(running.version, RunCommand::WaitForApproval { approval_id });
    let (waiting, _) = fixture.execute(Some(running), wait);
    assert_eq!(waiting.status, RunStatus::WaitingForApproval);
    assert_eq!(waiting.wait, Some(RunWait::Approval { approval_id }));

    for terminal_command in [
        RunCommand::Fail {
            code: "provider_error".to_owned(),
            message: "Provider failed".to_owned(),
            retryable: true,
        },
        RunCommand::Cancel {
            reason: Some("Operator cancelled".to_owned()),
        },
        RunCommand::MarkStalled {
            reason: vestrace_domain::run::StallReason::NoProgress,
        },
    ] {
        let fixture = Fixture::new();
        let (running, _) = fixture.running();
        let command = fixture.command(running.version, terminal_command);
        let (terminal, _) = fixture.execute(Some(running), command);
        assert!(terminal.status.is_terminal());
        assert!(terminal.completion.is_some());
        assert_eq!(terminal.finished_at, Some(fixture.at));
    }
}

#[test]
fn active_step_must_match_and_finish_before_run_completion() {
    let fixture = Fixture::new();
    let (running, _) = fixture.running();
    let step_id = RunStepId::new();
    let start_step = fixture.command(
        running.version,
        RunCommand::StartStep {
            step_id,
            kind: "validation".to_owned(),
            label: Some("Validate output".to_owned()),
        },
    );
    let (with_step, _) = fixture.execute(Some(running), start_step);
    assert_eq!(
        with_step.active_step.as_ref().map(|step| step.status),
        Some(RunStepStatus::Running)
    );

    let second_step = fixture.command(
        with_step.version,
        RunCommand::StartStep {
            step_id: RunStepId::new(),
            kind: "other".to_owned(),
            label: None,
        },
    );
    assert!(matches!(
        decide(Some(&with_step), &second_step),
        Err(RunDecisionError::InvalidTransition {
            status: RunStatus::Running
        })
    ));

    let complete_run = fixture.command(with_step.version, RunCommand::Complete { summary: None });
    assert!(matches!(
        decide(Some(&with_step), &complete_run),
        Err(RunDecisionError::InvalidTransition {
            status: RunStatus::Running
        })
    ));

    let wrong_step = fixture.command(
        with_step.version,
        RunCommand::CompleteStep {
            step_id: RunStepId::new(),
            output_references: vec![],
        },
    );
    assert!(matches!(
        decide(Some(&with_step), &wrong_step),
        Err(RunDecisionError::InvalidCommand(_))
    ));

    let complete_step = fixture.command(
        with_step.version,
        RunCommand::CompleteStep {
            step_id,
            output_references: vec!["artifact://result".to_owned()],
        },
    );
    let (without_step, _) = fixture.execute(Some(with_step), complete_step);
    assert_eq!(without_step.active_step, None);

    let complete_run =
        fixture.command(without_step.version, RunCommand::Complete { summary: None });
    let (completed, _) = fixture.execute(Some(without_step), complete_run);
    assert_eq!(completed.status, RunStatus::Completed);
}

#[test]
fn public_envelopes_and_state_round_trip_through_json() {
    let fixture = Fixture::new();
    let command = fixture.create_command("Serializable run");
    let command_json = serde_json::to_value(&command).unwrap();
    assert_eq!(
        serde_json::from_value::<RunCommandEnvelope>(command_json).unwrap(),
        command
    );

    let pending = decide(None, &command).unwrap().remove(0);
    let event = fixture.envelope(&command, RunVersion::INITIAL, pending);
    let event_json = serde_json::to_value(&event).unwrap();
    assert_eq!(
        serde_json::from_value::<RunEventEnvelope>(event_json).unwrap(),
        event
    );

    let state = apply(None, &event).unwrap();
    let state_json = serde_json::to_value(&state).unwrap();
    assert_eq!(
        serde_json::from_value::<RunState>(state_json).unwrap(),
        state
    );
    assert_eq!(replay(Vec::<RunEventEnvelope>::new()).unwrap(), None);
}
