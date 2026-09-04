use vestrace_domain::{
    DomainError,
    id::{AgentRunId, CorrelationId, RunCheckpointId, WorkItemId},
    run::{
        AgentRun, NewAgentRun, NewRunStep, RunActorRef, RunCheckpoint, RunCheckpointPayload,
        RunEvent, RunEventPayload, RunStatus, RunStep, RunTerminalResult, RunVersion,
    },
};

use crate::{ApplicationError, RequestContext};

use super::commands::{
    AddRunSteps, ApproveRun, CancelRun, CreateCheckpoint, CreateRun, NewRunStepInput, PauseRun,
    ResumeRun, TransitionRun, TransitionRunStep,
};
use super::ports::{
    CommitRun, RunClockPort, RunLeasePort, RunSnapshot, RunStorePort, WorkItem, WorkItemKind,
    WorkQueuePort, deterministic_idempotency_key,
};

pub struct RunCoordinator<S, C, Q, L>
where
    S: RunStorePort,
    C: RunClockPort,
    Q: WorkQueuePort,
    L: RunLeasePort,
{
    store: S,
    clock: C,
    work_queue: Q,
    #[allow(dead_code)]
    lease_port: L,
}

impl<S, C, Q, L> RunCoordinator<S, C, Q, L>
where
    S: RunStorePort,
    C: RunClockPort,
    Q: WorkQueuePort,
    L: RunLeasePort,
{
    pub fn new(store: S, clock: C, work_queue: Q, lease_port: L) -> Self {
        Self {
            store,
            clock,
            work_queue,
            lease_port,
        }
    }

    pub async fn create_run(
        &self,
        context: &RequestContext,
        command: CreateRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let at = self.clock.now();
        let run_id = AgentRunId::new();

        let run = AgentRun::create(
            NewAgentRun {
                id: run_id,
                workspace_id: context.workspace_id,
                objective: command.objective.clone(),
                coordinator_snapshot_id: command.coordinator_snapshot_id,
                execution_mode: command.execution_mode,
                parent: command.parent,
                budget_snapshot_id: None,
                resource_usage_snapshot_id: None,
            },
            at,
        )?;

        let event = RunEvent::new(
            run_id,
            context.workspace_id,
            RunVersion::INITIAL,
            vestrace_domain::run::ResumeCursor::from_version(RunVersion::INITIAL),
            // The authenticated caller, not `System`.
            //
            // This was hardcoded to `System`, so the canonical event log said
            // the system created every run while `agent_runs.principal_id`
            // named a person — the authoritative record disagreed with the
            // projection about who acted. It was invisible while one shared
            // administrator token mapped to one identity, and it is exactly
            // what per-principal credentials exist to fix.
            //
            // Taken from the context rather than from the command on purpose:
            // the context is the identity authentication resolved and the
            // middleware overwrote the caller's headers with, so it cannot be
            // asserted. A command field could be.
            RunActorRef::Principal(context.principal_id),
            RunEventPayload::RunCreated {
                objective: command.objective,
                execution_mode: command.execution_mode,
                coordinator_snapshot_id: run.coordinator_snapshot_id,
                parent: run.parent,
            },
            command.correlation_id.unwrap_or_default(),
            None,
            at,
        )?;

        let advance_item = WorkItem {
            id: WorkItemId::new(),
            run_id,
            kind: WorkItemKind::AdvanceRun,
            expected_run_version: RunVersion::INITIAL,
            available_at: at,
            idempotency_key: deterministic_idempotency_key(run_id, RunVersion::INITIAL, "advance"),
            attempt: 1,
        };

        let commit = CommitRun {
            run: run.clone(),
            event,
            new_steps: vec![],
            checkpoint: None,
            work_items: vec![advance_item],
        };

        self.store.create(context, commit).await
    }

    pub async fn transition_run(
        &self,
        context: &RequestContext,
        command: TransitionRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        self.transition_run_with(context, command, Vec::new()).await
    }

    /// A transition that also enqueues `extra` work items.
    ///
    /// They travel in the same commit as the transition rather than through a
    /// separate write, so a crash between the two cannot leave a run that has
    /// transitioned with nothing scheduled to act on it.
    async fn transition_run_with(
        &self,
        context: &RequestContext,
        command: TransitionRun,
        extra: Vec<WorkItem>,
    ) -> Result<RunSnapshot, ApplicationError> {
        let at = self.clock.now();
        let snapshot = self
            .store
            .load(context, command.run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::NotFound("run not found".into()))
            })?;

        if snapshot.run.version != command.expected_version {
            return Err(DomainError::RevisionConflict {
                expected: command.expected_version.value(),
                current: snapshot.run.version.value(),
            }
            .into());
        }

        let mut run = snapshot.run.clone();
        let _change = run.transition(
            command.expected_version,
            command.target,
            command.result.clone(),
            at,
        )?;

        let next_version = command.expected_version.next()?;
        let event = RunEvent::new(
            command.run_id,
            context.workspace_id,
            next_version,
            vestrace_domain::run::ResumeCursor::from_version(next_version),
            command.actor,
            RunEventPayload::RunStatusChanged {
                from: snapshot.run.status,
                to: command.target,
                result: command.result.clone(),
            },
            command.correlation_id.unwrap_or_default(),
            command.causation_event_id,
            at,
        )?;

        let mut work_items = extra;
        if command.target == RunStatus::Running && snapshot.run.status != RunStatus::Running {
            work_items.push(WorkItem {
                id: WorkItemId::new(),
                run_id: command.run_id,
                kind: WorkItemKind::AdvanceRun,
                expected_run_version: next_version,
                available_at: at,
                idempotency_key: deterministic_idempotency_key(
                    command.run_id,
                    next_version,
                    "advance",
                ),
                attempt: 1,
            });
        }
        if command.target.is_terminal() {
            let cancelled = self
                .work_queue
                .cancel_for_run(context, command.run_id, at)
                .await?;
            let _ = cancelled;
        }

        let commit = CommitRun {
            run: run.clone(),
            event,
            new_steps: vec![],
            checkpoint: None,
            work_items,
        };

        self.store.commit(context, commit).await
    }

    pub async fn add_steps(
        &self,
        context: &RequestContext,
        command: AddRunSteps,
    ) -> Result<RunSnapshot, ApplicationError> {
        let at = self.clock.now();
        let snapshot = self
            .store
            .load(context, command.run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::NotFound("run not found".into()))
            })?;

        if snapshot.run.version != command.expected_version {
            return Err(DomainError::RevisionConflict {
                expected: command.expected_version.value(),
                current: snapshot.run.version.value(),
            }
            .into());
        }

        let next_version = command.expected_version.next()?;

        let steps: Vec<RunStep> = command
            .steps
            .into_iter()
            .map(|dto| {
                let legacy_coordinator_input = !matches!(
                    dto.assigned_actor,
                    vestrace_domain::run::RunActorRef::AgentSnapshot(_)
                ) && matches!(dto.input, NewRunStepInput::None);
                if !legacy_coordinator_input {
                    return Err(ApplicationError::Unavailable(
                        "governed Run-step input authority is not configured".into(),
                    ));
                }
                RunStep::create(
                    NewRunStep {
                        id: dto.id,
                        run_id: command.run_id,
                        plan_step_reference: dto.plan_step_reference,
                        assigned_actor: dto.assigned_actor,
                        input_references: dto.input_references,
                    },
                    at,
                )
                .map_err(ApplicationError::from)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let event = RunEvent::new(
            command.run_id,
            context.workspace_id,
            next_version,
            vestrace_domain::run::ResumeCursor::from_version(next_version),
            command.actor,
            RunEventPayload::StepsAdded {
                steps: steps.clone(),
            },
            command.correlation_id.unwrap_or_default(),
            None,
            at,
        )?;

        let mut run = snapshot.run.clone();
        run.version = next_version;
        run.updated_at = at;

        let mut work_items = Vec::new();
        for step in &steps {
            if step.input_references.is_empty() {
                work_items.push(WorkItem {
                    id: WorkItemId::new(),
                    run_id: command.run_id,
                    kind: WorkItemKind::ExecuteStep { step_id: step.id },
                    expected_run_version: next_version,
                    available_at: at,
                    idempotency_key: deterministic_idempotency_key(
                        command.run_id,
                        next_version,
                        &format!("step:{}", step.id.as_uuid()),
                    ),
                    attempt: 1,
                });
            }
        }

        let commit = CommitRun {
            run: run.clone(),
            event,
            new_steps: steps,
            checkpoint: None,
            work_items,
        };

        self.store.commit(context, commit).await
    }

    pub async fn transition_step(
        &self,
        context: &RequestContext,
        command: TransitionRunStep,
    ) -> Result<RunSnapshot, ApplicationError> {
        let at = self.clock.now();
        let snapshot = self
            .store
            .load(context, command.run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::NotFound("run not found".into()))
            })?;

        if snapshot.run.version != command.expected_version {
            return Err(DomainError::RevisionConflict {
                expected: command.expected_version.value(),
                current: snapshot.run.version.value(),
            }
            .into());
        }

        let mut step = snapshot
            .steps
            .iter()
            .find(|s| s.id == command.step_id)
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::NotFound("step not found".into()))
            })?
            .clone();

        let prev_status = step.status;
        if let Some(failure) = &command.failure {
            step.fail(failure.clone(), at)?;
        } else {
            step.transition_to(command.target, at)?;
        }

        let next_version = command.expected_version.next()?;
        let event = RunEvent::new(
            command.run_id,
            context.workspace_id,
            next_version,
            vestrace_domain::run::ResumeCursor::from_version(next_version),
            command.actor,
            RunEventPayload::StepStatusChanged {
                step_id: command.step_id,
                from: prev_status,
                to: step.status,
                attempt: step.attempt,
            },
            CorrelationId::new(),
            None,
            at,
        )?;

        let mut run = snapshot.run.clone();
        run.version = next_version;
        run.updated_at = at;

        let commit = CommitRun {
            run: run.clone(),
            event,
            new_steps: vec![step],
            checkpoint: None,
            work_items: vec![],
        };

        self.store.commit(context, commit).await
    }

    pub async fn create_checkpoint(
        &self,
        context: &RequestContext,
        command: CreateCheckpoint,
    ) -> Result<RunSnapshot, ApplicationError> {
        let at = self.clock.now();
        let snapshot = self
            .store
            .load(context, command.run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::NotFound("run not found".into()))
            })?;

        if snapshot.run.version != command.expected_version {
            return Err(DomainError::RevisionConflict {
                expected: command.expected_version.value(),
                current: snapshot.run.version.value(),
            }
            .into());
        }

        let next_version = command.expected_version.next()?;
        let checkpoint_id = RunCheckpointId::new();

        let checkpoint = RunCheckpoint::new(
            checkpoint_id,
            context.workspace_id,
            command.run_id,
            next_version,
            snapshot.run.active_plan_revision_id,
            RunCheckpointPayload::V1(command.payload),
            at,
        )?;

        let event = RunEvent::new(
            command.run_id,
            context.workspace_id,
            next_version,
            vestrace_domain::run::ResumeCursor::from_version(next_version),
            command.actor,
            RunEventPayload::CheckpointCreated {
                checkpoint_id,
                resume_cursor: vestrace_domain::run::ResumeCursor::from_version(next_version),
            },
            CorrelationId::new(),
            None,
            at,
        )?;

        let mut run = snapshot.run.clone();
        run.version = next_version;
        run.checkpoint_id = Some(checkpoint_id);
        run.updated_at = at;

        let commit = CommitRun {
            run: run.clone(),
            event,
            new_steps: vec![],
            checkpoint: Some(checkpoint),
            work_items: vec![],
        };

        self.store.commit(context, commit).await
    }

    pub async fn pause_run(
        &self,
        context: &RequestContext,
        command: PauseRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        self.transition_run(
            context,
            TransitionRun {
                run_id: command.run_id,
                expected_version: command.expected_version,
                target: RunStatus::Paused,
                result: None,
                actor: command.actor,
                causation_event_id: None,
                correlation_id: command.correlation_id,
                idempotency_key: command.idempotency_key,
            },
        )
        .await
    }

    pub async fn resume_run(
        &self,
        context: &RequestContext,
        command: ResumeRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let next_version = command.expected_version.next()?;
        // Previously this work item was built and then thrown away with
        // `let _ = resume_item;`, so nothing ever enqueued `ResumeRun` and the
        // worker's `ResumeRunHandler` was unreachable.
        //
        // The `AdvanceRun` item the transition queues is not a substitute:
        // resuming has to re-establish the run's position from its checkpoint,
        // and advancing without that steps the run forward from stale state.
        let resume_item = WorkItem {
            id: WorkItemId::new(),
            run_id: command.run_id,
            kind: WorkItemKind::ResumeRun,
            expected_run_version: next_version,
            available_at: self.clock.now(),
            idempotency_key: deterministic_idempotency_key(command.run_id, next_version, "resume"),
            attempt: 1,
        };

        self.transition_run_with(
            context,
            TransitionRun {
                run_id: command.run_id,
                expected_version: command.expected_version,
                target: RunStatus::Running,
                result: None,
                actor: command.actor,
                causation_event_id: None,
                correlation_id: command.correlation_id,
                idempotency_key: command.idempotency_key.clone(),
            },
            vec![resume_item],
        )
        .await
    }

    /// Grant a pending approval and return the run to `Running`.
    ///
    /// Emits `run.approval_granted` naming the approver **in addition to** the
    /// status change, because an approval that is only a status change records
    /// that the run resumed and not who permitted it.
    pub async fn approve_run(
        &self,
        context: &RequestContext,
        command: ApproveRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let snapshot = self
            .store
            .load(context, command.run_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::NotFound("run not found".into()))
            })?;

        // Checked before the transition so the caller is told the run was not
        // awaiting approval, rather than being told about a version mismatch or
        // an illegal transition.
        if snapshot.run.status != RunStatus::WaitingForApproval {
            return Err(ApplicationError::Domain(DomainError::InvalidArgument(
                format!(
                    "run is {} and is not awaiting approval",
                    snapshot.run.status.as_str()
                ),
            )));
        }

        let approved = self
            .transition_run(
                context,
                TransitionRun {
                    run_id: command.run_id,
                    expected_version: command.expected_version,
                    target: RunStatus::Running,
                    result: None,
                    actor: command.actor.clone(),
                    causation_event_id: None,
                    correlation_id: command.correlation_id,
                    idempotency_key: command.idempotency_key,
                },
            )
            .await?;

        let at = self.clock.now();
        let next_version = approved.run.version.next()?;
        let event = RunEvent::new(
            command.run_id,
            context.workspace_id,
            next_version,
            vestrace_domain::run::ResumeCursor::from_version(next_version),
            command.actor,
            RunEventPayload::ApprovalGranted {
                approver_id: command.approver_id,
            },
            // Same correlation as the transition it accompanies, so both halves
            // of the approval trace back to one request.
            command.correlation_id.unwrap_or_default(),
            None,
            at,
        )?;

        let mut run = approved.run.clone();
        run.version = next_version;
        run.updated_at = at;

        self.store
            .commit(
                context,
                CommitRun {
                    run,
                    event,
                    new_steps: vec![],
                    checkpoint: None,
                    work_items: vec![],
                },
            )
            .await
    }

    pub async fn cancel_run(
        &self,
        context: &RequestContext,
        command: CancelRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let result = RunTerminalResult::Cancelled {
            reason: command.reason.clone(),
        };

        self.transition_run(
            context,
            TransitionRun {
                run_id: command.run_id,
                expected_version: command.expected_version,
                target: RunStatus::Cancelled,
                result: Some(result),
                actor: command.actor,
                causation_event_id: None,
                correlation_id: command.correlation_id,
                idempotency_key: command.idempotency_key,
            },
        )
        .await
    }
}

/// The run operations the transport layer issues.
///
/// Object-safe, because `RunCoordinator` is generic over its four ports and an
/// application state cannot hold it directly. Only the commands a caller can
/// legitimately issue appear here: step transitions and checkpoints belong to
/// the worker and are deliberately absent, so no HTTP route can drive a run's
/// internals by hand.
#[async_trait::async_trait]
pub trait RunOrchestrator: Send + Sync {
    async fn create_run(
        &self,
        context: &RequestContext,
        command: CreateRun,
    ) -> Result<RunSnapshot, ApplicationError>;

    async fn add_steps(
        &self,
        context: &RequestContext,
        command: AddRunSteps,
    ) -> Result<RunSnapshot, ApplicationError>;

    async fn pause_run(
        &self,
        context: &RequestContext,
        command: PauseRun,
    ) -> Result<RunSnapshot, ApplicationError>;

    async fn resume_run(
        &self,
        context: &RequestContext,
        command: ResumeRun,
    ) -> Result<RunSnapshot, ApplicationError>;

    async fn cancel_run(
        &self,
        context: &RequestContext,
        command: CancelRun,
    ) -> Result<RunSnapshot, ApplicationError>;

    async fn approve_run(
        &self,
        context: &RequestContext,
        command: ApproveRun,
    ) -> Result<RunSnapshot, ApplicationError>;
}

pub type SharedRunOrchestrator = std::sync::Arc<dyn RunOrchestrator>;

#[async_trait::async_trait]
impl<S, C, Q, L> RunOrchestrator for RunCoordinator<S, C, Q, L>
where
    S: RunStorePort + Send + Sync,
    C: RunClockPort + Send + Sync,
    Q: WorkQueuePort + Send + Sync,
    L: RunLeasePort + Send + Sync,
{
    async fn create_run(
        &self,
        context: &RequestContext,
        command: CreateRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        RunCoordinator::create_run(self, context, command).await
    }

    async fn add_steps(
        &self,
        context: &RequestContext,
        command: AddRunSteps,
    ) -> Result<RunSnapshot, ApplicationError> {
        RunCoordinator::add_steps(self, context, command).await
    }

    async fn pause_run(
        &self,
        context: &RequestContext,
        command: PauseRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        RunCoordinator::pause_run(self, context, command).await
    }

    async fn resume_run(
        &self,
        context: &RequestContext,
        command: ResumeRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        RunCoordinator::resume_run(self, context, command).await
    }

    async fn cancel_run(
        &self,
        context: &RequestContext,
        command: CancelRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        RunCoordinator::cancel_run(self, context, command).await
    }

    async fn approve_run(
        &self,
        context: &RequestContext,
        command: ApproveRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        RunCoordinator::approve_run(self, context, command).await
    }
}
