//! Database-backed tests for the text retrieval channel.
//!
//! # Why this file exists
//!
//! `PgTextRetriever` had no test of any kind. The retrieval tests that did
//! exist ran against in-memory doubles at the application layer — fusion,
//! degradation, context packs — none of which touch this SQL. So the channel
//! could return anything at all and the suite stayed green, and it did: the
//! query text reached only `ts_rank` in the SELECT list, with no match
//! predicate in the `WHERE` clause, so **every** active memory in the workspace
//! was a candidate for **every** query.
//!
//! That is invisible in a workspace with three memories. A search for a word
//! present in none of them returned all three, with exactly the scores a search
//! for a word present in one of them returned.
//!
//! `sqlx::test` connects as the database owner, so row level security is not
//! exercised here; the workspace separation these tests show rests on the
//! query's own predicate.

use sqlx::PgPool;
use vestrace_application::{
    NormalizedRetrievalRequest, RequestContext, TextRetriever, retrieval::EmbeddingStore,
};
use vestrace_domain::{
    CorpusGenerationId, MemoryStatus, PrincipalId, TimePerspective, WorkspaceId,
    id::{MemoryId, MemoryRevisionId},
    retrieval::RetrievalIntent,
};
use vestrace_infrastructure::{PgEmbeddingStore, PgStore, PgTextRetriever};

/// One memory, one active revision, and the search document that indexes it.
async fn seed_memory(pool: &PgPool, workspace_id: WorkspaceId, content: &str) -> MemoryId {
    let memory_id = MemoryId::new();
    let revision_id = MemoryRevisionId::new();

    sqlx::query(
        "INSERT INTO memories (id, workspace_id, kind, status, state_revision) \
         VALUES ($1, $2, 'fact', 'candidate', 1)",
    )
    .bind(memory_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .execute(pool)
    .await
    .expect("memory");

    sqlx::query(
        "INSERT INTO memory_revisions \
         (id, memory_id, workspace_id, revision_number, content, confidence, importance) \
         VALUES ($1, $2, $3, 1, $4, 1.0, 0.5)",
    )
    .bind(revision_id.as_uuid())
    .bind(memory_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .bind(content)
    .execute(pool)
    .await
    .expect("revision");

    sqlx::query("UPDATE memories SET active_revision_id = $1 WHERE id = $2")
        .bind(revision_id.as_uuid())
        .bind(memory_id.as_uuid())
        .execute(pool)
        .await
        .expect("activate");

    // `fts_vector` is generated from `content`, so it is deliberately not
    // written here — this is the same shape the write path produces.
    sqlx::query(
        "INSERT INTO search_documents (id, memory_id, workspace_id, title, content) \
         VALUES ($1, $2, $3, '', $4)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(memory_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .bind(content)
    .execute(pool)
    .await
    .expect("search document");

    memory_id
}

async fn seed_workspace(pool: &PgPool) -> RequestContext {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();

    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("ws-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .expect("workspace");

    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind("tester")
        .execute(pool)
        .await
        .expect("principal");

    RequestContext::new(workspace_id, principal_id)
}

async fn ready_generation(
    pool: &PgPool,
    context: &RequestContext,
    members: &[MemoryId],
) -> CorpusGenerationId {
    sqlx::query("DO $$ BEGIN EXECUTE format('ALTER FUNCTION public.vestrace_validate_embedding_corpus_generation_member() OWNER TO %I', current_user); END $$")
        .execute(pool).await.unwrap();
    let store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = store
        .ensure_space(context, "space", "model", 2)
        .await
        .unwrap();
    for memory_id in members {
        store
            .upsert(context, &space, *memory_id, &[1.0, 0.0])
            .await
            .unwrap();
    }
    let registration_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id=$1 AND space_id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space.id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let generation_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM embedding_corpus_generations WHERE workspace_id=$1 AND space_registration_id=$2 AND state='building'")
        .bind(context.workspace_id.as_uuid()).bind(registration_id).fetch_one(pool).await.unwrap();
    let state: String = sqlx::query_scalar(
        "SELECT state FROM embedding_corpus_generations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(generation_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(state, "building");
    let mut publish = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *publish)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_embedding_corpus_generation($1,$2,$3,$4)",
    )
    .bind(generation_id)
    .bind(context.workspace_id.as_uuid())
    .bind(registration_id)
    .bind(members.len() as i64)
    .fetch_one(&mut *publish)
    .await
    .unwrap();
    publish.commit().await.unwrap();
    CorpusGenerationId::from_uuid(generation_id)
}

fn request(
    context: &RequestContext,
    query: &str,
    generation_id: CorpusGenerationId,
) -> NormalizedRetrievalRequest {
    NormalizedRetrievalRequest {
        request_id: vestrace_domain::id::RetrievalRunId::new(),
        query: query.to_owned(),
        intent: RetrievalIntent::SemanticRecall,
        time_perspective: TimePerspective::Current,
        workspace_id: context.workspace_id,
        allowed_statuses: vec![MemoryStatus::Active, MemoryStatus::Candidate],
        allowed_kinds: Vec::new(),
        channel_limit: 20,
        token_budget: None,
        include_explanation: true,
        embedding_space_key: vestrace_domain::embedding::EmbeddingSpaceKey::new(
            context.workspace_id,
            "space",
            "model",
            2,
        )
        .unwrap(),
        corpus_generation_id: generation_id,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_query_returns_the_memory_that_matches_it(pool: PgPool) {
    let context = seed_workspace(&pool).await;
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let volcano = seed_memory(&pool, context.workspace_id, "the volcano erupted at dawn").await;
    let harbour = seed_memory(&pool, context.workspace_id, "the harbour was quiet").await;
    let ledger = seed_memory(&pool, context.workspace_id, "the ledger balanced").await;
    let generation = ready_generation(&pool, &context, &[volcano, harbour, ledger]).await;

    let candidates = retriever
        .search(&context, &request(&context, "volcano", generation))
        .await
        .expect("search");

    assert_eq!(
        candidates.len(),
        1,
        "a query for a word in one of three memories returned {} of them",
        candidates.len()
    );
    assert_eq!(candidates[0].memory_id, volcano);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_query_matching_nothing_returns_nothing(pool: PgPool) {
    // The case that makes the difference visible. Before the match predicate
    // this returned every memory in the workspace, scored zero, and a caller
    // could not tell that from three genuine weak matches.
    let context = seed_workspace(&pool).await;
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let harbour = seed_memory(&pool, context.workspace_id, "the harbour was quiet").await;
    let ledger = seed_memory(&pool, context.workspace_id, "the ledger balanced").await;
    let generation = ready_generation(&pool, &context, &[harbour, ledger]).await;

    let candidates = retriever
        .search(&context, &request(&context, "volcano", generation))
        .await
        .expect("search");

    assert!(
        candidates.is_empty(),
        "a query matching no memory returned {} candidate(s)",
        candidates.len()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_revised_memory_is_found_by_its_new_text_and_not_its_old(pool: PgPool) {
    // The index is a projection of the active revision, written in the same
    // transaction as the revision. If it were updated afterwards — or not at
    // all — a memory would keep answering to text it no longer holds.
    let context = seed_workspace(&pool).await;
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let memory = seed_memory(&pool, context.workspace_id, "the sky is green").await;
    let generation = ready_generation(&pool, &context, &[memory]).await;

    let revision_id = MemoryRevisionId::new();
    sqlx::query(
        "INSERT INTO memory_revisions \
         (id, memory_id, workspace_id, revision_number, content, confidence, importance) \
         VALUES ($1, $2, $3, 2, 'the sky is blue', 1.0, 0.5)",
    )
    .bind(revision_id.as_uuid())
    .bind(memory.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .execute(&pool)
    .await
    .expect("revision");
    sqlx::query("UPDATE memories SET active_revision_id = $1 WHERE id = $2")
        .bind(revision_id.as_uuid())
        .bind(memory.as_uuid())
        .execute(&pool)
        .await
        .expect("activate");
    sqlx::query("UPDATE search_documents SET content = 'the sky is blue' WHERE memory_id = $1")
        .bind(memory.as_uuid())
        .execute(&pool)
        .await
        .expect("reindex");

    let blue = retriever
        .search(&context, &request(&context, "blue", generation))
        .await
        .expect("search");
    assert_eq!(blue.len(), 1);
    assert_eq!(blue[0].revision_number, 2);

    let green = retriever
        .search(&context, &request(&context, "green", generation))
        .await
        .expect("search");
    assert!(
        green.is_empty(),
        "the superseded text still matched: {} candidate(s)",
        green.len()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_matching_memory_in_another_workspace_is_not_a_candidate(pool: PgPool) {
    let mine = seed_workspace(&pool).await;
    let theirs = seed_workspace(&pool).await;
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let ours = seed_memory(&pool, mine.workspace_id, "the volcano erupted at dawn").await;
    let theirs_memory =
        seed_memory(&pool, theirs.workspace_id, "the volcano erupted at dawn").await;
    let mine_generation = ready_generation(&pool, &mine, &[ours]).await;
    let _theirs_generation = ready_generation(&pool, &theirs, &[theirs_memory]).await;

    let candidates = retriever
        .search(&mine, &request(&mine, "volcano", mine_generation))
        .await
        .expect("search");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].memory_id, ours);
}

#[sqlx::test(migrations = "../../migrations")]
async fn ranking_puts_the_stronger_match_first(pool: PgPool) {
    // Ranking only means something once filtering works: before, every row
    // scored zero and the order came from the tiebreakers.
    let context = seed_workspace(&pool).await;
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let strong = seed_memory(
        &pool,
        context.workspace_id,
        "volcano volcano volcano eruption",
    )
    .await;
    let weak = seed_memory(
        &pool,
        context.workspace_id,
        "a passing mention of a volcano in a long paragraph about harbours and \
         ledgers and other unrelated matters entirely",
    )
    .await;
    let generation = ready_generation(&pool, &context, &[strong, weak]).await;

    let candidates = retriever
        .search(&context, &request(&context, "volcano", generation))
        .await
        .expect("search");

    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates[0].memory_id, strong,
        "the denser match did not rank first"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_ready_generation_omits_a_matching_memory_that_its_store_never_enrolled(pool: PgPool) {
    let context = seed_workspace(&pool).await;
    let enrolled = seed_memory(&pool, context.workspace_id, "enrolled fenceword").await;
    let excluded = seed_memory(&pool, context.workspace_id, "excluded fenceword").await;
    let generation = ready_generation(&pool, &context, &[enrolled]).await;
    let candidates = PgTextRetriever::new(PgStore::from_pool(pool.clone()))
        .search(&context, &request(&context, "fenceword", generation))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].memory_id, enrolled,
        "unenrolled matching memory {excluded} leaked"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_generation_pin_is_preserved_on_text_candidates(pool: PgPool) {
    let context = seed_workspace(&pool).await;
    let memory = seed_memory(&pool, context.workspace_id, "pinned fenceword").await;
    let generation = ready_generation(&pool, &context, &[memory]).await;
    let candidates = PgTextRetriever::new(PgStore::from_pool(pool.clone()))
        .search(&context, &request(&context, "fenceword", generation))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].corpus_generation_id, generation);
}
