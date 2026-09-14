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
use vestrace_application::{NormalizedRetrievalRequest, TextRetriever};
use vestrace_domain::{
    CorpusGenerationId, MemoryStatus, TimePerspective, WorkspaceId,
    id::{MemoryId, MemoryRevisionId},
    retrieval::RetrievalIntent,
};
use vestrace_infrastructure::{PgStore, PgTextRetriever};
mod common;
use common::canonical_memory_fixture::{self, Corpus};

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

async fn ready_generation(
    pool: &PgPool,
    corpus: &mut Corpus,
    members: &[MemoryId],
) -> CorpusGenerationId {
    corpus.publish_memories(pool, members).await
}

fn request(
    corpus: &Corpus,
    query: &str,
    generation_id: CorpusGenerationId,
) -> NormalizedRetrievalRequest {
    let context = &corpus.accepted.context;
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
        embedding_space_key: corpus.space_key.clone(),
        corpus_generation_id: generation_id,
    }
}

#[sqlx::test(migrations = false)]
async fn a_query_returns_the_memory_that_matches_it(pool: PgPool) {
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let volcano = seed_memory(&pool, context.workspace_id, "the volcano erupted at dawn").await;
    let harbour = seed_memory(&pool, context.workspace_id, "the harbour was quiet").await;
    let ledger = seed_memory(&pool, context.workspace_id, "the ledger balanced").await;
    let generation = ready_generation(&pool, &mut corpus, &[volcano, harbour, ledger]).await;

    let candidates = retriever
        .search(&context, &request(&corpus, "volcano", generation))
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

#[sqlx::test(migrations = false)]
async fn a_query_matching_nothing_returns_nothing(pool: PgPool) {
    // The case that makes the difference visible. Before the match predicate
    // this returned every memory in the workspace, scored zero, and a caller
    // could not tell that from three genuine weak matches.
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let harbour = seed_memory(&pool, context.workspace_id, "the harbour was quiet").await;
    let ledger = seed_memory(&pool, context.workspace_id, "the ledger balanced").await;
    let generation = ready_generation(&pool, &mut corpus, &[harbour, ledger]).await;

    let candidates = retriever
        .search(&context, &request(&corpus, "volcano", generation))
        .await
        .expect("search");

    assert!(
        candidates.is_empty(),
        "a query matching no memory returned {} candidate(s)",
        candidates.len()
    );
}

#[sqlx::test(migrations = false)]
async fn a_revised_memory_is_found_by_its_new_text_and_not_its_old(pool: PgPool) {
    // The index is a projection of the active revision, written in the same
    // transaction as the revision. If it were updated afterwards — or not at
    // all — a memory would keep answering to text it no longer holds.
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let memory = seed_memory(&pool, context.workspace_id, "the sky is green").await;
    let generation = ready_generation(&pool, &mut corpus, &[memory]).await;

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
        .search(&context, &request(&corpus, "blue", generation))
        .await
        .expect("search");
    assert!(
        blue.is_empty(),
        "revision two is not represented by revision one's generation"
    );
    let generation = ready_generation(&pool, &mut corpus, &[memory]).await;
    let blue = retriever
        .search(&context, &request(&corpus, "blue", generation))
        .await
        .unwrap();
    assert_eq!(blue.len(), 1);
    assert_eq!(blue[0].revision_id, revision_id);
    assert_eq!(blue[0].revision_number, 2);

    let green = retriever
        .search(&context, &request(&corpus, "green", generation))
        .await
        .expect("search");
    assert!(
        green.is_empty(),
        "the superseded text still matched: {} candidate(s)",
        green.len()
    );
}

#[sqlx::test(migrations = false)]
async fn a_matching_memory_in_another_workspace_is_not_a_candidate(pool: PgPool) {
    let mut mine_corpus = canonical_memory_fixture::new(&pool, "space").await;
    let mut theirs_corpus = canonical_memory_fixture::new_in_database(&pool, "space").await;
    let mine = mine_corpus.accepted.context.clone();
    let theirs = theirs_corpus.accepted.context.clone();
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let ours = seed_memory(&pool, mine.workspace_id, "the volcano erupted at dawn").await;
    let theirs_memory =
        seed_memory(&pool, theirs.workspace_id, "the volcano erupted at dawn").await;
    let mine_generation = ready_generation(&pool, &mut mine_corpus, &[ours]).await;
    let _theirs_generation = ready_generation(&pool, &mut theirs_corpus, &[theirs_memory]).await;

    let candidates = retriever
        .search(&mine, &request(&mine_corpus, "volcano", mine_generation))
        .await
        .expect("search");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].memory_id, ours);
}

#[sqlx::test(migrations = false)]
async fn ranking_puts_the_stronger_match_first(pool: PgPool) {
    // Ranking only means something once filtering works: before, every row
    // scored zero and the order came from the tiebreakers.
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
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
    let generation = ready_generation(&pool, &mut corpus, &[strong, weak]).await;

    let candidates = retriever
        .search(&context, &request(&corpus, "volcano", generation))
        .await
        .expect("search");

    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates[0].memory_id, strong,
        "the denser match did not rank first"
    );
}

#[sqlx::test(migrations = false)]
async fn a_ready_generation_omits_a_matching_memory_that_its_store_never_enrolled(pool: PgPool) {
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let enrolled = seed_memory(&pool, context.workspace_id, "enrolled fenceword").await;
    let excluded = seed_memory(&pool, context.workspace_id, "excluded fenceword").await;
    let generation = ready_generation(&pool, &mut corpus, &[enrolled]).await;
    let candidates = PgTextRetriever::new(PgStore::from_pool(pool.clone()))
        .search(&context, &request(&corpus, "fenceword", generation))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].memory_id, enrolled,
        "unenrolled matching memory {excluded} leaked"
    );
}

#[sqlx::test(migrations = false)]
async fn a_generation_pin_is_preserved_on_text_candidates(pool: PgPool) {
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let memory = seed_memory(&pool, context.workspace_id, "pinned fenceword").await;
    let generation = ready_generation(&pool, &mut corpus, &[memory]).await;
    let candidates = PgTextRetriever::new(PgStore::from_pool(pool.clone()))
        .search(&context, &request(&corpus, "fenceword", generation))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].corpus_generation_id, generation);
}

#[sqlx::test(migrations = false)]
async fn temporal_search_intersects_exact_represented_revisions(pool: PgPool) {
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let memory = seed_memory(&pool, context.workspace_id, "volcano historical").await;
    let old: uuid::Uuid = sqlx::query_scalar("SELECT active_revision_id FROM memories WHERE id=$1")
        .bind(memory.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE memory_revisions SET valid_from='2020-01-01',valid_until='2021-01-01' WHERE id=$1",
    )
    .bind(old)
    .execute(&pool)
    .await
    .unwrap();
    let generation = corpus.publish_memories(&pool, &[memory]).await;
    let new = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance,valid_from) VALUES($1,$2,$3,2,'volcano contemporary',1,0.5,'2021-01-01')")
        .bind(new).bind(memory.as_uuid()).bind(context.workspace_id.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(new)
        .bind(memory.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let retriever = PgTextRetriever::new(PgStore::from_pool(corpus.runtime.clone()));
    for perspective in [
        TimePerspective::Timeline,
        TimePerspective::AllHistory,
        TimePerspective::AsOf(
            chrono::DateTime::parse_from_rfc3339("2020-06-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ),
    ] {
        let mut query = request(&corpus, "volcano", generation);
        query.time_perspective = perspective;
        let found = retriever.search(&context, &query).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].revision_id.as_uuid(), old);
    }
    let mut query = request(&corpus, "volcano", generation);
    query.time_perspective = TimePerspective::AsOf(
        chrono::DateTime::parse_from_rfc3339("2022-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    assert!(
        retriever.search(&context, &query).await.unwrap().is_empty(),
        "AsOf must not admit the unrepresented second revision"
    );
    let generation = corpus.publish_memories(&pool, &[memory]).await;
    query = request(&corpus, "volcano", generation);
    query.time_perspective = TimePerspective::Timeline;
    let found = retriever.search(&context, &query).await.unwrap();
    assert_eq!(
        found
            .iter()
            .map(|c| c.revision_id.as_uuid())
            .collect::<Vec<_>>(),
        vec![old, new]
    );
}

#[sqlx::test(migrations = false)]
async fn duplicate_projections_do_not_consume_the_text_candidate_limit(pool: PgPool) {
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let first = seed_memory(&pool, context.workspace_id, "volcano volcano volcano").await;
    let second = seed_memory(&pool, context.workspace_id, "volcano").await;
    let generation = corpus
        .publish_memories(&pool, &[first, first, second])
        .await;
    let mut query = request(&corpus, "volcano", generation);
    query.channel_limit = 2;
    let found = PgTextRetriever::new(PgStore::from_pool(corpus.runtime.clone()))
        .search(&context, &query)
        .await
        .unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].memory_id, first);
    assert_eq!(found[1].memory_id, second);
}

#[sqlx::test(migrations = false)]
async fn source_erasure_revokes_text_access_before_ciphertext_destruction(pool: PgPool) {
    let mut corpus = canonical_memory_fixture::new(&pool, "space").await;
    let context = corpus.accepted.context.clone();
    let memory = seed_memory(&pool, context.workspace_id, "volcano erased secret").await;
    let revision: uuid::Uuid =
        sqlx::query_scalar("SELECT active_revision_id FROM memories WHERE id=$1")
            .bind(memory.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let source = corpus.publish_revision(&pool, revision).await;
    let generation = corpus.capture().await;
    let retriever = PgTextRetriever::new(PgStore::from_pool(corpus.runtime.clone()));
    assert_eq!(
        retriever
            .search(&context, &request(&corpus, "volcano", generation))
            .await
            .unwrap()
            .len(),
        1
    );
    let mut tx = corpus.runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut tx, context.workspace_id.as_uuid()).await;
    sqlx::query("SELECT vestrace_propagate_embedding_source_erasure($1,$2,$3)")
        .bind(uuid::Uuid::now_v7())
        .bind(context.workspace_id.as_uuid())
        .bind(source)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let state: String = sqlx::query_scalar("SELECT state FROM content_materials WHERE id=$1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "erasure_prepared");
    assert!(
        retriever
            .search(&context, &request(&corpus, "volcano", generation))
            .await
            .is_err(),
        "revoked generation cannot expose erased source text"
    );
    let next = corpus.capture().await;
    assert!(
        retriever
            .search(&context, &request(&corpus, "volcano", next))
            .await
            .unwrap()
            .is_empty()
    );
}
