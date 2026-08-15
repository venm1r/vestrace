//! `ExecuteStepHandler` had no test coverage at all, which is how it survived
//! marking every step `Running` and then `Succeeded` without invoking anything.
//!
//! These tests pin the property that was missing: a step reports success only
//! when the work it names was actually performed.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::run::ports::{
    CommitRun, RunClockPort, RunLease, RunSnapshot, RunStorePort, WorkItem, WorkItemKind,
};
use vestrace_application::run::{
    ExecuteStepHandler, RunWorkHandler, StepModelExecutor, StepModelOutcome, StepModelRequest,
};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::id::{
    AgentRunId, AgentRuntimeSnapshotId, ArtifactId, ModelExecutionId, PrincipalId, RunStepId,
    WorkItemId, WorkerId, WorkspaceId,
};
use vestrace_domain::run::{
    NewRunStep, RunActorRef, RunExecutionMode, RunReferenceKind, RunStatus, RunStep, RunStepStatus,
    RunVersion,
};
use vestrace_domain::time::Timestamp;

struct FixedClock(Timestamp);

impl RunClockPort for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// Captures every commit so a test can inspect what the handler actually wrote.
struct RecordingStore {
    snapshot: Mutex<RunSnapshot>,
    commits: Mutex<Vec<CommitRun>>,
}

impl RecordingStore {
    fn new(snapshot: RunSnapshot) -> Self {
        Self {
            snapshot: Mutex::new(snapshot),
            commits: Mutex::new(Vec::new()),
        }
    }

    fn last_commit(&self) -> CommitRun {
        self.commits
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("the handler committed nothing")
    }
}

#[async_trait]
impl RunStorePort for RecordingStore {
    async fn load(
        &self,
        _context: &RequestContext,
        _run_id: AgentRunId,
    ) -> Result<Option<RunSnapshot>, ApplicationError> {
        Ok(Some(self.snapshot.lock().unwrap().clone()))
    }

    async fn create(
        &self,
        _context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        self.commits.lock().unwrap().push(commit);
        Ok(self.snapshot.lock().unwrap().clone())
    }

    async fn commit(
        &self,
        _context: &RequestContext,
        commit: CommitRun,
    ) -> Result<RunSnapshot, ApplicationError> {
        self.commits.lock().unwrap().push(commit.clone());
        let snapshot = self.snapshot.lock().unwrap().clone();
        Ok(RunSnapshot {
            run: commit.run,
            steps: snapshot.steps,
            checkpoint: snapshot.checkpoint,
        })
    }
}

struct StubModel {
    outcome: Mutex<Option<Result<StepModelOutcome, ApplicationError>>>,
    seen_prompt: Mutex<Option<String>>,
}

impl StubModel {
    fn succeeding() -> Self {
        Self {
            outcome: Mutex::new(Some(Ok(StepModelOutcome {
                artifact_id: ArtifactId::new(),
                content_hash: "b".repeat(64),
                model_execution_id: ModelExecutionId::new(),
                prompt_tokens: 11,
                completion_tokens: 22,
                latency_ms: 33,
            }))),
            seen_prompt: Mutex::new(None),
        }
    }

    fn failing(error: ApplicationError) -> Self {
        Self {
            outcome: Mutex::new(Some(Err(error))),
            seen_prompt: Mutex::new(None),
        }
    }
}

#[async_trait]
impl StepModelExecutor for StubModel {
    async fn execute(
        &self,
        _context: &RequestContext,
        request: StepModelRequest,
    ) -> Result<StepModelOutcome, ApplicationError> {
        *self.seen_prompt.lock().unwrap() = Some(request.objective);
        self.outcome
            .lock()
            .unwrap()
            .take()
            .expect("the executor was invoked more than once")
    }
}

fn timestamp() -> Timestamp {
    vestrace_domain::time::now()
}

fn snapshot_with_step(actor: RunActorRef) -> (RunSnapshot, RunStepId) {
    let at = timestamp();
    let run_id = AgentRunId::new();
    let step = RunStep::create(
        NewRunStep {
            id: RunStepId::new(),
            run_id,
            plan_step_reference: None,
            assigned_actor: actor,
            input_references: vec![],
        },
        at,
    )
    .unwrap();
    let step_id = step.id;

    let run = vestrace_domain::run::AgentRun {
        id: run_id,
        workspace_id: WorkspaceId::new(),
        objective: "summarise the incident report".to_string(),
        coordinator_snapshot_id: AgentRuntimeSnapshotId::new(),
        active_plan_revision_id: None,
        execution_mode: RunExecutionMode::Autopilot,
        status: RunStatus::Running,
        current_step_id: Some(step_id),
        checkpoint_id: None,
        parent: None,
        root_run_id: run_id,
        budget_snapshot_id: None,
        resource_usage_snapshot_id: None,
        // Past creation: the run exists and has had steps added, so a further
        // event is a non-creation event and the domain requires version > 1.
        version: RunVersion::new(2).unwrap(),
        result: None,
        created_at: at,
        updated_at: at,
        finished_at: None,
    };

    (
        RunSnapshot {
            run,
            steps: vec![step],
            checkpoint: None,
        },
        step_id,
    )
}

fn work_item(run_id: AgentRunId, step_id: RunStepId) -> WorkItem {
    WorkItem {
        id: WorkItemId::new(),
        run_id,
        kind: WorkItemKind::ExecuteStep { step_id },
        expected_run_version: RunVersion::new(2).unwrap(),
        available_at: timestamp(),
        idempotency_key: format!("step:{}", step_id.as_uuid()),
        attempt: 1,
    }
}

fn lease(run_id: AgentRunId) -> RunLease {
    RunLease {
        run_id,
        worker_id: WorkerId::new(),
        generation: 1,
        acquired_at: timestamp(),
        heartbeat_at: timestamp(),
        lease_until: timestamp() + chrono::Duration::minutes(5),
    }
}

fn context(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::new(workspace_id, PrincipalId::new())
}

/// The defect this whole change exists to fix.
#[tokio::test]
async fn an_agent_step_without_a_model_executor_fails_instead_of_reporting_success() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())));

    handler
        .handle(
            &context(workspace),
            &store
                .load(&context(workspace), run_id)
                .await
                .unwrap()
                .unwrap(),
            &work_item(run_id, step_id),
            &lease(run_id),
        )
        .await
        .unwrap();

    let committed = store.last_commit();
    let step = &committed.new_steps[0];
    assert_eq!(
        step.status,
        RunStepStatus::Failed,
        "a step that was supposed to invoke a model and did not must not report success"
    );
    let failure = step.error.as_ref().expect("a failed step must say why");
    assert_eq!(failure.code, "model_executor_unconfigured");
    // Retrying will not create configuration.
    assert!(!failure.retryable);
}

#[tokio::test]
async fn an_agent_step_invokes_the_model_with_the_run_objective() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let model = Arc::new(StubModel::succeeding());
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())))
        .with_model_executor(model.clone());

    handler
        .handle(
            &context(workspace),
            &store
                .load(&context(workspace), run_id)
                .await
                .unwrap()
                .unwrap(),
            &work_item(run_id, step_id),
            &lease(run_id),
        )
        .await
        .unwrap();

    assert_eq!(
        model.seen_prompt.lock().unwrap().as_deref(),
        Some("summarise the incident report")
    );
}

#[tokio::test]
async fn a_completed_agent_step_references_its_output_and_its_cost() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())))
        .with_model_executor(Arc::new(StubModel::succeeding()));

    handler
        .handle(
            &context(workspace),
            &store
                .load(&context(workspace), run_id)
                .await
                .unwrap()
                .unwrap(),
            &work_item(run_id, step_id),
            &lease(run_id),
        )
        .await
        .unwrap();

    let committed = store.last_commit();
    let step = &committed.new_steps[0];
    assert_eq!(step.status, RunStepStatus::Succeeded);

    let kinds: Vec<_> = step
        .output_references
        .iter()
        .map(|reference| reference.kind)
        .collect();
    // Two references answering different questions: what was produced, and
    // what it cost.
    assert!(kinds.contains(&RunReferenceKind::Artifact));
    assert!(kinds.contains(&RunReferenceKind::ModelInvocation));
}

#[tokio::test]
async fn a_provider_outage_fails_the_step_as_retryable() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())))
        .with_model_executor(Arc::new(StubModel::failing(ApplicationError::Unavailable(
            "provider is rate limiting".into(),
        ))));

    handler
        .handle(
            &context(workspace),
            &store
                .load(&context(workspace), run_id)
                .await
                .unwrap()
                .unwrap(),
            &work_item(run_id, step_id),
            &lease(run_id),
        )
        .await
        .unwrap();

    let committed = store.last_commit();
    let failure = committed.new_steps[0].error.as_ref().unwrap();
    assert_eq!(failure.code, "model_invocation_failed");
    assert!(
        failure.retryable,
        "an outage may clear, so the step must be retryable"
    );
}

#[tokio::test]
async fn an_unusable_provider_response_fails_the_step_as_not_retryable() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())))
        .with_model_executor(Arc::new(StubModel::failing(ApplicationError::Internal(
            "model provider returned an unusable response".into(),
        ))));

    handler
        .handle(
            &context(workspace),
            &store
                .load(&context(workspace), run_id)
                .await
                .unwrap()
                .unwrap(),
            &work_item(run_id, step_id),
            &lease(run_id),
        )
        .await
        .unwrap();

    let failure = store.last_commit().new_steps[0]
        .error
        .clone()
        .expect("a failed step must say why");
    assert!(
        !failure.retryable,
        "retrying will produce the same unreadable reply"
    );
}

/// Steps assigned to a principal, a worker or the system were never meant to
/// call a model, so the new failure path must not touch them.
#[tokio::test]
async fn a_non_agent_step_still_completes_without_a_model_executor() {
    for actor in [
        RunActorRef::System,
        RunActorRef::Principal(PrincipalId::new()),
        RunActorRef::Worker(WorkerId::new()),
    ] {
        let (snapshot, step_id) = snapshot_with_step(actor.clone());
        let run_id = snapshot.run.id;
        let workspace = snapshot.run.workspace_id;
        let store = Arc::new(RecordingStore::new(snapshot));
        let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())));

        handler
            .handle(
                &context(workspace),
                &store
                    .load(&context(workspace), run_id)
                    .await
                    .unwrap()
                    .unwrap(),
                &work_item(run_id, step_id),
                &lease(run_id),
            )
            .await
            .unwrap();

        let step = store.last_commit().new_steps[0].clone();
        assert_eq!(step.status, RunStepStatus::Succeeded, "actor {actor:?}");
        assert!(step.output_references.is_empty());
    }
}

/// A failed step must still schedule the run to advance, or the run stalls in
/// `Running` with nothing queued to move it.
#[tokio::test]
async fn a_failed_step_still_schedules_the_run_to_advance() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())));

    handler
        .handle(
            &context(workspace),
            &store
                .load(&context(workspace), run_id)
                .await
                .unwrap()
                .unwrap(),
            &work_item(run_id, step_id),
            &lease(run_id),
        )
        .await
        .unwrap();

    let committed = store.last_commit();
    assert!(
        committed
            .work_items
            .iter()
            .any(|item| matches!(item.kind, WorkItemKind::AdvanceRun)),
        "a failed step must not leave the run with nothing scheduled"
    );
}
