use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    CorrelationId,
    id::WorkItemId,
    run::{ResumeCursor, RunActorRef, RunEvent, RunEventPayload, RunFailure, RunStepStatus},
    time::Timestamp,
};

use crate::{ApplicationError, RequestContext};

use super::super::ports::{
    CommitRun, RunClockPort, RunLease, RunSnapshot, RunStorePort, WorkItem, WorkItemKind,
    WorkItemKindDiscriminant, deterministic_idempotency_key,
};
use super::super::{RunWorkHandler, RunWorkOutcome, StepModelOutcome};

pub struct ExecuteStepHandler {
    store: Arc<dyn RunStorePort>,
    clock: Arc<dyn RunClockPort>,
    model: Option<super::super::SharedStepModelExecutor>,
}

impl ExecuteStepHandler {
    pub fn new(store: Arc<dyn RunStorePort>, clock: Arc<dyn RunClockPort>) -> Self {
        Self {
            store,
            clock,
            model: None,
        }
    }

    /// Supply the executor that performs an agent step's model call.
    ///
    /// Without it, an agent-assigned step **fails** rather than succeeding: a
    /// step that was supposed to invoke a model and did not has not been
    /// performed, and reporting success would make the whole run a false
    /// record. Steps assigned to a principal, a worker or the system are
    /// unaffected — they were never meant to call a model.
    pub fn with_model_executor(mut self, executor: super::super::SharedStepModelExecutor) -> Self {
        self.model = Some(executor);
        self
    }
}

/// How long to wait before re-attempting a step whose dispatch was contended
/// and quoted no interval of its own.
///
/// Deliberately short: nothing was called, so coming back early costs one
/// refused admission rather than a duplicate external effect.
const CONTENDED_DISPATCH_RETRY_SECONDS: u32 = 5;

/// Whether this step is one a model is supposed to perform.
///
/// Read from the step's assigned actor rather than from its kind or label: the
/// actor is the field that says who is responsible for the step, and inferring
/// it from a free-text label would let a rename change what executes.
fn is_agent_step(actor: &RunActorRef) -> bool {
    matches!(actor, RunActorRef::AgentSnapshot(_))
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

        if !is_agent_step(&step.assigned_actor) {
            // Nothing to invoke: this step belongs to a principal, a worker or
            // the system, and completing it here is what it always meant.
            return self.complete_step(context, &updated_run, &step, at).await;
        }

        let Some(executor) = self.model.as_ref() else {
            // The step was supposed to invoke a model and no executor exists.
            // Reporting success here is what made every run a false record.
            return self
                .fail_step(
                    context,
                    &updated_run,
                    &step,
                    RunFailure {
                        code: "model_executor_unconfigured".to_string(),
                        message: "the step is assigned to an agent but no model executor is \
                                  configured, so it was not performed"
                            .to_string(),
                        // Configuration will not change by retrying.
                        retryable: false,
                    },
                )
                .await;
        };

        let outcome = executor
            .execute(
                context,
                super::super::StepModelRequest {
                    run_id: updated_run.id,
                    step_id: step.id,
                },
            )
            .await;

        match outcome {
            // The result finalizer published this step inside the same
            // transaction that bound its encrypted output. Completing it again
            // here would advance the Run a second time from stale state.
            Ok(StepModelOutcome::Published) => Ok(RunWorkOutcome::Completed),
            Ok(StepModelOutcome::Denied { authorization_id }) => {
                self.fail_step(
                    context,
                    &updated_run,
                    &step,
                    RunFailure {
                        code: "provider_dispatch_denied".to_string(),
                        message: format!(
                            "policy refused this step's provider dispatch; \
                             the decision is recorded as {authorization_id}"
                        ),
                        // A refusal is a decision, not a transient fault.
                        retryable: false,
                    },
                )
                .await
            }
            // Nothing was called and the step is not finished. Completing the
            // item here would leave a step nobody is coming back for, and
            // failing it would report a failure for work that may be
            // succeeding elsewhere; the item is rescheduled instead. If the
            // other owner publishes first, the next pass sees a terminal step
            // and stops.
            Ok(StepModelOutcome::Conflict {
                retry_after_seconds,
            }) => Ok(RunWorkOutcome::Retry {
                available_at: at
                    + chrono::Duration::seconds(i64::from(
                        retry_after_seconds.unwrap_or(CONTENDED_DISPATCH_RETRY_SECONDS),
                    )),
                error: RunFailure {
                    code: "provider_dispatch_contended".to_string(),
                    message: "this step's provider dispatch could not be admitted yet; \
                              nothing was called and it will be attempted again"
                        .to_string(),
                    retryable: true,
                },
            }),
            Ok(StepModelOutcome::RecoveredUnknown) => {
                self.fail_step(
                    context,
                    &updated_run,
                    &step,
                    RunFailure {
                        code: "provider_dispatch_unknown".to_string(),
                        message: "a provider dispatch for this step left the process and its \
                                  outcome could not be established; it was adopted as unknown \
                                  and will not be called again"
                            .to_string(),
                        // Retrying would be a second effect for a call that may
                        // already have happened.
                        retryable: false,
                    },
                )
                .await
            }
            Err(error) => {
                // `Unavailable` is the application's word for "may succeed
                // later", so it is the one failure the step marks retryable.
                let retryable = matches!(error, ApplicationError::Unavailable(_));
                self.fail_step(
                    context,
                    &updated_run,
                    &step,
                    RunFailure {
                        code: "model_invocation_failed".to_string(),
                        message: error.to_string(),
                        retryable,
                    },
                )
                .await
            }
        }
    }
}

impl ExecuteStepHandler {
    /// Complete a step this process performed itself.
    ///
    /// Only non-agent steps reach it. A governed agent step is published by the
    /// provider-result finalizer, which advances the Run in the same
    /// transaction that binds the encrypted output; a second completion here
    /// would write a second success from state the finalizer has already moved.
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

    /// Record that the step did not happen, and why.
    ///
    /// The failure is written to the step and emitted as a status change, so
    /// the canonical history shows a failed step rather than a gap. An advance
    /// item still follows: the run must be told to move on, or it stalls in
    /// `Running` with nothing scheduled.
    async fn fail_step(
        &self,
        context: &RequestContext,
        run: &vestrace_domain::run::AgentRun,
        step: &vestrace_domain::run::RunStep,
        failure: RunFailure,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        let at = self.clock.now();
        let run_version = run.version.next()?;

        let prev_status = step.status;
        let mut failed_step = step.clone();
        failed_step.status = RunStepStatus::Failed;
        failed_step.finished_at = Some(at);
        failed_step.error = Some(failure);

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
                to: RunStepStatus::Failed,
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

        self.store
            .commit(
                context,
                CommitRun {
                    run: updated_run,
                    event,
                    new_steps: vec![failed_step],
                    checkpoint: None,
                    work_items: vec![advance_item],
                },
            )
            .await?;
        Ok(RunWorkOutcome::Completed)
    }
}
