//! Composed PostgreSQL proof for the production retrieval classification boundary.

use std::sync::Arc;

use sqlx::PgPool;
use vestrace_application::{RequestContext, RetrievalRequest, RetrievalService};
use vestrace_domain::{
    MemoryStatus, PrincipalId, TimePerspective, WorkspaceId,
    id::{MemoryId, MemoryRevisionId},
    retrieval::{ClassificationPolicy, WithholdingReason},
};
use vestrace_infrastructure::{PgRetrievalJournal, PgRevisionHydrator, PgStore, PgTextRetriever};

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
    let store = PgStore::from_pool(pool.clone());
    let service = RetrievalService::new(
        Arc::new(PgTextRetriever::new(store.clone())),
        Arc::new(PgRetrievalJournal::new(store.clone())),
    )
    .with_hydration(
        Arc::new(PgRevisionHydrator::new(store)),
        ClassificationPolicy::new(["internal"], false).unwrap(),
        "retrieval-policy-test-v1",
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
