//! Recovery must reconstruct the stream snapshot captured at its observed head.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, RequestContext, RunCheckpoint, RunRecoveryOperations, RunRecoveryService,
    RunRecoveryStore, hash_run_state,
};
use vestrace_domain::{
    id::{AgentRunId, CorrelationId, OperationId, PrincipalId, RunEventId, WorkspaceId},
    now,
    run::{AgentRun, RunActor, RunEvent, RunEventEnvelope, RunVersion, replay},
};

struct AdvancedTailStore {
    observed_head: RunVersion,
    events: Vec<RunEventEnvelope>,
    checkpoint: RunCheckpoint,
}

#[async_trait]
impl RunRecoveryStore for AdvancedTailStore {
    async fn load_stream_head(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Option<RunVersion>, ApplicationError> {
        Ok(Some(self.observed_head))
    }

    async fn load_events_through(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        through: RunVersion,
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError> {
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
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError> {
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
        Ok((self.checkpoint.sequence == sequence).then(|| self.checkpoint.clone()))
    }

    async fn load_latest_checkpoint(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Option<RunCheckpoint>, ApplicationError> {
        Ok(Some(self.checkpoint.clone()))
    }

    async fn save_checkpoint(
        &self,
        _context: &RequestContext,
        _checkpoint: &RunCheckpoint,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Internal(
            "snapshot-boundary test does not save checkpoints".to_owned(),
        ))
    }

    async fn replace_projection(
        &self,
        _context: &RequestContext,
        _expected_stream_version: RunVersion,
        _projection: &AgentRun,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Internal(
            "snapshot-boundary test does not replace projections".to_owned(),
        ))
    }
}

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn event(
    context: &RequestContext,
    run_id: AgentRunId,
    sequence: u64,
    payload: RunEvent,
) -> RunEventEnvelope {
    let occurred_at = now();
    RunEventEnvelope {
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

#[tokio::test]
async fn restore_ignores_events_committed_after_the_observed_head() {
    let context = context();
    let run_id = AgentRunId::new();
    let events = vec![
        event(
            &context,
            run_id,
            1,
            RunEvent::Created {
                principal_id: context.principal_id,
                title: "Snapshot boundary".to_owned(),
            },
        ),
        event(&context, run_id, 2, RunEvent::MarkedReady),
        event(&context, run_id, 3, RunEvent::Started),
        event(
            &context,
            run_id,
            4,
            RunEvent::Completed {
                summary: Some("committed after observed head".to_owned()),
            },
        ),
    ];
    let checkpoint_state = replay(events[..2].to_vec()).unwrap().unwrap();
    let checkpoint = RunCheckpoint {
        workspace_id: context.workspace_id,
        run_id,
        sequence: checkpoint_state.version,
        format_version: 1,
        state_hash: hash_run_state(&checkpoint_state).unwrap(),
        state: checkpoint_state,
        created_at: now(),
    };
    let store = Arc::new(AdvancedTailStore {
        observed_head: RunVersion::new(3).unwrap(),
        events: events.clone(),
        checkpoint,
    });
    let service = RunRecoveryService::new(store);

    let restored = service.restore(&context, run_id).await.unwrap();

    assert_eq!(restored, replay(events[..3].to_vec()).unwrap().unwrap());
}
