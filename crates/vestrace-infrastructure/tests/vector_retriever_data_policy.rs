use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sqlx::PgPool;
use vestrace_application::retrieval::{EmbeddingProvider, EmbeddingStore, SharedEmbeddingProvider};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings,
    EmbeddingPurpose, NormalizedRetrievalRequest, ProviderEgress, RequestContext, RetrievalRequest,
    VectorRetriever,
};
use vestrace_domain::retrieval::ClassificationPolicy;
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{
    CorpusGenerationId, DataDestination, DataPolicyId, MemoryId, MemoryRevisionId, PrincipalId,
    Sensitivity, TimePerspective, WorkspaceId, embedding::EmbeddingSpaceKey,
};
use vestrace_infrastructure::{PgEmbeddingStore, PgStore, PgVectorRetriever};

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

async fn ready_fixture(
    pool: &PgPool,
) -> (
    RequestContext,
    vestrace_application::retrieval::EmbeddingSpace,
    CorpusGenerationId,
) {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("vector-policy-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,'vector-policy')")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)").bind(memory_id.as_uuid()).bind(context.workspace_id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,'governed query',1.0,0.5)").bind(revision_id.as_uuid()).bind(memory_id.as_uuid()).bind(context.workspace_id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision_id.as_uuid())
        .bind(memory_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DO $$ BEGIN EXECUTE format('ALTER FUNCTION public.vestrace_validate_embedding_corpus_generation_member() OWNER TO %I', current_user); END $$").execute(pool).await.unwrap();
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(&context, "space", "vector-query-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space, memory_id, &[0.1, 0.2])
        .await
        .unwrap();
    let registration: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id=$1 AND space_id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space.id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let generation: uuid::Uuid = sqlx::query_scalar("SELECT id FROM embedding_corpus_generations WHERE workspace_id=$1 AND space_registration_id=$2 AND state='building'").bind(context.workspace_id.as_uuid()).bind(registration).fetch_one(pool).await.unwrap();
    let mut publish = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *publish)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i64>("SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,1)")
        .bind(generation)
        .bind(context.workspace_id.as_uuid())
        .bind(registration)
        .fetch_one(&mut *publish)
        .await
        .unwrap();
    publish.commit().await.unwrap();
    (context, space, CorpusGenerationId::from_uuid(generation))
}

#[sqlx::test(migrations = "../../migrations")]
async fn vector_search_gates_the_query_without_changing_the_retrieval_call(pool: PgPool) {
    let (context, space, generation) = ready_fixture(&pool).await;
    let normalized = NormalizedRetrievalRequest::normalize_with_pin(
        RetrievalRequest::new(context.workspace_id, "governed query")
            .with_time_perspective(TimePerspective::AllHistory),
        EmbeddingSpaceKey::new(
            context.workspace_id,
            space.name.clone(),
            space.model.clone(),
            space.dimensions,
        )
        .unwrap(),
        generation,
    )
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
            "http://127.0.0.1:12345/v1/embeddings",
            DataDestination::LocalModel,
            true,
            true,
        ),
    );
    let retriever = PgVectorRetriever::new(PgStore::from_pool(pool), governed, "space");

    let result = retriever.search(&context, &normalized).await.unwrap();

    assert_eq!(
        result.len(),
        1,
        "the published fixture has one vector candidate"
    );
    assert_eq!(*calls.lock().unwrap(), 1);
    let records = decisions.0.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].purpose, EmbeddingPurpose::RetrievalQuery);
    assert_eq!(records[0].causal_reference_id, request_id.as_uuid());
}

#[sqlx::test(migrations = "../../migrations")]
async fn policy_fixture_registers_and_publishes_its_generation(pool: PgPool) {
    let (context, _space, generation) = ready_fixture(&pool).await;
    let state: String = sqlx::query_scalar(
        "SELECT state FROM embedding_corpus_generations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(generation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "ready");
}

#[sqlx::test(migrations = "../../migrations")]
async fn policy_fixture_enrols_through_the_store_before_publication(pool: PgPool) {
    let (context, _space, generation) = ready_fixture(&pool).await;
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM embedding_corpus_generation_members WHERE workspace_id=$1 AND corpus_generation_id=$2")
        .bind(context.workspace_id.as_uuid()).bind(generation.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(members, 1);
}
