//! Application-level checkpoint, replay, and projection rebuild contract.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, RequestContext, RunCheckpoint, RunRecoveryOperations, RunRecoveryService,
    RunRecoveryStore, hash_run_state, project_run,
};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId},
    now,
    run::{
        AgentRun, LegacyRunEvent, LegacyRunEventEnvelope, RunActor, RunState, RunVersion, replay,
    },
};

struct InMemoryRecoveryStore {
    head: RunVersion,
    events: Vec<LegacyRunEventEnvelope>,
    checkpoint: Mutex<Option<RunCheckpoint>>,
    saved: Mutex<Vec<RunCheckpoint>>,
    replacements: Mutex<Vec<(RunVersion, AgentRun)>>,
}

impl InMemoryRecoveryStore {
    fn new(
        head: RunVersion,
        events: Vec<LegacyRunEventEnvelope>,
        checkpoint: Option<RunCheckpoint>,
    ) -> Self {
        Self {
            head,
            events,
            checkpoint: Mutex::new(checkpoint),
            saved: Mutex::new(Vec::new()),
            replacements: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl RunRecoveryStore for InMemoryRecoveryStore {
    async fn load_stream_head(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Option<RunVersion>, ApplicationError> {
        Ok(Some(self.head))
    }

    async fn load_events_through(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        through: RunVersion,
    ) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
        Ok(self
            .events
            .iter()
            .filter(|event| {
                event.workspace_id == context.workspace_id
                    && event.run_id == run_id
                    && event.sequence <= through
            })
            .cloned()
            .collect())
    }

    async fn load_events_after(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        after: RunVersion,
    ) -> Result<Vec<LegacyRunEventEnvelope>, ApplicationError> {
        Ok(self
            .events
            .iter()
            .filter(|event| {
                event.workspace_id == context.workspace_id
                    && event.run_id == run_id
                    && event.sequence > after
            })
            .cloned()
            .collect())
    }

    async fn load_checkpoint(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
        sequence: RunVersion,
    ) -> Result<Option<RunCheckpoint>, ApplicationError> {
        Ok(self
            .checkpoint
            .lock()
            .unwrap()
            .as_ref()
            .filter(|checkpoint| checkpoint.sequence == sequence)
            .cloned())
    }

    async fn load_latest_checkpoint(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Option<RunCheckpoint>, ApplicationError> {
        Ok(self.checkpoint.lock().unwrap().clone())
    }

    async fn save_checkpoint(
        &self,
        _context: &RequestContext,
        checkpoint: &RunCheckpoint,
    ) -> Result<(), ApplicationError> {
        *self.checkpoint.lock().unwrap() = Some(checkpoint.clone());
        self.saved.lock().unwrap().push(checkpoint.clone());
        Ok(())
    }

    async fn replace_projection(
        &self,
        _context: &RequestContext,
        expected_stream_version: RunVersion,
        projection: &AgentRun,
    ) -> Result<(), ApplicationError> {
        self.replacements
            .lock()
            .unwrap()
            .push((expected_stream_version, projection.clone()));
        Ok(())
    }
}

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn envelope(
    context: &RequestContext,
    run_id: AgentRunId,
    sequence: u64,
    payload: LegacyRunEvent,
) -> LegacyRunEventEnvelope {
    let occurred_at = now();
    LegacyRunEventEnvelope {
        event_id: RunEventId::new(),
        workspace_id: context.workspace_id,
        run_id,
        sequence: RunVersion::new(sequence).unwrap(),
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

fn lifecycle_events(context: &RequestContext, run_id: AgentRunId) -> Vec<LegacyRunEventEnvelope> {
    vec![
        envelope(
            context,
            run_id,
            1,
            LegacyRunEvent::Created {
                principal_id: context.principal_id,
                title: "Recoverable run".to_owned(),
            },
        ),
        envelope(context, run_id, 2, LegacyRunEvent::Prepared),
        envelope(context, run_id, 3, LegacyRunEvent::Started),
        envelope(
            context,
            run_id,
            4,
            LegacyRunEvent::Succeeded {
                summary: Some("done".to_owned()),
            },
        ),
    ]
}

fn checkpoint_at(events: &[LegacyRunEventEnvelope], sequence: usize) -> RunCheckpoint {
    let state = replay(events[..sequence].to_vec()).unwrap().unwrap();
    RunCheckpoint {
        workspace_id: state.workspace_id,
        run_id: state.id,
        sequence: state.version,
        format_version: 1,
        state_hash: hash_run_state(&state).unwrap(),
        state,
        created_at: now(),
    }
}

#[tokio::test]
async fn checkpoint_creation_replays_and_hashes_the_authoritative_stream() {
    let context = context();
    let run_id = AgentRunId::new();
    let events = lifecycle_events(&context, run_id);
    let store = Arc::new(InMemoryRecoveryStore::new(
        RunVersion::new(4).unwrap(),
        events.clone(),
        None,
    ));
    let service = RunRecoveryService::new(store.clone());

    let checkpoint = service.create_checkpoint(&context, run_id).await.unwrap();

    let expected = replay(events).unwrap().unwrap();
    assert_eq!(checkpoint.state, expected);
    assert_eq!(checkpoint.sequence, RunVersion::new(4).unwrap());
    assert_eq!(checkpoint.state_hash, hash_run_state(&expected).unwrap());
    assert_eq!(store.saved.lock().unwrap().as_slice(), &[checkpoint]);
}

#[tokio::test]
async fn restore_from_checkpoint_plus_tail_equals_full_replay() {
    let context = context();
    let run_id = AgentRunId::new();
    let events = lifecycle_events(&context, run_id);
    let checkpoint = checkpoint_at(&events, 2);
    let store = Arc::new(InMemoryRecoveryStore::new(
        RunVersion::new(4).unwrap(),
        events.clone(),
        Some(checkpoint),
    ));
    let service = RunRecoveryService::new(store);

    let restored = service.restore(&context, run_id).await.unwrap();

    assert_eq!(restored, replay(events).unwrap().unwrap());
}

#[tokio::test]
async fn restore_without_checkpoint_equals_full_replay() {
    let context = context();
    let run_id = AgentRunId::new();
    let events = lifecycle_events(&context, run_id);
    let store = Arc::new(InMemoryRecoveryStore::new(
        RunVersion::new(4).unwrap(),
        events.clone(),
        None,
    ));
    let service = RunRecoveryService::new(store);

    let restored = service.restore(&context, run_id).await.unwrap();

    assert_eq!(restored, replay(events).unwrap().unwrap());
}

#[tokio::test]
async fn restore_rejects_a_checkpoint_with_an_invalid_hash() {
    let context = context();
    let run_id = AgentRunId::new();
    let events = lifecycle_events(&context, run_id);
    let mut checkpoint = checkpoint_at(&events, 2);
    checkpoint.state_hash = "0".repeat(64);
    let store = Arc::new(InMemoryRecoveryStore::new(
        RunVersion::new(4).unwrap(),
        events,
        Some(checkpoint),
    ));
    let service = RunRecoveryService::new(store);

    let result = service.restore(&context, run_id).await;

    assert!(matches!(result, Err(ApplicationError::Storage(_))));
}

#[tokio::test]
async fn validation_rejects_a_checkpoint_that_disagrees_with_event_prefix() {
    let context = context();
    let run_id = AgentRunId::new();
    let events = lifecycle_events(&context, run_id);
    let mut checkpoint = checkpoint_at(&events, 2);
    checkpoint.state.title = "tampered but rehashed".to_owned();
    checkpoint.state_hash = hash_run_state(&checkpoint.state).unwrap();
    let store = Arc::new(InMemoryRecoveryStore::new(
        RunVersion::new(4).unwrap(),
        events,
        Some(checkpoint.clone()),
    ));
    let service = RunRecoveryService::new(store);

    let result = service
        .validate_checkpoint(&context, run_id, checkpoint.sequence)
        .await;

    assert!(matches!(result, Err(ApplicationError::Storage(_))));
}

#[tokio::test]
async fn rebuild_uses_the_shared_projection_mapper_and_guarded_stream_head() {
    let context = context();
    let run_id = AgentRunId::new();
    let events = lifecycle_events(&context, run_id);
    let checkpoint = checkpoint_at(&events, 2);
    let expected_state: RunState = replay(events.clone()).unwrap().unwrap();
    let store = Arc::new(InMemoryRecoveryStore::new(
        RunVersion::new(4).unwrap(),
        events,
        Some(checkpoint),
    ));
    let service = RunRecoveryService::new(store.clone());

    let rebuilt = service.rebuild_projection(&context, run_id).await.unwrap();

    assert_eq!(rebuilt, project_run(&expected_state));
    assert_eq!(
        store.replacements.lock().unwrap().as_slice(),
        &[(RunVersion::new(4).unwrap(), rebuilt)]
    );
}
