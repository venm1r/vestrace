use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use vestrace_application::run::ports::{
    AcquireRunLease, CommitRun, LeaseWorkRequest, RunClockPort, RunLease, RunLeasePort,
    RunSnapshot, RunStorePort, WorkItem, WorkItemKind, WorkItemKindDiscriminant, WorkQueuePort,
};
use vestrace_application::run::{
    RunWorkHandler, RunWorkHandlerRegistry, RunWorkOutcome, RunWorker, RunWorkerConfig,
};
use vestrace_application::{
    ApplicationError, DenyAllPolicyEngine, PolicyDecisionEngine, RequestContext,
};
use vestrace_domain::id::{
    AgentRunId, AgentRuntimeSnapshotId, CapabilityGrantId, PolicyDecisionId, PrincipalId,
    WorkItemId, WorkerId, WorkspaceId,
};
use vestrace_domain::run::{RunExecutionMode, RunFailure, RunStatus, RunVersion};
use vestrace_domain::time::Timestamp;
use vestrace_domain::{
    CapabilityGrant, CapabilityGrantSpec, PolicyDecision, evaluate_capability_grants,
};

struct MockClock {
    now: Mutex<Timestamp>,
}

impl MockClock {
    fn new(ts: Timestamp) -> Self {
        Self {
            now: Mutex::new(ts),
        }
    }

    fn advance(&self, dur: chrono::Duration) {
        let mut now = self.now.lock().unwrap();
        *now = *now + dur;
    }
}

impl RunClockPort for MockClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().unwrap()
    }
}

struct MockRunStore {
    snapshot: Mutex<Option<RunSnapshot>>,
    create_commit: Mutex<Option<CommitRun>>,
    commit_commit: Mutex<Option<CommitRun>>,
}

impl MockRunStore {
    fn new(snapshot: RunSnapshot) -> Self {
        Self {
            snapshot: Mutex::new(Some(snapshot)),
            create_commit: Mutex::new(None),
            commit_commit: Mutex::new(None),
        }
    }

    fn set_snapshot(&self, snapshot: RunSnapshot) {
        *self.snapshot.lock().unwrap() = Some(snapshot);
    }
}

#[async_trait]
impl RunStorePort for MockRunStore {
    async fn load(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError> {
        Ok(self.snapshot.lock().unwrap().clone())
    }

    async fn create(
        &self,
        _context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        *self.create_commit.lock().unwrap() = Some(commit.clone());
        self.snapshot
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| ApplicationError::Internal("snapshot not set".into()))
    }

    async fn commit(
        &self,
        _context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        *self.commit_commit.lock().unwrap() = Some(commit.clone());
        let snapshot = self
            .snapshot
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| ApplicationError::Internal("snapshot not set".into()))?;
        Ok(RunSnapshot {
            run: commit.run,
            steps: snapshot.steps,
            checkpoint: snapshot.checkpoint,
        })
    }
}

struct MockLeasePort {
    acquire_lease: Mutex<Option<RunLease>>,
    acquire_error: Mutex<Option<ApplicationError>>,
    acquire_count: AtomicU32,
    release_count: AtomicU32,
}

impl MockLeasePort {
    fn new() -> Self {
        Self {
            acquire_lease: Mutex::new(None),
            acquire_error: Mutex::new(None),
            acquire_count: AtomicU32::new(0),
            release_count: AtomicU32::new(0),
        }
    }

    fn set_acquire_ok(&self, lease: RunLease) {
        *self.acquire_lease.lock().unwrap() = Some(lease);
    }

    fn set_acquire_err(&self, error: ApplicationError) {
        *self.acquire_error.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl RunLeasePort for MockLeasePort {
    async fn acquire(
        &self,
        _context: &RequestContext,
        _request: AcquireRunLease,
    ) -> Result<RunLease, ApplicationError> {
        self.acquire_count.fetch_add(1, Ordering::SeqCst);
        if let Some(err) = self.acquire_error.lock().unwrap().take() {
            return Err(err);
        }
        self.acquire_lease
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| ApplicationError::Internal("acquire result not set".into()))
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
        self.release_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct MockWorkQueue {
    leased_item: Mutex<Option<WorkItem>>,
    completed: Mutex<Vec<WorkItemId>>,
    retried: Mutex<Vec<(WorkItemId, Timestamp, RunFailure)>>,
    dead_lettered: Mutex<Vec<(WorkItemId, RunFailure)>>,
    cancelled: Mutex<Vec<AgentRunId>>,
}

impl MockWorkQueue {
    fn new() -> Self {
        Self {
            leased_item: Mutex::new(None),
            completed: Mutex::new(Vec::new()),
            retried: Mutex::new(Vec::new()),
            dead_lettered: Mutex::new(Vec::new()),
            cancelled: Mutex::new(Vec::new()),
        }
    }

    fn set_leased_item(&self, item: WorkItem) {
        *self.leased_item.lock().unwrap() = Some(item);
    }

    fn was_completed(&self, id: WorkItemId) -> bool {
        self.completed.lock().unwrap().contains(&id)
    }

    fn was_retried(&self, id: WorkItemId) -> bool {
        self.retried
            .lock()
            .unwrap()
            .iter()
            .any(|(i, _, _)| *i == id)
    }

    fn was_dead_lettered(&self, id: WorkItemId) -> bool {
        self.dead_lettered
            .lock()
            .unwrap()
            .iter()
            .any(|(i, _)| *i == id)
    }

    fn was_cancelled(&self, run_id: AgentRunId) -> bool {
        self.cancelled.lock().unwrap().contains(&run_id)
    }
}

#[async_trait]
impl WorkQueuePort for MockWorkQueue {
    async fn lease_next(
        &self,
        _context: &RequestContext,
        _request: LeaseWorkRequest,
    ) -> Result<Option<WorkItem>, ApplicationError> {
        let item = self.leased_item.lock().unwrap().clone();
        if item.is_some() {
            *self.leased_item.lock().unwrap() = None;
        }
        Ok(item)
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
        item: &WorkItem,
        available_at: Timestamp,
        error: RunFailure,
    ) -> Result<(), ApplicationError> {
        self.retried
            .lock()
            .unwrap()
            .push((item.id, available_at, error));
        Ok(())
    }

    async fn dead_letter(
        &self,
        _context: &RequestContext,
        item: &WorkItem,
        error: RunFailure,
        _at: Timestamp,
    ) -> Result<(), ApplicationError> {
        self.dead_lettered.lock().unwrap().push((item.id, error));
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

struct MockHandler {
    kind: WorkItemKindDiscriminant,
    invoked: AtomicU32,
    outcome: Mutex<RunWorkOutcome>,
}

impl MockHandler {
    fn new(kind: WorkItemKindDiscriminant, outcome: RunWorkOutcome) -> Self {
        Self {
            kind,
            invoked: AtomicU32::new(0),
            outcome: Mutex::new(outcome),
        }
    }

    fn invoke_count(&self) -> u32 {
        self.invoked.load(Ordering::SeqCst)
    }

    fn set_outcome(&self, outcome: RunWorkOutcome) {
        *self.outcome.lock().unwrap() = outcome;
    }
}

#[async_trait]
impl RunWorkHandler for MockHandler {
    fn kind(&self) -> WorkItemKindDiscriminant {
        self.kind
    }

    async fn handle(
        &self,
        _context: &RequestContext,
        _snapshot: &RunSnapshot,
        _item: &WorkItem,
        _lease: &RunLease,
    ) -> Result<RunWorkOutcome, ApplicationError> {
        self.invoked.fetch_add(1, Ordering::SeqCst);
        Ok(self.outcome.lock().unwrap().clone())
    }
}

fn make_test_run(status: RunStatus, version: RunVersion) -> RunSnapshot {
    let run_id = AgentRunId::new();
    let workspace_id = WorkspaceId::new();
    let now = chrono::Utc::now();

    let run = vestrace_domain::run::AgentRun {
        id: run_id,
        workspace_id,
        objective: "test objective".into(),
        coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
        active_plan_revision_id: None,
        execution_mode: RunExecutionMode::Autopilot,
        status,
        current_step_id: None,
        checkpoint_id: None,
        parent: None,
        root_run_id: run_id,
        budget_snapshot_id: None,
        resource_usage_snapshot_id: None,
        version,
        result: None,
        created_at: now,
        updated_at: now,
        finished_at: None,
    };

    RunSnapshot {
        run,
        steps: Vec::new(),
        checkpoint: None,
    }
}

fn make_test_work_item(run_id: AgentRunId, version: RunVersion) -> WorkItem {
    WorkItem {
        id: WorkItemId::new(),
        run_id,
        kind: WorkItemKind::AdvanceRun,
        expected_run_version: version,
        available_at: chrono::Utc::now(),
        idempotency_key: "test-key".into(),
        attempt: 1,
    }
}

fn make_test_context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn make_worker(
    store: Arc<MockRunStore>,
    lease_port: Arc<MockLeasePort>,
    queue_port: Arc<MockWorkQueue>,
    clock: Arc<MockClock>,
    registry: RunWorkHandlerRegistry,
) -> RunWorker {
    RunWorker::new_with_policy(
        RunWorkerConfig {
            worker_id: WorkerId::new(),
            poll_interval: Duration::from_millis(10),
            lease_ttl: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(10),
            max_concurrency: 1,
        },
        store,
        lease_port,
        queue_port,
        clock,
        registry,
        Arc::new(TestAllowPolicy),
    )
    .unwrap()
}

struct TestAllowPolicy;

#[async_trait]
impl PolicyDecisionEngine for TestAllowPolicy {
    async fn decide(
        &self,
        context: &RequestContext,
        request: vestrace_domain::AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        let at = vestrace_domain::now();
        let grant = CapabilityGrant::issue(
            CapabilityGrantSpec {
                id: CapabilityGrantId::new(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                issuer_id: context.principal_id,
                capability: request.capability.clone(),
                operation: request.operation.clone(),
                resource_scope: request.resource_scope.clone(),
                valid_from: at - chrono::Duration::minutes(1),
                valid_until: None,
                budget: None,
                risk_ceiling: vestrace_domain::RiskCategory::Critical,
                conditions: request.conditions.clone(),
            },
            at,
        )?;

        Ok(evaluate_capability_grants(
            PolicyDecisionId::new(),
            context.workspace_id,
            context.principal_id,
            "test-allow-v1",
            &request,
            &[grant],
            at,
        )?)
    }
}

fn make_denying_worker(
    store: Arc<MockRunStore>,
    lease_port: Arc<MockLeasePort>,
    queue_port: Arc<MockWorkQueue>,
    clock: Arc<MockClock>,
    registry: RunWorkHandlerRegistry,
) -> RunWorker {
    RunWorker::new_with_policy(
        RunWorkerConfig {
            worker_id: WorkerId::new(),
            poll_interval: Duration::from_millis(10),
            lease_ttl: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(10),
            max_concurrency: 1,
        },
        store,
        lease_port,
        queue_port,
        clock,
        registry,
        Arc::new(DenyAllPolicyEngine),
    )
    .unwrap()
}

fn make_lease(run_id: AgentRunId) -> RunLease {
    RunLease {
        run_id,
        worker_id: WorkerId::new(),
        generation: 1,
        acquired_at: chrono::Utc::now(),
        heartbeat_at: chrono::Utc::now(),
        lease_until: chrono::Utc::now() + chrono::Duration::seconds(30),
    }
}

#[tokio::test]
async fn handler_runs_only_after_leases_acquired() {
    let snapshot = make_test_run(RunStatus::Running, RunVersion::INITIAL);
    let run_id = snapshot.run.id;
    let store = Arc::new(MockRunStore::new(snapshot));
    let lease_port = Arc::new(MockLeasePort::new());
    let queue_port = Arc::new(MockWorkQueue::new());
    let clock = Arc::new(MockClock::new(chrono::Utc::now()));

    let handler = Arc::new(MockHandler::new(
        WorkItemKindDiscriminant::AdvanceRun,
        RunWorkOutcome::Completed,
    ));
    let handler_ref = handler.clone();
    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(handler);

    lease_port.set_acquire_ok(make_lease(run_id));
    queue_port.set_leased_item(make_test_work_item(run_id, RunVersion::INITIAL));

    let worker = make_worker(store, lease_port.clone(), queue_port, clock, registry);
    let context = make_test_context();

    worker.run_once(&context).await.unwrap();

    assert_eq!(handler_ref.invoke_count(), 1);
    assert_eq!(lease_port.release_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn denied_work_item_is_dead_lettered_before_handler() {
    let snapshot = make_test_run(RunStatus::Running, RunVersion::INITIAL);
    let run_id = snapshot.run.id;
    let store = Arc::new(MockRunStore::new(snapshot));
    let lease_port = Arc::new(MockLeasePort::new());
    let queue_port = Arc::new(MockWorkQueue::new());
    let clock = Arc::new(MockClock::new(chrono::Utc::now()));

    let handler = Arc::new(MockHandler::new(
        WorkItemKindDiscriminant::AdvanceRun,
        RunWorkOutcome::Completed,
    ));
    let handler_ref = handler.clone();
    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(handler);

    let item = make_test_work_item(run_id, RunVersion::INITIAL);
    let item_id = item.id;
    queue_port.set_leased_item(item);

    let worker = make_denying_worker(
        store,
        lease_port.clone(),
        queue_port.clone(),
        clock,
        registry,
    );
    worker.run_once(&make_test_context()).await.unwrap();

    assert_eq!(handler_ref.invoke_count(), 0);
    assert_eq!(lease_port.acquire_count.load(Ordering::SeqCst), 0);
    assert!(queue_port.was_dead_lettered(item_id));
}

#[tokio::test]
async fn stale_expected_version_skips_handler() {
    let snapshot = make_test_run(RunStatus::Running, RunVersion::new(2).unwrap());
    let run_id = snapshot.run.id;
    let store = Arc::new(MockRunStore::new(snapshot));
    let lease_port = Arc::new(MockLeasePort::new());
    let queue_port = Arc::new(MockWorkQueue::new());
    let clock = Arc::new(MockClock::new(chrono::Utc::now()));

    let handler = Arc::new(MockHandler::new(
        WorkItemKindDiscriminant::AdvanceRun,
        RunWorkOutcome::Completed,
    ));
    let handler_ref = handler.clone();
    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(handler);

    let item = make_test_work_item(run_id, RunVersion::INITIAL);
    let item_id = item.id;
    queue_port.set_leased_item(item);

    let worker = make_worker(store, lease_port, queue_port.clone(), clock, registry);
    let context = make_test_context();

    worker.run_once(&context).await.unwrap();

    assert_eq!(handler_ref.invoke_count(), 0);
    assert!(queue_port.was_completed(item_id));
}

#[tokio::test]
async fn paused_run_cancels_work_without_handler() {
    let snapshot = make_test_run(RunStatus::Paused, RunVersion::INITIAL);
    let run_id = snapshot.run.id;
    let store = Arc::new(MockRunStore::new(snapshot));
    let lease_port = Arc::new(MockLeasePort::new());
    let queue_port = Arc::new(MockWorkQueue::new());
    let clock = Arc::new(MockClock::new(chrono::Utc::now()));

    let handler = Arc::new(MockHandler::new(
        WorkItemKindDiscriminant::AdvanceRun,
        RunWorkOutcome::Completed,
    ));
    let handler_ref = handler.clone();
    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(handler);

    queue_port.set_leased_item(make_test_work_item(run_id, RunVersion::INITIAL));

    let worker = make_worker(store, lease_port, queue_port.clone(), clock, registry);
    let context = make_test_context();

    worker.run_once(&context).await.unwrap();

    assert_eq!(handler_ref.invoke_count(), 0);
    assert!(queue_port.was_cancelled(run_id));
}

#[tokio::test]
async fn terminal_run_cancels_work_without_handler() {
    let snapshot = make_test_run(RunStatus::Succeeded, RunVersion::INITIAL);
    let run_id = snapshot.run.id;
    let store = Arc::new(MockRunStore::new(snapshot));
    let lease_port = Arc::new(MockLeasePort::new());
    let queue_port = Arc::new(MockWorkQueue::new());
    let clock = Arc::new(MockClock::new(chrono::Utc::now()));

    let handler = Arc::new(MockHandler::new(
        WorkItemKindDiscriminant::AdvanceRun,
        RunWorkOutcome::Completed,
    ));
    let handler_ref = handler.clone();
    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(handler);

    queue_port.set_leased_item(make_test_work_item(run_id, RunVersion::INITIAL));

    let worker = make_worker(store, lease_port, queue_port.clone(), clock, registry);
    let context = make_test_context();

    worker.run_once(&context).await.unwrap();

    assert_eq!(handler_ref.invoke_count(), 0);
    assert!(queue_port.was_cancelled(run_id));
}

#[tokio::test]
async fn retryable_error_schedules_retry() {
    let snapshot = make_test_run(RunStatus::Running, RunVersion::INITIAL);
    let run_id = snapshot.run.id;
    let store = Arc::new(MockRunStore::new(snapshot));
    let lease_port = Arc::new(MockLeasePort::new());
    let queue_port = Arc::new(MockWorkQueue::new());
    let clock = Arc::new(MockClock::new(chrono::Utc::now()));

    let handler = Arc::new(MockHandler::new(
        WorkItemKindDiscriminant::AdvanceRun,
        RunWorkOutcome::Retry {
            available_at: chrono::Utc::now() + chrono::Duration::seconds(1),
            error: RunFailure {
                code: "transient".into(),
                message: "temporary failure".into(),
                retryable: true,
            },
        },
    ));
    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(handler);

    lease_port.set_acquire_ok(make_lease(run_id));
    let item = make_test_work_item(run_id, RunVersion::INITIAL);
    let item_id = item.id;
    queue_port.set_leased_item(item);

    let worker = make_worker(store, lease_port, queue_port.clone(), clock, registry);
    let context = make_test_context();

    worker.run_once(&context).await.unwrap();

    assert!(queue_port.was_retried(item_id));
}

#[tokio::test]
async fn non_retryable_error_dead_letters() {
    let snapshot = make_test_run(RunStatus::Running, RunVersion::INITIAL);
    let run_id = snapshot.run.id;
    let store = Arc::new(MockRunStore::new(snapshot));
    let lease_port = Arc::new(MockLeasePort::new());
    let queue_port = Arc::new(MockWorkQueue::new());
    let clock = Arc::new(MockClock::new(chrono::Utc::now()));

    let handler = Arc::new(MockHandler::new(
        WorkItemKindDiscriminant::AdvanceRun,
        RunWorkOutcome::DeadLetter {
            error: RunFailure {
                code: "permanent".into(),
                message: "unrecoverable failure".into(),
                retryable: false,
            },
        },
    ));
    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(handler);

    lease_port.set_acquire_ok(make_lease(run_id));
    let item = make_test_work_item(run_id, RunVersion::INITIAL);
    let item_id = item.id;
    queue_port.set_leased_item(item);

    let worker = make_worker(store, lease_port, queue_port.clone(), clock, registry);
    let context = make_test_context();

    worker.run_once(&context).await.unwrap();

    assert!(queue_port.was_dead_lettered(item_id));
}

#[tokio::test]
async fn max_concurrency_not_1_returns_config_error() {
    let result = RunWorker::new(
        RunWorkerConfig {
            worker_id: WorkerId::new(),
            poll_interval: Duration::from_millis(10),
            lease_ttl: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(10),
            max_concurrency: 2,
        },
        Arc::new(MockRunStore::new(make_test_run(
            RunStatus::Created,
            RunVersion::INITIAL,
        ))),
        Arc::new(MockLeasePort::new()),
        Arc::new(MockWorkQueue::new()),
        Arc::new(MockClock::new(chrono::Utc::now())),
        RunWorkHandlerRegistry::new(),
    );

    assert!(result.is_err());
}
