use std::sync::{Arc, Mutex};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
    routing::get,
};
use tower::ServiceExt;
use vestrace_application::{
    AccessTokenAuthenticator, ApplicationError, AuthenticatedPrincipal, CreateRunCommand,
    HealthRepository, NullExecutionHistoryRepository, PolicyDecisionEngine, RequestContext,
    RunUseCases,
};
use vestrace_domain::{Capability, RiskCategory, id::AgentRunId, run::AgentRun};
use vestrace_http::{
    AppState, MetricsRegistry, build_router,
    route_inventory::{
        RouteDecision, RouteDescriptor, RouteExposure, inventory_lookup, mount, route_inventory,
        validate_route_inventory,
    },
};

struct HealthyRepository;
struct EmptyRunUseCases;
struct TestAllowPolicy;

struct WorkspaceAdminSpyPolicy {
    deny_workspace_admin: bool,
    admitted: Mutex<Vec<vestrace_domain::AuthorizationRequest>>,
}

impl WorkspaceAdminSpyPolicy {
    fn denying_workspace_admin() -> Self {
        Self {
            deny_workspace_admin: true,
            admitted: Mutex::new(Vec::new()),
        }
    }

    fn granting_workspace_admin() -> Self {
        Self {
            deny_workspace_admin: false,
            admitted: Mutex::new(Vec::new()),
        }
    }

    fn admitted(&self) -> Vec<vestrace_domain::AuthorizationRequest> {
        self.admitted.lock().unwrap().clone()
    }
}

struct RefusingAuthenticator;

#[async_trait::async_trait]
impl AccessTokenAuthenticator for RefusingAuthenticator {
    async fn resolve(&self, _: &str) -> Result<Option<AuthenticatedPrincipal>, ApplicationError> {
        Ok(None)
    }

    async fn record_use(&self, _: vestrace_domain::AccessTokenId) -> Result<(), ApplicationError> {
        Ok(())
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

#[async_trait::async_trait]
impl PolicyDecisionEngine for TestAllowPolicy {
    async fn decide(
        &self,
        context: &RequestContext,
        request: vestrace_domain::AuthorizationRequest,
    ) -> Result<vestrace_domain::PolicyDecision, ApplicationError> {
        let at = vestrace_domain::now();
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
                risk_ceiling: RiskCategory::Critical,
                conditions: request.conditions.clone(),
            },
            at,
        )?;
        Ok(vestrace_domain::evaluate_capability_grants(
            vestrace_domain::id::PolicyDecisionId::new(),
            context.workspace_id,
            context.principal_id,
            "route-inventory-test-allow-v1",
            &request,
            &[grant],
            at,
        )?)
    }
}

#[async_trait::async_trait]
impl PolicyDecisionEngine for WorkspaceAdminSpyPolicy {
    async fn decide(
        &self,
        context: &RequestContext,
        request: vestrace_domain::AuthorizationRequest,
    ) -> Result<vestrace_domain::PolicyDecision, ApplicationError> {
        let decision =
            if self.deny_workspace_admin && request.capability == Capability::WorkspaceAdmin {
                vestrace_domain::evaluate_capability_grants(
                    vestrace_domain::id::PolicyDecisionId::new(),
                    context.workspace_id,
                    context.principal_id,
                    "workspace-admin-spy-v1",
                    &request,
                    &[],
                    vestrace_domain::now(),
                )?
            } else {
                TestAllowPolicy.decide(context, request.clone()).await?
            };

        if decision.is_allowed() {
            self.admitted.lock().unwrap().push(request);
        }
        Ok(decision)
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

fn app() -> Router {
    app_with_policy(Arc::new(TestAllowPolicy))
}

fn app_with_policy(policy: Arc<dyn PolicyDecisionEngine>) -> Router {
    build_router(
        AppState::new(
            Arc::new(HealthyRepository),
            Arc::new(EmptyRunUseCases),
            Arc::new(StubMemoryUseCases),
            Arc::new(vestrace_application::RetrievalService::new(
                Arc::new(StubTextRetriever),
                Arc::new(StubRetrievalJournal),
            )),
            Arc::new(StubProviderRepository),
            Arc::new(StubModelRepository),
            Arc::new(StubAgentRepository),
            Arc::new(StubSkillRepository),
            Arc::new(StubRoutingDecisionRepository),
            Arc::new(StubModelExecutionRepository),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(MetricsRegistry::new()),
        )
        .with_policy(policy),
    )
}

fn authenticated_app() -> Router {
    build_router(
        AppState::new(
            Arc::new(HealthyRepository),
            Arc::new(EmptyRunUseCases),
            Arc::new(StubMemoryUseCases),
            Arc::new(vestrace_application::RetrievalService::new(
                Arc::new(StubTextRetriever),
                Arc::new(StubRetrievalJournal),
            )),
            Arc::new(StubProviderRepository),
            Arc::new(StubModelRepository),
            Arc::new(StubAgentRepository),
            Arc::new(StubSkillRepository),
            Arc::new(StubRoutingDecisionRepository),
            Arc::new(StubModelExecutionRepository),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(NullExecutionHistoryRepository::new()),
            Arc::new(MetricsRegistry::new()),
        )
        .with_policy(Arc::new(TestAllowPolicy))
        .with_authentication(vestrace_http::auth::Authentication::new(Arc::new(
            RefusingAuthenticator,
        ))),
    )
}

#[test]
#[should_panic(expected = "route descriptor is absent from the inventory")]
fn mounting_a_route_absent_from_the_inventory_panics() {
    let descriptor = RouteDescriptor {
        method: Method::GET,
        path_pattern: "/not-in-inventory",
        capability: Capability::AuditRead,
        risk: RiskCategory::Low,
        exposure: RouteExposure::Governed,
    };
    let router: Router<AppState> = Router::new();
    let _ = mount(
        router,
        &descriptor,
        get(|| async { StatusCode::NO_CONTENT }),
    );
}

#[tokio::test]
async fn every_inventory_entry_is_mounted() {
    for descriptor in route_inventory() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(descriptor.method.clone())
                    .uri(sample_path(descriptor.path_pattern))
                    .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                    .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        assert!(
            (status != StatusCode::NOT_FOUND || !body.is_empty())
                && status != StatusCode::METHOD_NOT_ALLOWED,
            "{} {} has an inventory entry but no mounted route",
            descriptor.method,
            descriptor.path_pattern
        );
    }
}

#[tokio::test]
async fn access_token_routes_require_workspace_administration() {
    let denied_policy = Arc::new(WorkspaceAdminSpyPolicy::denying_workspace_admin());
    for (method, path) in [
        (Method::GET, "/v1/access-tokens"),
        (Method::POST, "/v1/access-tokens"),
        (
            Method::DELETE,
            "/v1/access-tokens/01a00000-0000-7000-8000-000000000000",
        ),
        (
            Method::GET,
            "/v1/access-tokens/01a00000-0000-7000-8000-000000000000/value",
        ),
    ] {
        let response = app_with_policy(denied_policy.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                    .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"],
            "forbidden"
        );
    }

    assert!(denied_policy.admitted().is_empty());

    let admitted_policy = Arc::new(WorkspaceAdminSpyPolicy::granting_workspace_admin());
    for (method, path) in [
        (Method::GET, "/v1/access-tokens"),
        (Method::POST, "/v1/access-tokens"),
        (
            Method::DELETE,
            "/v1/access-tokens/01a00000-0000-7000-8000-000000000000",
        ),
        (
            Method::GET,
            "/v1/access-tokens/01a00000-0000-7000-8000-000000000000/value",
        ),
    ] {
        let response = app_with_policy(admitted_policy.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                    .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if path.ends_with("/value") {
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            let body = to_bytes(response.into_body(), 4096).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"],
                "access_token_value_not_readable"
            );
        } else {
            assert_ne!(response.status(), StatusCode::FORBIDDEN);
        }
    }
    assert_eq!(admitted_policy.admitted().len(), 4);
    assert!(
        admitted_policy.admitted().iter().all(|request| {
            request.capability == Capability::WorkspaceAdmin
                && request.requested_risk == RiskCategory::Critical
        }),
        "all access-token requests must require WorkspaceAdmin at Critical risk"
    );
}

#[tokio::test]
async fn unknown_governed_route_is_denied_not_bypassed() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/v1/not-a-real-surface")
                .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn top_level_metrics_is_governed() {
    assert!(matches!(
        inventory_lookup(&Method::GET, "/metrics"),
        RouteDecision::Governed(request)
            if request.capability == Capability::AuditRead && request.requested_risk == RiskCategory::Low
    ));
}

#[tokio::test]
async fn only_health_probes_are_public() {
    let public_paths: Vec<_> = route_inventory()
        .iter()
        .filter(|descriptor| descriptor.exposure == RouteExposure::PublicBounded)
        .map(|descriptor| descriptor.path_pattern)
        .collect();
    assert_eq!(public_paths, ["/health/live", "/health/ready"]);

    for path in public_paths {
        let response = authenticated_app()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert_eq!(body, r#"{"status":"ok"}"#);
    }

    let governed = authenticated_app()
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(governed.status(), StatusCode::UNAUTHORIZED);

    for method in [Method::OPTIONS, Method::POST] {
        let response = authenticated_app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/health/live")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

#[test]
#[should_panic(expected = "governed route cannot duplicate a public path pattern")]
fn governed_and_public_descriptors_cannot_share_a_path() {
    validate_route_inventory(&[
        RouteDescriptor {
            method: Method::GET,
            path_pattern: "/health/live",
            capability: Capability::AuditRead,
            risk: RiskCategory::Low,
            exposure: RouteExposure::PublicBounded,
        },
        RouteDescriptor {
            method: Method::POST,
            path_pattern: "/health/live",
            capability: Capability::AuditRead,
            risk: RiskCategory::Low,
            exposure: RouteExposure::Governed,
        },
    ]);
}

#[test]
#[should_panic(expected = "only health probes may be public")]
fn public_exception_cannot_be_widened() {
    validate_route_inventory(&[RouteDescriptor {
        method: Method::GET,
        path_pattern: "/metrics",
        capability: Capability::AuditRead,
        risk: RiskCategory::Low,
        exposure: RouteExposure::PublicBounded,
    }]);
}

fn sample_path(path_pattern: &str) -> String {
    let mut path = String::with_capacity(path_pattern.len());
    let mut segment = path_pattern.split('/');
    if path_pattern.starts_with('/') {
        path.push('/');
    }
    let _ = segment.next();
    for (index, value) in segment.enumerate() {
        if index > 0 {
            path.push('/');
        }
        if value.starts_with('{') && value.ends_with('}') {
            path.push_str("01a00000-0000-7000-8000-000000000000");
        } else {
            path.push_str(value);
        }
    }
    path
}
