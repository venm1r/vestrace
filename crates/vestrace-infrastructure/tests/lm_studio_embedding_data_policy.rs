//! Deliberate live acceptance against PostgreSQL and the configured LM Studio
//! embedding model. Ignored in the ordinary suite because both are external
//! prerequisites.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::retrieval::{EmbeddingProvider, SharedEmbeddingProvider};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings,
};
use vestrace_domain::id::RetrievalRunId;
use vestrace_domain::retrieval::ClassificationPolicy;
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{DataDestination, DataPolicyId, Sensitivity};
use vestrace_infrastructure::{
    OpenAiCompatibleEmbeddingClient, PgEmbeddingDataPolicyDecisionRepository, PgStore,
};

struct PersistedBeforeLiveCall {
    client: Arc<OpenAiCompatibleEmbeddingClient>,
    pool: PgPool,
    request_id: RetrievalRunId,
}

#[async_trait]
impl EmbeddingProvider for PersistedBeforeLiveCall {
    fn model(&self) -> &str {
        "text-embedding-nomic-embed-text-v1.5"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM embedding_data_policy_decisions
             WHERE purpose = 'retrieval_query'
               AND causal_reference_id = $1
               AND verdict = 'allowed'",
        )
        .bind(self.request_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        if count != 1 {
            return Err(ApplicationError::Storage(format!(
                "expected one committed embedding allowance before provider call, found {count}"
            )));
        }
        self.client.embed(inputs).await
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL 17 and LM Studio with text-embedding-nomic-embed-text-v1.5 loaded"]
async fn governed_query_commits_before_the_real_lm_studio_round_trip(pool: PgPool) {
    let client = Arc::new(
        OpenAiCompatibleEmbeddingClient::new(
            "http://localhost:12345/v1",
            "text-embedding-nomic-embed-text-v1.5",
            None,
        )
        .unwrap(),
    );
    let egress = client.egress().clone();
    assert_eq!(egress.destination(), DataDestination::LocalModel);
    let request_id = RetrievalRunId::new();
    let raw: SharedEmbeddingProvider = Arc::new(PersistedBeforeLiveCall {
        client,
        pool: pool.clone(),
        request_id,
    });
    let gate = EmbeddingDataPolicyGate::new(
        EmbeddingDataPolicySettings {
            classification_policy: ClassificationPolicy::new(Vec::<String>::new(), true).unwrap(),
            classification: Sensitivity::Confidential,
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "lm-studio-embedding-v1",
                Sensitivity::Confidential,
                BTreeSet::from([DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            mode: EmbeddingDataPolicyMode::Enforce,
        },
        Arc::new(PgEmbeddingDataPolicyDecisionRepository::new(
            PgStore::from_pool(pool.clone()),
        )),
    );
    let provider = gate.govern(raw, egress);

    let embeddings = provider
        .embed_retrieval_query(request_id, "A governed live embedding round trip.")
        .await
        .unwrap();

    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].len(), 768);
    assert!(embeddings[0].iter().any(|component| *component != 0.0));
    let stored: (String, String, i32) = sqlx::query_as(
        "SELECT purpose, verdict, input_count
         FROM embedding_data_policy_decisions
         WHERE causal_reference_id = $1",
    )
    .bind(request_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, ("retrieval_query".into(), "allowed".into(), 1));
}
