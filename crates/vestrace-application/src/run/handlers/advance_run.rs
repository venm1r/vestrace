use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    CorrelationId,
    id::{RunStepId, WorkItemId},
    run::{
        ResumeCursor, RunActorRef, RunEvent, RunEventPayload, RunStatus, RunTerminalResult,
        RunVersion,
    },
};

use crate::{ApplicationError, RequestContext};

use super::super::ports::{
    CommitRun, RunClockPort, RunLease, RunSnapshot, RunStorePort, WorkItem, WorkItemKind,
    WorkItemKindDiscriminant, WorkQueuePort, deterministic_idempotency_key,
};
use super::super::{RunWorkHandler, RunWorkOutcome};

pub struct AdvanceRunHandler {
    store: Arc<dyn RunStorePort>,
    queue: Arc<dyn WorkQueuePort>,
    clock: Arc<dyn RunClockPort>,
}

impl AdvanceRunHandler {
    pub fn new(
        store: Arc<dyn RunStorePort>,
        queue: Arc<dyn WorkQueuePort>,
        clock: Arc<dyn RunClockPort>,
    ) -> Self {
        Self {
            store,
            queue,
            clock,
        }
    }
}

#[async_trait]
impl RunWorkHandler for AdvanceRunHandler {
    fn kind(&self) -> WorkItemKindDiscriminant {
        WorkItemKindDiscriminant::AdvanceRun
    }

    async fn handle(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        _lease: &RunLease,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let run = &snapshot.run;
        let at = self.clock.now();
        let next_version = item.expected_run_version.next()?;

        match run.status {
            RunStatus::Created => {
                self.prepare_run(context, snapshot, item, next_version, at)
                    .await
            }
            RunStatus::Preparing => {
                self.start_run(context, snapshot, item, next_version, at)
                    .await
            }
            RunStatus::Running => {
                self.advance_running_run(context, snapshot, item, next_version, at)
                    .await
            }
            _ => Ok(RunWorkOutcome::Completed),
        }
    }
}

impl AdvanceRunHandler {
    async fn prepare_run(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        next_version: RunVersion,
        at: vestrace_domain::time::Timestamp,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let mut run = snapshot.run.clone();
        let from = run.status;
        run.transition(item.expected_run_version, RunStatus::Preparing, None, at)?;
        run.version = next_version;
        run.updated_at = at;

        let event = RunEvent::new(
            run.id,
            context.workspace_id,
            next_version,
            ResumeCursor::from_version(next_version),
            RunActorRef::System,
            RunEventPayload::RunStatusChanged {
                from,
                to: RunStatus::Preparing,
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
            run,
            event,
            new_steps: vec![],
            checkpoint: None,
            work_items: vec![advance_item],
        };

        self.store.commit(context, commit).await?;
        Ok(RunWorkOutcome::Completed)
    }

    async fn start_run(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        next_version: RunVersion,
        at: vestrace_domain::time::Timestamp,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let mut run = snapshot.run.clone();
        let from = run.status;
        run.transition(item.expected_run_version, RunStatus::Running, None, at)?;
        run.version = next_version;
        run.updated_at = at;

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

        let pending_ready: Vec<RunStepId> = snapshot
            .steps
            .iter()
            .filter(|s| {
                s.input_references.is_empty()
                    && s.status == vestrace_domain::run::RunStepStatus::Pending
            })
            .map(|s| s.id)
            .collect();

        let all_steps_terminal =
            !snapshot.steps.is_empty() && snapshot.steps.iter().all(|s| s.status.is_terminal());

        let mut work_items = Vec::new();

        if all_steps_terminal {
            return self
                .finalize_run(context, snapshot, item, next_version, at)
                .await;
        }

        for step_id in pending_ready {
            work_items.push(WorkItem {
                id: WorkItemId::new(),
                run_id: run.id,
                kind: WorkItemKind::ExecuteStep { step_id },
                expected_run_version: next_version,
                available_at: at,
                idempotency_key: deterministic_idempotency_key(
                    run.id,
                    next_version,
                    &format!("step:{}", step_id.as_uuid()),
                ),
                attempt: 1,
            });
        }

        let advance_item = WorkItem {
            id: WorkItemId::new(),
            run_id: run.id,
            kind: WorkItemKind::AdvanceRun,
            expected_run_version: next_version,
            available_at: at,
            idempotency_key: deterministic_idempotency_key(run.id, next_version, "advance"),
            attempt: 1,
        };
        work_items.push(advance_item);

        let commit = CommitRun {
            run,
            event,
            new_steps: vec![],
            checkpoint: None,
            work_items,
        };

        self.store.commit(context, commit).await?;
        Ok(RunWorkOutcome::Completed)
    }

    async fn advance_running_run(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        next_version: RunVersion,
        at: vestrace_domain::time::Timestamp,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let run = &snapshot.run;

        let pending_ready: Vec<RunStepId> = snapshot
            .steps
            .iter()
            .filter(|s| {
                s.input_references.is_empty()
                    && s.status == vestrace_domain::run::RunStepStatus::Pending
            })
            .map(|s| s.id)
            .collect();

        let all_steps_terminal =
            !snapshot.steps.is_empty() && snapshot.steps.iter().all(|s| s.status.is_terminal());

        if all_steps_terminal {
            return self
                .finalize_run(context, snapshot, item, next_version, at)
                .await;
        }

        if pending_ready.is_empty() {
            return Ok(RunWorkOutcome::Completed);
        }

        let mut work_items = Vec::with_capacity(pending_ready.len());
        for step_id in pending_ready {
            work_items.push(WorkItem {
                id: WorkItemId::new(),
                run_id: run.id,
                kind: WorkItemKind::ExecuteStep { step_id },
                expected_run_version: next_version,
                available_at: at,
                idempotency_key: deterministic_idempotency_key(
                    run.id,
                    next_version,
                    &format!("step:{}", step_id.as_uuid()),
                ),
                attempt: 1,
            });
        }

        let mut updated_run = run.clone();
        updated_run.version = next_version;
        updated_run.updated_at = at;

        let event = RunEvent::new(
            run.id,
            context.workspace_id,
            next_version,
            ResumeCursor::from_version(next_version),
            RunActorRef::System,
            RunEventPayload::CurrentStepChanged {
                previous: run.current_step_id,
                current: work_items.first().map(|wi| {
                    if let WorkItemKind::ExecuteStep { step_id } = wi.kind {
                        step_id
                    } else {
                        unreachable!()
                    }
                }),
            },
            CorrelationId::new(),
            None,
            at,
        )?;

        let commit = CommitRun {
            run: updated_run,
            event,
            new_steps: vec![],
            checkpoint: None,
            work_items,
        };

        self.store.commit(context, commit).await?;
        Ok(RunWorkOutcome::Completed)
    }

    async fn finalize_run(
        &self,
        context: &RequestContext,
        snapshot: &RunSnapshot,
        item: &WorkItem,
        next_version: RunVersion,
        at: vestrace_domain::time::Timestamp,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let any_failed = snapshot
            .steps
            .iter()
            .any(|s| s.status == vestrace_domain::run::RunStepStatus::Failed);
        let all_succeeded = snapshot
            .steps
            .iter()
            .all(|s| s.status == vestrace_domain::run::RunStepStatus::Succeeded);

        let (target, result) = if all_succeeded {
            (
                RunStatus::Succeeded,
                Some(RunTerminalResult::Succeeded { summary: None }),
            )
        } else if any_failed {
            (
                RunStatus::Failed,
                Some(RunTerminalResult::Failed {
                    code: "step_failure".into(),
                    message: "one or more steps failed".into(),
                    retryable: false,
                }),
            )
        } else {
            (
                RunStatus::Partial,
                Some(RunTerminalResult::Partial {
                    summary: "some steps did not complete successfully".into(),
                    remaining_work: vec![],
                }),
            )
        };

        let mut run = snapshot.run.clone();
        let from = run.status;
        run.transition(item.expected_run_version, target, result.clone(), at)?;
        run.version = next_version;
        run.updated_at = at;

        let event = RunEvent::new(
            run.id,
            context.workspace_id,
            next_version,
            ResumeCursor::from_version(next_version),
            RunActorRef::System,
            RunEventPayload::RunStatusChanged {
                from,
                to: target,
                result,
            },
            CorrelationId::new(),
            None,
            at,
        )?;

        let commit = CommitRun {
            run,
            event,
            new_steps: vec![],
            checkpoint: None,
            work_items: vec![],
        };

        self.store.commit(context, commit).await?;

        self.queue
            .cancel_for_run(context, snapshot.run.id, at)
            .await?;

        Ok(RunWorkOutcome::Completed)
    }
}
