use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use tracing::{Subscriber, field::Visit, span::Attributes};
use tracing_subscriber::{Layer, layer::Context, prelude::*};
use vestrace_application::{
    ApplicationError, CreateRunCommand, HealthRepository, NullExecutionHistoryRepository,
    RequestContext, RunCommandExecutor, RunCommandResult, RunUseCases,
};
use vestrace_domain::{
    id::AgentRunId,
    run::{AgentRun, RunCommandEnvelope},
};
use vestrace_http::{AppState, build_router};

const INVALID_REQUEST_ID: &str = "invalid-request-id-secret";
const INVALID_CORRELATION_ID: &str = "invalid-correlation-id-secret";
const SECRET_PATH: &str = "/customers/path-secret-7731";
const BODY_SECRET: &str = "request-body-secret-1882";
const AUTH_SECRET: &str = "Bearer authorization-secret-9642";
const COOKIE_SECRET: &str = "session=cookie-secret-5104";
const HEADER_SECRET: &str = "arbitrary-header-secret-2267";

struct HealthyRepository;
struct EmptyRunUseCases;
struct StubRunCommandExecutor;

#[async_trait::async_trait]
impl RunCommandExecutor for StubRunCommandExecutor {
    async fn execute(
        &self,
        _context: &RequestContext,
        _command: RunCommandEnvelope,
    ) -> Result<RunCommandResult, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "run command execution is unavailable in request span tests".to_owned(),
        ))
    }
}

#[async_trait::async_trait]
impl HealthRepository for HealthyRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        Ok(())
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
            "run creation is unavailable in request span tests".to_owned(),
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

#[derive(Clone, Default)]
struct RequestSpanCapture(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

impl RequestSpanCapture {
    fn spans(&self) -> Vec<BTreeMap<String, String>> {
        self.0.lock().unwrap().clone()
    }
}

impl<S> Layer<S> for RequestSpanCapture
where
    S: Subscriber,
{
    fn on_new_span(
        &self,
        attributes: &Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        if attributes.metadata().name() != "http_request" {
            return;
        }

        let mut fields = BTreeMap::new();
        attributes.record(&mut FieldVisitor(&mut fields));
        self.0.lock().unwrap().push(fields);
    }
}

struct FieldVisitor<'fields>(&'fields mut BTreeMap<String, String>);

impl Visit for FieldVisitor<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
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

#[test]
fn request_span_contains_only_sanitized_bounded_metadata() {
    let capture = RequestSpanCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let response = tracing::subscriber::with_default(subscriber, || {
        runtime.block_on(
            build_router(AppState::new(
                Arc::new(HealthyRepository),
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
            .oneshot(
                Request::get(SECRET_PATH)
                    .header("x-request-id", INVALID_REQUEST_ID)
                    .header("x-correlation-id", INVALID_CORRELATION_ID)
                    .header("authorization", AUTH_SECRET)
                    .header("cookie", COOKIE_SECRET)
                    .header("x-arbitrary", HEADER_SECRET)
                    .body(Body::from(BODY_SECRET))
                    .unwrap(),
            ),
        )
    })
    .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let request_id = response.headers()["x-request-id"].to_str().unwrap();
    let correlation_id = response.headers()["x-correlation-id"].to_str().unwrap();
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

    let spans = capture.spans();
    assert_eq!(spans.len(), 1, "{spans:?}");
    let span = &spans[0];
    assert_eq!(span.get("request_id").unwrap(), request_id);
    assert_eq!(span.get("correlation_id").unwrap(), correlation_id);
    assert_eq!(span.get("route").unwrap(), "unmatched");

    let recorded = format!("{span:?}");
    for secret in [
        INVALID_REQUEST_ID,
        INVALID_CORRELATION_ID,
        SECRET_PATH,
        BODY_SECRET,
        AUTH_SECRET,
        COOKIE_SECRET,
        HEADER_SECRET,
    ] {
        assert!(
            !recorded.contains(secret),
            "span leaked {secret}: {recorded}"
        );
    }
}
