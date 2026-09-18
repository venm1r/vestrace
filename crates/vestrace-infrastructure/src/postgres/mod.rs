pub mod access_token_repository;
pub mod ag_ui_repository;
pub mod agent_repository;
pub mod artifact_repository;
pub mod audit_repository;
pub mod backup_archive_repository;
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
pub mod embedding_retrieval_repository;
pub mod embedding_store;
pub mod embedding_transition_repository;
pub mod embedding_work_repository;
pub mod erasure;
pub mod event_repository;
pub mod execution_history_repository;
pub mod external_effect_repository;
pub mod fault_suite_evidence_repository;
mod health;
pub mod health_finding_repository;
pub mod idempotency_repository;
pub mod installation_drain;
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
pub mod qualification_work_repository;
pub mod recovery_qualification_evidence_repository;
pub mod recovery_repository;
pub mod relation_repository;
pub mod restore_cutover_repository;
pub mod retrieval_journal;
pub mod revision_hydrator;
pub mod routing_decision_repository;
pub mod run;
pub mod run_command_committer;
pub mod run_event_store;
pub mod run_recovery_store;
pub mod run_repository;
pub mod safety_authority_repository;
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
pub use backup_archive_repository::PgBackupArchiveRepository;
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
pub use embedding_retrieval_repository::PgEmbeddingRetrievalRepository;
pub use embedding_store::PgEmbeddingStore;
pub use embedding_transition_repository::PgEmbeddingTransitionRepository;
pub use embedding_work_repository::PgEmbeddingWorkRepository;
pub use erasure::PgMaterialErasureRepository;
pub use event_repository::PgEventRepository;
pub use execution_history_repository::PgExecutionHistoryRepository;
pub use external_effect_repository::PgExternalEffectRepository;
pub use fault_suite_evidence_repository::PgFaultSuiteEvidenceRepository;
pub use health_finding_repository::PgHealthFindingRepository;
pub use idempotency_repository::PgIdempotencyRepository;
pub use installation_drain::PgDrainMutationPermitRepository;
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
pub use pool::{
    DRAIN_HISTORICAL_MIGRATOR, HISTORICAL_MIGRATOR, PgGovernedMutationRepository, PgStore,
    QUALIFICATION_WORK_CLAIMS_HISTORICAL_MIGRATOR,
};
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
pub use qualification_work_repository::PgQualificationWorkRepository;
pub use recovery_qualification_evidence_repository::PgRecoveryQualificationEvidenceRepository;
pub use recovery_repository::PgRecoveryRepository;
pub use relation_repository::PgRelationRepository;
pub use restore_cutover_repository::PgRestoreCutoverRepository;
pub use retrieval_journal::PgRetrievalJournal;
pub use revision_hydrator::PgRevisionHydrator;
pub use routing_decision_repository::PgRoutingDecisionRepository;
pub use run::{PgRunLeasePort, PgWorkQueuePort, PostgresRunStore};
pub use run_command_committer::PgRunCommandCommitter;
pub use run_event_store::PgRunEventStore;
pub use run_recovery_store::PgRunRecoveryStore;
pub use run_repository::PgRunRepository;
pub use safety_authority_repository::PgSafetyAuthorityRepository;
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
    embedding_transition_coordinator:
        std::sync::Arc<vestrace_application::embedding::EmbeddingTransitionCoordinator>,
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
        Self::new_with_embedding_policy(store, vault, policy, data_policy, runs, None)
    }

    pub fn new_with_embedding_policy(
        store: PgStore,
        vault: std::sync::Arc<crate::crypto::HostMaterialKeyVault>,
        policy: vestrace_application::SharedPolicyDecisionEngine,
        data_policy: vestrace_application::ModelDataPolicySettings,
        runs: std::sync::Arc<dyn vestrace_application::run::RunStorePort>,
        embedding_policy: Option<std::sync::Arc<vestrace_application::EmbeddingDataPolicyGate>>,
    ) -> Self {
        use std::sync::Arc;

        let dispatch = provider_dispatch_repository::PgProviderDispatchRepository::new(
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
        );
        let dispatch: vestrace_application::SharedProviderDispatchRepository =
            Arc::new(match embedding_policy {
                Some(policy) => dispatch.with_embedding_policy(policy),
                None => dispatch,
            });
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
        let embedding_transition_coordinator = std::sync::Arc::new(
            vestrace_application::embedding::EmbeddingTransitionCoordinator::new(
                embedding_transitions.clone(),
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
            embedding_transition_coordinator,
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

    /// The sole coordinator that can bind transition attempts, derive their
    /// satisfactions, and prove a batch complete.
    pub fn embedding_transition_coordinator(
        &self,
    ) -> std::sync::Arc<vestrace_application::embedding::EmbeddingTransitionCoordinator> {
        self.embedding_transition_coordinator.clone()
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

/// The embedding services one worker process runs, composed once.
///
/// Four cycles, not the six `EmbeddingWorkKind` names. Erasure propagation has
/// no service at all, and transition coordination has only a command surface --
/// create an attempt, observe one, prove completeness, activate -- with nothing
/// that decides which command a given transition needs. Both are refused
/// before a claim is taken rather than stubbed, because a cycle that claimed
/// work it cannot service would look like progress while the backlog grew.
///
/// Nothing here is optional. A missing vault, decoder or repository refuses
/// construction instead of registering a cycle that would claim work and do
/// nothing with it.
pub struct EmbeddingWorkerRuntime {
    work: vestrace_application::embedding::SharedEmbeddingWorkRepository,
    executor: std::sync::Arc<ProductionEmbeddingExecutor>,
    index: std::sync::Arc<ProductionEmbeddingIndexService>,
    erasure: std::sync::Arc<ProductionEmbeddingErasureService>,
    claim_batch: u32,
    owner: String,
}

/// The executor a production worker runs, with every port bound to its one
/// production implementation.
pub type ProductionEmbeddingExecutor = vestrace_application::embedding::EmbeddingExecutor<
    embedding_result_repository::PgEmbeddingResultRepository,
    crate::crypto::HostMaterialKeyVault,
    crate::crypto::ContentMaterialCodec,
    embedding_result_finalization_repository::PgEmbeddingResultFinalizationRepository,
    EmbeddingOutputHmacCommitter,
>;

/// Erasure propagation and the local-index invalidation that follows it. It
/// shares the one registry the index service installs into: dropping from a
/// second registry would drop nothing anything ever reads.
pub type ProductionEmbeddingErasureService =
    vestrace_application::embedding::EmbeddingErasureService<
        embedding_erasure_repository::PgEmbeddingErasureRepository,
        crate::embedding_index::EmbeddingIndexRegistry,
        crate::embedding_index::FlatEmbeddingIndex,
    >;

pub type ProductionEmbeddingIndexService =
    vestrace_application::embedding::index::EmbeddingIndexService<
        embedding_index_repository::PgEmbeddingIndexRepository,
        crate::crypto::HostMaterialKeyVault,
        crate::embedding_index::ContentMaterialIndexDecoder,
        crate::embedding_index::FlatEmbeddingIndexFactory,
        crate::embedding_index::EmbeddingIndexRegistry,
    >;

/// What one bounded cycle did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmbeddingCycleOutcome {
    /// At least one claim was taken and carried to a durable outcome.
    pub did_work: bool,
    /// At least one claim ended in a failure the worker must report.
    pub failed: bool,
}

impl EmbeddingWorkerRuntime {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: PgStore,
        vault: std::sync::Arc<crate::crypto::HostMaterialKeyVault>,
        dispatch: vestrace_application::SharedProviderDispatchRepository,
        limits: &crate::config::EmbeddingWorkerLimits,
        owner: impl Into<String>,
        worker_id: vestrace_domain::id::WorkerId,
    ) -> Result<Self, vestrace_application::ApplicationError> {
        use std::sync::Arc;

        limits
            .validate()
            .map_err(vestrace_application::ApplicationError::InvalidConfiguration)?;
        let owner_name: String = owner.into();

        let preparation = Arc::new(
            vestrace_application::embedding::EmbeddingResultPreparationService::new(
                Arc::new(
                    embedding_result_repository::PgEmbeddingResultRepository::new(
                        store.clone(),
                        dispatch.clone(),
                    ),
                ),
                vault.clone(),
                Arc::new(crate::crypto::ContentMaterialCodec::new()),
            ),
        );
        let finalization = Arc::new(
            vestrace_application::embedding::EmbeddingResultFinalizationService::new(
                Arc::new(
                    embedding_result_finalization_repository::PgEmbeddingResultFinalizationRepository::new(
                        store.clone(),
                    ),
                ),
                vault.clone(),
                Arc::new(EmbeddingOutputHmacCommitter::new()),
            ),
        );
        // One budget for the whole process: the decoder and the index it builds
        // draw from the same allowance, so a large decode cannot leave the
        // builder without room it was promised.
        let budget = crate::embedding_index::IndexMemoryBudget::new(
            usize::try_from(limits.max_index_bytes).map_err(|_| {
                vestrace_application::ApplicationError::InvalidConfiguration(
                    "embedding.limits.max_index_bytes exceeds what this process can address"
                        .to_owned(),
                )
            })?,
        );
        // One registry for the process. The index service installs into it and
        // erasure propagation drops from it; two instances would let a corpus
        // keep answering from vectors the erasure believed it had dropped.
        let registry = Arc::new(crate::embedding_index::EmbeddingIndexRegistry::new());
        let index = Arc::new(
            vestrace_application::embedding::index::EmbeddingIndexService::new(
                Arc::new(embedding_index_repository::PgEmbeddingIndexRepository::new(
                    store.clone(),
                )),
                vault,
                Arc::new(crate::embedding_index::ContentMaterialIndexDecoder::new(
                    budget.clone(),
                )),
                Arc::new(crate::embedding_index::FlatEmbeddingIndexFactory::new(
                    budget,
                )),
                registry.clone(),
                crate::embedding_index::IndexLimits {
                    max_members: limits.max_index_members as usize,
                    max_bytes: usize::try_from(limits.max_index_bytes).unwrap_or(usize::MAX),
                },
                limits.build_chunk_size,
            ),
        );

        // The executor is composed last of the three, because answering a
        // retrieval query needs the local index the index service owns. A
        // process without one refuses such a job before dispatch rather than
        // paying for a response it cannot use.
        let retrieval = Arc::new(
            vestrace_application::embedding::EmbeddingRetrievalExecutionService::new(
                Arc::new(
                    embedding_retrieval_repository::PgEmbeddingRetrievalRepository::new(
                        store.clone(),
                    ),
                ),
                index.clone(),
                owner_name.clone(),
                limits.max_index_members.min(1024) as usize,
            )?,
        );
        let executor = Arc::new(
            vestrace_application::embedding::EmbeddingExecutor::new(
                dispatch,
                preparation,
                finalization,
                Arc::new(crate::providers::OpenAiGovernedModelAdapter),
                worker_id,
            )
            .with_retrieval_sink(retrieval),
        );

        let erasure = Arc::new(
            vestrace_application::embedding::EmbeddingErasureService::new(
                Arc::new(
                    embedding_erasure_repository::PgEmbeddingErasureRepository::new(store.clone()),
                ),
                registry,
            ),
        );

        Ok(Self {
            work: std::sync::Arc::new(embedding_work_repository::PgEmbeddingWorkRepository::new(
                store,
            )),
            executor,
            index,
            erasure,
            claim_batch: limits.claim_batch,
            owner: owner_name,
        })
    }

    pub fn erasure(&self) -> std::sync::Arc<ProductionEmbeddingErasureService> {
        self.erasure.clone()
    }

    /// One bounded pass dropping local indexes an erasure has invalidated.
    ///
    /// Not a claimed work kind, and deliberately not driven through
    /// `run_cycle`: a claim in `embedding_job_work_claims` names an embedding
    /// job, and an erasure is not one. The pass is idempotent, so a worker that
    /// misses it loses nothing durable -- query still validates the database
    /// guard and a retained index fails closed there.
    pub async fn reconcile_erasure_invalidations(
        &self,
        context: &vestrace_application::RequestContext,
    ) -> Result<usize, vestrace_application::ApplicationError> {
        self.erasure.reconcile_one(context).await
    }

    pub fn index(&self) -> std::sync::Arc<ProductionEmbeddingIndexService> {
        self.index.clone()
    }

    /// Runs one bounded cycle of one kind for one workspace.
    ///
    /// A claim is always finished, including when the work failed: an
    /// unfinished claim is one another worker must wait out, and a cycle that
    /// dropped it on error would turn every transient failure into a lease
    /// timeout.
    pub async fn run_cycle(
        &self,
        context: &vestrace_application::RequestContext,
        kind: vestrace_application::embedding::EmbeddingWorkKind,
    ) -> Result<EmbeddingCycleOutcome, vestrace_application::ApplicationError> {
        use vestrace_application::embedding::{EmbeddingWorkKind, EmbeddingWorkOutcome};

        if matches!(
            kind,
            EmbeddingWorkKind::CoordinateTransition | EmbeddingWorkKind::PropagateErasure
        ) {
            // Refused before claiming, not after: a cycle that took a claim it
            // cannot service would hold work away from nothing and report a
            // failure it caused itself.
            return Err(vestrace_application::ApplicationError::Unavailable(
                format!(
                    "embedding work kind {} has no cycle driver in this build",
                    kind.as_str()
                ),
            ));
        }
        let claims = self
            .work
            .claim(context, kind, &self.owner, self.claim_batch)
            .await?;
        let mut outcome = EmbeddingCycleOutcome::default();
        for claim in claims {
            outcome.did_work = true;
            let result = self.perform(context, kind, &claim).await;
            let finish = match &result {
                Ok(work) => *work,
                Err(_) => EmbeddingWorkOutcome::RetryableFailure,
            };
            if result.is_err() || matches!(finish, EmbeddingWorkOutcome::DefiniteFailure) {
                outcome.failed = true;
            }
            self.work.finish(context, &claim, finish).await?;
        }
        Ok(outcome)
    }

    async fn perform(
        &self,
        context: &vestrace_application::RequestContext,
        kind: vestrace_application::embedding::EmbeddingWorkKind,
        claim: &vestrace_application::embedding::EmbeddingWorkClaim,
    ) -> Result<
        vestrace_application::embedding::EmbeddingWorkOutcome,
        vestrace_application::ApplicationError,
    > {
        use vestrace_application::embedding::{
            EmbeddingExecutionOutcome, EmbeddingWorkKind, EmbeddingWorkOutcome,
        };

        match kind {
            EmbeddingWorkKind::Dispatch
            | EmbeddingWorkKind::ReconcileKeys
            | EmbeddingWorkKind::FinalizeResult => {
                // All three advance one job through the same executor: it reads
                // where the job actually is and resumes from there, so the kind
                // decides which queue was drained, not what is done to the job.
                let executed = self.executor.execute(context, claim.job_id).await?;
                Ok(match executed {
                    EmbeddingExecutionOutcome::Succeeded => EmbeddingWorkOutcome::Completed,
                    EmbeddingExecutionOutcome::Cancelled
                    | EmbeddingExecutionOutcome::FailedDefinite => {
                        EmbeddingWorkOutcome::DefiniteFailure
                    }
                    _ => EmbeddingWorkOutcome::RetryableFailure,
                })
            }
            EmbeddingWorkKind::BuildIndex => {
                self.index.reconcile_one(context, &self.owner).await?;
                Ok(EmbeddingWorkOutcome::Completed)
            }
            // Neither has a cycle driver. `EmbeddingTransitionCoordinator` is a
            // command surface -- create an attempt, observe one, prove
            // completeness, activate -- and choosing which command a given
            // transition needs is logic no service holds. Erasure propagation
            // has no service at all.
            // Erasure propagation has a service, but not one a claim can
            // reach: every claim names an embedding job and an erasure names a
            // material. It is driven by `reconcile_erasure_invalidations`.
            EmbeddingWorkKind::CoordinateTransition | EmbeddingWorkKind::PropagateErasure => Err(
                vestrace_application::ApplicationError::Unavailable(format!(
                    "embedding work kind {} has no cycle driver in this build",
                    kind.as_str()
                )),
            ),
        }
    }
}

mod embedding_adoption_repository;
mod embedding_erasure_repository;
mod embedding_index_repository;
mod embedding_retrieval_client;
mod embedding_write_route;
pub use embedding_adoption_repository::{
    GovernedEmbeddingJobPurpose, PgEmbeddingLegacyAdoptionRepository,
    PgGovernedContentMaterializer, PgGovernedEmbeddingJobFactory,
};
pub use embedding_erasure_repository::PgEmbeddingErasureRepository;
pub use embedding_index_repository::PgEmbeddingIndexRepository;
pub use embedding_retrieval_client::PgEmbeddingRetrievalJobClient;
pub use embedding_write_route::PgGovernedMemoryEmbeddingHandler;
