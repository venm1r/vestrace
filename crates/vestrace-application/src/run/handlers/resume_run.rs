use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    CorrelationId,
    id::WorkItemId,
    run::{ResumeCursor, RunActorRef, RunEvent, RunEventPayload, RunStatus},
};

use crate::{ApplicationError, RequestContext};

use super::super::ports::{
    CommitRun, RunClockPort, RunLease, RunSnapshot, RunStorePort, WorkItem, WorkItemKind,
    WorkItemKindDiscriminant, deterministic_idempotency_key,
};
use super::super::{RunWorkHandler, RunWorkOutcome};

pub struct ResumeRunHandler {
    store: Arc<dyn RunStorePort>,
    clock: Arc<dyn RunClockPort>,
}

impl ResumeRunHandler {
    pub fn new(store: Arc<dyn RunStorePort>, clock: Arc<dyn RunClockPort>) -> Self {
        Self { store, clock }
    }
}

#[async_trait]
impl RunWorkHandler for ResumeRunHandler {
    fn kind(&self) -> WorkItemKindDiscriminant {
        WorkItemKindDiscriminant::ResumeRun
    }

    async fn handle(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        _lease: &RunLease,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let run = &snapshot.run;

        if !run.status.can_resume() {
            return Ok(RunWorkOutcome::Completed);
        }

        let at = self.clock.now();
        let next_version = item.expected_run_version.next()?;

        let mut updated_run = run.clone();
        let from = updated_run.status;
        updated_run.transition(item.expected_run_version, RunStatus::Running, None, at)?;
        updated_run.version = next_version;
        updated_run.updated_at = at;

        let event = RunEvent::new(
            run.id,
            context.workspace_id,
            next_version,
            ResumeCursor::from_version(next_version),
            RunActorRef::System,
            RunEventPayload::RunStatusChanged {
                from,
                to: RunStatus::Running,
                result: None,
            },
            CorrelationId::new(),
            None,
            at,
        )?;

        let advance_item = WorkItem {
            id: WorkItemId::new(),
            run_id: run.id,
            kind: WorkItemKind::AdvanceRun,
            expected_run_version: next_version,
            available_at: at,
            idempotency_key: deterministic_idempotency_key(run.id, next_version, "advance"),
            attempt: 1,
        };

        let commit = CommitRun {
            run: updated_run,
            event,
            new_steps: vec![],
            checkpoint: None,
            work_items: vec![advance_item],
        };

        self.store.commit(context, commit).await?;
        Ok(RunWorkOutcome::Completed)
    }
}
