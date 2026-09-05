//! Composed PostgreSQL proof for the production retrieval classification boundary.

use std::sync::Arc;

use sqlx::PgPool;
use vestrace_application::{
    RequestContext, RetrievalRequest, RetrievalService, retrieval::EmbeddingStore,
};
use vestrace_domain::{
    MemoryStatus, PrincipalId, TimePerspective, WorkspaceId,
    id::{MemoryId, MemoryRevisionId},
    retrieval::{ClassificationPolicy, WithholdingReason},
};
use vestrace_infrastructure::{
    PgCorpusGenerationResolver, PgEmbeddingStore, PgRetrievalJournal, PgRevisionHydrator, PgStore,
    PgTextRetriever,
};

const EMBEDDING_SPACE_NAME: &str = "retrieval-classification";
const EMBEDDING_MODEL_NAME: &str = "retrieval-classification-model";

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
        .bind("retrieval-policy-test")
        .execute(pool)
        .await
        .expect("principal");

    RequestContext::new(workspace_id, principal_id)
}

async fn seed_memory(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    content: &str,
    classification: &str,
) -> (MemoryId, MemoryRevisionId) {
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
         (id, memory_id, workspace_id, revision_number, content, confidence, importance, classification) \
         VALUES ($1, $2, $3, 1, $4, 1.0, 0.5, $5)",
    )
    .bind(revision_id.as_uuid())
    .bind(memory_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .bind(content)
    .bind(classification)
    .execute(pool)
    .await
    .expect("revision");
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

    (memory_id, revision_id)
}

async fn seed_ready_corpus_generation(
    pool: &PgPool,
    context: &RequestContext,
    memory_ids: &[MemoryId],
) {
    let embedding_store = PgEmbeddingStore::new(PgStore::from_pool(pool.clone()));
    let space = embedding_store
        .ensure_space(context, EMBEDDING_SPACE_NAME, EMBEDDING_MODEL_NAME, 2)
        .await
        .expect("embedding space");
    for memory_id in memory_ids {
        embedding_store
            .upsert(context, &space, *memory_id, &[1.0, 0.0])
            .await
            .expect("embedding");
    }

    let registration_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id = $1 AND space_id = $2",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(space.id.as_uuid())
    .fetch_one(pool)
    .await
    .expect("embedding space registration");
    let generation_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_corpus_generations \
         WHERE workspace_id = $1 AND space_registration_id = $2 AND state = 'building'",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(registration_id)
    .fetch_one(pool)
    .await
    .expect("building corpus generation");
    let mut publish = pool.begin().await.expect("publish transaction");
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *publish)
        .await
        .expect("publish workspace scope");
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_embedding_corpus_generation($1, $2, $3, 0)",
    )
    .bind(generation_id)
    .bind(context.workspace_id.as_uuid())
    .bind(registration_id)
    .fetch_one(&mut *publish)
    .await
    .expect("publish corpus generation");
    publish.commit().await.expect("published corpus generation");
}

#[sqlx::test(migrations = "../../migrations")]
async fn postgres_retrieval_withholds_inadmissible_content_without_hiding_the_gap(pool: PgPool) {
    let context = seed_workspace(&pool).await;
    let (internal_memory, _) = seed_memory(
        &pool,
        context.workspace_id,
        "volcano internal briefing",
        "internal",
    )
    .await;
    let (restricted_memory, restricted_revision) = seed_memory(
        &pool,
        context.workspace_id,
        "volcano restricted secret",
        "restricted",
    )
    .await;
    seed_ready_corpus_generation(&pool, &context, &[internal_memory, restricted_memory]).await;
    let store = PgStore::from_pool(pool.clone());
    let service = RetrievalService::new(
        Arc::new(PgTextRetriever::new(store.clone())),
        Arc::new(PgRetrievalJournal::new(store.clone())),
    )
    .with_hydration(
        Arc::new(PgRevisionHydrator::new(store)),
        ClassificationPolicy::new(["internal"], false).unwrap(),
        "retrieval-policy-test-v1",
    )
    .with_corpus_generation_resolver(
        Arc::new(PgCorpusGenerationResolver::new(PgStore::from_pool(
            pool.clone(),
        ))),
        EMBEDDING_SPACE_NAME,
        EMBEDDING_MODEL_NAME,
    );
    let request = RetrievalRequest::new(context.workspace_id, "volcano")
        .with_time_perspective(TimePerspective::Timeline)
        .with_allowed_statuses(vec![MemoryStatus::Candidate]);

    let result = service.search(&context, request).await.unwrap();

    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].memory_id, internal_memory);
    assert_eq!(
        result.candidates[0].classification.as_deref(),
        Some("internal")
    );
    assert_eq!(result.withheld.len(), 1);
    assert_eq!(result.withheld[0].memory_id, restricted_memory);
    assert_eq!(result.withheld[0].revision_id, restricted_revision);
    assert_eq!(
        result.withheld[0].reason,
        WithholdingReason::ClassificationNotAdmissible {
            classification: "restricted".to_owned(),
        }
    );
    let rendered = serde_json::json!({
        "candidates": &result.candidates,
        "withheld": &result.withheld,
    });
    assert!(!rendered.to_string().contains("restricted secret"));

    let parameters: serde_json::Value =
        sqlx::query_scalar("SELECT parameters FROM retrieval_runs WHERE id = $1")
            .bind(result.run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("persisted retrieval decision");
    assert_eq!(
        parameters["retrieval_policy_version"],
        "retrieval-policy-test-v1"
    );
    assert_eq!(
        parameters["withheld"][0]["revision_id"],
        restricted_revision.to_string()
    );
    assert!(!parameters.to_string().contains("restricted secret"));
}
