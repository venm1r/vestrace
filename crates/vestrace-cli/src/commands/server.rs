use std::sync::Arc;

use anyhow::anyhow;
use tokio::signal;
use tracing::Subscriber;
use tracing_subscriber::{EnvFilter, filter, fmt::MakeWriter, prelude::*};
use vestrace_application::QualificationRuntime;
use vestrace_application::{
    ConfiguredCapabilityPolicyEngine, DenyAllPolicyEngine, MemoryService, RetrievalService,
    RunService,
};
use vestrace_domain::id::WorkerId;
use vestrace_http::{AppState, MetricsRegistry, build_router};
use vestrace_infrastructure::{
    AppConfig, LogFormat, ObservabilityConfig, PgAgUiRepository, PgAgentRepository,
    PgArtifactRepository, PgAuditRepository, PgCapabilityGrantRepository, PgConnectionRepository,
    PgEmbeddingDataPolicyDecisionRepository, PgEvaluationRepository, PgEventRepository,
    PgExecutionHistoryRepository, PgExternalEffectRepository, PgHealthFindingRepository,
    PgIdempotencyRepository, PgInvariantObserver, PgMemoryRepository, PgModelExecutionRepository,
    PgModelRepository, PgOutboxRepository, PgProvenanceRepository, PgProviderRepository,
    PgPurgeRepository, PgRelationRepository, PgRetrievalJournal, PgRoutingDecisionRepository,
    PgRunLeasePort, PgRunRepository, PgSecretStore, PgSkillRepository, PgStore, PgTextRetriever,
    PgTriggerRepository, PgVectorRetriever, PgWorkQueuePort, PgWorkflowRepository,
    PgWorkspaceCounts, PgWorkspaceSettingsRepository, PolicyEngineKind, PostgresRunStore,
};

pub async fn run(config: &AppConfig, dispatch_owner: WorkerId) -> anyhow::Result<()> {
    init_tracing(&config.observability)?;
    tracing::info!(bind = %config.http.bind, "starting vestrace server");

    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow!("database is unavailable"))?;
    store
        .migrate()
        .await
        .map_err(|_| anyhow!("database migrations are unavailable"))?;
    match store.migrations_are_compatible().await {
        Ok(true) => {}
        Ok(false) => return Err(anyhow!("database migration history is incompatible")),
        Err(_) => return Err(anyhow!("database migration verification is unavailable")),
    }
    if config.qualification.enabled {
        crate::commands::conformance::run_automatic_qualification(
            &config.qualification,
            QualificationRuntime::Server,
            &store,
        )
        .await?;
    }
    if config.recovery.enabled {
        crate::commands::recovery::run_startup_recovery(&config.workspaces, &store).await?;
    }

    let run_repository = Arc::new(PgRunRepository::new(store.clone()));
    let run_service = Arc::new(RunService::new(run_repository));

    let store_for_settings = store.clone();
    let store_for_diagnostics = store.clone();
    let store_for_health_findings = store.clone();
    let store_for_grants = store.clone();
    let store_for_purge = store.clone();
    let store_for_effects = store.clone();
    let store_for_grant_seed = store.clone();
    let store_for_vectors = store.clone();
    let store_for_evidence = store.clone();
    let store_for_counts = store.clone();
    let store_for_artifacts = store.clone();
    let store_for_triggers = store.clone();
    let store_for_connections = store.clone();
    let store_for_secrets = store.clone();
    let store_for_credentials = store.clone();
    let store_for_journal = store.clone();
    let store_for_catalog = store.clone();
    let store_for_memory = store.clone();
    let store_for_ag_ui = store.clone();
    let store_for_runs = store.clone();
    let memory_service = MemoryService::new(
        PgEventRepository::new(store_for_memory.clone()),
        PgMemoryRepository::new(store_for_memory.clone()),
        PgProvenanceRepository::new(store_for_memory.clone()),
        PgRelationRepository::new(store_for_memory.clone()),
        PgOutboxRepository::new(store.clone()),
        PgIdempotencyRepository::new(store.clone()),
    );
    let memory_use_cases: vestrace_application::SharedMemoryUseCases = Arc::new(memory_service);

    let text_retriever: vestrace_application::SharedTextRetriever =
        Arc::new(PgTextRetriever::new(store.clone()));
    let retrieval_journal: vestrace_application::SharedRetrievalJournal =
        Arc::new(PgRetrievalJournal::new(store_for_journal));
    // The vector channel, when a deployment configures an embedding model.
    // Without one the service runs with a single channel, which is what it has
    // always done — and the journal now records that the channel was not
    // configured rather than leaving its absence indistinguishable from a
    // channel that returned nothing.
    let embedding_provider = build_embedding_provider(config, &store)?;
    let vector_retriever: Option<vestrace_application::SharedVectorRetriever> =
        embedding_provider.as_ref().map(|provider| {
            Arc::new(PgVectorRetriever::new(
                store_for_vectors.clone(),
                Arc::clone(provider),
                config.embedding.space_name.clone(),
            )) as vestrace_application::SharedVectorRetriever
        });
    if vector_retriever.is_some() {
        tracing::info!(
            model = %config.embedding.model_name,
            space = %config.embedding.space_name,
            "retrieval has a vector channel"
        );
    }
    let retrieval_service = Arc::new(RetrievalService::with_channels(
        text_retriever,
        vector_retriever,
        None,
        None,
        retrieval_journal,
    ));

    let provider_repository: vestrace_application::SharedProviderRepository =
        Arc::new(PgProviderRepository::new(store_for_catalog.clone()));
    let model_repository: vestrace_application::SharedModelRepository =
        Arc::new(PgModelRepository::new(store_for_catalog.clone()));
    let agent_repository: vestrace_application::SharedAgentRepository =
        Arc::new(PgAgentRepository::new(store_for_catalog.clone()));
    let skill_repository: vestrace_application::SharedSkillRepository =
        Arc::new(PgSkillRepository::new(store_for_catalog.clone()));
    let routing_decision_repository: vestrace_application::SharedRoutingDecisionRepository =
        Arc::new(PgRoutingDecisionRepository::new(store_for_catalog.clone()));
    let model_execution_repository: vestrace_application::SharedModelExecutionRepository =
        Arc::new(PgModelExecutionRepository::new(store_for_catalog.clone()));
    let execution_history_repository: vestrace_application::SharedExecutionHistoryRepository =
        Arc::new(PgExecutionHistoryRepository::new(store_for_catalog.clone()));
    let workflow_repository: vestrace_application::SharedWorkflowRepository =
        Arc::new(PgWorkflowRepository::new(store_for_catalog.clone()));
    let evaluation_store = Arc::new(PgEvaluationRepository::new(store_for_catalog.clone()));
    let evaluation_repository: vestrace_application::SharedEvaluationRepository =
        evaluation_store.clone();
    let learning_repository: vestrace_application::SharedLearningRepository = evaluation_store;
    let metrics_registry = Arc::new(MetricsRegistry::new());
    // The grant store backs both the authorization engine and the surface that
    // issues grants, so there is one place a grant can come from.
    let capability_grants: vestrace_application::SharedCapabilityGrantRepository =
        Arc::new(PgCapabilityGrantRepository::new(store_for_grants));

    // The same engine the HTTP boundary uses, so a purge is authorized against
    // the grants that govern everything else — and revoking `memory.purge`
    // closes the surface on the next request rather than at the next restart.
    let policy_engine = build_policy_engine(&config.policy, capability_grants.clone())?;
    // Adapters a deployment has configured a destination for. With none, the
    // effect surface answers 501 — which is the honest state of a system that
    // has nowhere to act.
    let effect_adapters = build_effect_adapters(&config.effects)?;
    let external_effects = Arc::new(vestrace_application::PerformExternalEffectService::new(
        Arc::new(PgExternalEffectRepository::new(store_for_effects)),
        vestrace_application::AuthorizationBoundary::new(policy_engine.clone()),
        dispatch_owner,
    ));

    let purge: vestrace_application::SharedPurgeUseCase =
        Arc::new(vestrace_application::HardPurgeMemoryService::new(
            vestrace_application::GrantedPurgeAuthorizer::new(
                vestrace_application::AuthorizationBoundary::new(policy_engine.clone()),
            ),
            PgPurgeRepository::new(store_for_purge),
        ));

    let app_state = AppState::new_with_learning_and_policy(
        Arc::new(store),
        run_service,
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
        policy_engine,
    )
    .with_workspace_settings(Arc::new(PgWorkspaceSettingsRepository::new(
        store_for_settings,
    )))
    .with_audit_repository(Arc::new(PgAuditRepository::new(store_for_catalog.clone())))
    .with_system_health(
        Arc::new(PgInvariantObserver::new(store_for_diagnostics)),
        Arc::new(PgHealthFindingRepository::new(store_for_health_findings)),
        Arc::new(store_for_evidence),
    )
    .with_workspace_counts(Arc::new(PgWorkspaceCounts::new(store_for_counts)))
    .with_artifact_repository(Arc::new(PgArtifactRepository::new(store_for_artifacts)))
    .with_trigger_repository(Arc::new(PgTriggerRepository::new(store_for_triggers)))
    .with_connection_repository(Arc::new(PgConnectionRepository::new(store_for_connections)))
    .with_ag_ui(Arc::new(PgAgUiRepository::new(store_for_ag_ui)))
    .with_capability_grants(capability_grants)
    .with_purge(purge)
    .with_external_effects(external_effects, effect_adapters)
    // The authoritative run write path. It writes `run_steps` and
    // `run_work_items` as well as `agent_runs` and `run_events`, which is what
    // makes a run created over HTTP visible to the worker.
    .with_run_orchestrator(Arc::new(vestrace_application::run::RunCoordinator::new(
        PostgresRunStore::new(&store_for_runs),
        vestrace_application::run::SystemClock::new(),
        PgWorkQueuePort::new(&store_for_runs),
        PgRunLeasePort::new(&store_for_runs),
    )));
    // Absent master key means no secret storage at all. The surface then
    // answers 501, which is the honest outcome — the alternative, storing
    // credentials in the clear, would be trusted precisely because it worked.
    let app_state = match master_key(&config.secrets)? {
        Some(master) => {
            tracing::info!(
                key_version = %master.version(),
                "secret storage is enabled under local-file master key custody, which does not meet production crypto qualification"
            );
            app_state.with_secret_store(Arc::new(PgSecretStore::new(store_for_secrets, master)))
        }
        None => {
            tracing::warn!("no master key is configured; secret storage is unavailable");
            app_state
        }
    };
    // Credential administration is workspace scoped like every other store.
    let app_state = app_state
        .with_access_token_store(Arc::new(vestrace_infrastructure::PgAccessTokenStore::new(
            store_for_credentials.clone(),
        )))
        .with_token_entropy_source(Arc::new(vestrace_infrastructure::SystemTokenEntropy::new()));

    // A development bootstrap credential, if one is configured. It is written
    // into `access_tokens` as an ordinary row rather than compared in the
    // authentication path, so it is listable, attributable and revocable like
    // any other credential — and so there is exactly one way to authenticate.
    if let Err(error) = seed_bootstrap_credential(&config.auth, &store_for_credentials).await {
        return Err(anyhow!("bootstrap credential could not be seeded: {error}"));
    }
    // Authority for that principal, when the deployment authorizes from grants.
    // A credential with no grants authenticates and can do nothing.
    if let Err(error) = seed_bootstrap_grants(config, &store_for_grant_seed).await {
        return Err(anyhow!(
            "bootstrap capability grants could not be seeded: {error}"
        ));
    }

    let app_state = app_state.with_authentication(vestrace_http::auth::Authentication::new(
        Arc::new(vestrace_infrastructure::PgAccessTokenAuthenticator::new(
            store_for_credentials.pool().clone(),
        )),
    ));
    tracing::info!(
        "authentication resolves each request to the principal its access token was issued to"
    );
    let router = build_router(app_state);
    let listener = tokio::net::TcpListener::bind(config.http.bind)
        .await
        .map_err(|_| anyhow!("HTTP listener is unavailable"))?;
    tracing::info!("HTTP server listening on {}", config.http.bind);

    let server = axum::serve(listener, router);

    tokio::select! {
        result = server => {
            result.map_err(|_| anyhow!("HTTP server stopped unexpectedly"))
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received, stopping server");
            Ok(())
        }
    }
}

/// The external effect adapters this deployment has configured.
///
/// Each one needs a destination *and* a way to ask what happened. An adapter
/// without the second cannot be built at all — the constructor refuses rather
/// than declaring a reconciliation it cannot perform — so a misconfigured
/// deployment fails at startup instead of discovering it after the first
/// unknown outcome.
fn build_effect_adapters(
    config: &vestrace_infrastructure::EffectsConfig,
) -> anyhow::Result<Vec<Arc<dyn vestrace_domain::external_effects::ExternalEffectAdapter>>> {
    let mut adapters: Vec<Arc<dyn vestrace_domain::external_effects::ExternalEffectAdapter>> =
        Vec::new();
    for adapter in config.configured() {
        let webhook = vestrace_infrastructure::HttpWebhookEffectAdapter::new(
            &adapter.name,
            &adapter.dispatch_url,
            &adapter.read_back_url,
        )
        .map_err(|error| {
            anyhow!(
                "external effect adapter `{}` is not configurable: {error}",
                adapter.name
            )
        })?;
        tracing::info!(
            adapter = %adapter.name,
            dispatch = %adapter.dispatch_url,
            "an external effect adapter is configured; this deployment can act outside itself"
        );
        adapters.push(Arc::new(webhook));
    }
    if adapters.is_empty() {
        tracing::info!(
            "no external effect adapter is configured; the effect surface answers 501 and this              deployment cannot act outside itself"
        );
    }
    Ok(adapters)
}

/// Build the master key, when one is configured.
///
/// A malformed key is an error rather than a silent disable: an operator who
/// set a key intends secret storage to work, and quietly starting without it
/// would surface much later as an unexplained 501.
fn master_key(
    config: &vestrace_infrastructure::SecretsConfig,
) -> anyhow::Result<Option<Arc<vestrace_infrastructure::crypto::MasterKey>>> {
    use secrecy::ExposeSecret;
    let Some(key) = config.master_key.as_ref() else {
        return Ok(None);
    };
    let master = vestrace_infrastructure::crypto::MasterKey::from_base64(
        key.expose_secret(),
        config.effective_key_version(),
    )
    // The error is not chained: its rendering must never risk carrying the key.
    .map_err(|_| anyhow!("secrets.master_key must be 32 bytes encoded as base64"))?;
    Ok(Some(Arc::new(master)))
}

/// Write the configured bootstrap credential into the credential table.
///
/// # Why this exists at all
///
/// A system where every credential is minted through the API has a chicken and
/// egg problem: minting one requires authenticating, and authenticating
/// requires a credential. Something has to put the first one in place.
///
/// # Why it is a row and not a special case in the auth path
///
/// The previous build compared a configured string against the presented token
/// inside the middleware. That value authenticated but did not exist: it could
/// not be listed, could not be revoked without a restart, and left no record of
/// having been used. Here it is an ordinary row — same table, same expiry and
/// revocation rules, same `last_used_at` — so there is exactly one way to
/// authenticate and one place to look.
///
/// Idempotent by hash, so restarting does not accumulate duplicates and
/// changing the configured value seeds a new credential rather than mutating
/// the old one's row. Revoking the seeded credential is respected: this does
/// **not** resurrect a row an operator withdrew.
async fn seed_bootstrap_credential(
    config: &vestrace_infrastructure::AuthConfig,
    store: &vestrace_infrastructure::PgStore,
) -> anyhow::Result<()> {
    use secrecy::ExposeSecret;
    use vestrace_domain::identity::hash_presented_token;

    let (Some(token), Some(workspace), Some(principal)) = (
        config.admin_token.as_ref(),
        config.admin_workspace,
        config.admin_principal,
    ) else {
        tracing::warn!(
            "no bootstrap credential is configured; the API is reachable only with an \
             access token that already exists in the database"
        );
        return Ok(());
    };

    // The configured value must be a well-formed Vestrace credential. A
    // free-form string would hash to something no client could ever present.
    let token_hash = hash_presented_token(token.expose_secret()).map_err(|_| {
        anyhow!(
            "auth.admin_token must be a Vestrace access token: the prefix \"vst_\" \
             followed by 64 hex characters"
        )
    })?;

    // Inside a scoped transaction like every other write. `access_tokens`
    // forces row level security, so an unscoped connection is refused — which
    // is the policy working, not an obstacle to route around: seeding a
    // credential into a workspace requires being in that workspace.
    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(workspace),
        vestrace_domain::PrincipalId::from_uuid(principal),
    );
    let mut scoped = store
        .begin_scoped(&context)
        .await
        .map_err(|error| anyhow!("{error}"))?;

    let inserted = sqlx::query(
        r#"
        INSERT INTO access_tokens
            (id, workspace_id, principal_id, token_hash, label, created_at)
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (token_hash) DO NOTHING
        "#,
    )
    .bind(uuid::Uuid::now_v7())
    .bind(workspace)
    .bind(principal)
    .bind(&token_hash)
    .bind("bootstrap (configured)")
    .execute(scoped.connection())
    .await
    .map_err(|error| anyhow!("{error}"))?
    .rows_affected();

    scoped.commit().await.map_err(|error| anyhow!("{error}"))?;

    if inserted == 1 {
        tracing::warn!(
            workspace = %workspace,
            principal = %principal,
            "seeded a bootstrap access token from configuration; anything that can read this \
             process's environment can authenticate as that principal. Mint per-user \
             credentials and revoke this one"
        );
    } else {
        tracing::info!("the configured bootstrap access token is already present");
    }
    Ok(())
}

/// Build the embedding provider, when one is configured.
///
/// Absent configuration is not an error: a deployment without an embedding model
/// has a text channel and says so, which is the honest degradation. What would
/// be wrong is configuring one and quietly running without it.
pub(crate) fn build_embedding_provider(
    config: &AppConfig,
    store: &PgStore,
) -> anyhow::Result<Option<vestrace_application::SharedGovernedEmbeddingProvider>> {
    if !config.embedding.enabled {
        return Ok(None);
    }
    let client = vestrace_infrastructure::OpenAiCompatibleEmbeddingClient::new(
        &config.embedding.base_url,
        &config.embedding.model_name,
        None,
    )
    .map_err(|error| anyhow!("invalid embedding configuration: {error}"))?;
    let egress = client.egress().clone();
    let Some(data_policy) = config
        .policy
        .data
        .as_ref()
        .and_then(|data| data.embedding.as_ref())
    else {
        return Err(anyhow!(
            "policy.data.embedding.mode, policy.data.embedding.admissible_labels, policy.data.embedding.allow_unclassified, policy.data.embedding.classification, policy.data.embedding.maximum_sensitivity, and policy.data.embedding.allowed_destinations are required when embedding.enabled is true"
        ));
    };
    let classification_policy = vestrace_domain::retrieval::ClassificationPolicy::new(
        data_policy.admissible_labels.iter().cloned(),
        data_policy.allow_unclassified,
    )
    .map_err(|error| anyhow!("policy.data.embedding.admissible_labels is invalid: {error}"))?;
    let policy = vestrace_domain::trust::DataPolicy::new(
        vestrace_domain::DataPolicyId::new(),
        config.policy.version.clone(),
        data_policy.maximum_sensitivity,
        data_policy.allowed_destinations.clone(),
        None,
    )
    .map_err(|error| anyhow!("policy.data.embedding is invalid: {error}"))?;
    let mode = match data_policy.mode {
        vestrace_infrastructure::DataPolicyMode::Enforce => {
            vestrace_application::EmbeddingDataPolicyMode::Enforce
        }
        vestrace_infrastructure::DataPolicyMode::Observe => {
            vestrace_application::EmbeddingDataPolicyMode::Observe
        }
    };
    let gate = vestrace_application::EmbeddingDataPolicyGate::new(
        vestrace_application::EmbeddingDataPolicySettings {
            classification_policy,
            classification: data_policy.classification,
            policy,
            mode,
        },
        Arc::new(PgEmbeddingDataPolicyDecisionRepository::new(store.clone())),
    );
    Ok(Some(gate.govern(Arc::new(client), egress)))
}

/// Give the bootstrap principal the grants its configuration says it should
/// have.
///
/// # The same chicken-and-egg problem, one layer up
///
/// Issuing a grant requires `workspace.admin`, and holding `workspace.admin`
/// requires a grant. Something has to place the first ones, exactly as
/// [`seed_bootstrap_credential`] places the first credential.
///
/// # Why these are ordinary rows
///
/// They are issued through the same `CapabilityGrant::issue` the API calls, into
/// the same table, and are listable and revocable like any other. An operator
/// revoking a seeded grant is respected: this inserts only what is missing and
/// never resurrects something withdrawn, because the identity it checks is the
/// capability rather than the row.
///
/// The scope is `/v1` and the operation is `http`, which under the hierarchical
/// rule covers every path and method beneath them. That is broad on purpose and
/// is exactly as broad as the configured static list it replaces — the
/// difference being that these are attached to a subject, expire if given an
/// expiry, and can be revoked one at a time.
async fn seed_bootstrap_grants(
    config: &vestrace_infrastructure::AppConfig,
    store: &vestrace_infrastructure::PgStore,
) -> anyhow::Result<()> {
    use std::str::FromStr;
    use vestrace_application::CapabilityGrantRepository;
    use vestrace_domain::security::{CapabilityGrant, CapabilityGrantSpec};

    if config.policy.engine != PolicyEngineKind::CapabilityGrants {
        return Ok(());
    }
    let (Some(workspace), Some(principal)) =
        (config.auth.admin_workspace, config.auth.admin_principal)
    else {
        tracing::warn!(
            "the capability-grant engine is selected but no bootstrap principal is              configured; every governed request will be denied until grants are issued"
        );
        return Ok(());
    };

    let risk_ceiling: vestrace_domain::RiskCategory = serde_json::from_value(
        serde_json::Value::String(config.policy.risk_ceiling.clone()),
    )
    .map_err(|_| {
        anyhow!(
            "invalid policy.risk_ceiling {:?}",
            config.policy.risk_ceiling
        )
    })?;

    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(workspace),
        vestrace_domain::PrincipalId::from_uuid(principal),
    );
    let repository = vestrace_infrastructure::PgCapabilityGrantRepository::new(store.clone());

    let existing = repository
        .list(&context)
        .await
        .map_err(|error| anyhow!("existing grants could not be read: {error}"))?;

    let now = vestrace_domain::time::now();
    let mut seeded = 0usize;
    for name in &config.policy.capabilities {
        let capability = vestrace_domain::Capability::from_str(name.trim())
            .map_err(|_| anyhow!("policy.capabilities names an unknown capability {name:?}"))?;
        // Already issued, or issued and then revoked: either way this is not
        // ours to decide again.
        if existing
            .iter()
            .any(|grant| grant.subject_id.as_uuid() == principal && grant.capability == capability)
        {
            continue;
        }

        let grant = CapabilityGrant::issue(
            CapabilityGrantSpec {
                id: vestrace_domain::CapabilityGrantId::new(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                issuer_id: context.principal_id,
                capability,
                operation: "http".to_string(),
                resource_scope: "/v1".to_string(),
                valid_from: now,
                valid_until: None,
                budget: None,
                risk_ceiling,
                conditions: Vec::new(),
            },
            now,
        )
        .map_err(|error| anyhow!("a bootstrap grant could not be issued: {error}"))?;

        repository
            .insert(&context, &grant)
            .await
            .map_err(|error| anyhow!("a bootstrap grant could not be stored: {error}"))?;
        seeded += 1;
    }

    if seeded > 0 {
        tracing::warn!(
            workspace = %workspace,
            principal = %principal,
            seeded,
            "seeded bootstrap capability grants from configuration; they are scoped to /v1              and held by one principal. Issue narrower grants and revoke these"
        );
    } else {
        tracing::info!("bootstrap capability grants are already present or were revoked");
    }
    Ok(())
}

/// Build the authorization engine the HTTP surface will use.
///
/// The default is deny-all. `configured-capabilities` is a stopgap for
/// deployments that have no capability-grant store yet: it authorizes a fixed
/// set of capabilities from configuration, with no subject scoping, validity
/// window or revocation. It is logged loudly at startup so nobody mistakes it
/// for a governed authorization path.
/// Build the authorization engine from configuration.
///
/// Shared with the worker, which previously hardcoded `DenyAllPolicyEngine` and
/// therefore refused every work item it leased, whatever `policy.engine` said.
pub(crate) fn build_policy_engine(
    config: &vestrace_infrastructure::PolicyConfig,
    grants: vestrace_application::SharedCapabilityGrantRepository,
) -> anyhow::Result<vestrace_application::SharedPolicyDecisionEngine> {
    match config.engine {
        PolicyEngineKind::DenyAll => Ok(Arc::new(DenyAllPolicyEngine)),
        PolicyEngineKind::CapabilityGrants => {
            let engine =
                vestrace_application::StoredGrantPolicyEngine::new(grants, &config.version)
                    .map_err(|error| anyhow!("invalid policy configuration: {error}"))?;
            tracing::info!(
                policy_version = %config.version,
                "authorization consults durable capability grants; a principal holding none                  is denied every governed request"
            );
            Ok(Arc::new(engine))
        }
        PolicyEngineKind::ConfiguredCapabilities => {
            let risk_ceiling: vestrace_domain::RiskCategory = serde_json::from_value(
                serde_json::Value::String(config.risk_ceiling.clone()),
            )
            .map_err(|_| {
                anyhow!(
                    "invalid policy.risk_ceiling {:?}; expected low, medium, high or critical",
                    config.risk_ceiling
                )
            })?;
            let capabilities: Vec<&str> = config.capabilities.iter().map(String::as_str).collect();
            let engine = ConfiguredCapabilityPolicyEngine::from_strings(
                &config.version,
                &capabilities,
                risk_ceiling,
            )
            .map_err(|error| anyhow!("invalid policy configuration: {error}"))?;
            tracing::warn!(
                policy_version = %config.version,
                capabilities = ?config.capabilities,
                risk_ceiling = ?risk_ceiling,
                "authorization is using configured static capabilities; \
                 this is not a capability grant and cannot be revoked"
            );
            Ok(Arc::new(engine))
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// Install the process-wide tracing subscriber.
///
/// Shared with the worker, which previously installed none: every
/// `tracing::info!` and `tracing::warn!` it emitted — including the warning
/// that it serves no workspaces — went nowhere, so a misconfigured worker was
/// indistinguishable from a healthy idle one.
pub(crate) fn init_tracing(config: &ObservabilityConfig) -> anyhow::Result<()> {
    let result = match config.format {
        LogFormat::Text => build_text_subscriber(config, std::io::stderr)?.try_init(),
        LogFormat::Json => build_json_subscriber(config, std::io::stderr)?.try_init(),
    };

    result.map_err(|_| anyhow!("failed to initialize tracing"))
}

fn build_text_subscriber<W>(
    config: &ObservabilityConfig,
    writer: W,
) -> anyhow::Result<impl Subscriber + Send + Sync>
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let (targets, user_filter) = output_filters(config)?;
    Ok(tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(writer))
        .with(targets)
        .with(user_filter))
}

fn build_json_subscriber<W>(
    config: &ObservabilityConfig,
    writer: W,
) -> anyhow::Result<impl Subscriber + Send + Sync>
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let (targets, user_filter) = output_filters(config)?;
    Ok(tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json().with_writer(writer))
        .with(targets)
        .with(user_filter))
}

/// The two filters every subscriber applies, as **global** filters.
///
/// # Why these are layers rather than one `with_filter`
///
/// They used to be combined into a single per-layer filter attached to the
/// formatting layer with `.with_filter(...)`. That silently discarded most of
/// this system's log output, and the way it did so is worth writing down.
///
/// A per-layer filter records its decision in a thread-local bitmap, set only
/// when `Subscriber::enabled` runs. `enabled` does not always run: when a
/// callsite's cached `Interest` is `always`, `tracing` skips it and calls
/// `on_event` directly. `Filtered::on_event` then reads a bit that nothing on
/// that thread ever set for that callsite, finds it false, and drops the event.
///
/// The visible effect was not "no logs" — startup logged normally, which is why
/// this survived. It was that a warning emitted from inside the async runtime,
/// after the thread had processed a rejected `sqlx::query` event, vanished. The
/// outbox drain reported a message dead-lettered and printed nothing; so did
/// every `warn!` and `error!` in the application, infrastructure and HTTP
/// crates.
///
/// Global filters take no per-layer state and short-circuit the whole
/// subscriber, which is what these two want anyway: the target allowlist exists
/// to clamp dependency output for everyone, not for one layer.
type MetadataFilter = filter::FilterFn<for<'a, 'b> fn(&'a tracing::Metadata<'b>) -> bool>;

fn output_filters(config: &ObservabilityConfig) -> anyhow::Result<(MetadataFilter, EnvFilter)> {
    let user_filter = EnvFilter::try_new(&config.log_filter)
        .map_err(|_| anyhow!("invalid observability log filter"))?;
    let targets: for<'a, 'b> fn(&'a tracing::Metadata<'b>) -> bool = is_vestrace_target;
    Ok((filter::filter_fn(targets), user_filter))
}

/// Dependency crates log a great deal, and `sqlx` logs statements. Clamping by
/// target here means a verbose user filter cannot widen the output to include
/// them.
fn is_vestrace_target(metadata: &tracing::Metadata<'_>) -> bool {
    matches!(
        metadata.target().split("::").next(),
        Some(
            "vestrace"
                | "vestrace_cli"
                | "vestrace_http"
                | "vestrace_infrastructure"
                | "vestrace_application"
                | "vestrace_domain"
                | "vestrace_integration_tests"
        )
    )
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Write},
        sync::{Arc, Mutex},
    };

    use tracing::Level;
    use tracing_subscriber::fmt::MakeWriter;

    use super::{LogFormat, ObservabilityConfig};

    const SQL_SECRET: &str = "SELECT 'db-statement-secret'";
    const CONNECTION_SECRET: &str =
        "postgres://secret-user:secret-password@database.internal/vestrace";
    const ALLOWED_MESSAGE: &str = "allowed vestrace event";

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl SharedWriter {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> MakeWriter<'writer> for SharedWriter {
        type Writer = Self;

        fn make_writer(&'writer self) -> Self::Writer {
            self.clone()
        }
    }

    fn verbose_config(format: LogFormat) -> ObservabilityConfig {
        ObservabilityConfig {
            log_filter: "trace".to_owned(),
            format,
        }
    }

    fn emit_allowed_and_dependency_events() {
        tracing::info!(target: "vestrace_cli", ALLOWED_MESSAGE);
        tracing::event!(
            target: "sqlx::query",
            Level::TRACE,
            db.statement = SQL_SECRET,
            db.connection_string = CONNECTION_SECRET,
            "dependency query event"
        );
    }

    fn assert_output_is_clamped(output: &str) {
        assert!(output.contains(ALLOWED_MESSAGE), "{output}");
        assert!(!output.contains("dependency query event"), "{output}");
        assert!(!output.contains(SQL_SECRET), "{output}");
        assert!(!output.contains(CONNECTION_SECRET), "{output}");
        assert!(!output.contains("secret-password"), "{output}");
    }

    /// A library crate's warning reaches the log.
    ///
    /// # What this does and does not catch
    ///
    /// It asserts the property that was violated in every deployment: an event
    /// whose target is a library crate is written. It runs against the
    /// **global** dispatcher, because that is what a deployed process installs.
    ///
    /// It does **not** reproduce the original failure. That needed the
    /// production interleaving — a callsite whose cached `Interest` is `always`,
    /// emitted on a thread whose per-layer filter bit was last set false by a
    /// rejected `sqlx::query` event — and this test passes against the broken
    /// wiring too. The guard against the wiring returning is
    /// `subscribers_filter_globally_rather_than_per_layer` in the command
    /// contract, and the evidence that the fix works is live: the same worker
    /// build logged 0 library warnings before and 17 after.
    #[test]
    fn library_target_events_reach_the_global_subscriber() {
        let writer = SharedWriter::default();
        let config = ObservabilityConfig {
            log_filter: "info".to_owned(),
            format: LogFormat::Text,
        };
        let subscriber = super::build_text_subscriber(&config, writer.clone()).unwrap();
        tracing::subscriber::set_global_default(subscriber)
            .expect("only this test installs a global subscriber");

        tracing::event!(target: "sqlx::query", Level::INFO, "dependency query event");
        tracing::event!(
            target: "vestrace_application::outbox",
            Level::WARN,
            "outbox delivery failed and will be retried"
        );

        let output = writer.contents();
        assert!(
            output.contains("outbox delivery failed and will be retried"),
            "a library warning was dropped: {output:?}"
        );
        assert!(!output.contains("dependency query event"), "{output}");
    }

    #[test]
    fn verbose_text_filter_cannot_enable_dependency_events() {
        let writer = SharedWriter::default();
        let subscriber =
            super::build_text_subscriber(&verbose_config(LogFormat::Text), writer.clone()).unwrap();

        tracing::subscriber::with_default(subscriber, emit_allowed_and_dependency_events);

        assert_output_is_clamped(&writer.contents());
    }

    #[test]
    fn verbose_json_filter_cannot_enable_dependency_events() {
        let writer = SharedWriter::default();
        let subscriber =
            super::build_json_subscriber(&verbose_config(LogFormat::Json), writer.clone()).unwrap();

        tracing::subscriber::with_default(subscriber, emit_allowed_and_dependency_events);

        let output = writer.contents();
        assert_output_is_clamped(&output);
        let event: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert!(event.is_object(), "{event}");
    }
}
