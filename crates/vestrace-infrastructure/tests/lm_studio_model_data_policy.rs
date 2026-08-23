//! Deliberate live acceptance against LM Studio and PostgreSQL.
//!
//! This is ignored in the ordinary suite because both processes are external
//! prerequisites. Run it explicitly with `DATABASE_URL` set while LM Studio is
//! serving `prism-ml/bonsai-27b` at `http://localhost:12345/v1`.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::run::{
    ProviderStepModelExecutor, StepModelExecutor, StepModelRequest, StepModelSettings,
};
use vestrace_application::{
    ApplicationError, GenerationRequest, GenerationResponse, ModelDataPolicyMode,
    ModelDataPolicySettings, ModelRecord, ModelRepository, ProviderEgress, ProviderError,
    ProviderRepository, RequestContext, ResolvedTextGenerationProvider, TextGenerationProvider,
    TextGenerationProviderFactory,
};
use vestrace_domain::id::{AgentRunId, ModelId, ProviderId, RunStepId};
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{
    DataDestination, DataPolicyId, PrincipalId, ProviderLocality, Sensitivity, WorkspaceId, now,
};
use vestrace_infrastructure::{
    OpenAiCompatibleClient, PgArtifactRepository, PgModelDataPolicyDecisionRepository,
    PgModelExecutionRepository, PgModelRepository, PgProviderRepository, PgStore,
};

struct PersistedBeforeLiveCall {
    client: Arc<OpenAiCompatibleClient>,
    pool: PgPool,
    run_id: AgentRunId,
    step_id: RunStepId,
}

#[async_trait]
impl TextGenerationProvider for PersistedBeforeLiveCall {
    async fn generate(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError> {
        // A different pooled connection must see the row before the actual
        // LM Studio client is invoked. An uncommitted insert cannot satisfy
        // this query, so this observes persistence ordering rather than call
        // order inside a double.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM model_data_policy_decisions
             WHERE run_id = $1 AND step_id = $2 AND verdict = 'allowed'",
        )
        .bind(self.run_id.as_uuid())
        .bind(self.step_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        if count != 1 {
            return Err(ProviderError::Unavailable(format!(
                "expected one committed model data-policy allowance before provider call, found {count}"
            )));
        }
        self.client.generate(request).await
    }
}

struct LiveFactory {
    provider: Arc<dyn TextGenerationProvider>,
    egress: ProviderEgress,
}

#[async_trait]
impl TextGenerationProviderFactory for LiveFactory {
    async fn provider_for(
        &self,
        _: &RequestContext,
    ) -> Result<ResolvedTextGenerationProvider, ApplicationError> {
        Ok(ResolvedTextGenerationProvider {
            provider: self.provider.clone(),
            egress: self.egress.clone(),
        })
    }
}

#[tokio::test]
#[ignore = "needs LM Studio with prism-ml/bonsai-27b loaded"]
async fn openai_compatible_client_reaches_the_named_lm_studio_model() {
    let client = OpenAiCompatibleClient::new("http://localhost:12345/v1", None).unwrap();

    let response = client
        .generate(GenerationRequest {
            model: "prism-ml/bonsai-27b".into(),
            prompt: "Reply with the single word bonsai.".into(),
            max_tokens: Some(4),
        })
        .await
        .unwrap();

    assert_eq!(client.egress().destination(), DataDestination::LocalModel);
    assert_eq!(response.model, "prism-ml/bonsai-27b");
    assert!(response.prompt_tokens > 0);
    assert!(response.completion_tokens > 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL 17 and LM Studio with prism-ml/bonsai-27b loaded"]
async fn loopback_allowance_commits_before_the_real_lm_studio_call(pool: PgPool) {
    let workspace_id = WorkspaceId::new();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("lm-studio-{}", workspace_id.as_uuid()))
        .execute(&pool)
        .await
        .unwrap();
    let context = RequestContext::new(workspace_id, PrincipalId::new());
    let store = PgStore::from_pool(pool.clone());
    let provider_id = ProviderId::new();
    PgProviderRepository::new(store.clone())
        .create(&context, provider_id, "lm-studio", ProviderLocality::Local)
        .await
        .unwrap();
    PgModelRepository::new(store.clone())
        .create(
            &context,
            &ModelRecord {
                id: ModelId::new(),
                provider_id,
                workspace_id,
                model_name: "prism-ml/bonsai-27b".into(),
                context_window: 4096,
                input_cost_per_mtoken: 0.0,
                output_cost_per_mtoken: 0.0,
                created_at: now(),
            },
        )
        .await
        .unwrap();

    let run_id = AgentRunId::new();
    let step_id = RunStepId::new();
    let client = Arc::new(OpenAiCompatibleClient::new("http://localhost:12345/v1", None).unwrap());
    let egress = client.egress().clone();
    assert_eq!(egress.destination(), DataDestination::LocalModel);
    let live_provider = Arc::new(PersistedBeforeLiveCall {
        client,
        pool: pool.clone(),
        run_id,
        step_id,
    });
    let executor = ProviderStepModelExecutor::new(
        Arc::new(LiveFactory {
            provider: live_provider,
            egress,
        }),
        Arc::new(PgModelRepository::new(store.clone())),
        Arc::new(PgArtifactRepository::new(store.clone())),
        Arc::new(PgModelExecutionRepository::new(store.clone())),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store)),
        StepModelSettings {
            model_name: "prism-ml/bonsai-27b".into(),
            max_tokens: Some(4),
        },
        ModelDataPolicySettings {
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "lm-studio-local-v1",
                Sensitivity::Confidential,
                BTreeSet::from([DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            classification: Sensitivity::Confidential,
            mode: ModelDataPolicyMode::Enforce,
        },
    );

    let outcome = executor
        .execute(
            &context,
            StepModelRequest {
                run_id,
                step_id,
                objective: "Reply with the single word bonsai.".into(),
            },
        )
        .await
        .unwrap();

    assert!(outcome.completion_tokens > 0);
    let stored: (String, String, String) = sqlx::query_as(
        "SELECT destination, verdict, policy_version
         FROM model_data_policy_decisions
         WHERE run_id = $1 AND step_id = $2",
    )
    .bind(run_id.as_uuid())
    .bind(step_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        (
            "local_model".into(),
            "allowed".into(),
            "lm-studio-local-v1".into()
        )
    );
}
