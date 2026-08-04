use chrono::{TimeZone, Utc};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId},
    run::{
        PendingRunEvent, RunActor, RunCommand, RunCommandEnvelope, RunEventEnvelope, RunStatus,
        RunVersion, apply, decide, replay,
    },
    time::Timestamp,
};

struct Fixture {
    workspace_id: WorkspaceId,
    principal_id: PrincipalId,
    run_id: AgentRunId,
    command_id: OperationId,
    correlation_id: CorrelationId,
    at: Timestamp,
}

impl Fixture {
    fn new() -> Self {
        Self {
            workspace_id: WorkspaceId::new(),
            principal_id: PrincipalId::new(),
            run_id: AgentRunId::new(),
            command_id: OperationId::new(),
            correlation_id: CorrelationId::new(),
            at: Utc.with_ymd_and_hms(2026, 8, 4, 8, 0, 0).single().unwrap(),
        }
    }

    fn create_command(&self, title: &str) -> RunCommandEnvelope {
        RunCommandEnvelope {
            command_id: self.command_id,
            idempotency_key: None,
            workspace_id: self.workspace_id,
            run_id: self.run_id,
            actor: RunActor::Principal(self.principal_id),
            expected_version: RunVersion::ZERO,
            correlation_id: self.correlation_id,
            issued_at: self.at,
            command: RunCommand::Create {
                principal_id: self.principal_id,
                title: title.to_owned(),
            },
        }
    }

    fn envelope(&self, sequence: RunVersion, pending: PendingRunEvent) -> RunEventEnvelope {
        let event_type = pending.event.event_type().to_owned();
        let event_version = pending.event.event_version();

        RunEventEnvelope {
            event_id: pending.event_id,
            workspace_id: self.workspace_id,
            run_id: self.run_id,
            sequence,
            event_type,
            event_version,
            actor: RunActor::Principal(self.principal_id),
            causation_id: self.command_id,
            correlation_id: self.correlation_id,
            payload: pending.event,
            occurred_at: pending.occurred_at,
            recorded_at: self.at,
        }
    }
}

#[test]
fn create_event_builds_version_one_state_and_replays_deterministically() {
    let fixture = Fixture::new();
    let pending = decide(None, &fixture.create_command("  First run  ")).unwrap();
    assert_eq!(pending.len(), 1);

    let event = fixture.envelope(RunVersion::INITIAL, pending[0].clone());
    let state = apply(None, &event).unwrap();

    assert_eq!(state.title, "First run");
    assert_eq!(state.status, RunStatus::Created);
    assert_eq!(state.version, RunVersion::INITIAL);
    assert_eq!(replay([event.clone()]).unwrap(), Some(state.clone()));
    assert_eq!(replay([event]).unwrap(), Some(state));
}
