use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Method, Request},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use tracing::Instrument;
use uuid::Uuid;
use vestrace_application::{
    AgentRepository, ArtifactRepository, AuditRepository, AuthorizationBoundary,
    ConnectionRepository, DenyAllPolicyEngine, EvaluationRepository, ExecutionHistoryRepository,
    HealthInspectionService, HealthMonitorService, HealthRepository, MemoryUseCases,
    ModelExecutionRepository, ModelRepository, ProviderRepository, RetrievalService,
    RoutingDecisionRepository, RunCommandExecutor, RunUseCases, SharedAgentRepository,
    SharedArtifactRepository, SharedAuditRepository, SharedCapabilityGrantRepository,
    SharedConnectionRepository, SharedEvaluationRepository, SharedExecutionHistoryRepository,
    SharedHealthFindingRepository, SharedInvariantObserver, SharedLearningRepository,
    SharedMemoryUseCases, SharedModelExecutionRepository, SharedModelRepository,
    SharedPolicyDecisionEngine, SharedProviderRepository, SharedRoutingDecisionRepository,
    SharedRunCommandExecutor, SharedRunUseCases, SharedRuntimeEvidenceProvider,
    SharedSkillRepository, SharedTriggerRepository, SharedWorkflowRepository,
    SharedWorkspaceCountsProvider, SharedWorkspaceSettingsRepository, SkillRepository,
    TriggerRepository, UnavailableLearningRepository, WorkflowRepository, WorkspaceSettingsService,
};
use vestrace_domain::{AuthorizationRequest, Capability, RiskCategory};

use crate::{health, metrics::MetricsRegistry};

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
const CORRELATION_ID_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

#[derive(Clone)]
pub struct AppState {
    health_repository: Arc<dyn HealthRepository>,
    run_use_cases: SharedRunUseCases,
    memory_use_cases: SharedMemoryUseCases,
    retrieval_service: Arc<RetrievalService>,
    provider_repository: SharedProviderRepository,
    model_repository: SharedModelRepository,
    agent_repository: SharedAgentRepository,
    skill_repository: SharedSkillRepository,
    routing_decision_repository: SharedRoutingDecisionRepository,
    model_execution_repository: SharedModelExecutionRepository,
    execution_history_repository: SharedExecutionHistoryRepository,
    workflow_repository: SharedWorkflowRepository,
    evaluation_repository: SharedEvaluationRepository,
    learning_repository: SharedLearningRepository,
    metrics_registry: Arc<MetricsRegistry>,
    authorization_boundary: AuthorizationBoundary,
    /// Optional so existing constructors keep working; the surface answers 501
    /// until a deployment supplies storage for it.
    workspace_settings: Option<Arc<WorkspaceSettingsService>>,
    audit_repository: Option<SharedAuditRepository>,
    health_monitor: Option<Arc<HealthMonitorService>>,
    purge_use_case: Option<vestrace_application::SharedPurgeUseCase>,
    external_effects: Option<Arc<vestrace_application::PerformExternalEffectService>>,
    effect_adapters: std::collections::BTreeMap<
        String,
        Arc<dyn vestrace_domain::external_effects::ExternalEffectAdapter>,
    >,
    capability_grants: Option<SharedCapabilityGrantRepository>,
    runtime_evidence: Option<SharedRuntimeEvidenceProvider>,
    workspace_counts: Option<SharedWorkspaceCountsProvider>,
    artifact_repository: Option<SharedArtifactRepository>,
    trigger_repository: Option<SharedTriggerRepository>,
    connection_repository: Option<SharedConnectionRepository>,
    secret_store: Option<vestrace_application::SharedSecretStore>,
    access_token_store: Option<vestrace_application::SharedAccessTokenStore>,
    token_entropy_source: Option<vestrace_application::SharedTokenEntropySource>,
    ag_ui: Option<vestrace_application::SharedAgUiRepository>,
    run_orchestrator: Option<vestrace_application::run::SharedRunOrchestrator>,
    authentication: Option<crate::auth::Authentication>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        health_repository: Arc<dyn HealthRepository>,
        run_use_cases: SharedRunUseCases,
        memory_use_cases: SharedMemoryUseCases,
        retrieval_service: Arc<RetrievalService>,
        provider_repository: SharedProviderRepository,
        model_repository: SharedModelRepository,
        agent_repository: SharedAgentRepository,
        skill_repository: SharedSkillRepository,
        routing_decision_repository: SharedRoutingDecisionRepository,
        model_execution_repository: SharedModelExecutionRepository,
        execution_history_repository: SharedExecutionHistoryRepository,
        workflow_repository: SharedWorkflowRepository,
        evaluation_repository: SharedEvaluationRepository,
        metrics_registry: Arc<MetricsRegistry>,
    ) -> Self {
        Self::new_with_learning(
            health_repository,
            run_use_cases,
            memory_use_cases,
            retrieval_service,
            provider_repository,
            model_repository,
            agent_repository,
            skill_repository,
            routing_decision_repository,
            model_execution_repository,
            execution_history_repository,
            workflow_repository,
            evaluation_repository,
            Arc::new(UnavailableLearningRepository),
            metrics_registry,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_learning(
        health_repository: Arc<dyn HealthRepository>,
        run_use_cases: SharedRunUseCases,
        memory_use_cases: SharedMemoryUseCases,
        retrieval_service: Arc<RetrievalService>,
        provider_repository: SharedProviderRepository,
        model_repository: SharedModelRepository,
        agent_repository: SharedAgentRepository,
        skill_repository: SharedSkillRepository,
        routing_decision_repository: SharedRoutingDecisionRepository,
        model_execution_repository: SharedModelExecutionRepository,
        execution_history_repository: SharedExecutionHistoryRepository,
        workflow_repository: SharedWorkflowRepository,
        evaluation_repository: SharedEvaluationRepository,
        learning_repository: SharedLearningRepository,
        metrics_registry: Arc<MetricsRegistry>,
    ) -> Self {
        Self::new_with_learning_and_policy(
            health_repository,
            run_use_cases,
            memory_use_cases,
            retrieval_service,
            provider_repository,
            model_repository,
            agent_repository,
            skill_repository,
            routing_decision_repository,
            model_execution_repository,
            execution_history_repository,
            workflow_repository,
            evaluation_repository,
            learning_repository,
            metrics_registry,
            Arc::new(DenyAllPolicyEngine),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_learning_and_policy(
        health_repository: Arc<dyn HealthRepository>,
        run_use_cases: SharedRunUseCases,
        memory_use_cases: SharedMemoryUseCases,
        retrieval_service: Arc<RetrievalService>,
        provider_repository: SharedProviderRepository,
        model_repository: SharedModelRepository,
        agent_repository: SharedAgentRepository,
        skill_repository: SharedSkillRepository,
        routing_decision_repository: SharedRoutingDecisionRepository,
        model_execution_repository: SharedModelExecutionRepository,
        execution_history_repository: SharedExecutionHistoryRepository,
        workflow_repository: SharedWorkflowRepository,
        evaluation_repository: SharedEvaluationRepository,
        learning_repository: SharedLearningRepository,
        metrics_registry: Arc<MetricsRegistry>,
        policy_engine: SharedPolicyDecisionEngine,
    ) -> Self {
        Self {
            health_repository,
            run_use_cases,
            memory_use_cases,
            retrieval_service,
            provider_repository,
            model_repository,
            agent_repository,
            skill_repository,
            routing_decision_repository,
            model_execution_repository,
            execution_history_repository,
            workflow_repository,
            evaluation_repository,
            learning_repository,
            metrics_registry,
            workspace_settings: None,
            audit_repository: None,
            health_monitor: None,
            purge_use_case: None,
            external_effects: None,
            effect_adapters: std::collections::BTreeMap::new(),
            capability_grants: None,
            runtime_evidence: None,
            workspace_counts: None,
            artifact_repository: None,
            trigger_repository: None,
            connection_repository: None,
            secret_store: None,
            access_token_store: None,
            token_entropy_source: None,
            ag_ui: None,
            run_orchestrator: None,
            authentication: None,
            authorization_boundary: AuthorizationBoundary::new(policy_engine),
        }
    }

    pub fn with_policy(mut self, policy_engine: SharedPolicyDecisionEngine) -> Self {
        self.authorization_boundary = AuthorizationBoundary::new(policy_engine);
        self
    }

    pub(crate) fn health_repository(&self) -> &dyn HealthRepository {
        self.health_repository.as_ref()
    }

    pub(crate) fn run_use_cases(&self) -> &dyn RunUseCases {
        self.run_use_cases.as_ref()
    }

    // `run_command_executor` was removed with the legacy write path. It wrote
    // `agent_runs` and `run_events` without `run_steps` or `run_work_items`, so
    // a run created through it was invisible to the worker. Keeping the
    // accessor would have made it cheap to reintroduce a second writer to the
    // same aggregate.

    /// Supply the durable run coordinator.
    ///
    /// This is the authoritative write path for runs. Until it was wired in,
    /// `/v1/runs` drove a second mechanism that wrote `agent_runs` and
    /// `run_events` but never `run_steps` or `run_work_items`, so a run created
    /// through the API was invisible to the worker.
    pub fn with_run_orchestrator(
        mut self,
        orchestrator: vestrace_application::run::SharedRunOrchestrator,
    ) -> Self {
        self.run_orchestrator = Some(orchestrator);
        self
    }

    pub fn run_orchestrator(
        &self,
    ) -> Result<&vestrace_application::run::SharedRunOrchestrator, crate::api::ApiError> {
        self.run_orchestrator
            .as_ref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("run orchestration"))
    }

    pub(crate) fn memory_use_cases(&self) -> &dyn MemoryUseCases {
        self.memory_use_cases.as_ref()
    }

    pub(crate) fn retrieval_service(&self) -> &RetrievalService {
        &self.retrieval_service
    }

    pub(crate) fn provider_repository(&self) -> &dyn ProviderRepository {
        self.provider_repository.as_ref()
    }

    pub(crate) fn model_repository(&self) -> &dyn ModelRepository {
        self.model_repository.as_ref()
    }

    pub(crate) fn agent_repository(&self) -> &dyn AgentRepository {
        self.agent_repository.as_ref()
    }

    pub(crate) fn skill_repository(&self) -> &dyn SkillRepository {
        self.skill_repository.as_ref()
    }

    pub(crate) fn routing_decision_repository(&self) -> &dyn RoutingDecisionRepository {
        self.routing_decision_repository.as_ref()
    }

    pub(crate) fn model_execution_repository(&self) -> &dyn ModelExecutionRepository {
        self.model_execution_repository.as_ref()
    }

    pub(crate) fn execution_history_repository(&self) -> &dyn ExecutionHistoryRepository {
        self.execution_history_repository.as_ref()
    }

    pub(crate) fn workflow_repository(&self) -> &dyn WorkflowRepository {
        self.workflow_repository.as_ref()
    }

    pub(crate) fn evaluation_repository(&self) -> &dyn EvaluationRepository {
        self.evaluation_repository.as_ref()
    }

    pub(crate) fn learning_repository(&self) -> &dyn vestrace_application::LearningRepository {
        self.learning_repository.as_ref()
    }

    /// Supply durable settings storage. Without it the settings surface answers
    /// 501 like any other unimplemented route.
    pub fn with_workspace_settings(
        mut self,
        repository: SharedWorkspaceSettingsRepository,
    ) -> Self {
        self.workspace_settings = Some(Arc::new(WorkspaceSettingsService::new(repository)));
        self
    }

    pub fn workspace_settings_service(
        &self,
    ) -> Result<&WorkspaceSettingsService, crate::api::ApiError> {
        self.workspace_settings
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("workspace settings API"))
    }

    /// Supply envelope-encrypted secret storage. Without a master key there is
    /// no store, and the surface answers 501 rather than degrading to plaintext.
    pub fn with_access_token_store(
        mut self,
        store: vestrace_application::SharedAccessTokenStore,
    ) -> Self {
        self.access_token_store = Some(store);
        self
    }

    pub fn access_token_store(
        &self,
    ) -> Result<&dyn vestrace_application::AccessTokenStore, crate::api::ApiError> {
        self.access_token_store
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("access token API"))
    }

    pub fn with_token_entropy_source(
        mut self,
        source: vestrace_application::SharedTokenEntropySource,
    ) -> Self {
        self.token_entropy_source = Some(source);
        self
    }

    pub fn token_entropy_source(
        &self,
    ) -> Result<&dyn vestrace_application::TokenEntropySource, crate::api::ApiError> {
        self.token_entropy_source
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("access token API"))
    }

    pub fn with_secret_store(mut self, store: vestrace_application::SharedSecretStore) -> Self {
        self.secret_store = Some(store);
        self
    }

    pub fn secret_store(
        &self,
    ) -> Result<&dyn vestrace_application::SecretStore, crate::api::ApiError> {
        self.secret_store
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("secret storage API"))
    }

    /// Supply the AG-UI endpoint registry and run-event reader.
    pub fn with_ag_ui(mut self, repository: vestrace_application::SharedAgUiRepository) -> Self {
        self.ag_ui = Some(repository);
        self
    }

    pub fn ag_ui_repository(
        &self,
    ) -> Result<&vestrace_application::SharedAgUiRepository, crate::api::ApiError> {
        self.ag_ui
            .as_ref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("AG-UI"))
    }

    /// Supply the append-only audit trail. Without it the audit surface answers
    /// 501 like any other unimplemented route.
    pub fn with_audit_repository(mut self, repository: SharedAuditRepository) -> Self {
        self.audit_repository = Some(repository);
        self
    }

    pub fn audit_repository(&self) -> Result<&dyn AuditRepository, crate::api::ApiError> {
        self.audit_repository
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("audit API"))
    }

    /// Supply the invariant observer and environment-observation ports behind
    /// the system health surface.
    ///
    /// The observer measures; the registry inside [`HealthInspectionService`]
    /// decides what each measurement means. Before this there was no registry
    /// and the adapter decided both.
    pub fn with_system_health(
        mut self,
        observer: SharedInvariantObserver,
        findings: SharedHealthFindingRepository,
        runtime_evidence: SharedRuntimeEvidenceProvider,
    ) -> Self {
        self.health_monitor = Some(Arc::new(HealthMonitorService::new(
            observer,
            findings,
            HealthInspectionService::new(vestrace_application::standard_invariants()),
        )));
        self.runtime_evidence = Some(runtime_evidence);
        self
    }

    /// Supply the purge use case, which is the only way a memory can be
    /// destroyed.
    ///
    /// Without it `DELETE /v1/memories/{id}` answers 501 rather than pretending
    /// to delete, which is what the surface did before it existed: nothing.
    pub fn with_purge(mut self, purge: vestrace_application::SharedPurgeUseCase) -> Self {
        self.purge_use_case = Some(purge);
        self
    }

    pub(crate) fn purge_use_case(
        &self,
    ) -> Result<&dyn vestrace_application::PurgeUseCase, crate::api::ApiError> {
        self.purge_use_case
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("memory purge API"))
    }

    /// Supply the external effect path: the service that records an intent
    /// before anything leaves the process, and the adapters that can perform
    /// one.
    ///
    /// Without both, `POST /v1/effects` answers 501. An adapter registered here
    /// is one a deployment has configured a destination for; a caller names it
    /// and never a URL.
    pub fn with_external_effects(
        mut self,
        service: Arc<vestrace_application::PerformExternalEffectService>,
        adapters: Vec<Arc<dyn vestrace_domain::external_effects::ExternalEffectAdapter>>,
    ) -> Self {
        self.effect_adapters = adapters
            .into_iter()
            .map(|adapter| (adapter.descriptor().name().to_owned(), adapter))
            .collect();
        self.external_effects = Some(service);
        self
    }

    pub(crate) fn external_effects(
        &self,
    ) -> Result<&vestrace_application::PerformExternalEffectService, crate::api::ApiError> {
        self.external_effects
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("external effect API"))
    }

    pub(crate) fn effect_adapter(
        &self,
        name: &str,
    ) -> Result<
        Arc<dyn vestrace_domain::external_effects::ExternalEffectAdapter>,
        crate::api::ApiError,
    > {
        self.effect_adapters.get(name).cloned().ok_or_else(|| {
            crate::api::ApiError::bad_request(format!(
                "no external effect adapter named `{name}` is configured; a caller names an                  adapter and never a destination"
            ))
        })
    }

    /// Supply the durable capability grant store.
    ///
    /// Without it the grant surface answers 501 and the only engines available
    /// are the deny-all default and the configured static list.
    pub fn with_capability_grants(mut self, grants: SharedCapabilityGrantRepository) -> Self {
        self.capability_grants = Some(grants);
        self
    }

    pub(crate) fn capability_grants(
        &self,
    ) -> Result<&dyn vestrace_application::CapabilityGrantRepository, crate::api::ApiError> {
        self.capability_grants
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("capability grant API"))
    }

    pub(crate) fn health_monitor(&self) -> Result<&HealthMonitorService, crate::api::ApiError> {
        self.health_monitor
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("system health API"))
    }

    pub async fn runtime_evidence(
        &self,
    ) -> Result<
        vestrace_application::RuntimeQualificationEvidence,
        vestrace_application::ApplicationError,
    > {
        let provider = self.runtime_evidence.as_ref().ok_or_else(|| {
            vestrace_application::ApplicationError::Internal(
                "runtime evidence provider is not configured".to_owned(),
            )
        })?;
        provider.runtime_evidence().await
    }

    /// Supply aggregate counts for the metrics summary.
    pub fn with_workspace_counts(mut self, provider: SharedWorkspaceCountsProvider) -> Self {
        self.workspace_counts = Some(provider);
        self
    }

    pub async fn workspace_counts(
        &self,
        context: &vestrace_application::RequestContext,
    ) -> Result<vestrace_application::WorkspaceCounts, vestrace_application::ApplicationError> {
        let provider = self.workspace_counts.as_ref().ok_or_else(|| {
            vestrace_application::ApplicationError::Internal(
                "workspace counts provider is not configured".to_owned(),
            )
        })?;
        provider.workspace_counts(context).await
    }

    /// Supply the artifact registry. Without it the artifact surface answers
    /// 501 like any other unimplemented route.
    pub fn with_artifact_repository(mut self, repository: SharedArtifactRepository) -> Self {
        self.artifact_repository = Some(repository);
        self
    }

    pub fn artifact_repository(&self) -> Result<&dyn ArtifactRepository, crate::api::ApiError> {
        self.artifact_repository
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("artifact API"))
    }

    /// Supply the trigger registry.
    pub fn with_trigger_repository(mut self, repository: SharedTriggerRepository) -> Self {
        self.trigger_repository = Some(repository);
        self
    }

    pub fn trigger_repository(&self) -> Result<&dyn TriggerRepository, crate::api::ApiError> {
        self.trigger_repository
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("trigger API"))
    }

    /// Supply the connection registry. Credentials are not part of it.
    pub fn with_connection_repository(mut self, repository: SharedConnectionRepository) -> Self {
        self.connection_repository = Some(repository);
        self
    }

    pub fn connection_repository(&self) -> Result<&dyn ConnectionRepository, crate::api::ApiError> {
        self.connection_repository
            .as_deref()
            .ok_or_else(|| crate::api::ApiError::not_implemented("connection API"))
    }

    /// Enable credential-backed authentication. Without it the server keeps
    /// trusting identity headers, which is not authentication.
    pub fn with_authentication(mut self, authentication: crate::auth::Authentication) -> Self {
        self.authentication = Some(authentication);
        self
    }

    pub fn is_authenticated(&self) -> bool {
        self.authentication.is_some()
    }

    pub fn metrics_registry(&self) -> &MetricsRegistry {
        &self.metrics_registry
    }

    pub(crate) fn authorization_boundary(&self) -> &AuthorizationBoundary {
        &self.authorization_boundary
    }
}

pub fn build_router(state: AppState) -> Router {
    let authentication = state.authentication.clone();
    let router = Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .route("/metrics", get(crate::metrics::metrics_handler))
        .nest("/v1", crate::api::api_routes())
        .nest("/ag-ui", crate::api::ag_ui::ag_ui_routes())
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state,
            authorize_http_request,
        ))
        .layer(middleware::from_fn(add_request_context));

    // Authentication runs outermost so an unauthenticated request never reaches
    // authorization, and so the identity headers authorization reads have
    // already been replaced with the ones the credential resolved to.
    match authentication {
        Some(authentication) => router.layer(middleware::from_fn(
            move |request: Request<Body>, next: Next| {
                let authentication = authentication.clone();
                async move { crate::auth::authenticate(authentication, request, next).await }
            },
        )),
        None => router,
    }
}

async fn authorize_http_request(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(authorization_request) =
        http_authorization_request(request.method(), request.uri().path())
    else {
        return next.run(request).await;
    };

    let context = match crate::api::context::request_context(request.headers()) {
        Ok(context) => context,
        Err(error) => return error.into_response(),
    };

    match state
        .authorization_boundary()
        .require(&context, authorization_request)
        .await
    {
        Ok(_) => next.run(request).await,
        Err(error) => crate::api::ApiError::from_application(error).into_response(),
    }
}

fn http_authorization_request(method: &Method, path: &str) -> Option<AuthorizationRequest> {
    let capability = http_capability(method, path)?;
    Some(AuthorizationRequest::new(
        capability,
        format!("http.{}", method.as_str().to_ascii_lowercase()),
        path,
        http_risk(method, path),
    ))
}

fn http_risk(method: &Method, path: &str) -> RiskCategory {
    if path.ends_with("/approve") {
        return RiskCategory::Critical;
    }
    // Writing or removing a credential is critical whatever the verb: a wrong
    // value silently breaks every consumer of that secret, and a deletion is
    // not recoverable from within the system.
    if path.starts_with("/v1/secrets") && *method != Method::GET && *method != Method::HEAD {
        return RiskCategory::Critical;
    }
    // Destroying a memory is irreversible and leaves no superseded revision to
    // read back. Approving a run is already critical here; erasing the record a
    // run might have been approved on the basis of is not less so.
    if path.starts_with("/v1/memories") && *method == Method::DELETE {
        return RiskCategory::Critical;
    }
    // Acting outside the process is not a write like the others: nothing here
    // can undo it.
    if path.starts_with("/v1/effects") {
        return RiskCategory::Critical;
    }
    if *method == Method::GET || *method == Method::HEAD {
        return RiskCategory::Low;
    }
    if path.starts_with("/ag-ui/")
        || path.starts_with("/v1/runs")
        || path.starts_with("/v1/executions")
        || path.starts_with("/v1/workflow-executions")
    {
        RiskCategory::High
    } else {
        RiskCategory::Medium
    }
}

/// Exposed for contract tests: a route with no mapping here bypasses the
/// authorization middleware entirely.
pub fn http_capability_for_test(method: &Method, path: &str) -> Option<Capability> {
    http_capability(method, path)
}

fn http_capability(method: &Method, path: &str) -> Option<Capability> {
    let relative = if let Some(path) = path.strip_prefix("/v1/") {
        path
    } else if matches!(
        path,
        "/ag-ui/endpoints" | "/ag-ui/events/stream" | "/ag-ui/run"
    ) {
        return Some(if *method == Method::GET {
            Capability::ExecutionRead
        } else {
            Capability::ExecutionWrite
        });
    } else {
        return None;
    };

    let base = relative.split('/').next()?;
    let read = *method == Method::GET || *method == Method::HEAD;
    Some(match base {
        "runs" => {
            if relative.ends_with("/pause")
                || relative.ends_with("/resume")
                || relative.ends_with("/cancel")
                || relative.ends_with("/approve")
            {
                Capability::ExecutionWrite
            } else if read {
                Capability::ExecutionRead
            } else {
                Capability::ExecutionWrite
            }
        }
        "events" => Capability::EventWrite,
        "memories" => {
            if read {
                Capability::MemoryRead
            } else if *method == Method::DELETE {
                // Destroying a memory is not a write. It takes its own
                // capability, which the local development configuration
                // pointedly does not grant, so the surface is closed until
                // somebody issues the grant deliberately.
                Capability::MemoryPurge
            } else {
                Capability::MemoryWrite
            }
        }
        "retrieval" => Capability::ContextRetrieve,
        // An external effect leaves the process and cannot be recalled.
        "effects" => Capability::ExecutionWrite,
        // Settings change how the runtime behaves for the whole workspace.
        "settings" => Capability::WorkspaceAdmin,
        "agents" => {
            if read {
                Capability::AgentRead
            } else {
                Capability::AgentWrite
            }
        }
        "skills" => {
            if read {
                Capability::SkillRead
            } else {
                Capability::SkillWrite
            }
        }
        "workflows" => {
            if read {
                Capability::WorkflowRead
            } else {
                Capability::WorkflowWrite
            }
        }
        "workflow-executions" | "executions" => {
            if read {
                Capability::ExecutionRead
            } else {
                Capability::ExecutionWrite
            }
        }
        "models" => {
            if read {
                Capability::ModelRead
            } else {
                Capability::ModelWrite
            }
        }
        "providers" => {
            if read {
                Capability::ProviderRead
            } else {
                Capability::ProviderWrite
            }
        }
        "routing" => {
            if read {
                Capability::ModelRead
            } else {
                Capability::ModelWrite
            }
        }
        "evaluations" | "evaluation-facts" => {
            if read {
                Capability::EvaluationRead
            } else {
                Capability::EvaluationWrite
            }
        }
        "learning" => {
            if read {
                Capability::LearningRead
            } else {
                Capability::LearningWrite
            }
        }
        "artifacts" => Capability::ExportRead,
        "audit" => Capability::AuditRead,
        "metrics" => Capability::AuditRead,
        // Issuing, listing or revoking authority is the most consequential
        // administrative act there is, so it takes the administrative
        // capability rather than one of its own.
        "capability-grants" => Capability::WorkspaceAdmin,
        "triggers" | "connections" | "system" | "profile" | "secrets" => Capability::WorkspaceAdmin,
        _ => return None,
    })
}

async fn add_request_context(mut request: Request<Body>, next: Next) -> Response {
    let request_id = validated_id(request.headers(), &REQUEST_ID_HEADER);
    let correlation_id = validated_id(request.headers(), &CORRELATION_ID_HEADER);
    let request_id_value = header_value(request_id);
    let correlation_id_value = header_value(correlation_id);

    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id_value.clone());
    request
        .headers_mut()
        .insert(CORRELATION_ID_HEADER, correlation_id_value.clone());
    let route = match request.uri().path() {
        "/health/live" => "/health/live",
        "/health/ready" => "/health/ready",
        p if p.starts_with("/v1") || p.starts_with("/ag-ui") => "api",
        "/metrics" => "/metrics",
        _ => "unmatched",
    };

    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        correlation_id = %correlation_id,
        method = %request.method(),
        route,
    );
    let mut response = next.run(request).instrument(span).await;
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id_value);
    response
        .headers_mut()
        .insert(CORRELATION_ID_HEADER, correlation_id_value);
    response
}

fn validated_id(headers: &HeaderMap, name: &HeaderName) -> Uuid {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|value| value.get_version() == Some(uuid::Version::SortRand))
        .unwrap_or_else(Uuid::now_v7)
}

fn header_value(id: Uuid) -> HeaderValue {
    HeaderValue::from_str(&id.to_string()).expect("a UUID is always a valid HTTP header value")
}

#[cfg(test)]
mod tests {
    use super::{http_authorization_request, http_capability};
    use axum::http::Method;
    use vestrace_domain::{Capability, RiskCategory};

    #[test]
    fn http_mapping_preserves_operation_risk() {
        let read = http_authorization_request(&Method::GET, "/v1/runs").unwrap();
        assert_eq!(read.capability, Capability::ExecutionRead);
        assert_eq!(read.requested_risk, RiskCategory::Low);

        let write = http_authorization_request(&Method::POST, "/v1/runs").unwrap();
        assert_eq!(write.capability, Capability::ExecutionWrite);
        assert_eq!(write.requested_risk, RiskCategory::High);

        let approval = http_authorization_request(&Method::POST, "/v1/runs/id/approve").unwrap();
        assert_eq!(approval.requested_risk, RiskCategory::Critical);
    }

    #[test]
    fn unknown_transport_path_is_not_promoted_to_a_governed_action() {
        assert!(http_capability(&Method::GET, "/v1/v1/runs").is_none());
        assert!(http_capability(&Method::GET, "/ag-ui/unknown").is_none());
        assert!(http_capability(&Method::GET, "/unmatched").is_none());
    }
}

