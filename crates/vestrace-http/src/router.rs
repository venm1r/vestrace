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
    ModelExecutionRepository, ModelRepository, RetrievalService, RoutingDecisionRepository,
    RunUseCases, SharedAgentRepository, SharedArtifactRepository, SharedAuditRepository,
    SharedCapabilityGrantRepository, SharedConnectionRepository,
    SharedConnectionRevisionRepository, SharedEvaluationRepository,
    SharedExecutionHistoryRepository, SharedHealthFindingRepository, SharedInvariantObserver,
    SharedLearningRepository, SharedMemoryUseCases, SharedModelExecutionRepository,
    SharedModelRepository, SharedModelRevisionRepository, SharedPolicyDecisionEngine,
    SharedProviderRepository, SharedRoutingDecisionRepository, SharedRunUseCases,
    SharedRuntimeEvidenceProvider, SharedSkillRepository, SharedTriggerRepository,
    SharedWorkflowRepository, SharedWorkspaceCountsProvider, SharedWorkspaceSettingsRepository,
    SkillRepository, TriggerRepository, UnavailableLearningRepository, WorkflowRepository,
    WorkspaceSettingsService,
};

use crate::{
    health,
    metrics::MetricsRegistry,
    route_inventory::{RouteDecision, inventory_lookup, mount, route_descriptor},
};

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
const CORRELATION_ID_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

#[derive(Clone)]
pub struct AppState {
    health_repository: Arc<dyn HealthRepository>,
    run_use_cases: SharedRunUseCases,
    memory_use_cases: SharedMemoryUseCases,
    retrieval_service: Arc<RetrievalService>,
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
    connection_revision_repository: Option<SharedConnectionRevisionRepository>,
    model_revision_repository: Option<SharedModelRevisionRepository>,
    qualification_job_repository:
        Option<std::sync::Arc<dyn vestrace_application::QualificationJobRepository>>,
    credential_activation_repository:
        Option<std::sync::Arc<dyn vestrace_application::CredentialActivationRepository>>,
    embedding_job_repository: Option<vestrace_application::SharedEmbeddingJobRepository>,
    embedding_retrieval_repository:
        Option<vestrace_application::embedding::SharedEmbeddingRetrievalRepository>,
    embedding_transition_repository:
        Option<vestrace_application::SharedEmbeddingTransitionRepository>,
    secret_store: Option<vestrace_application::SharedSecretStore>,
    access_token_store: Option<vestrace_application::SharedAccessTokenStore>,
    access_token_mutation_repository: Option<
        vestrace_application::SharedGovernedMutationRepository<
            vestrace_application::AccessTokenMutation,
        >,
    >,
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
        _provider_repository: SharedProviderRepository,
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
            _provider_repository,
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
        _provider_repository: SharedProviderRepository,
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
            _provider_repository,
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
        _provider_repository: SharedProviderRepository,
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
            connection_revision_repository: None,
            model_revision_repository: None,
            qualification_job_repository: None,
            credential_activation_repository: None,
            embedding_job_repository: None,
            embedding_retrieval_repository: None,
            embedding_transition_repository: None,
            secret_store: None,
            access_token_store: None,
            access_token_mutation_repository: None,
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

    pub fn access_token_store_handle(
        &self,
    ) -> Result<vestrace_application::SharedAccessTokenStore, crate::api::ApiError> {
        self.access_token_store
            .clone()
            .ok_or_else(|| crate::api::ApiError::not_implemented("access token API"))
    }

    /// Supply the transaction authority used by credential minting. Without
    /// it, creating a credential is unavailable rather than risking a token
    /// record without its audit event.
    pub fn with_access_token_mutation_repository(
        mut self,
        repository: vestrace_application::SharedGovernedMutationRepository<
            vestrace_application::AccessTokenMutation,
        >,
    ) -> Self {
        self.access_token_mutation_repository = Some(repository);
        self
    }

    pub fn access_token_mutation_repository(
        &self,
    ) -> Result<
        &dyn vestrace_application::GovernedMutationRepository<
            vestrace_application::AccessTokenMutation,
        >,
        crate::api::ApiError,
    > {
        self.access_token_mutation_repository
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

    /// Supply the only readable governed Connection projection authority.
    /// Absence fails closed instead of falling back to the legacy registry.
    pub fn with_connection_revision_repository(
        mut self,
        repository: SharedConnectionRevisionRepository,
    ) -> Self {
        self.connection_revision_repository = Some(repository);
        self
    }

    pub fn connection_revision_repository(
        &self,
    ) -> Result<&dyn vestrace_application::ConnectionRevisionRepository, crate::api::ApiError> {
        self.connection_revision_repository
            .as_deref()
            .ok_or_else(|| {
                crate::api::ApiError::from_application(
                    vestrace_application::ApplicationError::Unavailable(
                        "governed connection projection is not configured".to_owned(),
                    ),
                )
            })
    }

    /// Supply the only readable governed Model and Provider projection authority.
    /// Absence fails closed instead of falling back to legacy registries.
    pub fn with_model_revision_repository(
        mut self,
        repository: SharedModelRevisionRepository,
    ) -> Self {
        self.model_revision_repository = Some(repository);
        self
    }

    pub fn model_revision_repository(
        &self,
    ) -> Result<&dyn vestrace_application::ModelRevisionRepository, crate::api::ApiError> {
        self.model_revision_repository.as_deref().ok_or_else(|| {
            crate::api::ApiError::from_application(
                vestrace_application::ApplicationError::Unavailable(
                    "governed model projection is not configured".to_owned(),
                ),
            )
        })
    }

    /// Supply the one authority that may request a qualification job.
    ///
    /// Absence fails closed: a surface that quietly did nothing would report a
    /// qualification that no probe will ever run.
    pub fn with_qualification_job_repository(
        mut self,
        repository: std::sync::Arc<dyn vestrace_application::QualificationJobRepository>,
    ) -> Self {
        self.qualification_job_repository = Some(repository);
        self
    }

    pub fn qualification_job_repository(
        &self,
    ) -> Result<&dyn vestrace_application::QualificationJobRepository, crate::api::ApiError> {
        self.qualification_job_repository.as_deref().ok_or_else(|| {
            crate::api::ApiError::from_application(
                vestrace_application::ApplicationError::Unavailable(
                    "governed qualification authority is not configured".to_owned(),
                ),
            )
        })
    }

    /// Supply the sole authority that may publish, replace, or revoke a slot's
    /// resolved credential revision.
    pub fn with_credential_activation_repository(
        mut self,
        repository: std::sync::Arc<dyn vestrace_application::CredentialActivationRepository>,
    ) -> Self {
        self.credential_activation_repository = Some(repository);
        self
    }

    pub fn credential_activation_repository(
        &self,
    ) -> Result<&dyn vestrace_application::CredentialActivationRepository, crate::api::ApiError>
    {
        self.credential_activation_repository
            .as_deref()
            .ok_or_else(|| {
                crate::api::ApiError::from_application(
                    vestrace_application::ApplicationError::Unavailable(
                        "governed credential activation authority is not configured".to_owned(),
                    ),
                )
            })
    }

    /// Supply the sole authority that may accept a successor after an
    /// acknowledged ambiguous embedding effect. Absence fails closed: the
    /// acknowledgement must never report a replacement job it did not create.
    pub fn with_embedding_job_repository(
        mut self,
        repository: vestrace_application::SharedEmbeddingJobRepository,
    ) -> Self {
        self.embedding_job_repository = Some(repository);
        self
    }

    pub fn embedding_job_repository(
        &self,
    ) -> Result<&dyn vestrace_application::EmbeddingJobRepository, crate::api::ApiError> {
        self.embedding_job_repository.as_deref().ok_or_else(|| {
            crate::api::ApiError::from_application(
                vestrace_application::ApplicationError::Unavailable(
                    "governed embedding job acceptance is not configured".to_owned(),
                ),
            )
        })
    }

    /// The authority that records one authorized successor to a retrieval
    /// attempt whose generation moved. Absent by default and fails closed: a
    /// surface that reported a successor it had not durably recorded would
    /// promise a second provider call nobody would make.
    pub fn with_embedding_retrieval_repository(
        mut self,
        repository: vestrace_application::embedding::SharedEmbeddingRetrievalRepository,
    ) -> Self {
        self.embedding_retrieval_repository = Some(repository);
        self
    }

    pub fn embedding_retrieval_repository(
        &self,
    ) -> Result<
        &dyn vestrace_application::embedding::EmbeddingRetrievalRepository,
        crate::api::ApiError,
    > {
        self.embedding_retrieval_repository
            .as_deref()
            .ok_or_else(|| {
                crate::api::ApiError::from_application(
                    vestrace_application::ApplicationError::Unavailable(
                        "governed embedding retrieval retry is not configured".to_owned(),
                    ),
                )
            })
    }

    pub fn with_embedding_transition_repository(
        mut self,
        repository: vestrace_application::SharedEmbeddingTransitionRepository,
    ) -> Self {
        self.embedding_transition_repository = Some(repository);
        self
    }

    pub fn embedding_transition_repository(
        &self,
    ) -> Result<&dyn vestrace_application::EmbeddingTransitionRepository, crate::api::ApiError>
    {
        self.embedding_transition_repository
            .as_deref()
            .ok_or_else(|| {
                crate::api::ApiError::from_application(
                    vestrace_application::ApplicationError::Unavailable(
                        "governed embedding transition carry acknowledgement is not configured"
                            .to_owned(),
                    ),
                )
            })
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
    let router = Router::new();
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/health/live"),
        get(health::live),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/health/ready"),
        get(health::ready),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/metrics"),
        get(crate::metrics::metrics_handler),
    )
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
    let authorization_request = match inventory_lookup(request.method(), request.uri().path()) {
        RouteDecision::PublicBounded => return next.run(request).await,
        RouteDecision::NotInInventory => {
            return crate::api::ApiError::refused(
                "route_not_in_inventory",
                "the request path is not registered as a governed HTTP surface",
            )
            .into_response();
        }
        RouteDecision::Governed(request) => request,
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

#[cfg(test)]
fn http_authorization_request(
    method: &Method,
    path: &str,
) -> Option<vestrace_domain::AuthorizationRequest> {
    match inventory_lookup(method, path) {
        RouteDecision::Governed(request) => Some(request),
        RouteDecision::PublicBounded | RouteDecision::NotInInventory => None,
    }
}

pub fn inventory_lookup_for_test(method: &Method, path: &str) -> RouteDecision {
    inventory_lookup(method, path)
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
    use super::http_authorization_request;
    use crate::route_inventory::{RouteDecision, inventory_lookup};
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
    fn unknown_transport_path_is_denied() {
        assert!(matches!(
            inventory_lookup(&Method::GET, "/v1/v1/runs"),
            RouteDecision::NotInInventory
        ));
        assert!(matches!(
            inventory_lookup(&Method::GET, "/ag-ui/unknown"),
            RouteDecision::NotInInventory
        ));
        assert!(matches!(
            inventory_lookup(&Method::GET, "/unmatched"),
            RouteDecision::NotInInventory
        ));
    }
}
