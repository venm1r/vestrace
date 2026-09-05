//! PostgreSQL evidence that a retrieval request is fenced to one corpus generation.

use std::{borrow::Cow, collections::BTreeSet, str::FromStr, sync::Arc};

use async_trait::async_trait;
use sqlx::{
    PgPool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyGate, EmbeddingDataPolicyMode, EmbeddingDataPolicySettings,
    NormalizedRetrievalRequest, ProviderEgress, PurgeRepository, RequestContext, RetrievalRequest,
    TextRetriever, VectorRetriever,
    retrieval::{CorpusGenerationResolver, EmbeddingProvider, EmbeddingStore},
};
use vestrace_domain::{
    CorpusGenerationId, DataDestination, DataPolicyId, MemoryId, MemoryRevisionId, PrincipalId,
    Sensitivity, TimePerspective, WorkspaceId, embedding::EmbeddingSpaceKey,
    retrieval::ClassificationPolicy, trust::DataPolicy,
};
use vestrace_infrastructure::{
    PgCorpusGenerationResolver, PgEmbeddingStore, PgPurgeRepository, PgStore, PgTextRetriever,
    PgVectorRetriever,
};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const PROVISIONER: &str = include_str!("../../../docker/postgres/init-runtime-role.sh");

struct QueryProvider;

#[async_trait]
impl EmbeddingProvider for QueryProvider {
    fn model(&self) -> &str {
        "generation-fence-model"
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
    let gate = EmbeddingDataPolicyGate::new(
        EmbeddingDataPolicySettings {
            classification_policy: ClassificationPolicy::new(Vec::<String>::new(), true).unwrap(),
            classification: Sensitivity::Internal,
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "generation-fence-policy",
                Sensitivity::Internal,
                BTreeSet::from([DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            mode: EmbeddingDataPolicyMode::Enforce,
        },
        Arc::new(Decisions),
    );
    gate.govern(
        Arc::new(QueryProvider),
        ProviderEgress::new(
            "http://127.0.0.1:12345/v1/embeddings",
            DataDestination::LocalModel,
            true,
            true,
        ),
    )
}

async fn seed_context(pool: &PgPool) -> RequestContext {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("generation-fence-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,'generation-fence')",
    )
    .bind(context.principal_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    context
}

async fn seed_memory(pool: &PgPool, context: &RequestContext) -> MemoryId {
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)")
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,'generation fence',1.0,0.5)")
        .bind(revision_id.as_uuid())
        .bind(memory_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision_id.as_uuid())
        .bind(memory_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO search_documents(id,memory_id,workspace_id,title,content) \
         VALUES($1,$2,$3,'','generation fence')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(memory_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    memory_id
}

fn provisioner_sql_from(marker: &str) -> &'static str {
    let start = PROVISIONER
        .find(marker)
        .unwrap_or_else(|| panic!("missing provisioner marker {marker}"));
    PROVISIONER[start..]
        .rsplit_once("\nSQL\n")
        .map(|(sql, _)| sql)
        .expect("the provisioner must contain the SQL heredoc terminator")
}

async fn install_extensions_from_real_provisioner(pool: &PgPool) {
    let statements = PROVISIONER
        .lines()
        .filter(|line| line.starts_with("CREATE EXTENSION IF NOT EXISTS "))
        .collect::<Vec<_>>()
        .join("\n");
    sqlx::raw_sql(&statements).execute(pool).await.unwrap();
}

async fn hand_database_to_runtime(pool: &PgPool) {
    sqlx::query("ALTER SCHEMA public OWNER TO vestrace")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I OWNER TO vestrace', current_database()); END $$",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL").unwrap();
    let parsed = PgConnectOptions::from_str(&runtime_url).unwrap();
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .unwrap();
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
        .unwrap()
}

async fn publish_building_generation(
    pool: &PgPool,
    context: &RequestContext,
    space_id: uuid::Uuid,
) -> CorpusGenerationId {
    let registration_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id = $1 AND space_id = $2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let generation_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_corpus_generations \
         WHERE workspace_id = $1 AND space_registration_id = $2 AND state = 'building'",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(registration_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut publish = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *publish)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_embedding_corpus_generation($1, $2, $3, 0)",
    )
    .bind(generation_id)
    .bind(context.workspace_id.as_uuid())
    .bind(registration_id)
    .fetch_one(&mut *publish)
    .await
    .unwrap();
    publish.commit().await.unwrap();
    CorpusGenerationId::from_uuid(generation_id)
}

fn request(
    context: &RequestContext,
    space: &vestrace_application::retrieval::EmbeddingSpace,
    generation: CorpusGenerationId,
) -> NormalizedRetrievalRequest {
    NormalizedRetrievalRequest::normalize_with_pin(
        RetrievalRequest::new(context.workspace_id, "generation fence")
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

fn request_with_limit(
    context: &RequestContext,
    space: &vestrace_application::retrieval::EmbeddingSpace,
    generation: CorpusGenerationId,
    limit: u32,
) -> NormalizedRetrievalRequest {
    let mut request = request(context, space, generation);
    request.channel_limit = limit;
    request
}

fn assert_candidate_generations(
    candidates: &[vestrace_domain::RetrievalCandidate],
    expected: CorpusGenerationId,
    channel: &str,
) {
    assert!(
        !candidates.is_empty(),
        "{channel} returned no candidates, so it did not prove its fence"
    );
    for candidate in candidates {
        assert_eq!(
            candidate.corpus_generation_id, expected,
            "{channel} candidate {} carried generation {} instead of requested generation {expected}",
            candidate.memory_id, candidate.corpus_generation_id,
        );
    }
}

fn assert_exact_database_error<T: std::fmt::Debug>(
    result: Result<T, sqlx::Error>,
    code: &str,
    message: &str,
) {
    let error = result.expect_err("the database operation must be refused");
    let database = error
        .as_database_error()
        .unwrap_or_else(|| panic!("expected a database refusal, got {error}"));
    assert_eq!(database.code().as_deref(), Some(code), "{error}");
    assert_eq!(database.message(), message, "{error}");
}

fn assert_unavailable_without_candidates(
    result: Result<Vec<vestrace_domain::RetrievalCandidate>, ApplicationError>,
    message: String,
) {
    assert!(
        result.as_ref().map_or(true, Vec::is_empty),
        "a refusal must not return candidates: {result:?}"
    );
    assert_eq!(
        result.expect_err("the pin must be refused").to_string(),
        message
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_ready_generation_of_another_space_is_not_a_valid_pin(pool: PgPool) {
    let context = seed_context(&pool).await;
    let first_memory = seed_memory(&pool, &context).await;
    let second_memory = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space_a = store
        .ensure_space(&context, "generation-fence-a", "generation-fence-model", 2)
        .await
        .unwrap();
    let space_b = store
        .ensure_space(&context, "generation-fence-b", "generation-fence-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space_a, first_memory, &[1.0, 0.0])
        .await
        .unwrap();
    store
        .upsert(&context, &space_b, second_memory, &[1.0, 0.0])
        .await
        .unwrap();
    let _generation_a = publish_building_generation(&pool, &context, space_a.id.as_uuid()).await;
    let generation_b = publish_building_generation(&pool, &context, space_b.id.as_uuid()).await;

    let retriever = PgVectorRetriever::new(
        PgStore::from_pool(pool),
        query_provider(),
        "generation-fence-a",
    );
    let error = retriever
        .search(&context, &request(&context, &space_a, generation_b))
        .await
        .expect_err("a generation of another space must be refused");
    assert_eq!(
        error.to_string(),
        format!(
            "unavailable: embedding corpus generation {generation_b} does not exist for space generation-fence-a"
        )
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_ready_generation_fences_each_channel_fusion_and_rerank(pool: PgPool) {
    let context = seed_context(&pool).await;
    let stale_member = seed_memory(&pool, &context).await;
    let first_ready_member = seed_memory(&pool, &context).await;
    let second_ready_member = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();

    // This row is closer to the query than the first Ready row. The requested
    // generation has two members and the channel limit is three, so removing
    // the predicate has room to expose this stale-generation intruder.
    store
        .upsert(&context, &space, stale_member, &[1.0, 0.0])
        .await
        .unwrap();
    let stale_generation = publish_building_generation(&pool, &context, space.id.as_uuid()).await;
    store
        .upsert(&context, &space, first_ready_member, &[0.0, 1.0])
        .await
        .unwrap();
    store
        .upsert(&context, &space, second_ready_member, &[0.8, 0.6])
        .await
        .unwrap();
    let ready_generation = publish_building_generation(&pool, &context, space.id.as_uuid()).await;

    let request = request_with_limit(&context, &space, ready_generation, 3);
    assert!(
        request.channel_limit > 2,
        "the channel limit must leave room beyond the Ready generation's two rows"
    );
    let stale_distance: f64 = sqlx::query_scalar(
        "SELECT embedding <=> '[1,0]'::vector FROM memory_embeddings \
         WHERE workspace_id = $1 AND memory_id = $2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(stale_member.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let ready_distance: f64 = sqlx::query_scalar(
        "SELECT embedding <=> '[1,0]'::vector FROM memory_embeddings \
         WHERE workspace_id = $1 AND memory_id = $2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(first_ready_member.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        stale_distance < ready_distance,
        "the stale member must rank inside an unfenced channel limit"
    );

    let vector = PgVectorRetriever::new(
        PgStore::from_pool(pool.clone()),
        query_provider(),
        "generation-fence",
    );
    let text = PgTextRetriever::new(PgStore::from_pool(pool));
    let vector_candidates = vector.search(&context, &request).await.unwrap();
    let text_candidates = text.search(&context, &request).await.unwrap();
    assert_candidate_generations(&vector_candidates, ready_generation, "vector");
    assert_candidate_generations(&text_candidates, ready_generation, "text");
    let fused = vestrace_application::retrieval::reciprocal_rank_fusion_pinned(
        &[vector_candidates, text_candidates],
        60.0,
        ready_generation,
    )
    .unwrap();
    assert_candidate_generations(&fused, ready_generation, "fusion");
    let reranked = vestrace_application::rerank(fused);
    let reranked_candidates = reranked
        .iter()
        .map(|ranked| ranked.candidate.clone())
        .collect::<Vec<_>>();
    assert_candidate_generations(&reranked_candidates, ready_generation, "rerank");
    assert_ne!(stale_generation, ready_generation);
}

#[sqlx::test(migrations = "../../migrations")]
async fn membership_cannot_bind_an_embedding_from_another_space(pool: PgPool) {
    let context = seed_context(&pool).await;
    let first_memory = seed_memory(&pool, &context).await;
    let second_memory = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space_a = store
        .ensure_space(&context, "generation-fence-a", "generation-fence-model", 2)
        .await
        .unwrap();
    let space_b = store
        .ensure_space(&context, "generation-fence-b", "generation-fence-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space_a, first_memory, &[1.0, 0.0])
        .await
        .unwrap();
    store
        .upsert(&context, &space_b, second_memory, &[0.0, 1.0])
        .await
        .unwrap();
    let generation_a: uuid::Uuid = sqlx::query_scalar(
        "SELECT generation.id FROM embedding_corpus_generations generation
          JOIN embedding_space_registrations registration
            ON registration.workspace_id = generation.workspace_id
           AND registration.id = generation.space_registration_id
         WHERE generation.workspace_id = $1 AND registration.space_id = $2 AND generation.state = 'building'",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space_a.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let embedding_b: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM memory_embeddings WHERE workspace_id = $1 AND memory_id = $2 AND space_id = $3",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(second_memory.as_uuid())
    .bind(space_b.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut enrolled = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *enrolled)
        .await
        .unwrap();
    assert_exact_database_error(
        sqlx::query("SELECT vestrace_enrol_embedding_corpus_generation_member($1,$2,$3)")
            .bind(context.workspace_id.as_uuid())
            .bind(generation_a)
            .bind(embedding_b)
            .execute(&mut *enrolled)
            .await,
        "23514",
        "embedding corpus member must belong to its generation space",
    );
    enrolled.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_missing_generation_for_the_space_returns_no_candidates_and_its_exact_refusal(
    pool: PgPool,
) {
    let context = seed_context(&pool).await;
    let memory = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space, memory, &[1.0, 0.0])
        .await
        .unwrap();
    let _ready = publish_building_generation(&pool, &context, space.id.as_uuid()).await;
    let missing = CorpusGenerationId::new();
    let retriever = PgVectorRetriever::new(
        PgStore::from_pool(pool),
        query_provider(),
        "generation-fence",
    );
    assert_unavailable_without_candidates(
        retriever
            .search(&context, &request(&context, &space, missing))
            .await,
        format!(
            "unavailable: embedding corpus generation {missing} does not exist for space generation-fence"
        ),
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stale_generation_returns_no_candidates_and_its_exact_refusal(pool: PgPool) {
    let context = seed_context(&pool).await;
    let stale_memory = seed_memory(&pool, &context).await;
    let ready_memory = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space, stale_memory, &[1.0, 0.0])
        .await
        .unwrap();
    let stale = publish_building_generation(&pool, &context, space.id.as_uuid()).await;
    store
        .upsert(&context, &space, ready_memory, &[0.0, 1.0])
        .await
        .unwrap();
    let _ready = publish_building_generation(&pool, &context, space.id.as_uuid()).await;
    let retriever = PgVectorRetriever::new(
        PgStore::from_pool(pool),
        query_provider(),
        "generation-fence",
    );
    assert_unavailable_without_candidates(
        retriever
            .search(&context, &request(&context, &space, stale))
            .await,
        format!(
            "unavailable: embedding corpus generation {stale} for space generation-fence is stale"
        ),
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_space_without_a_ready_generation_is_refused_before_any_candidates(pool: PgPool) {
    let context = seed_context(&pool).await;
    let memory = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space, memory, &[1.0, 0.0])
        .await
        .unwrap();
    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(pool));
    let result = resolver
        .resolve(&context, "generation-fence", "generation-fence-model")
        .await;
    assert_eq!(
        result
            .expect_err("a space without a Ready generation must be unavailable")
            .to_string(),
        "unavailable: embedding space generation-fence has no Ready generation"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn reembedding_a_ready_member_stales_its_generation_without_rewriting_count(pool: PgPool) {
    let context = seed_context(&pool).await;
    let memory = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space, memory, &[1.0, 0.0])
        .await
        .unwrap();
    let generation = publish_building_generation(&pool, &context, space.id.as_uuid()).await;

    store
        .upsert(&context, &space, memory, &[0.0, 1.0])
        .await
        .unwrap();

    let state_and_count: (String, i64) = sqlx::query_as(
        "SELECT state, member_count FROM embedding_corpus_generations WHERE workspace_id = $1 AND id = $2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(generation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_and_count, ("stale".to_owned(), 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn purging_a_ready_member_stales_it_and_removes_its_embedding_and_membership(pool: PgPool) {
    let context = seed_context(&pool).await;
    let memory = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();
    store
        .upsert(&context, &space, memory, &[1.0, 0.0])
        .await
        .unwrap();
    let generation = publish_building_generation(&pool, &context, space.id.as_uuid()).await;

    let purge = PgPurgeRepository::new(PgStore::from_pool(pool.clone()));
    purge
        .purge_memory(&context, memory, "fence test", "fence-approval")
        .await
        .unwrap();

    let state_and_count: (String, i64) = sqlx::query_as(
        "SELECT state, member_count FROM embedding_corpus_generations WHERE workspace_id = $1 AND id = $2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(generation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_and_count, ("stale".to_owned(), 1));
    let remaining: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM memory_embeddings WHERE workspace_id = $1 AND memory_id = $2),
            (SELECT count(*) FROM embedding_corpus_generation_members member
              LEFT JOIN memory_embeddings embedding ON embedding.id = member.memory_embedding_id
             WHERE member.workspace_id = $1 AND embedding.id IS NULL)",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(memory.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_store_writers_enrol_one_open_generation_without_failing(pool: PgPool) {
    let context = seed_context(&pool).await;
    let first = seed_memory(&pool, &context).await;
    let second = seed_memory(&pool, &context).await;
    let space_store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = space_store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();
    let first_store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let second_store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));

    let (first_result, second_result) = tokio::join!(
        first_store.upsert(&context, &space, first, &[1.0, 0.0]),
        second_store.upsert(&context, &space, second, &[0.0, 1.0]),
    );
    assert!(
        first_result.is_ok(),
        "first concurrent writer failed: {first_result:?}"
    );
    assert!(
        second_result.is_ok(),
        "second concurrent writer failed: {second_result:?}"
    );
    let generation_and_members: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM embedding_corpus_generations generation
              JOIN embedding_space_registrations registration
                ON registration.workspace_id = generation.workspace_id
               AND registration.id = generation.space_registration_id
             WHERE generation.workspace_id = $1 AND registration.space_id = $2 AND generation.state = 'building'),
            (SELECT count(*) FROM embedding_corpus_generation_members member
              JOIN embedding_corpus_generations generation
                ON generation.workspace_id = member.workspace_id
               AND generation.id = member.corpus_generation_id
              JOIN embedding_space_registrations registration
                ON registration.workspace_id = generation.workspace_id
               AND registration.id = generation.space_registration_id
             WHERE member.workspace_id = $1 AND registration.space_id = $2 AND generation.state = 'building')",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(generation_and_members, (1, 2));
}

#[sqlx::test(migrations = false)]
async fn an_embedding_written_before_0191_is_retrievable_from_its_backfilled_generation_when_migration_bypasses_rls(
    pool: PgPool,
) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .unwrap();
    let runtime = runtime_pool(&pool).await;
    let through_0190 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 190)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    through_0190.run(&runtime).await.unwrap();

    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let memory = MemoryId::new();
    let revision = MemoryRevisionId::new();
    let space_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("pre-0191-{}", context.workspace_id))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,'pre-0191')")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)")
        .bind(memory.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,'generation fence',1.0,0.5)")
        .bind(revision.as_uuid())
        .bind(memory.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision.as_uuid())
        .bind(memory.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'generation-fence',2,'generation-fence-model')")
        .bind(space_id)
        .bind(context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_embeddings(id,memory_id,workspace_id,space_id,embedding) VALUES($1,$2,$3,$4,'[1,0]'::vector)")
        .bind(uuid::Uuid::now_v7())
        .bind(memory.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(space_id)
        .execute(&pool)
        .await
        .unwrap();

    let only_0191 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version == 191)
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    only_0191.run(&pool).await.unwrap();
    let generation: uuid::Uuid = sqlx::query_scalar(
        "SELECT generation.id FROM embedding_corpus_generations generation
          JOIN embedding_space_registrations registration
            ON registration.workspace_id = generation.workspace_id
           AND registration.id = generation.space_registration_id
         WHERE generation.workspace_id = $1 AND registration.space_id = $2 AND generation.state = 'ready'",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let space = vestrace_application::retrieval::EmbeddingSpace {
        id: vestrace_domain::id::EmbeddingSpaceId::from_uuid(space_id),
        name: "generation-fence".to_owned(),
        model: "generation-fence-model".to_owned(),
        dimensions: 2,
    };
    let retriever = PgVectorRetriever::new(
        PgStore::from_pool(pool.clone()),
        query_provider(),
        "generation-fence",
    );
    let candidates = retriever
        .search(
            &context,
            &request(&context, &space, CorpusGenerationId::from_uuid(generation)),
        )
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].memory_id, memory);
    assert_eq!(
        candidates[0].corpus_generation_id,
        CorpusGenerationId::from_uuid(generation)
    );
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn an_embedding_written_before_0191_is_not_backfilled_when_the_restricted_runtime_migrates(
    pool: PgPool,
) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .unwrap();
    let runtime = runtime_pool(&pool).await;
    let through_0190 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 190)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    through_0190.run(&runtime).await.unwrap();

    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let memory = MemoryId::new();
    let revision = MemoryRevisionId::new();
    let space_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("pre-0191-runtime-{}", context.workspace_id))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,'pre-0191-runtime')",
    )
    .bind(context.principal_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)")
        .bind(memory.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,'generation fence',1.0,0.5)")
        .bind(revision.as_uuid())
        .bind(memory.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision.as_uuid())
        .bind(memory.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'generation-fence',2,'generation-fence-model')")
        .bind(space_id)
        .bind(context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_embeddings(id,memory_id,workspace_id,space_id,embedding) VALUES($1,$2,$3,$4,'[1,0]'::vector)")
        .bind(uuid::Uuid::now_v7())
        .bind(memory.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(space_id)
        .execute(&pool)
        .await
        .unwrap();

    let only_0191 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version == 191)
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    only_0191.run(&runtime).await.unwrap();
    for helper in [
        "public.vestrace_open_embedding_corpus_generation(UUID, UUID, UUID)",
        "public.vestrace_enrol_embedding_corpus_generation_member(UUID, UUID, UUID)",
        "public.vestrace_stale_embedding_corpus_generations_for(UUID, UUID)",
        "public.vestrace_remove_embedding_corpus_generation_members_for(UUID, UUID)",
    ] {
        let can_execute: bool = sqlx::query_scalar(
            "SELECT has_function_privilege(current_user, $1::REGPROCEDURE, 'EXECUTE')",
        )
        .bind(helper)
        .fetch_one(&runtime)
        .await
        .unwrap();
        assert!(
            can_execute,
            "restricted runtime role must retain EXECUTE on {helper}"
        );
    }
    let generations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_corpus_generations WHERE workspace_id = $1",
    )
    .bind(context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        generations, 0,
        "restricted runtime migration must not create a corpus generation"
    );

    let resolver = PgCorpusGenerationResolver::new(PgStore::from_pool(pool));
    assert_eq!(
        resolver
            .resolve(&context, "generation-fence", "generation-fence-model")
            .await
            .expect_err("a skipped restricted-runtime backfill must leave no Ready generation")
            .to_string(),
        "unavailable: embedding space generation-fence has no Ready generation"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn membership_is_checked_when_the_store_enrols_a_building_generation(pool: PgPool) {
    let context = seed_context(&pool).await;
    let memory_id = seed_memory(&pool, &context).await;
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool));
    let space = store
        .ensure_space(&context, "generation-fence", "generation-fence-model", 2)
        .await
        .unwrap();

    store
        .upsert(&context, &space, memory_id, &[1.0, 0.0])
        .await
        .unwrap();
}
