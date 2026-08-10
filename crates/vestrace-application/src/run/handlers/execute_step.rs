use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    CorrelationId,
    id::WorkItemId,
    run::{ResumeCursor, RunActorRef, RunEvent, RunEventPayload, RunStepStatus},
    time::Timestamp,
};

use crate::{ApplicationError, RequestContext};

use super::super::ports::{
    CommitRun, RunClockPort, RunLease, RunSnapshot, RunStorePort, WorkItem, WorkItemKind,
    WorkItemKindDiscriminant, deterministic_idempotency_key,
};
use super::super::{RunWorkHandler, RunWorkOutcome};

pub struct ExecuteStepHandler {
    store: Arc<dyn RunStorePort>,
    clock: Arc<dyn RunClockPort>,
}

impl ExecuteStepHandler {
    pub fn new(store: Arc<dyn RunStorePort>, clock: Arc<dyn RunClockPort>) -> Self {
        Self { store, clock }
    }
}

#[async_trait]
impl RunWorkHandler for ExecuteStepHandler {
    fn kind(&self) -> WorkItemKindDiscriminant {
        WorkItemKindDiscriminant::ExecuteStep
    }

    async fn handle(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        _lease: &RunLease,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let step_id = match &item.kind {
            WorkItemKind::ExecuteStep { step_id } => *step_id,
            _ => return Ok(RunWorkOutcome::Completed),
        };

        let mut step = snapshot
            .steps
            .iter()
            .find(|s| s.id == step_id)
            .ok_or_else(|| {
                ApplicationError::Internal(format!("step {step_id} not found in snapshot"))
            })?
            .clone();

        if step.status.is_terminal() {
            return Ok(RunWorkOutcome::Completed);
        }

        let at = self.clock.now();
        let run_version = item.expected_run_version.next()?;

        let prev_status = step.status;
        step.status = RunStepStatus::Running;
        step.started_at = Some(at);

        let mut updated_run = snapshot.run.clone();
        updated_run.version = run_version;
        updated_run.updated_at = at;
        updated_run.current_step_id = Some(step_id);

        let event = RunEvent::new(
            updated_run.id,
            context.workspace_id,
            run_version,
            ResumeCursor::from_version(run_version),
            RunActorRef::System,
            RunEventPayload::StepStatusChanged {
                step_id,
                from: prev_status,
                to: RunStepStatus::Running,
                attempt: step.attempt,
            },
            CorrelationId::new(),
            None,
            at,
        )?;

        let advance_item = WorkItem {
            id: WorkItemId::new(),
            run_id: updated_run.id,
            kind: WorkItemKind::AdvanceRun,
            expected_run_version: run_version,
            available_at: at,
            idempotency_key: deterministic_idempotency_key(updated_run.id, run_version, "advance"),
            attempt: 1,
        };

        let commit = CommitRun {
            run: updated_run.clone(),
            event,
            new_steps: vec![step.clone()],
            checkpoint: None,
            work_items: vec![advance_item],
        };

        self.store.commit(context, commit).await?;

        self.complete_step(context, &updated_run, &step, at).await
    }
}

impl ExecuteStepHandler {
    async fn complete_step(
        &self,
        context: &RequestContext,
        run: &vestrace_domain::run::AgentRun,
        step: &vestrace_domain::run::RunStep,
        _started_at: Timestamp,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let at = self.clock.now();
        let run_version = run.version.next()?;

        let prev_status = step.status;
        let mut completed_step = step.clone();
        completed_step.status = RunStepStatus::Succeeded;
        completed_step.finished_at = Some(at);

        let mut updated_run = run.clone();
        updated_run.version = run_version;
        updated_run.updated_at = at;

        let event = RunEvent::new(
            run.id,
            context.workspace_id,
            run_version,
            ResumeCursor::from_version(run_version),
            RunActorRef::System,
            RunEventPayload::StepStatusChanged {
                step_id: step.id,
                from: prev_status,
                to: RunStepStatus::Succeeded,
                attempt: step.attempt,
            },
            CorrelationId::new(),
            None,
            at,
        )?;

        let advance_item = WorkItem {
            id: WorkItemId::new(),
            run_id: run.id,
            kind: WorkItemKind::AdvanceRun,
            expected_run_version: run_version,
            available_at: at,
            idempotency_key: deterministic_idempotency_key(run.id, run_version, "advance"),
            attempt: 1,
        };

        let commit = CommitRun {
            run: updated_run,
            event,
            new_steps: vec![completed_step],
            checkpoint: None,
            work_items: vec![advance_item],
        };

        self.store.commit(context, commit).await?;
        Ok(RunWorkOutcome::Completed)
    }
}
