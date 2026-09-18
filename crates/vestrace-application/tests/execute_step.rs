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
    ExecuteStepHandler, RunWorkHandler, RunWorkOutcome, StepModelExecutor, StepModelOutcome,
    StepModelRequest,
};
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::id::{
    AgentRunId, AgentRuntimeSnapshotId, PolicyDecisionId, PrincipalId, RunStepId, WorkItemId,
    WorkerId, WorkspaceId,
};
use vestrace_domain::run::{
    NewRunStep, RunActorRef, RunExecutionMode, RunStatus, RunStep, RunStepStatus, RunVersion,
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

    fn commits(&self) -> Vec<CommitRun> {
        self.commits.lock().unwrap().clone()
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
    seen_request: Mutex<Option<StepModelRequest>>,
}

impl StubModel {
    fn answering(outcome: StepModelOutcome) -> Self {
        Self {
            outcome: Mutex::new(Some(Ok(outcome))),
            seen_request: Mutex::new(None),
        }
    }

    fn succeeding() -> Self {
        Self::answering(StepModelOutcome::Published)
    }

    fn failing(error: ApplicationError) -> Self {
        Self {
            outcome: Mutex::new(Some(Err(error))),
            seen_request: Mutex::new(None),
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
        *self.seen_request.lock().unwrap() = Some(request);
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

/// The work item names the step. It does not carry the prompt.
///
/// The Run objective is public display metadata that never passed the
/// data-policy boundary; the model-visible input is durable encrypted material
/// the executor reconstructs for itself. Handing the objective to an executor
/// is how a title used to reach a provider, and the request type is what stops
/// that happening again.
#[tokio::test]
async fn an_agent_step_names_only_the_step_the_executor_must_rediscover() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let objective = snapshot.run.objective.clone();
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

    let seen = model
        .seen_request
        .lock()
        .unwrap()
        .expect("the executor must be invoked for an agent step");
    assert_eq!(seen.run_id, run_id);
    assert_eq!(seen.step_id, step_id);
    assert!(
        !format!("{seen:?}").contains(&objective),
        "the Run objective must not reach the executor"
    );
}

/// A published step is already complete. Completing it again would advance the
/// Run a second time from state the result finalizer has already moved.
#[tokio::test]
async fn a_published_agent_step_is_not_completed_a_second_time_by_the_handler() {
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

    // Exactly the one Running transition this handler owns. The success belongs
    // to the provider-result finalizer.
    let commits = store.commits();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].new_steps[0].status, RunStepStatus::Running);
    assert!(commits[0].new_steps[0].output_references.is_empty());
}

/// A refusal is a decision, and the step records it as one.
#[tokio::test]
async fn a_denied_dispatch_fails_the_step_without_promising_a_retry() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let authorization_id = PolicyDecisionId::new();
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())))
        .with_model_executor(Arc::new(StubModel::answering(StepModelOutcome::Denied {
            authorization_id,
        })));

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
        .expect("a denied step must say why");
    assert_eq!(failure.code, "provider_dispatch_denied");
    assert!(failure.message.contains(&authorization_id.to_string()));
    assert!(!failure.retryable);
}

/// Nothing was called and the step is not finished, so the item must survive.
/// Completing it would leave a step nobody is coming back for; failing it would
/// report a failure for work that may be succeeding elsewhere.
#[tokio::test]
async fn a_contended_dispatch_reschedules_the_item_rather_than_dropping_it() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let handled_at = timestamp();
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(handled_at)))
        .with_model_executor(Arc::new(StubModel::answering(StepModelOutcome::Conflict {
            retry_after_seconds: Some(9),
        })));

    let outcome = handler
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

    let RunWorkOutcome::Retry {
        available_at,
        error,
    } = outcome
    else {
        panic!("a contended dispatch must leave the item schedulable, got {outcome:?}")
    };
    // The interval admission quoted, not one this handler invented.
    assert_eq!(available_at, handled_at + chrono::Duration::seconds(9));
    assert_eq!(error.code, "provider_dispatch_contended");
    assert!(error.retryable);
    // Only the Running transition: the step itself is untouched, because it is
    // still someone's to finish.
    assert_eq!(store.commits().len(), 1);
    assert!(store.last_commit().new_steps[0].error.is_none());
}

/// An adopted Unknown dispatch may already have happened. The step fails, and
/// it fails in a way that cannot be retried into a second effect.
#[tokio::test]
async fn an_unknown_dispatch_fails_the_step_and_forbids_a_second_call() {
    let (snapshot, step_id) =
        snapshot_with_step(RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()));
    let run_id = snapshot.run.id;
    let workspace = snapshot.run.workspace_id;
    let store = Arc::new(RecordingStore::new(snapshot));
    let handler = ExecuteStepHandler::new(store.clone(), Arc::new(FixedClock(timestamp())))
        .with_model_executor(Arc::new(StubModel::answering(
            StepModelOutcome::RecoveredUnknown,
        )));

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
        .expect("an unknown dispatch must say why the step did not succeed");
    assert_eq!(failure.code, "provider_dispatch_unknown");
    assert!(!failure.retryable);
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

/// `ConfiguredProviderDispatchPolicyEvaluator` is the one production evaluator
/// behind `ProviderDispatchPolicyEvaluator`. Until it existed the trait had
/// only test doubles, so nothing proved that a governed dispatch consults the
/// capability engine and the model-data boundary before any disclosure, or
/// that the Run decision it writes is bound to the exact dispatch cause.
///
/// These tests also pin route-blindness: the evaluator reads its destination
/// from the immutable target chosen by the pinned connection revision and
/// never from the request, the intent, or a configured default.
mod configured_provider_dispatch_policy {
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use vestrace_application::{
        ApplicationError, ConfiguredProviderDispatchPolicyEvaluator, EffectiveModelRequest,
        ModelDataPolicyMode, ModelDataPolicySettings, PolicyDecisionEngine, ProviderDispatchCause,
        ProviderDispatchPolicyEvaluator, ProviderDispatchTarget, RequestContext,
        SharedPolicyDecisionEngine,
    };
    use vestrace_domain::QualificationJobId;
    use vestrace_domain::id::{AgentRunId, PrincipalId, RunStepId, WorkspaceId};
    use vestrace_domain::trust::DataPolicy;
    use vestrace_domain::{
        AuthorizationRequest, Capability, ConnectionKind, DataDestination, DataPolicyId,
        DeliverySemantics, EffectPrecondition, EffectReversibility, ExternalEffectIntent,
        IdempotencyProfile, PolicyDecision, PolicyDecisionId, PolicyDecisionReason,
        PolicyDecisionResult, PolicyInputState, RiskCategory, Sensitivity,
    };

    /// Answers with one configured verdict and records every request, so a test
    /// can prove the exact capability tuple reached the engine.
    struct ScriptedEngine {
        result: PolicyDecisionResult,
        seen: Mutex<Vec<AuthorizationRequest>>,
    }

    impl ScriptedEngine {
        fn new(result: PolicyDecisionResult) -> Arc<Self> {
            Arc::new(Self {
                result,
                seen: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl PolicyDecisionEngine for ScriptedEngine {
        async fn decide(
            &self,
            context: &RequestContext,
            request: AuthorizationRequest,
        ) -> Result<PolicyDecision, ApplicationError> {
            self.seen.lock().unwrap().push(request.clone());
            let input_state = PolicyInputState::from_request(
                context.workspace_id,
                context.principal_id,
                &request,
            );
            Ok(PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "test-policy-v1".to_owned(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                capability: request.capability,
                operation: request.operation.clone(),
                resource_scope: request.resource_scope.clone(),
                result: self.result,
                reason: if self.result == PolicyDecisionResult::Allow {
                    PolicyDecisionReason::GrantMatched
                } else {
                    PolicyDecisionReason::DefaultDeny
                },
                input_state,
                matched_grant_id: None,
                decided_at: vestrace_domain::time::now(),
            })
        }
    }

    struct Ids {
        workspace: WorkspaceId,
        principal: PrincipalId,
        run: AgentRunId,
        step: RunStepId,
    }

    fn ids() -> Ids {
        Ids {
            workspace: WorkspaceId::new(),
            principal: PrincipalId::new(),
            run: AgentRunId::new(),
            step: RunStepId::new(),
        }
    }

    fn context(ids: &Ids) -> RequestContext {
        RequestContext::new(ids.workspace, ids.principal)
    }

    fn intent(ids: &Ids) -> ExternalEffectIntent {
        ExternalEffectIntent::new(
            format!("run://{}", ids.run),
            ids.workspace,
            ids.principal,
            "openai-compatible",
            "chat",
            "https://provider.policy.test/v1/chat/completions",
            "sha256:arguments",
            "produce a governed answer",
            vec![
                EffectPrecondition::new("model-snapshot", uuid::Uuid::now_v7().to_string())
                    .unwrap(),
            ],
            "sha256:preconditions",
            RiskCategory::Medium,
            EffectReversibility::Unknown,
            IdempotencyProfile::ProviderKey,
            DeliverySemantics::AtLeastOnce,
            Capability::ExportRead,
            None::<String>,
            None::<String>,
            chrono::Utc::now(),
        )
        .unwrap()
    }

    fn run_step_cause(ids: &Ids) -> ProviderDispatchCause {
        ProviderDispatchCause::RunStep {
            run_id: ids.run,
            step_id: ids.step,
            snapshot_id: uuid::Uuid::now_v7(),
        }
    }

    fn target(destination: DataDestination) -> ProviderDispatchTarget {
        ProviderDispatchTarget {
            kind: ConnectionKind::OpenAiChatCompletionsV1,
            runtime_base_url: "https://provider.policy.test/v1".to_owned(),
            destination,
        }
    }

    /// A policy that permits exactly the named destination at the configured floor.
    fn settings(
        allowed: [DataDestination; 1],
        mode: ModelDataPolicyMode,
    ) -> ModelDataPolicySettings {
        ModelDataPolicySettings {
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "data-policy-v1",
                Sensitivity::Confidential,
                BTreeSet::from(allowed),
                None,
            )
            .unwrap(),
            classification: Sensitivity::Confidential,
            mode,
        }
    }

    fn evaluator(
        engine: SharedPolicyDecisionEngine,
        settings: ModelDataPolicySettings,
    ) -> ConfiguredProviderDispatchPolicyEvaluator {
        ConfiguredProviderDispatchPolicyEvaluator::new(engine, settings)
    }

    #[tokio::test]
    async fn it_asks_the_engine_for_the_exact_effect_capability_tuple() {
        let ids = ids();
        let engine = ScriptedEngine::new(PolicyDecisionResult::Allow);
        let intent = intent(&ids);
        let evaluation = evaluator(
            engine.clone(),
            settings(
                [DataDestination::RemoteProvider],
                ModelDataPolicyMode::Enforce,
            ),
        )
        .evaluate(
            &context(&ids),
            &intent,
            &run_step_cause(&ids),
            &target(DataDestination::RemoteProvider),
            &EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();

        let seen = engine.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "the engine is consulted exactly once");
        assert_eq!(seen[0].capability, intent.required_capability());
        assert_eq!(seen[0].operation, intent.operation());
        assert_eq!(seen[0].resource_scope, intent.target());
        assert!(evaluation.authorization.is_allowed());
    }

    #[tokio::test]
    async fn it_binds_the_run_decision_to_the_exact_dispatch_cause() {
        let ids = ids();
        let evaluation = evaluator(
            ScriptedEngine::new(PolicyDecisionResult::Allow),
            settings(
                [DataDestination::RemoteProvider],
                ModelDataPolicyMode::Enforce,
            ),
        )
        .evaluate(
            &context(&ids),
            &intent(&ids),
            &run_step_cause(&ids),
            &target(DataDestination::RemoteProvider),
            &EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();

        let record = evaluation
            .model_data_policy
            .expect("a Run step gets exactly one model-data decision");
        assert_eq!(record.run_id, ids.run);
        assert_eq!(record.step_id, ids.step);
        assert!(record.allowed);
        assert_eq!(record.destination, DataDestination::RemoteProvider);
    }

    #[tokio::test]
    async fn it_takes_its_destination_from_the_supplied_target() {
        let ids = ids();
        // The policy admits only the local destination. The same intent, whose
        // own target string names an https endpoint, is evaluated against the
        // destination the pinned revision chose - so this must be allowed.
        let evaluation = evaluator(
            ScriptedEngine::new(PolicyDecisionResult::Allow),
            settings([DataDestination::LocalModel], ModelDataPolicyMode::Enforce),
        )
        .evaluate(
            &context(&ids),
            &intent(&ids),
            &run_step_cause(&ids),
            &target(DataDestination::LocalModel),
            &EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();

        let record = evaluation.model_data_policy.expect("Run step decision");
        assert_eq!(record.destination, DataDestination::LocalModel);
        assert!(record.allowed, "the pinned destination is the admitted one");
        assert!(evaluation.authorization.is_allowed());
    }

    #[tokio::test]
    async fn it_still_records_the_run_decision_when_the_capability_is_refused() {
        let ids = ids();
        let evaluation = evaluator(
            ScriptedEngine::new(PolicyDecisionResult::Deny),
            settings(
                [DataDestination::RemoteProvider],
                ModelDataPolicyMode::Enforce,
            ),
        )
        .evaluate(
            &context(&ids),
            &intent(&ids),
            &run_step_cause(&ids),
            &target(DataDestination::RemoteProvider),
            &EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();

        assert!(!evaluation.authorization.is_allowed());
        let record = evaluation
            .model_data_policy
            .expect("a denied Run step still records what the boundary was asked");
        assert_eq!(record.run_id, ids.run);
        assert_eq!(record.step_id, ids.step);
    }

    #[tokio::test]
    async fn it_denies_the_authorization_when_the_data_boundary_refuses_under_enforce() {
        let ids = ids();
        // Capability is granted; the destination is not admitted by the data
        // policy. Under Enforce the dispatch must not be authorized, because
        // an allowed authorization beside a denied Enforce record is exactly
        // the combination the dispatch repository refuses.
        let evaluation = evaluator(
            ScriptedEngine::new(PolicyDecisionResult::Allow),
            settings([DataDestination::LocalModel], ModelDataPolicyMode::Enforce),
        )
        .evaluate(
            &context(&ids),
            &intent(&ids),
            &run_step_cause(&ids),
            &target(DataDestination::RemoteProvider),
            &EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();

        assert!(
            !evaluation.authorization.is_allowed(),
            "an enforced data-boundary refusal denies the dispatch"
        );
        let record = evaluation.model_data_policy.expect("Run step decision");
        assert!(!record.allowed);
        assert_eq!(record.mode, ModelDataPolicyMode::Enforce);
    }

    #[tokio::test]
    async fn it_keeps_the_authorization_allowed_when_the_data_boundary_refuses_under_observe() {
        let ids = ids();
        let evaluation = evaluator(
            ScriptedEngine::new(PolicyDecisionResult::Allow),
            settings([DataDestination::LocalModel], ModelDataPolicyMode::Observe),
        )
        .evaluate(
            &context(&ids),
            &intent(&ids),
            &run_step_cause(&ids),
            &target(DataDestination::RemoteProvider),
            &EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();

        assert!(
            evaluation.authorization.is_allowed(),
            "observe mode proceeds and preserves the denied verdict"
        );
        let record = evaluation.model_data_policy.expect("Run step decision");
        assert!(!record.allowed, "the refusal is still recorded");
        assert_eq!(record.mode, ModelDataPolicyMode::Observe);
    }

    #[tokio::test]
    async fn it_issues_no_run_decision_for_a_qualification_probe() {
        let ids = ids();
        let evaluation = evaluator(
            ScriptedEngine::new(PolicyDecisionResult::Allow),
            settings([DataDestination::LocalModel], ModelDataPolicyMode::Enforce),
        )
        .evaluate(
            &context(&ids),
            &intent(&ids),
            &ProviderDispatchCause::QualificationProbe {
                qualification_job_id: QualificationJobId::new(),
                qualification_target_id: uuid::Uuid::now_v7(),
                probe_ordinal: "01".to_owned(),
            },
            // A destination the data policy would refuse: a structural probe
            // carries no governed content, so it must not be gated by it.
            &target(DataDestination::RemoteProvider),
            &EffectiveModelRequest::models_list(),
        )
        .await
        .unwrap();

        assert!(
            evaluation.model_data_policy.is_none(),
            "a probe has no Run or step identity, so it gets no Run decision"
        );
        assert!(evaluation.authorization.is_allowed());
    }
}

/// `GovernedProviderStepExecutor` is the only production path from a work item
/// to a provider. These tests pin the order it may not depart from: classify
/// before anything, rediscover rather than be told, and call the adapter at
/// most once — never after a dispatch has already left the process.
///
/// Nothing here opens a socket or reaches a database. The authorities are
/// scripted so the ordering itself is what is under test.
mod governed_provider_step_executor {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use vestrace_application::run::{
        GovernedModelAdapter, GovernedProviderStepExecutor, RUN_STEP_DISPATCH_TTL_SECONDS,
        StepModelExecutor, StepModelOutcome, StepModelRequest,
    };
    use vestrace_application::{
        ApplicationError, ConnectionAuth, EffectiveChatEvidence, EffectiveChatFinishReason,
        EffectiveChatMessage, EffectiveChatResult, EffectiveModelRequest, EffectiveModelResponse,
        EffectiveRequestLimits, EffectiveSampling, FinalizeProviderResult, PrepareProviderResult,
        PreparedProviderResult, ProviderDispatchAuthority, ProviderDispatchCause,
        ProviderDispatchOutcome, ProviderDispatchRepository, ProviderDispatchRequest,
        ProviderDispatchTarget, ProviderError, ProviderPostNetworkCompletion,
        ProviderResultIdentities, ProviderResultPublication, ProviderResultRepository,
        ProviderUsage, RequestContext, RunStepAttemptRecovery, RunStepDispatchPlan,
        RunStepExecutionAttempt, SharedProviderDispatchRepository, UnitOfWork,
    };
    use vestrace_domain::id::{
        AgentRunId, ArtifactId, ArtifactRevisionId, ExternalEffectReceiptId, ModelExecutionId,
        PrincipalId, RunStepId, WorkItemId, WorkerId, WorkspaceId,
    };
    use vestrace_domain::{
        Capability, ConnectionId, ConnectionKind, ConnectionRevisionId, ContentMaterialId,
        DataDestination, DeliverySemantics, EffectPrecondition, EffectReversibility,
        ExternalEffectId, ExternalEffectIntent, ExternalEffectLifecycleTransitionId,
        ExternalEffectReceipt, IdempotencyProfile, IntentNonce, MaterialKeyCreationIntentId,
        MaterialKeyId, ModelRequestEvidenceId, PolicyDecisionId, PreparedMaterialAttachmentId,
        RiskCategory, SizeClass,
    };

    struct Ids {
        workspace: WorkspaceId,
        principal: PrincipalId,
        run: AgentRunId,
        step: RunStepId,
        connection: ConnectionId,
        revision: ConnectionRevisionId,
        snapshot: uuid::Uuid,
        evidence: ModelRequestEvidenceId,
        effect: ExternalEffectId,
    }

    /// Builds the identities together with the one intent that owns the effect
    /// id, because `ExternalEffectIntent::new` allocates. A fixture that made
    /// up a separate effect id would be describing a state the durable loader
    /// cannot produce.
    fn ids() -> (Ids, ExternalEffectIntent) {
        let mut ids = Ids {
            workspace: WorkspaceId::new(),
            principal: PrincipalId::new(),
            run: AgentRunId::new(),
            step: RunStepId::new(),
            connection: ConnectionId::new(),
            revision: ConnectionRevisionId::new(),
            snapshot: uuid::Uuid::now_v7(),
            evidence: ModelRequestEvidenceId::new(),
            effect: ExternalEffectId::new(),
        };
        let intent = intent(&ids);
        ids.effect = intent.id();
        (ids, intent)
    }

    fn context(ids: &Ids) -> RequestContext {
        RequestContext::new(ids.workspace, ids.principal)
    }

    fn intent(ids: &Ids) -> ExternalEffectIntent {
        ExternalEffectIntent::new(
            format!("run://{}", ids.run),
            ids.workspace,
            ids.principal,
            "openai-compatible",
            "chat",
            "https://pinned.executor.test/v1/chat/completions",
            "sha256:arguments",
            "produce a governed answer",
            vec![EffectPrecondition::new("model-snapshot", ids.snapshot.to_string()).unwrap()],
            "sha256:preconditions",
            RiskCategory::Medium,
            EffectReversibility::Unknown,
            IdempotencyProfile::ProviderKey,
            DeliverySemantics::AtLeastOnce,
            Capability::ExportRead,
            None::<String>,
            None::<String>,
            Utc::now(),
        )
        .unwrap()
    }

    fn attempt(ids: &Ids) -> RunStepExecutionAttempt {
        RunStepExecutionAttempt {
            id: uuid::Uuid::now_v7(),
            workspace_id: ids.workspace,
            run_id: ids.run,
            step_id: ids.step,
            model_binding_snapshot_id: ids.snapshot,
            input_material_intent_id: MaterialKeyCreationIntentId::new(),
            input_content_material_id: ContentMaterialId::new(),
            input_material_key_id: MaterialKeyId::new(),
            input_intent_nonce: IntentNonce::new(),
            input_prepared_attachment_id: PreparedMaterialAttachmentId::new(),
            external_effect_id: ids.effect,
            model_request_evidence_id: ids.evidence,
        }
    }

    fn authority(ids: &Ids) -> ProviderDispatchAuthority {
        ProviderDispatchAuthority {
            effect_id: ids.effect,
            authorization_id: PolicyDecisionId::new(),
            connection_id: ids.connection,
            connection_revision_id: ids.revision,
            concurrency_lease_id: uuid::Uuid::now_v7(),
            credential_lease_id: None,
            dispatch_transition_id: ExternalEffectLifecycleTransitionId::new(),
            dispatch_expires_at: Utc::now() + chrono::Duration::seconds(60),
        }
    }

    fn effective_request() -> EffectiveModelRequest {
        EffectiveModelRequest::chat_completions(
            "pinned-model",
            vec![EffectiveChatMessage::user("the governed input")],
            EffectiveSampling::new(0.2, 1.0).unwrap(),
            EffectiveRequestLimits::new(256, 4, 32_768).unwrap(),
            Vec::new(),
            false,
        )
        .unwrap()
    }

    fn result_identities() -> ProviderResultIdentities {
        ProviderResultIdentities {
            material_intent_id: MaterialKeyCreationIntentId::new(),
            content_material_id: ContentMaterialId::new(),
            material_key_id: MaterialKeyId::new(),
            intent_nonce: IntentNonce::new(),
            prepared_attachment_id: PreparedMaterialAttachmentId::new(),
            receipt_id: ExternalEffectReceiptId::new(),
            artifact_id: ArtifactId::new(),
            artifact_revision_id: ArtifactRevisionId::new(),
            model_execution_id: ModelExecutionId::new(),
            advance_work_item_id: WorkItemId::new(),
        }
    }

    /// What one `prepare_dispatch` call was asked to admit. Recorded rather
    /// than the request itself, which owns a request that must not be cloned.
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct SeenDispatch {
        connection_id: ConnectionId,
        connection_revision_id: ConnectionRevisionId,
        evidence_id: ModelRequestEvidenceId,
        effect_id: ExternalEffectId,
        cause: ProviderDispatchCause,
        ttl_seconds: u16,
        has_credential: bool,
    }

    struct ScriptedDispatch {
        recovery: RunStepAttemptRecovery,
        plan_attempt: RunStepExecutionAttempt,
        plan_intent: ExternalEffectIntent,
        plan_connection: ConnectionId,
        plan_revision: ConnectionRevisionId,
        outcome: Mutex<Option<ProviderDispatchOutcome>>,
        seen: Mutex<Vec<SeenDispatch>>,
        plan_calls: AtomicUsize,
        completions: Mutex<Vec<ExternalEffectReceipt>>,
    }

    impl ScriptedDispatch {
        fn new(
            ids: &Ids,
            plan_intent: ExternalEffectIntent,
            recovery: RunStepAttemptRecovery,
        ) -> Self {
            Self {
                recovery,
                plan_attempt: attempt(ids),
                plan_intent,
                plan_connection: ids.connection,
                plan_revision: ids.revision,
                outcome: Mutex::new(None),
                seen: Mutex::new(Vec::new()),
                plan_calls: AtomicUsize::new(0),
                completions: Mutex::new(Vec::new()),
            }
        }

        /// Hands back a plan whose intent names a different effect than its
        /// attempt: the one shape the durable loader refuses to produce and the
        /// executor must therefore refuse to act on.
        fn with_mismatched_intent(mut self, intent: ExternalEffectIntent) -> Self {
            self.plan_intent = intent;
            self
        }

        fn with_outcome(self, outcome: ProviderDispatchOutcome) -> Self {
            *self.outcome.lock().unwrap() = Some(outcome);
            self
        }
    }

    #[async_trait]
    impl ProviderDispatchRepository for ScriptedDispatch {
        async fn recover_run_step_attempt(
            &self,
            _context: &RequestContext,
            _run_id: AgentRunId,
            _step_id: RunStepId,
            _recovered_at: DateTime<Utc>,
        ) -> Result<RunStepAttemptRecovery, ApplicationError> {
            Ok(self.recovery)
        }

        async fn load_run_step_dispatch_plan(
            &self,
            _context: &RequestContext,
            _run_id: AgentRunId,
            _step_id: RunStepId,
        ) -> Result<RunStepDispatchPlan, ApplicationError> {
            self.plan_calls.fetch_add(1, Ordering::SeqCst);
            Ok(RunStepDispatchPlan {
                attempt: self.plan_attempt.clone(),
                intent: self.plan_intent.clone(),
                connection_id: self.plan_connection,
                connection_revision_id: self.plan_revision,
                credential: None,
            })
        }

        async fn prepare_dispatch(
            &self,
            request: ProviderDispatchRequest,
        ) -> Result<ProviderDispatchOutcome, ApplicationError> {
            self.seen.lock().unwrap().push(SeenDispatch {
                connection_id: request.connection_id,
                connection_revision_id: request.connection_revision_id,
                evidence_id: request.model_request_evidence_id,
                effect_id: request.intent.id(),
                cause: request.cause.clone(),
                ttl_seconds: request.dispatch_ttl_seconds,
                has_credential: request.credential.is_some(),
            });
            self.outcome
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| ApplicationError::Internal("dispatch was prepared twice".into()))
        }

        async fn complete_post_network(
            &self,
            _context: &RequestContext,
            completion: ProviderPostNetworkCompletion,
        ) -> Result<(), ApplicationError> {
            self.completions.lock().unwrap().push(completion.receipt);
            Ok(())
        }

        async fn release_after_provider_result_in(
            &self,
            _context: &RequestContext,
            _unit_of_work: &mut dyn UnitOfWork,
            _authority: &ProviderDispatchAuthority,
            _receipt: &ExternalEffectReceipt,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn recover_lost_post_network(
            &self,
            _context: &RequestContext,
            _authority: &ProviderDispatchAuthority,
            _recovered_at: DateTime<Utc>,
        ) -> Result<vestrace_application::ProviderLostDispatchRecovery, ApplicationError> {
            Err(ApplicationError::Internal(
                "the executor must not recover a lost dispatch itself".into(),
            ))
        }
    }

    struct ScriptedResults {
        prepared: AtomicUsize,
        recovered: AtomicUsize,
        finalized: AtomicUsize,
        run: AgentRunId,
        step: RunStepId,
        effect: ExternalEffectId,
    }

    impl ScriptedResults {
        fn new(ids: &Ids) -> Self {
            Self {
                prepared: AtomicUsize::new(0),
                recovered: AtomicUsize::new(0),
                finalized: AtomicUsize::new(0),
                run: ids.run,
                step: ids.step,
                effect: ids.effect,
            }
        }

        fn prepared_result(&self) -> PreparedProviderResult {
            PreparedProviderResult {
                preparation_id: uuid::Uuid::now_v7(),
                effect_id: self.effect,
                run_id: self.run,
                step_id: self.step,
                identities: result_identities(),
                size_class: SizeClass::FourKiB,
                evidence: EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
                usage: ProviderUsage::Unknown,
            }
        }
    }

    #[async_trait]
    impl ProviderResultRepository for ScriptedResults {
        async fn prepare(
            &self,
            _request: PrepareProviderResult,
        ) -> Result<PreparedProviderResult, ApplicationError> {
            Err(ApplicationError::Internal(
                "a governed Run step must prepare under its dispatch authority".into(),
            ))
        }

        async fn prepare_after_dispatch(
            &self,
            _request: PrepareProviderResult,
            _authority: &ProviderDispatchAuthority,
        ) -> Result<PreparedProviderResult, ApplicationError> {
            self.prepared.fetch_add(1, Ordering::SeqCst);
            Ok(self.prepared_result())
        }

        async fn recover_result_prepared(
            &self,
            _context: &RequestContext,
            _effect_id: ExternalEffectId,
        ) -> Result<PreparedProviderResult, ApplicationError> {
            self.recovered.fetch_add(1, Ordering::SeqCst);
            Ok(self.prepared_result())
        }

        async fn finalize(
            &self,
            _context: &RequestContext,
            _request: FinalizeProviderResult,
        ) -> Result<ProviderResultPublication, ApplicationError> {
            self.finalized.fetch_add(1, Ordering::SeqCst);
            Ok(ProviderResultPublication {
                publication_id: uuid::Uuid::now_v7(),
                preparation_id: uuid::Uuid::now_v7(),
                artifact_id: ArtifactId::new(),
                artifact_revision_id: ArtifactRevisionId::new(),
                content_material_id: ContentMaterialId::new(),
                model_execution_id: ModelExecutionId::new(),
                size_class: SizeClass::FourKiB,
            })
        }

        async fn finalize_in(
            &self,
            _context: &RequestContext,
            _unit_of_work: &mut dyn UnitOfWork,
            _request: FinalizeProviderResult,
        ) -> Result<ProviderResultPublication, ApplicationError> {
            Err(ApplicationError::Internal(
                "the executor must not open its own publication transaction".into(),
            ))
        }
    }

    /// Counts calls and records the destination it was handed, so a test can
    /// prove both that the adapter ran exactly once and that it was never asked
    /// to choose where.
    struct CountingAdapter {
        calls: AtomicUsize,
        seen: Mutex<Vec<(ConnectionKind, String)>>,
        answer: Mutex<Option<Result<EffectiveModelResponse, ProviderError>>>,
    }

    impl CountingAdapter {
        fn answering(answer: Result<EffectiveModelResponse, ProviderError>) -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                seen: Mutex::new(Vec::new()),
                answer: Mutex::new(Some(answer)),
            })
        }

        fn completing() -> Arc<Self> {
            Self::answering(Ok(EffectiveModelResponse::ChatCompletions(
                EffectiveChatResult::new(
                    zeroize::Zeroizing::new("the governed answer".to_owned()),
                    EffectiveChatEvidence::Completed(EffectiveChatFinishReason::Stop),
                    ProviderUsage::Unknown,
                )
                .unwrap(),
            )))
        }
    }

    #[async_trait]
    impl GovernedModelAdapter for CountingAdapter {
        async fn execute(
            &self,
            kind: ConnectionKind,
            runtime_base_url: &str,
            _auth: ConnectionAuth,
            _request: EffectiveModelRequest,
        ) -> Result<EffectiveModelResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen
                .lock()
                .unwrap()
                .push((kind, runtime_base_url.to_owned()));
            self.answer
                .lock()
                .unwrap()
                .take()
                .expect("the adapter was called more than once")
        }
    }

    fn prepared_outcome(ids: &Ids) -> ProviderDispatchOutcome {
        ProviderDispatchOutcome::Prepared {
            authority: Box::new(authority(ids)),
            target: ProviderDispatchTarget {
                kind: ConnectionKind::OpenAiChatCompletionsV1,
                runtime_base_url: "https://pinned.executor.test/v1".to_owned(),
                destination: DataDestination::RemoteProvider,
            },
            q1_request: None,
            request: effective_request(),
            auth: ConnectionAuth::None,
        }
    }

    fn executor(
        dispatch: Arc<ScriptedDispatch>,
        results: Arc<ScriptedResults>,
        adapter: Arc<CountingAdapter>,
    ) -> GovernedProviderStepExecutor<ScriptedResults> {
        let shared: SharedProviderDispatchRepository = dispatch;
        GovernedProviderStepExecutor::new(shared, results, adapter, WorkerId::new())
    }

    /// The route is the pinned binding's, and the executor never names it.
    #[tokio::test]
    async fn it_dispatches_the_rediscovered_binding_and_calls_the_adapter_once() {
        let (ids, plan_intent) = ids();
        let dispatch = Arc::new(
            ScriptedDispatch::new(&ids, plan_intent, RunStepAttemptRecovery::ResumeReserved)
                .with_outcome(prepared_outcome(&ids)),
        );
        let results = Arc::new(ScriptedResults::new(&ids));
        let adapter = CountingAdapter::completing();
        let outcome = executor(dispatch.clone(), results.clone(), adapter.clone())
            .execute(
                &context(&ids),
                StepModelRequest {
                    run_id: ids.run,
                    step_id: ids.step,
                },
            )
            .await
            .expect("a reserved attempt must execute");

        assert_eq!(outcome, StepModelOutcome::Published);
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            adapter.seen.lock().unwrap().as_slice(),
            [(
                ConnectionKind::OpenAiChatCompletionsV1,
                "https://pinned.executor.test/v1".to_owned()
            )]
        );
        assert_eq!(dispatch.plan_calls.load(Ordering::SeqCst), 1);
        assert_eq!(results.prepared.load(Ordering::SeqCst), 1);
        assert_eq!(results.finalized.load(Ordering::SeqCst), 1);

        let seen = dispatch.seen.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![SeenDispatch {
                connection_id: ids.connection,
                connection_revision_id: ids.revision,
                evidence_id: ids.evidence,
                effect_id: ids.effect,
                cause: ProviderDispatchCause::RunStep {
                    run_id: ids.run,
                    step_id: ids.step,
                    snapshot_id: ids.snapshot,
                },
                ttl_seconds: RUN_STEP_DISPATCH_TTL_SECONDS,
                has_credential: false,
            }]
        );
    }

    /// A refusal costs nothing on the wire.
    #[tokio::test]
    async fn a_denied_dispatch_reaches_no_adapter() {
        let (ids, plan_intent) = ids();
        let authorization_id = PolicyDecisionId::new();
        let dispatch = Arc::new(
            ScriptedDispatch::new(&ids, plan_intent, RunStepAttemptRecovery::ResumeReserved)
                .with_outcome(ProviderDispatchOutcome::Denied { authorization_id }),
        );
        let results = Arc::new(ScriptedResults::new(&ids));
        let adapter = CountingAdapter::completing();
        let outcome = executor(dispatch, results.clone(), adapter.clone())
            .execute(
                &context(&ids),
                StepModelRequest {
                    run_id: ids.run,
                    step_id: ids.step,
                },
            )
            .await
            .expect("a denial is an answer, not a fault");

        assert_eq!(outcome, StepModelOutcome::Denied { authorization_id });
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
        assert_eq!(results.prepared.load(Ordering::SeqCst), 0);
        assert_eq!(results.finalized.load(Ordering::SeqCst), 0);
    }

    /// The dispatch already left the process. Calling again would be a second
    /// effect for a request that may well have been performed.
    #[tokio::test]
    async fn an_adopted_unknown_never_calls_the_adapter_again() {
        for recovery in [
            RunStepAttemptRecovery::AdoptedUnknown,
            RunStepAttemptRecovery::AlreadyUnknown,
        ] {
            let (ids, plan_intent) = ids();
            let dispatch = Arc::new(ScriptedDispatch::new(&ids, plan_intent, recovery));
            let results = Arc::new(ScriptedResults::new(&ids));
            let adapter = CountingAdapter::completing();
            let outcome = executor(dispatch.clone(), results.clone(), adapter.clone())
                .execute(
                    &context(&ids),
                    StepModelRequest {
                        run_id: ids.run,
                        step_id: ids.step,
                    },
                )
                .await
                .expect("an unknown dispatch is classified, not retried");

            assert_eq!(outcome, StepModelOutcome::RecoveredUnknown);
            assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
            // Not even the plan is loaded: there is nothing left to dispatch.
            assert_eq!(dispatch.plan_calls.load(Ordering::SeqCst), 0);
            assert_eq!(results.finalized.load(Ordering::SeqCst), 0);
        }
    }

    /// A live dispatch inside its deadline belongs to whoever admitted it.
    #[tokio::test]
    async fn a_live_dispatch_deadline_yields_without_touching_anything() {
        let (ids, plan_intent) = ids();
        let dispatch = Arc::new(ScriptedDispatch::new(
            &ids,
            plan_intent,
            RunStepAttemptRecovery::AwaitDispatchDeadline,
        ));
        let results = Arc::new(ScriptedResults::new(&ids));
        let adapter = CountingAdapter::completing();
        let outcome = executor(dispatch.clone(), results.clone(), adapter.clone())
            .execute(
                &context(&ids),
                StepModelRequest {
                    run_id: ids.run,
                    step_id: ids.step,
                },
            )
            .await
            .unwrap();

        // The database owns the other worker's deadline, so no interval is
        // invented here.
        assert_eq!(
            outcome,
            StepModelOutcome::Conflict {
                retry_after_seconds: None
            }
        );
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
        assert_eq!(dispatch.plan_calls.load(Ordering::SeqCst), 0);
    }

    /// The content is already durable. Publication is all that remains, and the
    /// provider is not asked a second time for what it already answered.
    #[tokio::test]
    async fn a_prepared_result_is_published_without_a_second_call() {
        let (ids, plan_intent) = ids();
        let dispatch = Arc::new(ScriptedDispatch::new(
            &ids,
            plan_intent,
            RunStepAttemptRecovery::ResumeResultPrepared {
                effect_id: ids.effect,
            },
        ));
        let results = Arc::new(ScriptedResults::new(&ids));
        let adapter = CountingAdapter::completing();
        let outcome = executor(dispatch.clone(), results.clone(), adapter.clone())
            .execute(
                &context(&ids),
                StepModelRequest {
                    run_id: ids.run,
                    step_id: ids.step,
                },
            )
            .await
            .expect("a prepared result must publish");

        assert_eq!(outcome, StepModelOutcome::Published);
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
        assert_eq!(results.recovered.load(Ordering::SeqCst), 1);
        assert_eq!(results.finalized.load(Ordering::SeqCst), 1);
        assert_eq!(results.prepared.load(Ordering::SeqCst), 0);
    }

    /// An already-published attempt is finished. Nothing is republished.
    #[tokio::test]
    async fn a_published_attempt_is_reported_without_republishing() {
        let (ids, plan_intent) = ids();
        let dispatch = Arc::new(ScriptedDispatch::new(
            &ids,
            plan_intent,
            RunStepAttemptRecovery::Published,
        ));
        let results = Arc::new(ScriptedResults::new(&ids));
        let adapter = CountingAdapter::completing();
        let outcome = executor(dispatch, results.clone(), adapter.clone())
            .execute(
                &context(&ids),
                StepModelRequest {
                    run_id: ids.run,
                    step_id: ids.step,
                },
            )
            .await
            .unwrap();

        assert_eq!(outcome, StepModelOutcome::Published);
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
        assert_eq!(results.finalized.load(Ordering::SeqCst), 0);
    }

    /// A provider failure is recorded against the original effect before the
    /// error is returned: a failure visible only in logs is a failure the
    /// effect's own history denies.
    #[tokio::test]
    async fn a_provider_failure_is_completed_before_it_is_reported() {
        let (ids, plan_intent) = ids();
        let dispatch = Arc::new(
            ScriptedDispatch::new(&ids, plan_intent, RunStepAttemptRecovery::ResumeAdmitted)
                .with_outcome(prepared_outcome(&ids)),
        );
        let results = Arc::new(ScriptedResults::new(&ids));
        let adapter = CountingAdapter::answering(Err(ProviderError::RateLimited));
        let error = executor(dispatch.clone(), results.clone(), adapter.clone())
            .execute(
                &context(&ids),
                StepModelRequest {
                    run_id: ids.run,
                    step_id: ids.step,
                },
            )
            .await
            .expect_err("a rate-limited provider must not report success");

        assert!(matches!(error, ApplicationError::Unavailable(_)));
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
        assert_eq!(dispatch.completions.lock().unwrap().len(), 1);
        assert_eq!(results.prepared.load(Ordering::SeqCst), 0);
        assert_eq!(results.finalized.load(Ordering::SeqCst), 0);
    }

    /// A plan whose intent and attempt name different effects would admit one
    /// effect and complete another. It is refused before the adapter exists.
    #[tokio::test]
    async fn a_plan_whose_intent_disagrees_with_its_attempt_is_refused() {
        let (_, unrelated_intent) = ids();
        let (ids, plan_intent) = ids();
        let dispatch = Arc::new(
            ScriptedDispatch::new(&ids, plan_intent, RunStepAttemptRecovery::ResumeReserved)
                .with_mismatched_intent(unrelated_intent)
                .with_outcome(prepared_outcome(&ids)),
        );
        let results = Arc::new(ScriptedResults::new(&ids));
        let adapter = CountingAdapter::completing();
        let error = executor(dispatch.clone(), results.clone(), adapter.clone())
            .execute(
                &context(&ids),
                StepModelRequest {
                    run_id: ids.run,
                    step_id: ids.step,
                },
            )
            .await
            .expect_err("a disagreeing plan must not be dispatched");

        assert!(matches!(error, ApplicationError::Conflict(_)));
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
        assert_eq!(dispatch.seen.lock().unwrap().len(), 0);
        assert_eq!(results.finalized.load(Ordering::SeqCst), 0);
    }
}
