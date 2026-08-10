//! Application-level command execution contract for R1.3.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use vestrace_application::{
    ApplicationError, RequestContext, RunCommandCommitter, RunCommandExecutor, RunCommandService,
    RunEventStore,
};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId},
    now,
    run::{
        AgentRun, LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunCommand, RunCommandEnvelope,
        RunStatus, RunVersion,
    },
};

#[derive(Default)]
struct InMemoryEventStore {
    events: Mutex<Vec<LegacyRunEventEnvelope>>,
}

#[async_trait]
impl RunEventStore for InMemoryEventStore {
    async fn load_stream(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.workspace_id == context.workspace_id && event.run_id == run_id)
            .cloned()
            .collect())
    }

    async fn append(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        _expected_version: RunVersion,
        _events: &[LegacyRunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError> {
        panic!("RunCommandService must use the atomic committer, not RunEventStore::append")
    }
}

#[derive(Default)]
struct RecordingCommitter {
    commits: Mutex<Vec<(RunVersion, Vec<LegacyRunEventEnvelope>, AgentRun)>>,
}

#[async_trait]
impl RunCommandCommitter for RecordingCommitter {
    async fn commit(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        expected_version: RunVersion,
        events: &[LegacyRunEventEnvelope],
        projection: &AgentRun,
    ) -> Result<RunVersion, ApplicationError> {
        self.commits
            .lock()
            .unwrap()
            .push((expected_version, events.to_vec(), projection.clone()));
        Ok(projection.version)
    }
}

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn database_timestamp(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    let submicrosecond_nanos = i64::from(timestamp.timestamp_subsec_nanos() % 1_000);
    timestamp - Duration::nanoseconds(submicrosecond_nanos)
}

fn command(
    context: &RequestContext,
    run_id: AgentRunId,
    expected_version: RunVersion,
    command: RunCommand,
) -> RunCommandEnvelope {
    RunCommandEnvelope {
        command_id: OperationId::new(),
        idempotency_key: None,
        workspace_id: context.workspace_id,
        run_id,
        actor: RunActor::Principal(context.principal_id),
        expected_version,
        correlation_id: CorrelationId::new(),
        issued_at: database_timestamp(now()),
        command,
    }
}

fn created_event(context: &RequestContext, run_id: AgentRunId) -> LegacyRunEventEnvelope {
    let occurred_at = database_timestamp(now());
    let payload = LegacyRunEvent::Created {
        principal_id: context.principal_id,
        title: "Projection test".to_owned(),
    };
    LegacyRunEventEnvelope {
        event_id: RunEventId::new(),
        workspace_id: context.workspace_id,
        run_id,
        sequence: RunVersion::INITIAL,
        event_type: payload.event_type().to_owned(),
        event_version: payload.event_version(),
        actor: RunActor::Principal(context.principal_id),
        causation_id: OperationId::new(),
        correlation_id: CorrelationId::new(),
        payload,
        occurred_at,
        recorded_at: occurred_at,
    }
}

#[tokio::test]
async fn create_command_builds_an_event_derived_projection() {
    let context = context();
    let run_id = AgentRunId::new();
    let event_store = Arc::new(InMemoryEventStore::default());
    let committer = Arc::new(RecordingCommitter::default());
    let service = RunCommandService::new(event_store, committer.clone());
    let envelope = command(
        &context,
        run_id,
        RunVersion::ZERO,
        RunCommand::Create {
            principal_id: context.principal_id,
            title: "  Projection test  ".to_owned(),
        },
    );

    let result = service.execute(&context, envelope.clone()).await.unwrap();

    assert_eq!(result.run.id, run_id);
    assert_eq!(result.run.objective, "Projection test");
    assert_eq!(result.run.status, RunStatus::Created);
    assert_eq!(result.run.version, RunVersion::INITIAL);
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].sequence, RunVersion::INITIAL);
    assert_eq!(result.events[0].actor, envelope.actor);
    assert_eq!(result.events[0].causation_id, envelope.command_id);
    assert_eq!(result.events[0].correlation_id, envelope.correlation_id);
    assert_eq!(result.events[0].occurred_at, envelope.issued_at);
    assert_eq!(result.run.created_at, envelope.issued_at);
    assert_eq!(result.run.updated_at, envelope.issued_at);

    let commits = committer.commits.lock().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].0, RunVersion::ZERO);
    assert_eq!(commits[0].1, result.events);
    assert_eq!(commits[0].2, result.run);
}

#[tokio::test]
async fn existing_command_replays_the_stream_and_advances_the_projection() {
    let context = context();
    let run_id = AgentRunId::new();
    let event_store = Arc::new(InMemoryEventStore {
        events: Mutex::new(vec![created_event(&context, run_id)]),
    });
    let committer = Arc::new(RecordingCommitter::default());
    let service = RunCommandService::new(event_store, committer.clone());
    let envelope = command(&context, run_id, RunVersion::INITIAL, RunCommand::Prepare);

    let result = service.execute(&context, envelope).await.unwrap();

    assert_eq!(result.run.status, RunStatus::Preparing);
    assert_eq!(result.run.version.value(), 2);
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].sequence.value(), 2);
    assert_eq!(committer.commits.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_transition_does_not_reach_the_committer() {
    let context = context();
    let run_id = AgentRunId::new();
    let event_store = Arc::new(InMemoryEventStore {
        events: Mutex::new(vec![created_event(&context, run_id)]),
    });
    let committer = Arc::new(RecordingCommitter::default());
    let service = RunCommandService::new(event_store, committer.clone());

    let result = service
        .execute(
            &context,
            command(&context, run_id, RunVersion::INITIAL, RunCommand::Start),
        )
        .await;

    assert!(matches!(result, Err(ApplicationError::Domain(_))));
    assert!(committer.commits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn stale_expected_version_is_reported_as_a_conflict() {
    let context = context();
    let run_id = AgentRunId::new();
    let event_store = Arc::new(InMemoryEventStore {
        events: Mutex::new(vec![created_event(&context, run_id)]),
    });
    let committer = Arc::new(RecordingCommitter::default());
    let service = RunCommandService::new(event_store, committer.clone());

    let result = service
        .execute(
            &context,
            command(&context, run_id, RunVersion::ZERO, RunCommand::Prepare),
        )
        .await;

    assert!(matches!(result, Err(ApplicationError::Conflict(_))));
    assert!(committer.commits.lock().unwrap().is_empty());
}
