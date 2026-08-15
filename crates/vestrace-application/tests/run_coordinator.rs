use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::run::{
    commands::{
        AddRunSteps, CancelRun, CreateCheckpoint, CreateRun, NewRunStepDto, PauseRun, ResumeRun,
        TransitionRun, TransitionRunStep,
    },
    coordinator::RunCoordinator,
    ports::{
        AcquireRunLease, CommitRun, LeaseWorkRequest, RunClockPort, RunLease, RunLeasePort,
        RunSnapshot, RunStorePort, WorkItem, WorkItemKind, WorkQueuePort,
    },
};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::{
    DomainError,
    id::{AgentRunId, AgentRuntimeSnapshotId, RunStepId},
    run::{
        RunActorRef, RunCheckpointPayloadV1, RunExecutionMode, RunFailure, RunStatus,
        RunStepStatus, RunTerminalResult, RunVersion,
    },
    time::Timestamp,
};

struct FakeClock {
    now: Mutex<Timestamp>,
}

impl FakeClock {
    fn new(at: Timestamp) -> Self {
        Self {
            now: Mutex::new(at),
        }
    }
}

impl RunClockPort for FakeClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().unwrap()
    }
}

#[derive(Clone)]
struct InMemoryStore {
    runs: Arc<Mutex<HashMap<AgentRunId, RunSnapshot>>>,
    events: Arc<Mutex<Vec<vestrace_domain::run::RunEvent>>>,
    work_items: Arc<Mutex<Vec<WorkItem>>>,
}

impl InMemoryStore {
    fn new() -> Self {
        Self {
            runs: Arc::new(Mutex::new(HashMap::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            work_items: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl RunStorePort for InMemoryStore {
    async fn load(
        &self,
        _context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError> {
        Ok(self.runs.lock().unwrap().get(&run_id).cloned())
    }

    async fn create(
        &self,
        _context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let mut runs = self.runs.lock().unwrap();
        if runs.contains_key(&commit.run.id) {
            return Err(DomainError::InvalidArgument("run already exists".into()).into());
        }
        self.events.lock().unwrap().push(commit.event);
        self.work_items.lock().unwrap().extend(commit.work_items);
        let snapshot = RunSnapshot {
            run: commit.run.clone(),
            steps: commit.new_steps.clone(),
            checkpoint: commit.checkpoint.clone(),
        };
        runs.insert(snapshot.run.id, snapshot.clone());
        Ok(snapshot)
    }

    async fn commit(
        &self,
        _context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let mut runs = self.runs.lock().unwrap();
        let existing = runs.get(&commit.run.id).cloned().ok_or_else(|| {
            ApplicationError::Domain(DomainError::NotFound("run not found".into()))
        })?;
        if existing.run.version
            != commit
                .run
                .version
                .previous()
                .unwrap_or(existing.run.version)
        {
            return Err(DomainError::RevisionConflict {
                expected: commit.run.version.value().saturating_sub(1),
                current: existing.run.version.value(),
            }
            .into());
        }
        self.events.lock().unwrap().push(commit.event);
        self.work_items.lock().unwrap().extend(commit.work_items);
        let mut snapshot = existing;
        snapshot.run = commit.run.clone();
        for step in &commit.new_steps {
            if let Some(idx) = snapshot.steps.iter().position(|s| s.id == step.id) {
                snapshot.steps[idx] = step.clone();
            } else {
                snapshot.steps.push(step.clone());
            }
        }
        snapshot.checkpoint = commit.checkpoint.clone();
        runs.insert(snapshot.run.id, snapshot.clone());
        Ok(snapshot)
    }
}

#[async_trait]
impl WorkQueuePort for InMemoryStore {
    async fn lease_next(
        &self,
        _context: &RequestContext,
        _request: LeaseWorkRequest,
    ) -> Result<Option<WorkItem>, ApplicationError> {
        let mut items = self.work_items.lock().unwrap();
        Ok(items.first().cloned().inspect(|_| {
            items.remove(0);
        }))
    }

    async fn complete(
        &self,
        _context: &RequestContext,
        item: &WorkItem,
        _at: Timestamp,
    ) -> Result<(), ApplicationError> {
        self.work_items.lock().unwrap().retain(|w| w.id != item.id);
        Ok(())
    }

    async fn retry(
        &self,
        _context: &RequestContext,
        item: &WorkItem,
        available_at: Timestamp,
        _error: RunFailure,
    ) -> Result<(), ApplicationError> {
        let mut items = self.work_items.lock().unwrap();
        if let Some(w) = items.iter_mut().find(|w| w.id == item.id) {
            w.available_at = available_at;
            w.attempt += 1;
        }
        Ok(())
    }

    async fn cancel_for_run(
        &self,
        _context: &RequestContext,
        run_id: AgentRunId,
        _at: Timestamp,
    ) -> Result<u64, ApplicationError> {
        let mut items = self.work_items.lock().unwrap();
        let before = items.len();
        items.retain(|w| w.run_id != run_id);
        Ok((before - items.len()) as u64)
    }

    async fn dead_letter(
        &self,
        _context: &RequestContext,
        _item: &WorkItem,
        _error: RunFailure,
        _at: Timestamp,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

#[async_trait]
impl RunLeasePort for InMemoryStore {
    async fn acquire(
        &self,
        _context: &RequestContext,
        request: AcquireRunLease,
    ) -> Result<RunLease, ApplicationError> {
        Ok(RunLease {
            run_id: request.run_id,
            worker_id: request.worker_id,
            generation: 1,
            acquired_at: request.now,
            heartbeat_at: request.now,
            lease_until: request.lease_until,
        })
    }

    async fn heartbeat(
        &self,
        _context: &RequestContext,
        lease: &RunLease,
        extend_until: Timestamp,
    ) -> Result<RunLease, ApplicationError> {
        Ok(RunLease {
            lease_until: extend_until,
            ..lease.clone()
        })
    }

    async fn release(
        &self,
        _context: &RequestContext,
        _lease: &RunLease,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

fn context() -> RequestContext {
    RequestContext::new(
        vestrace_domain::id::WorkspaceId::new(),
        vestrace_domain::id::PrincipalId::new(),
    )
}

fn now() -> Timestamp {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2026, 8, 8, 12, 0, 0)
        .single()
        .unwrap()
}

fn make_coordinator() -> (
    RunCoordinator<InMemoryStore, FakeClock, InMemoryStore, InMemoryStore>,
    InMemoryStore,
) {
    let store = InMemoryStore::new();
    let clock = FakeClock::new(now());
    let coord = RunCoordinator::new(store.clone(), clock, store.clone(), store.clone());
    (coord, store)
}

#[tokio::test]
async fn create_run_succeeds() {
    let (coord, store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Summarize changes".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(snapshot.run.status, RunStatus::Created);
    assert_eq!(snapshot.run.version, RunVersion::INITIAL);
    assert_eq!(snapshot.run.objective, "Summarize changes");

    let events = store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0].payload.is_creation());

    let items = store.work_items.lock().unwrap();
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].kind, WorkItemKind::AdvanceRun));
}

#[tokio::test]
async fn transition_run_from_created_to_preparing() {
    let (coord, _store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Test".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    let result = coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: snapshot.run.version,
                target: RunStatus::Preparing,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-2".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(result.run.status, RunStatus::Preparing);
    assert_eq!(result.run.version, RunVersion::new(2).unwrap());
}

#[tokio::test]
async fn transition_run_version_conflict_rejected() {
    let (coord, _store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Test".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    let wrong_version = RunVersion::new(5).unwrap();
    let result = coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: wrong_version,
                target: RunStatus::Preparing,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-2".into(),
            },
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn add_steps_to_run() {
    let (coord, _store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Test".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    let step_id = RunStepId::new();
    let result = coord
        .add_steps(
            &ctx,
            AddRunSteps {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: snapshot.run.version,
                steps: vec![NewRunStepDto {
                    id: step_id,
                    plan_step_reference: Some("step-1".into()),
                    assigned_actor: RunActorRef::System,
                    input_references: vec![],
                }],
                actor: RunActorRef::System,
                idempotency_key: "key-2".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(result.steps.len(), 1);
    assert_eq!(result.steps[0].id, step_id);
    assert_eq!(result.run.version, RunVersion::new(2).unwrap());
}

#[tokio::test]
async fn transition_step_succeeds() {
    let (coord, _store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Test".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    let step_id = RunStepId::new();
    coord
        .add_steps(
            &ctx,
            AddRunSteps {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: snapshot.run.version,
                steps: vec![NewRunStepDto {
                    id: step_id,
                    plan_step_reference: Some("step-1".into()),
                    assigned_actor: RunActorRef::System,
                    input_references: vec![],
                }],
                actor: RunActorRef::System,
                idempotency_key: "key-2".into(),
            },
        )
        .await
        .unwrap();

    let result = coord
        .transition_step(
            &ctx,
            TransitionRunStep {
                run_id: snapshot.run.id,
                step_id,
                expected_version: RunVersion::new(2).unwrap(),
                target: RunStepStatus::Ready,
                failure: None,
                actor: RunActorRef::System,
                idempotency_key: "key-3".into(),
            },
        )
        .await
        .unwrap();

    let step = result.steps.iter().find(|s| s.id == step_id).unwrap();
    assert_eq!(step.status, RunStepStatus::Ready);
}

#[tokio::test]
async fn create_checkpoint_succeeds() {
    let (coord, _store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Test".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    let result = coord
        .create_checkpoint(
            &ctx,
            CreateCheckpoint {
                run_id: snapshot.run.id,
                expected_version: snapshot.run.version,
                payload: RunCheckpointPayloadV1 {
                    completed_steps: vec![],
                    ready_steps: vec![],
                    pending_work_items: vec![],
                    pending_tool_calls: vec![],
                    pending_approvals: vec![],
                    active_subruns: vec![],
                    context_references: vec![],
                    artifact_references: vec![],
                    budget_snapshot_id: None,
                },
                actor: RunActorRef::System,
                idempotency_key: "key-2".into(),
            },
        )
        .await
        .unwrap();

    assert!(result.checkpoint.is_some());
    assert_eq!(result.run.version, RunVersion::new(2).unwrap());
    assert!(result.run.checkpoint_id.is_some());
}

#[tokio::test]
async fn pause_and_resume_run() {
    let (coord, _store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Test".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: snapshot.run.version,
                target: RunStatus::Preparing,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-2".into(),
            },
        )
        .await
        .unwrap();

    let running = coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: RunVersion::new(2).unwrap(),
                target: RunStatus::Running,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-3".into(),
            },
        )
        .await
        .unwrap();

    let paused = coord
        .pause_run(
            &ctx,
            PauseRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: running.run.version,
                actor: RunActorRef::System,
                idempotency_key: "key-4".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(paused.run.status, RunStatus::Paused);

    let resumed = coord
        .resume_run(
            &ctx,
            ResumeRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: paused.run.version,
                actor: RunActorRef::System,
                idempotency_key: "key-5".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(resumed.run.status, RunStatus::Running);
}

#[tokio::test]
async fn cancel_run_cancels_work_items() {
    let (coord, store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Test".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: snapshot.run.version,
                target: RunStatus::Preparing,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-2".into(),
            },
        )
        .await
        .unwrap();

    let running = coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: RunVersion::new(2).unwrap(),
                target: RunStatus::Running,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-3".into(),
            },
        )
        .await
        .unwrap();

    let items_before = store.work_items.lock().unwrap().len();

    let cancelled = coord
        .cancel_run(
            &ctx,
            CancelRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: running.run.version,
                reason: Some("user requested".into()),
                actor: RunActorRef::System,
                idempotency_key: "key-4".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(cancelled.run.status, RunStatus::Cancelled);
    let items_after = store.work_items.lock().unwrap().len();
    assert!(items_after < items_before);
}

#[tokio::test]
async fn full_lifecycle_create_to_succeeded() {
    let (coord, _store) = make_coordinator();
    let ctx = context();

    let snapshot = coord
        .create_run(
            &ctx,
            CreateRun {
                correlation_id: None,
                objective: "Summarize PR".into(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                idempotency_key: "key-1".into(),
            },
        )
        .await
        .unwrap();

    coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: snapshot.run.version,
                target: RunStatus::Preparing,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-2".into(),
            },
        )
        .await
        .unwrap();

    coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: RunVersion::new(2).unwrap(),
                target: RunStatus::Running,
                result: None,
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-3".into(),
            },
        )
        .await
        .unwrap();

    let succeeded = coord
        .transition_run(
            &ctx,
            TransitionRun {
                correlation_id: None,
                run_id: snapshot.run.id,
                expected_version: RunVersion::new(3).unwrap(),
                target: RunStatus::Succeeded,
                result: Some(RunTerminalResult::Succeeded {
                    summary: Some("Done".into()),
                }),
                actor: RunActorRef::System,
                causation_event_id: None,
                idempotency_key: "key-4".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(succeeded.run.status, RunStatus::Succeeded);
    assert_eq!(succeeded.run.version, RunVersion::new(4).unwrap());
    assert!(succeeded.run.status.is_terminal());
}
