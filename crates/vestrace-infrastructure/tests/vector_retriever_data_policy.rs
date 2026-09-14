mod common;

use async_trait::async_trait;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::sync::{Arc, Mutex};
use std::{collections::BTreeSet, str::FromStr, time::Duration};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings,
    NormalizedRetrievalRequest, ProviderEgress, RetrievalRequest, VectorRetriever,
    embedding::EmbeddingRetrievalOutcome, retrieval::EmbeddingProvider,
};
use vestrace_domain::{
    DataDestination, DataPolicyId, MemoryId, Sensitivity, TimePerspective,
    retrieval::ClassificationPolicy, trust::DataPolicy,
};
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

async fn runtime_pool_single(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .expect("single-session runtime pool must connect to the SQLx test database")
}
async fn ready_fixture(pool: &PgPool) -> (common::canonical_memory_fixture::Corpus, MemoryId) {
    let mut corpus = common::canonical_memory_fixture::new(pool, "vector-policy").await;
    let workspace = corpus.accepted.context.workspace_id;
    let memory = MemoryId::new();
    let revision = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)").bind(memory.as_uuid()).bind(workspace.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,'governed query',1.0,0.5)").bind(revision).bind(memory.as_uuid()).bind(workspace.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision)
        .bind(memory.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    corpus.publish_memories(pool, &[memory]).await;
    (corpus, memory)
}
fn request(corpus: &common::canonical_memory_fixture::Corpus) -> NormalizedRetrievalRequest {
    NormalizedRetrievalRequest::normalize_with_pin(
        RetrievalRequest::new(corpus.accepted.context.workspace_id, "governed query")
            .with_time_perspective(TimePerspective::AllHistory),
        corpus.space_key.clone(),
        corpus.generation,
    )
    .unwrap()
}
#[sqlx::test(migrations = false)]
async fn vector_search_gates_the_query_without_changing_the_retrieval_call(pool: PgPool) {
    let (corpus, memory) = ready_fixture(&pool).await;
    let request = request(&corpus);
    let (outcome, calls) =
        common::canonical_query_fixture::retrieve(&pool, &corpus, &request).await;
    let EmbeddingRetrievalOutcome::Completed(candidates) = outcome else {
        panic!("canonical retrieval must complete")
    };
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].memory_id, memory);
    assert_eq!(
        calls, 1,
        "the HTTP provider must receive exactly the actual query"
    );
    let decisions:Vec<(String,uuid::Uuid,String)>=sqlx::query_as("SELECT purpose,causal_reference_id,verdict FROM embedding_data_policy_decisions WHERE causal_reference_id=$1").bind(request.request_id.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(
        decisions,
        vec![(
            "retrieval_query".to_owned(),
            request.request_id.as_uuid(),
            "allowed".to_owned()
        )]
    );
    let causal_chain:i64=sqlx::query_scalar("SELECT count(*) FROM embedding_retrieval_fences fence JOIN embedding_jobs job ON job.workspace_id=fence.workspace_id AND job.id=fence.job_id JOIN model_request_evidence_roots evidence ON evidence.workspace_id=job.workspace_id AND evidence.id=job.model_request_evidence_id AND evidence.cause_kind='embedding_job' AND evidence.cause_id=job.id WHERE fence.workspace_id=$1 AND fence.request_id=$2 AND fence.generation_id=$3")
        .bind(corpus.accepted.context.workspace_id.as_uuid()).bind(request.request_id.as_uuid()).bind(corpus.generation.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        causal_chain, 1,
        "request, job, canonical generation and actual MRE must share one cause"
    );
}
#[sqlx::test(migrations = false)]
async fn retired_inline_adapter_never_calls_the_provider(pool: PgPool) {
    let (corpus, _) = ready_fixture(&pool).await;
    let calls = Arc::new(Mutex::new(0));
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
        Arc::new(Provider(calls.clone())),
        ProviderEgress::new(
            "http://127.0.0.1:1234/v1/embeddings",
            DataDestination::LocalModel,
            true,
            true,
        ),
    );
    let retriever = PgVectorRetriever::new(
        PgStore::from_pool(corpus.runtime.clone()),
        governed,
        "vector-policy",
    );
    assert!(
        retriever
            .search(&corpus.accepted.context, &request(&corpus))
            .await
            .is_err()
    );
    assert_eq!(*calls.lock().unwrap(), 0);
    assert!(decisions.0.lock().unwrap().is_empty());
}
#[sqlx::test(migrations = false)]
async fn policy_fixture_registers_and_publishes_its_generation(pool: PgPool) {
    let (corpus, _) = ready_fixture(&pool).await;
    let state: String = sqlx::query_scalar(
        "SELECT state FROM embedding_corpus_generations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(corpus.accepted.context.workspace_id.as_uuid())
    .bind(corpus.generation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "ready");
}
#[sqlx::test(migrations = false)]
async fn policy_fixture_enrols_through_the_store_before_publication(pool: PgPool) {
    let (corpus, _) = ready_fixture(&pool).await;
    let mut tx = corpus.runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(
        &mut tx,
        corpus.accepted.context.workspace_id.as_uuid(),
    )
    .await;
    let count:i64=sqlx::query_scalar("SELECT count(DISTINCT revision_id) FROM vestrace_resolve_embedding_memory_references($1,$2,NULL)").bind(corpus.accepted.context.workspace_id.as_uuid()).bind(corpus.generation.as_uuid()).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = false)]
async fn governed_query_denial_records_the_request_and_prevents_network(pool: PgPool) {
    let (corpus, _) = ready_fixture(&pool).await;
    let request = request(&corpus);
    let (_, calls) =
        common::canonical_query_fixture::retrieve_with_policy(&pool, &corpus, &request, false)
            .await;
    assert_eq!(calls, 0);
    let records:Vec<(String,String,i32)>=sqlx::query_as("SELECT purpose,verdict,input_count FROM embedding_data_policy_decisions WHERE causal_reference_id=$1")
        .bind(request.request_id.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(
        records,
        vec![("retrieval_query".into(), "denied".into(), 1)]
    );
    let dispatches:i64=sqlx::query_scalar("SELECT count(*) FROM embedding_retrieval_fences f JOIN embedding_jobs j ON j.workspace_id=f.workspace_id AND j.id=f.job_id JOIN external_effect_lifecycle_transitions t ON t.workspace_id=j.workspace_id AND t.effect_id=j.external_effect_id WHERE f.workspace_id=$1 AND f.request_id=$2 AND t.status='dispatching'")
        .bind(corpus.accepted.context.workspace_id.as_uuid()).bind(request.request_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        dispatches, 0,
        "policy denial precedes irreversible Dispatching"
    );
}
#[sqlx::test(migrations = false)]
async fn decision_record_failure_prevents_query_network(pool: PgPool) {
    let (corpus, _) = ready_fixture(&pool).await;
    let request = request(&corpus);
    let (_, calls) =
        common::canonical_query_fixture::retrieve_with_recording_failure(&pool, &corpus, &request)
            .await;
    assert_eq!(calls, 0, "a missing disclosure record must prevent HTTP");
}

#[sqlx::test(migrations = false)]
async fn retrieval_policy_recording_never_checks_out_a_second_dispatch_connection(pool: PgPool) {
    let (corpus, _) = ready_fixture(&pool).await;
    let request = request(&corpus);
    let runtime = runtime_pool_single(&pool).await;
    let (outcome, calls) = tokio::time::timeout(
        Duration::from_secs(10),
        common::canonical_query_fixture::retrieve_on_runtime(&runtime, &corpus, &request),
    )
    .await
    .expect("a single runtime connection must complete policy recording and dispatch");
    assert!(
        matches!(outcome, EmbeddingRetrievalOutcome::Completed(_)),
        "the single-session query must complete: {outcome:?}"
    );
    assert_eq!(calls, 1, "one allowed query reaches the provider once");
    let records: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT causal_reference_id, verdict FROM embedding_data_policy_decisions \
         WHERE causal_reference_id=$1",
    )
    .bind(request.request_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        records,
        vec![(request.request_id.as_uuid(), "allowed".to_owned())]
    );
    runtime.close().await;
}
