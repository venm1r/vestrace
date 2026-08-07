use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vestrace_application::{
    ApplicationError, CreateRunCommand, HealthRepository, NullExecutionHistoryRepository,
    RequestContext, RunCommandExecutor, RunCommandResult, RunUseCases,
};
use vestrace_domain::{
    id::AgentRunId,
    run::{AgentRun, RunCommandEnvelope},
};
use vestrace_http::{AppState, build_router};

const VALID_REQUEST_ID: &str = "01890f3e-7b28-7c00-8000-000000000001";
const VALID_CORRELATION_ID: &str = "01890f3e-7b28-7c00-8000-000000000002";

struct FakeHealthRepository {
    available: bool,
    checks: AtomicUsize,
}

struct EmptyRunUseCases;

impl FakeHealthRepository {
    fn new(available: bool) -> Self {
        Self {
            available,
            checks: AtomicUsize::new(0),
        }
    }

    fn check_count(&self) -> usize {
        self.checks.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl HealthRepository for FakeHealthRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if self.available {
            Ok(())
        } else {
            Err(ApplicationError::Unavailable(
                "postgres://secret-user:secret-password@database:5432/vestrace: raw SQL failure"
                    .to_owned(),
            ))
        }
    }
}

#[async_trait::async_trait]
impl RunUseCases for EmptyRunUseCases {
    async fn create_run(
        &self,
        _context: &RequestContext,
        _command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "run creation is unavailable in health tests".to_owned(),
        ))
    }

    async fn list_runs(
        &self,
        _context: &RequestContext,
        _limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError> {
        Ok(Vec::new())
    }

    async fn get_run(
        &self,
        _context: &RequestContext,
        _id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError> {
        Ok(None)
    }
}

struct StubRunCommandExecutor;

#[async_trait::async_trait]
impl RunCommandExecutor for StubRunCommandExecutor {
    async fn execute(
        &self,
        _context: &RequestContext,
        _command: RunCommandEnvelope,
    ) -> Result<RunCommandResult, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "run command execution is unavailable in health tests".to_owned(),
        ))
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
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::ProviderRecord>, ApplicationError> {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
impl vestrace_application::ModelRepository for StubModelRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::ModelRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::ModelRecord>, ApplicationError> {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::ModelId,
    ) -> Result<Option<vestrace_application::ModelRecord>, ApplicationError> {
        Ok(None)
    }
}

#[async_trait::async_trait]
impl vestrace_application::AgentRepository for StubAgentRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::AgentRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::AgentRecord>, ApplicationError> {
        Ok(Vec::new())
    }
    async fn find_by_id(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::AgentId,
    ) -> Result<Option<vestrace_application::AgentRecord>, ApplicationError> {
        Ok(None)
    }
}

#[async_trait::async_trait]
impl vestrace_application::SkillRepository for StubSkillRepository {
    async fn create(
        &self,
        _: &vestrace_application::RequestContext,
        _: &vestrace_application::SkillRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
    async fn list(
        &self,
        _: &vestrace_application::RequestContext,
    ) -> Result<Vec<vestrace_application::SkillRecord>, ApplicationError> {
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
        _: vestrace_domain::id::RetrievalRunId,
        _: &str,
        _: &str,
        _: usize,
        _: i32,
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

fn router(repository: Arc<FakeHealthRepository>) -> axum::Router {
    build_router(AppState::new(
        repository,
        Arc::new(EmptyRunUseCases),
        Arc::new(StubRunCommandExecutor),
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
    ))
}

async fn body(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn live_returns_exact_safe_body_without_checking_dependencies() {
    let repository = Arc::new(FakeHealthRepository::new(false));

    let response = router(repository.clone())
        .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await, r#"{"status":"ok"}"#);
    assert_eq!(repository.check_count(), 0);
}

#[tokio::test]
async fn ready_returns_exact_ok_body_when_repository_is_healthy() {
    let repository = Arc::new(FakeHealthRepository::new(true));

    let response = router(repository.clone())
        .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await, r#"{"status":"ok"}"#);
    assert_eq!(repository.check_count(), 1);
}

#[tokio::test]
async fn ready_returns_opaque_not_ready_body_when_repository_fails() {
    let repository = Arc::new(FakeHealthRepository::new(false));

    let response = router(repository.clone())
        .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = body(response).await;
    assert_eq!(body, r#"{"status":"not_ready"}"#);
    assert!(!body.contains("secret"));
    assert!(!body.contains("SQL"));
    assert_eq!(repository.check_count(), 1);
}

#[tokio::test]
async fn valid_request_and_correlation_ids_are_propagated() {
    let repository = Arc::new(FakeHealthRepository::new(true));

    let response = router(repository)
        .oneshot(
            Request::get("/health/live")
                .header("x-request-id", VALID_REQUEST_ID)
                .header("x-correlation-id", VALID_CORRELATION_ID)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.headers()["x-request-id"], VALID_REQUEST_ID);
    assert_eq!(response.headers()["x-correlation-id"], VALID_CORRELATION_ID);
}

#[tokio::test]
async fn missing_or_invalid_ids_are_replaced_with_uuid_v7_values() {
    let repository = Arc::new(FakeHealthRepository::new(true));

    let response = router(repository)
        .oneshot(
            Request::get("/health/live")
                .header("x-request-id", "not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let request_id = response.headers()["x-request-id"].to_str().unwrap();
    let correlation_id = response.headers()["x-correlation-id"].to_str().unwrap();
    assert_ne!(request_id, "not-a-uuid");
    assert_eq!(
        uuid::Uuid::parse_str(request_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(
        uuid::Uuid::parse_str(correlation_id)
            .unwrap()
            .get_version_num(),
        7
    );
}
