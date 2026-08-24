use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::retrieval::{EmbeddingProvider, SharedEmbeddingProvider};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings,
    EmbeddingPurpose, NormalizedRetrievalRequest, ProviderEgress, RequestContext, RetrievalRequest,
    VectorRetriever,
};
use vestrace_domain::retrieval::ClassificationPolicy;
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{DataDestination, DataPolicyId, PrincipalId, Sensitivity, WorkspaceId};
use vestrace_infrastructure::{PgStore, PgVectorRetriever};

struct Provider(Arc<Mutex<u32>>);

#[async_trait]
impl EmbeddingProvider for Provider {
    fn model(&self) -> &str {
        "vector-query-model"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
        *self.0.lock().unwrap() += 1;
        Ok(inputs.iter().map(|_| vec![0.1, 0.2]).collect())
    }
}

#[derive(Default)]
struct Decisions(Mutex<Vec<EmbeddingDataPolicyDecisionRecord>>);

#[async_trait]
impl EmbeddingDataPolicyDecisionRepository for Decisions {
    async fn record(
        &self,
        record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        self.0.lock().unwrap().push(record.clone());
        Ok(())
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn vector_search_gates_the_query_without_changing_the_retrieval_call(pool: PgPool) {
    let workspace_id = WorkspaceId::new();
    let context = RequestContext::new(workspace_id, PrincipalId::new());
    let normalized = NormalizedRetrievalRequest::normalize(RetrievalRequest::new(
        workspace_id,
        "governed query",
    ))
    .unwrap();
    let request_id = normalized.request_id;
    let calls = Arc::new(Mutex::new(0));
    let raw: SharedEmbeddingProvider = Arc::new(Provider(calls.clone()));
    let decisions = Arc::new(Decisions::default());
    let gate = EmbeddingDataPolicyGate::new(
        EmbeddingDataPolicySettings {
            classification_policy: ClassificationPolicy::new(Vec::<String>::new(), true).unwrap(),
            classification: Sensitivity::Internal,
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "vector-query-policy-v1",
                Sensitivity::Internal,
                BTreeSet::from([DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            mode: EmbeddingDataPolicyMode::Enforce,
        },
        decisions.clone(),
    );
    let governed = gate.govern(
        raw,
        ProviderEgress::new(
            "http://localhost:12345/v1/embeddings",
            DataDestination::LocalModel,
            true,
            true,
        ),
    );
    let retriever = PgVectorRetriever::new(PgStore::from_pool(pool), governed, "space");

    let result = retriever.search(&context, &normalized).await.unwrap();

    assert!(result.is_empty(), "no vector space was provisioned");
    assert_eq!(*calls.lock().unwrap(), 1);
    let records = decisions.0.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].purpose, EmbeddingPurpose::RetrievalQuery);
    assert_eq!(records[0].causal_reference_id, request_id.as_uuid());
}
