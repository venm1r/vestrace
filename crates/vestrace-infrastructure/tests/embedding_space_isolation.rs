use std::{collections::BTreeSet, str::FromStr, sync::Arc};

use async_trait::async_trait;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings,
    NormalizedRetrievalRequest, ProviderDispatchRepository, ProviderEgress, RequestContext,
    RetrievalRequest, VectorRetriever,
    retrieval::{EmbeddingProvider, EmbeddingStore},
};
use vestrace_domain::{
    CorpusGenerationId, DataDestination, DataPolicyId, MemoryId, MemoryRevisionId, PrincipalId,
    Sensitivity, TimePerspective, WorkspaceId, embedding::EmbeddingSpaceKey,
    retrieval::ClassificationPolicy, trust::DataPolicy,
};
use vestrace_infrastructure::{PgEmbeddingStore, PgStore, PgVectorRetriever};

#[path = "common/mod.rs"]
mod common;

struct SameWireModel;

#[async_trait]
impl EmbeddingProvider for SameWireModel {
    fn model(&self) -> &str {
        "same-wire-model"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
        Ok(inputs.iter().map(|_| vec![1.0, 0.0]).collect())
    }
}

#[derive(Default)]
struct Decisions;

#[async_trait]
impl EmbeddingDataPolicyDecisionRepository for Decisions {
    async fn record(
        &self,
        _record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

fn query_provider() -> vestrace_application::SharedGovernedEmbeddingProvider {
    EmbeddingDataPolicyGate::new(
        EmbeddingDataPolicySettings {
            classification_policy: ClassificationPolicy::new(Vec::<String>::new(), true).unwrap(),
            classification: Sensitivity::Internal,
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "same-wire-model-policy",
                Sensitivity::Internal,
                BTreeSet::from([DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            mode: EmbeddingDataPolicyMode::Enforce,
        },
        Arc::new(Decisions),
    )
    .govern(
        Arc::new(SameWireModel),
        ProviderEgress::new(
            "http://127.0.0.1:12345/v1/embeddings",
            DataDestination::LocalModel,
            true,
            true,
        ),
    )
}

async fn runtime_pool(source: &PgPool) -> PgPool {
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
        .max_connections(2)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .expect("runtime must connect to the SQLx test database")
}

async fn seed_context(pool: &PgPool) -> RequestContext {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("same-wire-model-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(format!("same-wire-model-{}", context.principal_id))
        .execute(pool)
        .await
        .unwrap();
    context
}

async fn seed_memory(pool: &PgPool, context: &RequestContext, content: &str) -> MemoryId {
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)")
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,$4,1.0,0.5)")
        .bind(revision_id.as_uuid())
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(content)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision_id.as_uuid())
        .bind(memory_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO search_documents(id,memory_id,workspace_id,title,content) VALUES($1,$2,$3,'',$4)")
        .bind(Uuid::now_v7())
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(content)
        .execute(pool)
        .await
        .unwrap();
    memory_id
}

async fn registration(pool: &PgPool, context: &RequestContext, space_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id=$1 AND space_id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn building_generation(
    pool: &PgPool,
    context: &RequestContext,
    registration_id: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM embedding_corpus_generations \
         WHERE workspace_id=$1 AND space_registration_id=$2 AND state='building'",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(registration_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn publish_generation(
    runtime: &PgPool,
    context: &RequestContext,
    generation_id: Uuid,
    registration_id: Uuid,
) -> CorpusGenerationId {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i64>("SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,0)")
        .bind(generation_id)
        .bind(context.workspace_id.as_uuid())
        .bind(registration_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    CorpusGenerationId::from_uuid(generation_id)
}

fn request(
    context: &RequestContext,
    space: &vestrace_application::retrieval::EmbeddingSpace,
    generation: CorpusGenerationId,
) -> NormalizedRetrievalRequest {
    NormalizedRetrievalRequest::normalize_with_pin(
        RetrievalRequest::new(context.workspace_id, "same wire model")
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
    .unwrap()
}

struct TwoSpaces {
    context: RequestContext,
    first: vestrace_application::retrieval::EmbeddingSpace,
    first_registration: Uuid,
    second_registration: Uuid,
    first_generation: Uuid,
    second_generation: Uuid,
    first_memory: MemoryId,
    second_memory: MemoryId,
    second_embedding: Uuid,
}

async fn two_same_wire_model_spaces(pool: &PgPool, runtime: &PgPool) -> TwoSpaces {
    let context = seed_context(pool).await;
    common::prepare_legacy_embedding_runtime_ownership(pool).await;
    let first_memory = seed_memory(pool, &context, "first same-wire candidate").await;
    let second_memory = seed_memory(pool, &context, "second same-wire candidate").await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(runtime.clone()));
    let first = store
        .ensure_space(&context, "same-wire-first", "same-wire-model", 2)
        .await
        .unwrap();
    let second = store
        .ensure_space(&context, "same-wire-second", "same-wire-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &first, first_memory, &[1.0, 0.0])
        .await
        .unwrap();
    store
        .upsert(&context, &second, second_memory, &[1.0, 0.0])
        .await
        .unwrap();
    let first_registration = registration(pool, &context, first.id.as_uuid()).await;
    let second_registration = registration(pool, &context, second.id.as_uuid()).await;
    let first_generation = building_generation(pool, &context, first_registration).await;
    let second_generation = building_generation(pool, &context, second_registration).await;
    let second_embedding: Uuid = sqlx::query_scalar(
        "SELECT id FROM memory_embeddings WHERE workspace_id=$1 AND memory_id=$2 AND space_id=$3",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(second_memory.as_uuid())
    .bind(second.id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    TwoSpaces {
        context,
        first,
        first_registration,
        second_registration,
        first_generation,
        second_generation,
        first_memory,
        second_memory,
        second_embedding,
    }
}

fn assert_exact_check_violation<T: std::fmt::Debug>(result: Result<T, sqlx::Error>, message: &str) {
    let error = result.expect_err("the cross-space operation must be refused");
    let database = error
        .as_database_error()
        .expect("the refusal must be from PostgreSQL");
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert_eq!(database.message(), message);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_vector_written_to_one_space_cannot_enrol_in_the_other(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = two_same_wire_model_spaces(&pool, &runtime).await;
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    assert_exact_check_violation(
        sqlx::query("SELECT vestrace_enrol_embedding_corpus_generation_member($1,$2,$3)")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.first_generation)
            .bind(fixture.second_embedding)
            .execute(&mut *transaction)
            .await,
        "embedding corpus member must belong to its generation space",
    );
    transaction.rollback().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_generation_of_one_same_wire_space_is_not_a_valid_pin_for_the_other(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = two_same_wire_model_spaces(&pool, &runtime).await;
    let second_generation = publish_generation(
        &runtime,
        &fixture.context,
        fixture.second_generation,
        fixture.second_registration,
    )
    .await;
    let retriever = PgVectorRetriever::new(
        PgStore::from_pool(runtime.clone()),
        query_provider(),
        "same-wire-first",
    );
    let error = retriever
        .search(
            &fixture.context,
            &request(&fixture.context, &fixture.first, second_generation),
        )
        .await
        .expect_err("a generation from the other same-wire space must be refused");
    assert_eq!(
        error.to_string(),
        format!(
            "unavailable: embedding corpus generation {second_generation} does not exist for space same-wire-first"
        )
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_embedding_job_dispatch_plan_remains_pinned_to_its_registered_space(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = common::accept_embedding_job(&pool, &runtime).await;
    let other_space = PgEmbeddingStore::new(PgStore::from_pool(runtime.clone()))
        .ensure_space(
            &fixture.context,
            "same-wire-other-job-space",
            "text-embedding-nomic-embed-text-v1.5",
            768,
        )
        .await
        .unwrap();
    let other_registration = registration(&pool, &fixture.context, other_space.id.as_uuid()).await;
    common::make_dispatchable(&pool, &runtime, &fixture).await;
    let plan = common::dispatch_repository(&runtime)
        .load_embedding_dispatch_plan(&fixture.context, fixture.job_id)
        .await
        .expect("the job has one dispatch plan");
    assert_eq!(
        plan.attempt.space_registration_id.as_uuid(),
        fixture.space_registration_id
    );
    assert_ne!(
        plan.attempt.space_registration_id.as_uuid(),
        other_registration,
        "dispatch derives the job's registration and cannot be redirected to the other same-wire space"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_retrieval_fenced_to_one_same_wire_space_returns_no_candidate_of_the_other(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = two_same_wire_model_spaces(&pool, &runtime).await;
    let first_generation = publish_generation(
        &runtime,
        &fixture.context,
        fixture.first_generation,
        fixture.first_registration,
    )
    .await;
    let retriever = PgVectorRetriever::new(
        PgStore::from_pool(runtime.clone()),
        query_provider(),
        "same-wire-first",
    );
    let candidates = retriever
        .search(
            &fixture.context,
            &request(&fixture.context, &fixture.first, first_generation),
        )
        .await
        .unwrap();
    assert!(
        !candidates.is_empty(),
        "the fenced retrieval must name its first-space candidate"
    );
    for candidate in &candidates {
        assert_eq!(candidate.corpus_generation_id, first_generation);
        assert_eq!(candidate.memory_id, fixture.first_memory);
        assert_ne!(
            candidate.memory_id, fixture.second_memory,
            "second-space candidate {} leaked through first-space retrieval",
            fixture.second_memory
        );
    }
    runtime.close().await;
}
