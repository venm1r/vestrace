use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vestrace_application::{
    ApplicationError, CreateRunCommand, GrantPolicyEngine, HealthRepository,
    NullExecutionHistoryRepository, PolicyDecisionEngine, RequestContext, RunUseCases,
};
use vestrace_domain::{
    Capability, CapabilityGrant, CapabilityGrantSpec, RiskCategory,
    id::{AgentRunId, CapabilityGrantId, PrincipalId, WorkspaceId},
    now,
    run::AgentRun,
};
use vestrace_http::{AppState, build_router};

struct HealthyRepository;
struct EmptyRunUseCases;

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
            "run creation is unavailable in this test".to_owned(),
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
    async fn find_revision(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::id::MemoryRevisionId,
    ) -> Result<Option<vestrace_domain::MemoryRevision>, vestrace_application::ApplicationError>
    {
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

fn app() -> axum::Router {
    build_router(
        AppState::new(
            Arc::new(HealthyRepository),
            Arc::new(EmptyRunUseCases),
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
        .with_policy(test_policy()),
    )
}

fn test_policy() -> Arc<dyn PolicyDecisionEngine> {
    let workspace_id =
        WorkspaceId::from_uuid("00000000-0000-0000-0000-000000000001".parse().unwrap());
    let principal_id =
        PrincipalId::from_uuid("00000000-0000-0000-0000-000000000002".parse().unwrap());
    let at = now();
    let grants = [
        (
            Capability::ExecutionRead,
            "http.get",
            "/v1/runs",
            RiskCategory::Low,
        ),
        (
            Capability::ExecutionWrite,
            "http.post",
            "/ag-ui/run",
            RiskCategory::High,
        ),
        (
            Capability::AuditRead,
            "http.get",
            "/metrics",
            RiskCategory::Low,
        ),
    ]
    .into_iter()
    .map(|(capability, operation, resource_scope, risk_ceiling)| {
        CapabilityGrant::issue(
            CapabilityGrantSpec {
                id: CapabilityGrantId::new(),
                workspace_id,
                subject_id: principal_id,
                issuer_id: principal_id,
                capability,
                operation: operation.to_owned(),
                resource_scope: resource_scope.to_owned(),
                valid_from: at,
                valid_until: None,
                budget: None,
                risk_ceiling,
                conditions: Vec::new(),
            },
            at,
        )
        .unwrap()
    });
    Arc::new(GrantPolicyEngine::new("router-contract-v1", grants).unwrap())
}

#[tokio::test]
async fn v1_is_applied_exactly_once() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    assert!(matches!(
        vestrace_http::inventory_lookup_for_test(&axum::http::Method::GET, "/v1/v1/runs"),
        vestrace_http::route_inventory::RouteDecision::NotInInventory
    ));
}

#[tokio::test]
/// AG-UI creates or extends a Run through the same orchestrator `/v1/runs`
/// uses. This test's router has none configured, so the honest answer is "not
/// implemented", not a stub refusal that never looked.
async fn ag_ui_run_answers_not_implemented_when_no_orchestrator_is_configured() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ag-ui/run")
                .header("content-type", "application/json")
                .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                .body(Body::from(r#"{"message":"do the thing"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "not_implemented");
    assert!(!body.to_string().contains("do the thing"));
}

#[tokio::test]
async fn metrics_returns_prometheus_format() {
    let response = app()
        .oneshot(
            Request::get("/metrics")
                .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        body.contains("# TYPE vestrace_http_requests_total counter"),
        "metrics should contain Prometheus counter type, got: {body}"
    );
}

/// The 501 message must describe the build, not a retired phase name. "P0
/// foundation" was an earlier stabilization phase; the current roadmap names
/// phases C/L/G/H/E/T on the way to v1.0, so citing P0 tells an operator to look
/// for something that no longer exists.
#[test]
fn unimplemented_surfaces_do_not_cite_a_retired_phase_name() {
    const API_ERROR_SOURCE: &str = include_str!("../src/api/error.rs");

    assert!(
        !API_ERROR_SOURCE.contains("P0 foundation"),
        "the not-implemented message still cites the retired P0 phase"
    );
    assert!(
        API_ERROR_SOURCE.contains("is not implemented in this build"),
        "the not-implemented message should describe the build"
    );
}

/// A route with no capability mapping bypasses the authorization middleware
/// entirely, so every governed surface must map to one. Settings change how the
/// runtime behaves, which is workspace administration.
#[test]
fn settings_routes_require_workspace_administration() {
    use axum::http::Method;
    use vestrace_http::route_inventory::RouteDecision;

    assert!(matches!(
        vestrace_http::inventory_lookup_for_test(&Method::GET, "/v1/settings"),
        RouteDecision::Governed(request)
            if request.capability == vestrace_domain::Capability::WorkspaceAdmin
    ));
    assert!(matches!(
        vestrace_http::inventory_lookup_for_test(&Method::PUT, "/v1/settings"),
        RouteDecision::Governed(request)
            if request.capability == vestrace_domain::Capability::WorkspaceAdmin
    ));
}

/// Destroying a memory takes its own capability.
///
/// `DELETE /v1/memories/{id}` is the only irreversible operation on the
/// surface. Mapping it to `memory.write` would have meant every client that can
/// record a memory could also destroy one, and the local development
/// configuration — which grants `memory.write` and deliberately omits
/// `memory.purge` — would have been granting it all along.
#[test]
fn purging_a_memory_is_not_a_write() {
    use axum::http::Method;
    use vestrace_domain::Capability;
    use vestrace_http::{inventory_lookup_for_test, route_inventory::RouteDecision};

    assert!(matches!(
        inventory_lookup_for_test(
            &Method::DELETE,
            "/v1/memories/01a00000-0000-7000-8000-000000000000"
        ),
        RouteDecision::Governed(request) if request.capability == Capability::MemoryPurge
    ));
    assert!(matches!(
        inventory_lookup_for_test(&Method::POST, "/v1/memories"),
        RouteDecision::Governed(request) if request.capability == Capability::MemoryWrite
    ));
    assert!(matches!(
        inventory_lookup_for_test(
            &Method::GET,
            "/v1/memories/01a00000-0000-7000-8000-000000000000"
        ),
        RouteDecision::Governed(request) if request.capability == Capability::MemoryRead
    ));
}

/// Answering a finding is an administrative act.
///
/// `FindingDisposition` carries an actor, a policy version and an audit
/// reference precisely so a silence has an author. Routing it anywhere weaker
/// than workspace administration would let whoever can read the health surface
/// also decide that an error-severity finding should stop being reported.
#[test]
fn dispositioning_a_finding_takes_workspace_administration() {
    use axum::http::Method;
    use vestrace_domain::Capability;
    use vestrace_http::{inventory_lookup_for_test, route_inventory::RouteDecision};

    assert!(matches!(
        inventory_lookup_for_test(
            &Method::POST,
            "/v1/system/health/findings/01a00000-0000-7000-8000-000000000000/disposition"
        ),
        RouteDecision::Governed(request) if request.capability == Capability::WorkspaceAdmin
    ));
}
