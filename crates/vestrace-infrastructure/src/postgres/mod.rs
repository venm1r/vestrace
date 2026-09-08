pub mod access_token_repository;
pub mod ag_ui_repository;
pub mod agent_repository;
pub mod artifact_repository;
pub mod audit_repository;
pub mod capability_grant_repository;
pub mod cognitive_mutation_repository;
pub mod connection_repository;
pub mod connection_revision_repository;
pub mod credential_activation;
pub mod credential_dispatch_lease;
pub mod credential_guard;
pub mod credential_intent;
pub mod embedding_data_policy_decision_repository;
pub mod embedding_job_repository;
pub mod embedding_key_repository;
pub mod embedding_result_finalization_repository;
pub mod embedding_result_repository;
pub mod embedding_store;
pub mod embedding_transition_repository;
pub mod erasure;
pub mod event_repository;
pub mod execution_history_repository;
pub mod external_effect_repository;
pub mod fault_suite_evidence_repository;
mod health;
pub mod health_finding_repository;
pub mod idempotency_repository;
pub mod installation_fingerprint;
pub mod installation_permit;
pub mod invariant_observer;
pub mod material_intent;
pub mod memory_encoding;
pub mod memory_repository;
pub mod model_binding_repository;
pub mod model_data_policy_decision_repository;
pub mod model_execution_repository;
pub mod model_repository;
pub mod model_request_evidence_repository;
pub mod model_revision_repository;
pub mod outbox_repository;
mod pool;
pub mod provenance_repository;
pub mod provider_dispatch_repository;
pub mod provider_repository;
pub mod provider_result_repository;
pub mod purge_repository;
pub mod qualification_baseline_repository;
pub mod qualification_job_repository;
pub mod qualification_repository;
pub mod recovery_qualification_evidence_repository;
pub mod recovery_repository;
pub mod relation_repository;
pub mod retrieval_journal;
pub mod revision_hydrator;
pub mod routing_decision_repository;
pub mod run;
pub mod run_command_committer;
pub mod run_event_store;
pub mod run_recovery_store;
pub mod run_repository;
pub mod secret_store;
pub mod settings_repository;
pub mod shared_memory_revision_reader;
pub mod skill_repository;
pub mod startup_recovery_source;
pub mod text_retriever;
mod transaction;
pub mod trigger_repository;
pub mod vector_retriever;
pub mod workflow_evaluation_repository;
pub mod workspace_counts;

pub use access_token_repository::{PgAccessTokenAuthenticator, PgAccessTokenStore};
pub use ag_ui_repository::PgAgUiRepository;
pub use agent_repository::PgAgentRepository;
pub use artifact_repository::PgArtifactRepository;
pub use audit_repository::PgAuditRepository;
pub use capability_grant_repository::PgCapabilityGrantRepository;
pub use cognitive_mutation_repository::PgCognitiveMutationRepository;
pub use connection_repository::PgConnectionRepository;
pub use connection_revision_repository::PgConnectionRevisionRepository;
pub use credential_activation::PgCredentialActivationRepository;
pub use credential_dispatch_lease::{
    PgCredentialDispatchLeaseRepository, PgCredentialMaterialPreparer,
};
pub use credential_guard::PgCredentialGuardRepository;
pub use credential_intent::PgCredentialIntentRepository;
pub use embedding_data_policy_decision_repository::PgEmbeddingDataPolicyDecisionRepository;
pub use embedding_job_repository::PgEmbeddingJobRepository;
pub use embedding_key_repository::PgEmbeddingOutputKeyRepository;
pub use embedding_result_finalization_repository::{
    EmbeddingOutputHmacCommitter, PgEmbeddingResultFinalizationRepository,
};
pub use embedding_result_repository::PgEmbeddingResultRepository;
pub use embedding_store::PgEmbeddingStore;
pub use embedding_transition_repository::PgEmbeddingTransitionRepository;
pub use erasure::PgMaterialErasureRepository;
pub use event_repository::PgEventRepository;
pub use execution_history_repository::PgExecutionHistoryRepository;
pub use external_effect_repository::PgExternalEffectRepository;
pub use fault_suite_evidence_repository::PgFaultSuiteEvidenceRepository;
pub use health_finding_repository::PgHealthFindingRepository;
pub use idempotency_repository::PgIdempotencyRepository;
pub use installation_fingerprint::{
    HostInstallationFingerprintVault, INSTALLATION_FINGERPRINT_RECORD_FILE,
    INSTALLATION_FINGERPRINT_VAULT_ROOT_ENV, InstallationFingerprintReadinessCause,
    PgInstallationFingerprintReadiness, PgInstallationFingerprintSupervisor,
};
pub use installation_permit::PgInstallationMutationPermit;
pub use invariant_observer::PgInvariantObserver;
pub use material_intent::PgMaterialIntentRepository;
pub use memory_repository::PgMemoryRepository;
pub use model_binding_repository::PgModelBindingRepository;
pub use model_data_policy_decision_repository::PgModelDataPolicyDecisionRepository;
pub use model_execution_repository::PgModelExecutionRepository;
pub use model_repository::PgModelRepository;
pub use model_request_evidence_repository::PgModelRequestEvidenceRepository;
pub use model_revision_repository::PgModelRevisionRepository;
pub use outbox_repository::{PgMemoryTextSource, PgOutboxRepository};
pub use pool::{PgGovernedMutationRepository, PgStore};
pub use provenance_repository::PgProvenanceRepository;
pub use provider_dispatch_repository::PgProviderDispatchRepository;
pub use provider_repository::PgProviderRepository;
pub use provider_result_repository::PgProviderResultRepository;
pub use purge_repository::PgPurgeRepository;
pub use qualification_baseline_repository::PgQualificationBaselineRepository;
pub use qualification_job_repository::{
    OpenAiQualificationQ1Adapter, PgQualificationJobRepository, PgQualificationProbeRunner,
    QualificationQ1Adapter,
};
pub use qualification_repository::PgQualificationRepository;
pub use recovery_qualification_evidence_repository::PgRecoveryQualificationEvidenceRepository;
pub use recovery_repository::PgRecoveryRepository;
pub use relation_repository::PgRelationRepository;
pub use retrieval_journal::PgRetrievalJournal;
pub use revision_hydrator::PgRevisionHydrator;
pub use routing_decision_repository::PgRoutingDecisionRepository;
pub use run::{PgRunLeasePort, PgWorkQueuePort, PostgresRunStore};
pub use run_command_committer::PgRunCommandCommitter;
pub use run_event_store::PgRunEventStore;
pub use run_recovery_store::PgRunRecoveryStore;
pub use run_repository::PgRunRepository;
pub use secret_store::PgSecretStore;
pub use settings_repository::PgWorkspaceSettingsRepository;
pub use shared_memory_revision_reader::PgSharedMemoryRevisionReader;
pub use skill_repository::PgSkillRepository;
pub use startup_recovery_source::PgStartupRecoverySource;
pub use text_retriever::PgTextRetriever;
pub use transaction::{PgScopedTransaction, PgTransactionManager};
pub use trigger_repository::PgTriggerRepository;
pub use vector_retriever::{PgCorpusGenerationResolver, PgVectorRetriever};
pub use vestrace_application::RuntimeQualificationEvidence;
pub use workflow_evaluation_repository::{PgEvaluationRepository, PgWorkflowRepository};
pub use workspace_counts::PgWorkspaceCounts;

/// The one governed provider graph both long-running roots run on.
///
/// Server and worker used to build this separately, which is two opinions
/// about what is dispatchable wearing one name: a difference in either graph
/// would show up as a Run that the surface accepted and the worker refused, or
/// worse, the reverse. They now consume the same bundle, so a divergence is a
/// compile error rather than a production surprise.
///
/// Nothing here is optional. A missing vault, policy engine or repository is a
/// startup refusal at the call site, because a process that composed a partial
/// graph would answer requests it cannot honour.
pub struct GovernedProviderRuntime {
    dispatch: vestrace_application::SharedProviderDispatchRepository,
    results: std::sync::Arc<provider_result_repository::PgProviderResultRepository>,
    embedding_jobs: vestrace_application::SharedEmbeddingJobRepository,
    embedding_termination: std::sync::Arc<vestrace_application::EmbeddingJobTerminationService>,
    embedding_transitions: vestrace_application::SharedEmbeddingTransitionRepository,
}

impl GovernedProviderRuntime {
    /// Composes the governed graph from the runtime-scoped store, the host
    /// vault, and the deployment's declared policy.
    ///
    /// The vault is supplied rather than built here: its roots are validated at
    /// startup against the two separate mounts, and taking a path instead would
    /// let this constructor open a vault the process never proved writable.
    pub fn new(
        store: PgStore,
        vault: std::sync::Arc<crate::crypto::HostMaterialKeyVault>,
        policy: vestrace_application::SharedPolicyDecisionEngine,
        data_policy: vestrace_application::ModelDataPolicySettings,
        runs: std::sync::Arc<dyn vestrace_application::run::RunStorePort>,
    ) -> Self {
        use std::sync::Arc;

        let dispatch: vestrace_application::SharedProviderDispatchRepository = Arc::new(
            provider_dispatch_repository::PgProviderDispatchRepository::new(
                Arc::new(installation_permit::PgInstallationMutationPermit::new(
                    store.clone(),
                )),
                Arc::new(
                    model_request_evidence_repository::PgModelRequestEvidenceRepository::new(
                        vault.clone(),
                    ),
                ),
                Arc::new(external_effect_repository::PgExternalEffectRepository::new(
                    store.clone(),
                )),
                Arc::new(
                    credential_dispatch_lease::PgCredentialDispatchLeaseRepository::new(
                        store.clone(),
                        vault.clone(),
                    ),
                ),
                Arc::new(
                    model_data_policy_decision_repository::PgModelDataPolicyDecisionRepository::new(
                        store.clone(),
                    ),
                ),
                Arc::new(pool::PgGovernedMutationRepository::new(store.clone())),
                Arc::new(
                    vestrace_application::ConfiguredProviderDispatchPolicyEvaluator::new(
                        policy.clone(),
                        data_policy,
                    ),
                ),
            ),
        );
        let embedding_jobs: vestrace_application::SharedEmbeddingJobRepository = Arc::new(
            embedding_job_repository::PgEmbeddingJobRepository::new(store.clone()),
        );
        let embedding_termination =
            Arc::new(vestrace_application::EmbeddingJobTerminationService::new(
                embedding_jobs.clone(),
                policy,
            ));
        let embedding_transitions: vestrace_application::SharedEmbeddingTransitionRepository =
            Arc::new(
                embedding_transition_repository::PgEmbeddingTransitionRepository::new(
                    store.clone(),
                ),
            );
        let results = Arc::new(
            provider_result_repository::PgProviderResultRepository::new(
                Arc::new(installation_permit::PgInstallationMutationPermit::new(
                    store.clone(),
                )),
                vault,
                Arc::new(crate::crypto::ContentMaterialCodec::new()),
                Arc::new(external_effect_repository::PgExternalEffectRepository::new(
                    store,
                )),
                runs,
            )
            .with_provider_dispatch(dispatch.clone()),
        );
        Self {
            dispatch,
            results,
            embedding_jobs,
            embedding_termination,
            embedding_transitions,
        }
    }

    /// The authority every governed dispatch is admitted through.
    pub fn dispatch(&self) -> vestrace_application::SharedProviderDispatchRepository {
        self.dispatch.clone()
    }

    /// Governed acceptance for embedding jobs.
    ///
    /// Acceptance is its own port; dispatch is not. An accepted embedding job
    /// reaches the provider through `dispatch()` above -- the same instance the
    /// Run-step executor holds -- which is what makes "one dispatch authority,
    /// two callers" a fact about this composition rather than a claim in a
    /// comment. Returning a second dispatch repository here is the defect this
    /// method is shaped to prevent.
    pub fn embedding_jobs(&self) -> vestrace_application::SharedEmbeddingJobRepository {
        self.embedding_jobs.clone()
    }

    /// The configured-policy authority for a cancellation that is still known
    /// to be before provider dispatch.
    pub fn embedding_termination(
        &self,
    ) -> std::sync::Arc<vestrace_application::EmbeddingJobTerminationService> {
        self.embedding_termination.clone()
    }

    /// The one authority that creates immutable transition versions.
    pub fn embedding_transitions(
        &self,
    ) -> vestrace_application::SharedEmbeddingTransitionRepository {
        self.embedding_transitions.clone()
    }

    /// The one production route from a Run-step work item to a provider.
    ///
    /// Built per worker because the attempt it admits is owned by that worker's
    /// id; the authorities underneath are shared with the server.
    pub fn step_executor(
        &self,
        worker_id: vestrace_domain::id::WorkerId,
    ) -> std::sync::Arc<
        vestrace_application::run::GovernedProviderStepExecutor<
            provider_result_repository::PgProviderResultRepository,
        >,
    > {
        std::sync::Arc::new(
            vestrace_application::run::GovernedProviderStepExecutor::new(
                self.dispatch.clone(),
                self.results.clone(),
                std::sync::Arc::new(crate::providers::OpenAiGovernedModelAdapter),
                worker_id,
            ),
        )
    }
}
