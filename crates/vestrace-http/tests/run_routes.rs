use std::sync::{Arc, Mutex};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vestrace_application::{
    ApplicationError, CreateRunCommand, DenyAllPolicyEngine, HealthRepository,
    NullExecutionHistoryRepository, PolicyDecisionEngine, RequestContext, RunCommandExecutor,
    RunCommandResult, RunUseCases,
};
use vestrace_domain::{
    PrincipalId, WorkspaceId,
    id::{AgentRunId, AgentRuntimeSnapshotId},
    now,
    run::{AgentRun, NewAgentRun, RunCommand, RunCommandEnvelope, RunExecutionMode},
};
use vestrace_http::{AppState, build_router};

struct HealthyRepository;

#[derive(Default)]
struct FakeRunUseCases {
    runs: Mutex<Vec<AgentRun>>,
}

struct FakeRunCommands {
    runs: Arc<FakeRunUseCases>,
}

#[async_trait::async_trait]
impl HealthRepository for HealthyRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl RunUseCases for FakeRunUseCases {
    async fn create_run(
        &self,
        _context: &RequestContext,
        _command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError> {
        panic!("HTTP run creation must use RunCommandExecutor")
    }

    async fn list_runs(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError> {
        Ok(self
            .runs
            .lock()
            .unwrap()
            .iter()
            .filter(|run| run.workspace_id == context.workspace_id)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn get_run(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError> {
        Ok(self
            .runs
            .lock()
            .unwrap()
            .iter()
            .find(|run| run.workspace_id == context.workspace_id && run.id == id)
            .cloned())
    }
}

#[async_trait::async_trait]
impl vestrace_application::run::RunOrchestrator for FakeRunCommands {
    async fn create_run(
        &self,
        context: &RequestContext,
        command: vestrace_application::run::CreateRun,
    ) -> Result<vestrace_application::run::RunSnapshot, ApplicationError> {
        let run = AgentRun::create(
            NewAgentRun {
                id: AgentRunId::new(),
                workspace_id: context.workspace_id,
                objective: command.objective,
                coordinator_snapshot_id: command.coordinator_snapshot_id,
                execution_mode: command.execution_mode,
                parent: command.parent,
                budget_snapshot_id: None,
                resource_usage_snapshot_id: None,
            },
            now(),
        )
        .unwrap();
        self.runs.runs.lock().unwrap().push(run.clone());
        Ok(vestrace_application::run::RunSnapshot {
            run,
            steps: Vec::new(),
            checkpoint: None,
        })
    }

    async fn add_steps(
        &self,
        _context: &RequestContext,
        _command: vestrace_application::run::AddRunSteps,
    ) -> Result<vestrace_application::run::RunSnapshot, ApplicationError> {
        unreachable!("these tests do not exercise step creation")
    }

    async fn pause_run(
        &self,
        _context: &RequestContext,
        _command: vestrace_application::run::PauseRun,
    ) -> Result<vestrace_application::run::RunSnapshot, ApplicationError> {
        unreachable!("these tests do not exercise pause")
    }

    async fn resume_run(
        &self,
        _context: &RequestContext,
        _command: vestrace_application::run::ResumeRun,
    ) -> Result<vestrace_application::run::RunSnapshot, ApplicationError> {
        unreachable!("these tests do not exercise resume")
    }

    async fn cancel_run(
        &self,
        _context: &RequestContext,
        _command: vestrace_application::run::CancelRun,
    ) -> Result<vestrace_application::run::RunSnapshot, ApplicationError> {
        unreachable!("these tests do not exercise cancel")
    }

    async fn approve_run(
        &self,
        _context: &RequestContext,
        _command: vestrace_application::run::ApproveRun,
    ) -> Result<vestrace_application::run::RunSnapshot, ApplicationError> {
        unreachable!("these tests do not exercise approval")
    }
}

struct StubMemoryUseCases;

#[async_trait::async_trait]
impl vestrace_application::MemoryUseCases for StubMemoryUseCases {
    async fn record_event(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::RecordEventCommand,
    ) -> Result<vestrace_domain::Event, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn remember_memory(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::RememberMemoryCommand,
    ) -> Result<vestrace_domain::Memory, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn revise_memory(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::ReviseMemoryCommand,
    ) -> Result<vestrace_domain::Memory, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn link_knowledge(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_application::LinkKnowledgeCommand,
    ) -> Result<vestrace_domain::KnowledgeRelation, vestrace_application::ApplicationError> {
        Err(vestrace_application::ApplicationError::Unavailable(
            "stub".to_owned(),
        ))
    }
    async fn find_memory(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::MemoryId,
    ) -> Result<Option<vestrace_domain::Memory>, vestrace_application::ApplicationError> {
        Ok(None)
    }
}

struct StubTextRetriever;

#[async_trait::async_trait]
impl vestrace_application::TextRetriever for StubTextRetriever {
    async fn search(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::NormalizedRetrievalRequest,
    ) -> Result<Vec<vestrace_domain::RetrievalCandidate>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
}

struct StubRetrievalJournal;

#[async_trait::async_trait]
impl vestrace_application::RetrievalJournal for StubRetrievalJournal {
    async fn record_run(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::retrieval::RetrievalRunRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn record_context_pack(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::ContextPackId,
        _: vestrace_domain::id::RetrievalRunId,
        _: vestrace_domain::WorkspaceId,
        _: u32,
        _: u32,
        _: &serde_json::Value,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
}

struct StubProviderRepository;
struct StubModelRepository;
struct StubAgentRepository;
struct StubSkillRepository;

#[async_trait::async_trait]
impl vestrace_application::ProviderRepository for StubProviderRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::ProviderId,
        _: &str,
        _: vestrace_domain::ProviderLocality,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::ProviderRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
impl vestrace_application::ModelRepository for StubModelRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::ModelRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::ModelRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::ModelId,
    ) -> Result<Option<vestrace_application::ModelRecord>, vestrace_application::ApplicationError>
    {
        Ok(None)
    }
}

#[async_trait::async_trait]
impl vestrace_application::AgentRepository for StubAgentRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::AgentRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::AgentRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::AgentId,
    ) -> Result<Option<vestrace_application::AgentRecord>, vestrace_application::ApplicationError>
    {
        Ok(None)
    }
}

#[async_trait::async_trait]
impl vestrace_application::SkillRepository for StubSkillRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::SkillRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::SkillRecord>, vestrace_application::ApplicationError>
    {
        Ok(Vec::new())
    }
}

struct StubRoutingDecisionRepository;
struct StubModelExecutionRepository;

#[async_trait::async_trait]
impl vestrace_application::RoutingDecisionRepository for StubRoutingDecisionRepository {
    async fn record(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::RoutingDecisionRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<
        Vec<vestrace_application::RoutingDecisionRecord>,
        vestrace_application::ApplicationError,
    > {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
impl vestrace_application::ModelExecutionRepository for StubModelExecutionRepository {
    async fn record(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::ModelExecutionRecord,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<
        Vec<vestrace_application::ModelExecutionRecord>,
        vestrace_application::ApplicationError,
    > {
        Ok(Vec::new())
    }
}

fn app(run_use_cases: Arc<FakeRunUseCases>) -> axum::Router {
    app_with_policy(run_use_cases, Arc::new(TestAllowPolicy))
}

fn default_app(run_use_cases: Arc<FakeRunUseCases>) -> axum::Router {
    app_with_policy(run_use_cases, Arc::new(DenyAllPolicyEngine))
}

fn app_with_policy(
    run_use_cases: Arc<FakeRunUseCases>,
    policy: Arc<dyn PolicyDecisionEngine>,
) -> axum::Router {
    let run_commands = Arc::new(FakeRunCommands {
        runs: run_use_cases.clone(),
    });
    build_router(
        AppState::new(
            Arc::new(HealthyRepository),
            run_use_cases,
            Arc::new(StubMemoryUseCases),
            std::sync::Arc::new(vestrace_application::RetrievalService::new(
                std::sync::Arc::new(StubTextRetriever),
                std::sync::Arc::new(StubRetrievalJournal),
            )),
            std::sync::Arc::new(StubProviderRepository),
            std::sync::Arc::new(StubModelRepository),
            std::sync::Arc::new(StubAgentRepository),
            std::sync::Arc::new(StubSkillRepository),
            std::sync::Arc::new(StubRoutingDecisionRepository),
            std::sync::Arc::new(StubModelExecutionRepository),
            std::sync::Arc::new(NullExecutionHistoryRepository::new()),
            std::sync::Arc::new(NullExecutionHistoryRepository::new()),
            std::sync::Arc::new(NullExecutionHistoryRepository::new()),
            std::sync::Arc::new(vestrace_http::MetricsRegistry::new()),
        )
        .with_policy(policy)
        .with_run_orchestrator(run_commands),
    )
}

struct TestAllowPolicy;

#[async_trait::async_trait]
impl PolicyDecisionEngine for TestAllowPolicy {
    async fn decide(
        &self,
        context: &RequestContext,
        request: vestrace_domain::AuthorizationRequest,
    ) -> Result<vestrace_domain::PolicyDecision, ApplicationError> {
        let at = now();
        let grant = vestrace_domain::CapabilityGrant::issue(
            vestrace_domain::CapabilityGrantSpec {
                id: vestrace_domain::id::CapabilityGrantId::new(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                issuer_id: context.principal_id,
                capability: request.capability.clone(),
                operation: request.operation.clone(),
                resource_scope: request.resource_scope.clone(),
                valid_from: at,
                valid_until: None,
                budget: None,
                risk_ceiling: vestrace_domain::RiskCategory::Critical,
                conditions: request.conditions.clone(),
            },
            at,
        )?;
        Ok(vestrace_domain::evaluate_capability_grants(
            vestrace_domain::id::PolicyDecisionId::new(),
            context.workspace_id,
            context.principal_id,
            "test-allow-v1",
            &request,
            &[grant],
            at,
        )?)
    }
}

fn identity_request(method: &str, uri: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
        .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
        .body(body)
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn create_run_requires_workspace_and_principal_headers() {
    let response = app(Arc::new(FakeRunUseCases::default()))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"title": "Missing context"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_run_returns_201_and_the_persisted_contract() {
    let response = app(Arc::new(FakeRunUseCases::default()))
        .oneshot(identity_request(
            "POST",
            "/v1/runs",
            Body::from(serde_json::json!({"title": "Verify retention policy"}).to_string()),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["title"], "Verify retention policy");
    assert_eq!(body["status"], "created");
    assert_eq!(body["version"], 1);
    assert!(body.get("id").is_some());
    assert!(body.get("created_at").is_some());
    assert!(body.get("updated_at").is_some());
}

#[tokio::test]
async fn default_policy_denies_before_run_command_executor() {
    let run_use_cases = Arc::new(FakeRunUseCases::default());
    let response = default_app(run_use_cases.clone())
        .oneshot(identity_request(
            "POST",
            "/v1/runs",
            Body::from(serde_json::json!({"title": "Must be denied"}).to_string()),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(run_use_cases.runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn list_runs_returns_200_without_mock_fields() {
    let run_use_cases = Arc::new(FakeRunUseCases::default());
    let workspace_id =
        WorkspaceId::from_uuid("00000000-0000-0000-0000-000000000001".parse().unwrap());
    let principal_id =
        PrincipalId::from_uuid("00000000-0000-0000-0000-000000000002".parse().unwrap());
    run_use_cases.runs.lock().unwrap().push(
        AgentRun::create(
            NewAgentRun {
                id: AgentRunId::new(),
                workspace_id,
                objective: "Persisted run".to_string(),
                coordinator_snapshot_id: AgentRuntimeSnapshotId::from_uuid(principal_id.as_uuid()),
                execution_mode: RunExecutionMode::Autopilot,
                parent: None,
                budget_snapshot_id: None,
                resource_usage_snapshot_id: None,
            },
            now(),
        )
        .unwrap(),
    );

    let response = app(run_use_cases)
        .oneshot(identity_request("GET", "/v1/runs", Body::empty()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let item = body.as_array().unwrap().first().unwrap();
    assert_eq!(item["title"], "Persisted run");
    assert!(item.get("agent").is_none());
    assert!(item.get("tokens_used").is_none());
    assert!(item.get("cost").is_none());
    assert!(item.get("duration").is_none());
}

#[tokio::test]
async fn get_unknown_run_returns_404() {
    let unknown = AgentRunId::new();
    let response = app(Arc::new(FakeRunUseCases::default()))
        .oneshot(identity_request(
            "GET",
            &format!("/v1/runs/{unknown}"),
            Body::empty(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn malformed_run_id_returns_400() {
    let response = app(Arc::new(FakeRunUseCases::default()))
        .oneshot(identity_request(
            "GET",
            "/v1/runs/not-a-uuid",
            Body::empty(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
