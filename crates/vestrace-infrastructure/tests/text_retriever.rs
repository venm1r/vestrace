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
use vestrace_application::{NormalizedRetrievalRequest, RequestContext, TextRetriever};
use vestrace_domain::{
    MemoryStatus, PrincipalId, TimePerspective, WorkspaceId,
    id::{MemoryId, MemoryRevisionId},
    retrieval::RetrievalIntent,
};
use vestrace_infrastructure::{PgStore, PgTextRetriever};

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

fn request(context: &RequestContext, query: &str) -> NormalizedRetrievalRequest {
    NormalizedRetrievalRequest {
        query: query.to_owned(),
        intent: RetrievalIntent::SemanticRecall,
        time_perspective: TimePerspective::Current,
        workspace_id: context.workspace_id,
        allowed_statuses: vec![MemoryStatus::Active, MemoryStatus::Candidate],
        allowed_kinds: Vec::new(),
        channel_limit: 20,
        token_budget: None,
        include_explanation: true,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_query_returns_the_memory_that_matches_it(pool: PgPool) {
    let context = seed_workspace(&pool).await;
    let retriever = PgTextRetriever::new(PgStore::from_pool(pool.clone()));

    let volcano = seed_memory(&pool, context.workspace_id, "the volcano erupted at dawn").await;
    seed_memory(&pool, context.workspace_id, "the harbour was quiet").await;
    seed_memory(&pool, context.workspace_id, "the ledger balanced").await;

    let candidates = retriever
        .search(&context, &request(&context, "volcano"))
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

    seed_memory(&pool, context.workspace_id, "the harbour was quiet").await;
    seed_memory(&pool, context.workspace_id, "the ledger balanced").await;

    let candidates = retriever
        .search(&context, &request(&context, "volcano"))
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
        .search(&context, &request(&context, "blue"))
        .await
        .expect("search");
    assert_eq!(blue.len(), 1);
    assert_eq!(blue[0].revision_number, 2);

    let green = retriever
        .search(&context, &request(&context, "green"))
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
    seed_memory(&pool, theirs.workspace_id, "the volcano erupted at dawn").await;

    let candidates = retriever
        .search(&mine, &request(&mine, "volcano"))
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
    seed_memory(
        &pool,
        context.workspace_id,
        "a passing mention of a volcano in a long paragraph about harbours and \
         ledgers and other unrelated matters entirely",
    )
    .await;

    let candidates = retriever
        .search(&context, &request(&context, "volcano"))
        .await
        .expect("search");

    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates[0].memory_id, strong,
        "the denser match did not rank first"
    );
}
