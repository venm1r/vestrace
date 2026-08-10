use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Duration;
use vestrace_application::run::{
    AdvanceRunHandler, ExecuteStepHandler, ResumeRunHandler, RunClockPort, RunLease, RunSnapshot,
    RunStorePort, RunWorkHandler, RunWorkOutcome, WorkItem, WorkItemKind, WorkQueuePort,
};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::{
    id::{AgentRunId, PrincipalId, RunStepId, WorkItemId, WorkerId, WorkspaceId},
    now,
    run::{
        AgentRun, NewAgentRun, NewRunStep, RunActorRef, RunExecutionMode, RunFailure, RunStatus,
        RunStep, RunStepStatus, RunVersion,
    },
    time::Timestamp,
};

struct MockClock {
    now: Mutex<Timestamp>,
}

impl MockClock {
    fn new() -> Self {
        Self {
            now: Mutex::new(now()),
        }
    }
}

impl RunClockPort for MockClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().unwrap()
    }
}

struct MockStore {
    snapshots: Mutex<Vec<RunSnapshot>>,
    commits: Mutex<Vec<CommitRecord>>,
}

struct CommitRecord {
    run: AgentRun,
    new_steps: Vec<RunStep>,
    work_items: Vec<WorkItem>,
}

impl MockStore {
    fn new() -> Self {
        Self {
            snapshots: Mutex::new(vec![]),
            commits: Mutex::new(vec![]),
        }
    }

    fn push_snapshot(&self, snapshot: RunSnapshot) {
        self.snapshots.lock().unwrap().push(snapshot);
    }

    fn latest_run(&self) -> AgentRun {
        self.commits
            .lock()
            .unwrap()
            .last()
            .map(|c| c.run.clone())
            .or_else(|| self.snapshots.lock().unwrap().last().map(|s| s.run.clone()))
            .expect("no run")
    }

    fn latest_steps(&self) -> Vec<RunStep> {
        let commits = self.commits.lock().unwrap();
        if let Some(last) = commits.last() {
            if !last.new_steps.is_empty() {
                return last.new_steps.clone();
            }
        }
        self.snapshots
            .lock()
            .unwrap()
            .last()
            .map(|s| s.steps.clone())
            .unwrap_or_default()
    }

    fn work_items_enqueued(&self) -> Vec<WorkItem> {
        self.commits
            .lock()
            .unwrap()
            .last()
            .map(|c| c.work_items.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl RunStorePort for MockStore {
    async fn load(
        &self,
        _context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError> {
        let snapshots = self.snapshots.lock().unwrap();
        let commits = self.commits.lock().unwrap();

        if let Some(last_commit) = commits.last() {
            if last_commit.run.id == run_id {
                let steps = if last_commit.new_steps.is_empty() {
                    snapshots
                        .iter()
                        .find(|s| s.run.id == run_id)
                        .map(|s| s.steps.clone())
                        .unwrap_or_default()
                } else {
                    last_commit.new_steps.clone()
                };
                return Ok(Some(RunSnapshot {
                    run: last_commit.run.clone(),
                    steps,
                    checkpoint: None,
                }));
            }
        }

        Ok(snapshots.iter().find(|s| s.run.id == run_id).cloned())
    }

    async fn create(
        &self,
        _context: &RequestContext,
        commit: vestrace_application::run::CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let snapshot = RunSnapshot {
            run: commit.run.clone(),
            steps: commit.new_steps.clone(),
            checkpoint: commit.checkpoint.clone(),
        };
        self.commits.lock().unwrap().push(CommitRecord {
            run: commit.run,
            new_steps: commit.new_steps,
            work_items: commit.work_items,
        });
        Ok(snapshot)
    }

    async fn commit(
        &self,
        _context: &RequestContext,
        commit: vestrace_application::run::CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        let snapshot = RunSnapshot {
            run: commit.run.clone(),
            steps: if commit.new_steps.is_empty() {
                self.snapshots
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|s| s.run.id == commit.run.id)
                    .map(|s| s.steps.clone())
                    .unwrap_or_default()
            } else {
                commit.new_steps.clone()
            },
            checkpoint: commit.checkpoint.clone(),
        };
        self.commits.lock().unwrap().push(CommitRecord {
            run: commit.run,
            new_steps: commit.new_steps,
            work_items: commit.work_items,
        });
        Ok(snapshot)
    }
}

struct MockQueue {
    completed: Mutex<Vec<WorkItemId>>,
    cancelled: Mutex<Vec<AgentRunId>>,
}

impl MockQueue {
    fn new() -> Self {
        Self {
            completed: Mutex::new(vec![]),
            cancelled: Mutex::new(vec![]),
        }
    }
}

#[async_trait]
impl WorkQueuePort for MockQueue {
    async fn lease_next(
        &self,
        _context: &RequestContext,
        _request: vestrace_application::run::LeaseWorkRequest,
    ) -> Result<Option<WorkItem>, ApplicationError> {
        Ok(None)
    }

    async fn complete(
        &self,
        _context: &RequestContext,
        item: &WorkItem,
        _at: Timestamp,
    ) -> Result<(), ApplicationError> {
        self.completed.lock().unwrap().push(item.id);
        Ok(())
    }

    async fn retry(
        &self,
        _context: &RequestContext,
        _item: &WorkItem,
        _available_at: Timestamp,
        _error: RunFailure,
    ) -> Result<(), ApplicationError> {
        Ok(())
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

    async fn cancel_for_run(
        &self,
        _context: &RequestContext,
        run_id: AgentRunId,
        _at: Timestamp,
    ) -> Result<u64, ApplicationError> {
        self.cancelled.lock().unwrap().push(run_id);
        Ok(1)
    }
}

fn make_run(status: RunStatus, version: RunVersion) -> AgentRun {
    let run_id = AgentRunId::new();
    let ws_id = WorkspaceId::new();
    let at = now();
    let mut run = AgentRun::create(
        NewAgentRun {
            id: run_id,
            workspace_id: ws_id,
            objective: "lifecycle test".into(),
            coordinator_snapshot_id: vestrace_domain::id::AgentRuntimeSnapshotId::new(),
            execution_mode: RunExecutionMode::Autopilot,
            parent: None,
            budget_snapshot_id: None,
            resource_usage_snapshot_id: None,
        },
        at,
    )
    .unwrap();
    run.status = status;
    run.version = version;
    run
}

fn make_step(run_id: AgentRunId, status: RunStepStatus) -> RunStep {
    RunStep::create(
        NewRunStep {
            id: RunStepId::new(),
            run_id,
            plan_step_reference: None,
            assigned_actor: RunActorRef::System,
            input_references: vec![],
        },
        now(),
    )
    .map(|mut s| {
        s.status = status;
        s
    })
    .unwrap()
}

fn make_snapshot(run: AgentRun, steps: Vec<RunStep>) -> RunSnapshot {
    RunSnapshot {
        run,
        steps,
        checkpoint: None,
    }
}

fn make_work_item(run_id: AgentRunId, version: RunVersion, kind: WorkItemKind) -> WorkItem {
    WorkItem {
        id: WorkItemId::new(),
        run_id,
        kind,
        expected_run_version: version,
        available_at: now(),
        idempotency_key: "test".into(),
        attempt: 1,
    }
}

fn ctx() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

#[tokio::test]
async fn advance_run_activates_created_run_and_enqueues_step_execution() {
    let store = Arc::new(MockStore::new());
    let queue = Arc::new(MockQueue::new());
    let clock = Arc::new(MockClock::new());

    let run = make_run(RunStatus::Created, RunVersion::INITIAL);
    let step = make_step(run.id, RunStepStatus::Pending);
    let run_id = run.id;
    let snapshot = make_snapshot(run, vec![step]);
    store.push_snapshot(snapshot);

    let handler = AdvanceRunHandler::new(store.clone(), queue.clone(), clock.clone());
    let context = ctx();
    let lease = RunLease {
        run_id,
        worker_id: WorkerId::new(),
        generation: 1,
        acquired_at: clock.now(),
        heartbeat_at: clock.now(),
        lease_until: clock.now() + Duration::seconds(60),
    };

    // Step 1: Created → Preparing
    let snap = store.load(&context, run_id).await.unwrap().unwrap();
    let item1 = make_work_item(run_id, RunVersion::INITIAL, WorkItemKind::AdvanceRun);
    let outcome1 = handler
        .handle(&context, &snap, &item1, &lease)
        .await
        .unwrap();
    assert_eq!(outcome1, RunWorkOutcome::Completed);

    let run_after_prepare = store.latest_run();
    assert_eq!(run_after_prepare.status, RunStatus::Preparing);
    assert_eq!(run_after_prepare.version.value(), 2);

    let items_after_prepare = store.work_items_enqueued();
    assert!(
        items_after_prepare
            .iter()
            .any(|wi| matches!(wi.kind, WorkItemKind::AdvanceRun))
    );

    // Step 2: Preparing → Running + enqueue ExecuteStep
    let snap2 = store.load(&context, run_id).await.unwrap().unwrap();
    let item2 = make_work_item(run_id, run_after_prepare.version, WorkItemKind::AdvanceRun);
    let outcome2 = handler
        .handle(&context, &snap2, &item2, &lease)
        .await
        .unwrap();
    assert_eq!(outcome2, RunWorkOutcome::Completed);

    let run_after_start = store.latest_run();
    assert_eq!(run_after_start.status, RunStatus::Running);
    assert_eq!(run_after_start.version.value(), 3);

    let work_items = store.work_items_enqueued();
    assert!(!work_items.is_empty());
    let has_execute_step = work_items
        .iter()
        .any(|wi| matches!(wi.kind, WorkItemKind::ExecuteStep { .. }));
    assert!(has_execute_step, "should enqueue ExecuteStep work item");
    let has_advance = work_items
        .iter()
        .any(|wi| matches!(wi.kind, WorkItemKind::AdvanceRun));
    assert!(has_advance, "should enqueue next AdvanceRun work item");
}

#[tokio::test]
async fn advance_run_finalizes_when_all_steps_terminal() {
    let store = Arc::new(MockStore::new());
    let queue = Arc::new(MockQueue::new());
    let clock = Arc::new(MockClock::new());

    let run = make_run(RunStatus::Running, RunVersion::INITIAL);
    let step = make_step(run.id, RunStepStatus::Succeeded);
    let run_id = run.id;
    let snapshot = make_snapshot(run, vec![step]);
    store.push_snapshot(snapshot);

    let handler = AdvanceRunHandler::new(store.clone(), queue.clone(), clock.clone());
    let context = ctx();
    let item = make_work_item(run_id, RunVersion::INITIAL, WorkItemKind::AdvanceRun);
    let lease = RunLease {
        run_id,
        worker_id: WorkerId::new(),
        generation: 1,
        acquired_at: clock.now(),
        heartbeat_at: clock.now(),
        lease_until: clock.now() + Duration::seconds(60),
    };

    let outcome = handler
        .handle(
            &context,
            &store.load(&context, run_id).await.unwrap().unwrap(),
            &item,
            &lease,
        )
        .await
        .unwrap();

    assert_eq!(outcome, RunWorkOutcome::Completed);
    let updated_run = store.latest_run();
    assert_eq!(updated_run.status, RunStatus::Succeeded);
    assert!(updated_run.finished_at.is_some());

    let cancelled = queue.cancelled.lock().unwrap();
    assert!(
        cancelled.contains(&run_id),
        "should cancel remaining work items for finalized run"
    );
}

#[tokio::test]
async fn execute_step_transitions_pending_to_succeeded() {
    let store = Arc::new(MockStore::new());
    let clock = Arc::new(MockClock::new());

    let run = make_run(RunStatus::Running, RunVersion::INITIAL);
    let step = make_step(run.id, RunStepStatus::Pending);
    let step_id = step.id;
    let run_id = run.id;
    let snapshot = make_snapshot(run, vec![step]);
    store.push_snapshot(snapshot);

    let handler = ExecuteStepHandler::new(store.clone(), clock.clone());
    let context = ctx();
    let item = make_work_item(
        run_id,
        RunVersion::INITIAL,
        WorkItemKind::ExecuteStep { step_id },
    );
    let lease = RunLease {
        run_id,
        worker_id: WorkerId::new(),
        generation: 1,
        acquired_at: clock.now(),
        heartbeat_at: clock.now(),
        lease_until: clock.now() + Duration::seconds(60),
    };

    let outcome = handler
        .handle(
            &context,
            &store.load(&context, run_id).await.unwrap().unwrap(),
            &item,
            &lease,
        )
        .await
        .unwrap();

    assert_eq!(outcome, RunWorkOutcome::Completed);

    let steps = store.latest_steps();
    let updated_step = steps.iter().find(|s| s.id == step_id).unwrap();
    assert_eq!(updated_step.status, RunStepStatus::Succeeded);
    assert!(updated_step.finished_at.is_some());

    let work_items = store.work_items_enqueued();
    assert!(
        work_items
            .iter()
            .any(|wi| matches!(wi.kind, WorkItemKind::AdvanceRun)),
        "should enqueue AdvanceRun after step completion"
    );
}

#[tokio::test]
async fn resume_run_transitions_paused_to_running() {
    let store = Arc::new(MockStore::new());
    let clock = Arc::new(MockClock::new());

    let run = make_run(RunStatus::Paused, RunVersion::INITIAL);
    let run_id = run.id;
    let snapshot = make_snapshot(run, vec![]);
    store.push_snapshot(snapshot);

    let handler = ResumeRunHandler::new(store.clone(), clock.clone());
    let context = ctx();
    let item = make_work_item(run_id, RunVersion::INITIAL, WorkItemKind::ResumeRun);
    let lease = RunLease {
        run_id,
        worker_id: WorkerId::new(),
        generation: 1,
        acquired_at: clock.now(),
        heartbeat_at: clock.now(),
        lease_until: clock.now() + Duration::seconds(60),
    };

    let outcome = handler
        .handle(
            &context,
            &store.load(&context, run_id).await.unwrap().unwrap(),
            &item,
            &lease,
        )
        .await
        .unwrap();

    assert_eq!(outcome, RunWorkOutcome::Completed);
    let updated_run = store.latest_run();
    assert_eq!(updated_run.status, RunStatus::Running);

    let work_items = store.work_items_enqueued();
    assert!(
        work_items
            .iter()
            .any(|wi| matches!(wi.kind, WorkItemKind::AdvanceRun)),
        "should enqueue AdvanceRun after resume"
    );
}

#[tokio::test]
async fn full_lifecycle_create_advance_execute_finalize() {
    let store = Arc::new(MockStore::new());
    let queue = Arc::new(MockQueue::new());
    let clock = Arc::new(MockClock::new());

    let run = make_run(RunStatus::Created, RunVersion::INITIAL);
    let step = make_step(run.id, RunStepStatus::Pending);
    let step_id = step.id;
    let run_id = run.id;
    store.push_snapshot(make_snapshot(run, vec![step]));

    let context = ctx();
    let lease = RunLease {
        run_id,
        worker_id: WorkerId::new(),
        generation: 1,
        acquired_at: clock.now(),
        heartbeat_at: clock.now(),
        lease_until: clock.now() + Duration::seconds(60),
    };

    let advance = Arc::new(AdvanceRunHandler::new(
        store.clone(),
        queue.clone(),
        clock.clone(),
    ));
    let execute = Arc::new(ExecuteStepHandler::new(store.clone(), clock.clone()));

    // Step 1: Created → Preparing
    let snap = store.load(&context, run_id).await.unwrap().unwrap();
    let item = make_work_item(run_id, RunVersion::INITIAL, WorkItemKind::AdvanceRun);
    advance
        .handle(&context, &snap, &item, &lease)
        .await
        .unwrap();
    assert_eq!(store.latest_run().status, RunStatus::Preparing);

    // Step 2: Preparing → Running + enqueue ExecuteStep
    let run_after_prepare = store.latest_run();
    let snap2 = store.load(&context, run_id).await.unwrap().unwrap();
    let item2 = make_work_item(run_id, run_after_prepare.version, WorkItemKind::AdvanceRun);
    advance
        .handle(&context, &snap2, &item2, &lease)
        .await
        .unwrap();
    let run_after_start = store.latest_run();
    assert_eq!(run_after_start.status, RunStatus::Running);

    // Step 3: ExecuteStep → Succeeded
    let step_item = make_work_item(
        run_id,
        run_after_start.version,
        WorkItemKind::ExecuteStep { step_id },
    );
    let snap3 = store.load(&context, run_id).await.unwrap().unwrap();
    execute
        .handle(&context, &snap3, &step_item, &lease)
        .await
        .unwrap();

    let steps = store.latest_steps();
    let updated_step = steps.iter().find(|s| s.id == step_id).unwrap();
    assert_eq!(updated_step.status, RunStepStatus::Succeeded);

    // Step 4: AdvanceRun → finalize (all steps terminal → Succeeded)
    let run_after_step = store.latest_run();
    let advance_item2 = make_work_item(run_id, run_after_step.version, WorkItemKind::AdvanceRun);
    let snap4 = store.load(&context, run_id).await.unwrap().unwrap();
    advance
        .handle(&context, &snap4, &advance_item2, &lease)
        .await
        .unwrap();

    let final_run = store.latest_run();
    assert_eq!(final_run.status, RunStatus::Succeeded);
    assert!(final_run.finished_at.is_some());
}
