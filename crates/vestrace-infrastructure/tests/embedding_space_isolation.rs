//! Same-wire canonical registrations preserve distinct corpus identities.
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    NormalizedRetrievalRequest, ProviderDispatchRepository, RequestContext, RetrievalRequest,
    TextRetriever, retrieval::EmbeddingStore,
};
use vestrace_domain::{MemoryId, MemoryRevisionId, TimePerspective};
use vestrace_infrastructure::{PgEmbeddingStore, PgStore, PgTextRetriever};
mod common;
use common::canonical_memory_fixture::{self, Corpus};
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

struct TwoSpaces {
    first: Corpus,
    second: Corpus,
    first_memory: MemoryId,
    second_memory: MemoryId,
}
async fn two_spaces(pool: &PgPool) -> TwoSpaces {
    let mut first = canonical_memory_fixture::new(pool, "same-wire-first").await;
    let first_memory =
        seed_memory(pool, &first.accepted.context, "first same-wire candidate").await;
    first.publish_memories(pool, &[first_memory]).await;
    let mut second =
        canonical_memory_fixture::additional_space(pool, &first, "same-wire-second").await;
    let second_memory =
        seed_memory(pool, &second.accepted.context, "second same-wire candidate").await;
    second.publish_memories(pool, &[second_memory]).await;
    assert_eq!(first.space_key.model(), second.space_key.model());
    assert_eq!(first.space_key.dimensions(), second.space_key.dimensions());
    assert_eq!(
        first.accepted.context.workspace_id,
        second.accepted.context.workspace_id
    );
    assert_ne!(
        first.accepted.space_registration_id,
        second.accepted.space_registration_id
    );
    TwoSpaces {
        first,
        second,
        first_memory,
        second_memory,
    }
}
fn request(corpus: &Corpus) -> NormalizedRetrievalRequest {
    NormalizedRetrievalRequest::normalize_with_pin(
        RetrievalRequest::new(corpus.accepted.context.workspace_id, "same wire candidate")
            .with_time_perspective(TimePerspective::AllHistory),
        corpus.space_key.clone(),
        corpus.generation,
    )
    .unwrap()
}
#[sqlx::test(migrations = false)]
async fn a_vector_written_to_one_space_cannot_enrol_in_the_other(pool: PgPool) {
    let f = two_spaces(&pool).await;
    let workspace = f.first.accepted.context.workspace_id.as_uuid();
    let generation = Uuid::now_v7();
    let mut tx = f.first.runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut tx, workspace).await;
    let version:i64=sqlx::query_scalar("SELECT guard_version FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$2").bind(workspace).bind(f.first.accepted.space_registration_id).fetch_one(&mut *tx).await.unwrap();
    sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,$4)")
        .bind(generation)
        .bind(workspace)
        .bind(f.first.accepted.space_registration_id)
        .bind(version)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *tx)
        .await
        .unwrap();
    common::result_preparation_fixture::scoped(&mut tx, workspace).await;
    let old:Uuid=sqlx::query_scalar("SELECT embedding_projection_entry_id FROM embedding_corpus_generation_members WHERE corpus_generation_id=$1 ORDER BY member_ordinal LIMIT 1").bind(generation).fetch_one(&mut *tx).await.unwrap();
    sqlx::query("DELETE FROM embedding_corpus_generation_members WHERE corpus_generation_id=$1 AND embedding_projection_entry_id=$2").bind(generation).bind(old).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO embedding_corpus_generation_members(workspace_id,corpus_generation_id,member_ordinal,embedding_projection_entry_id) SELECT workspace_id,$1,projection_ordinal,id FROM embedding_projection_entries WHERE workspace_id=$2 AND space_registration_id=$3 ORDER BY projection_ordinal LIMIT 1")
        .bind(generation).bind(workspace).bind(f.second.accepted.space_registration_id).execute(&mut *tx).await.unwrap();
    let error = tx
        .commit()
        .await
        .expect_err("cross-space projection must fail canonical membership validation");
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    assert!(
        error
            .to_string()
            .contains("canonical generation requires exact Live projection members")
    );
}
#[sqlx::test(migrations = false)]
async fn a_generation_of_one_same_wire_space_is_not_a_valid_pin_for_the_other(pool: PgPool) {
    let f = two_spaces(&pool).await;
    let mut query = request(&f.first);
    query.corpus_generation_id = f.second.generation;
    let error = PgTextRetriever::new(PgStore::from_pool(f.first.runtime.clone()))
        .search(&f.first.accepted.context, &query)
        .await
        .expect_err("cross-space pin must fail before search");
    assert!(error.to_string().contains("generation"), "{error}");
}
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn an_embedding_job_dispatch_plan_remains_pinned_to_its_registered_space(pool: PgPool) {
    let runtime = common::runtime_pool(&pool).await;
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

#[sqlx::test(migrations = false)]
async fn a_retrieval_fenced_to_one_same_wire_space_returns_no_candidate_of_the_other(pool: PgPool) {
    let f = two_spaces(&pool).await;
    let (outcome, calls) =
        common::canonical_query_fixture::retrieve(&pool, &f.first, &request(&f.first)).await;
    assert_eq!(calls, 1, "one governed query provider call");
    let vestrace_application::embedding::EmbeddingRetrievalOutcome::Completed(candidates) = outcome
    else {
        panic!("canonical query must complete: {outcome:?}")
    };
    assert!(
        !candidates.is_empty(),
        "correct-space candidate must be present"
    );
    for candidate in candidates {
        assert_eq!(candidate.corpus_generation_id, f.first.generation);
        assert_eq!(candidate.memory_id, f.first_memory);
        assert_ne!(candidate.memory_id, f.second_memory);
    }
}
