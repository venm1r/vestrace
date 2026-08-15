//! Content storage added by migration 0134.
//!
//! Migration 0042 was named `..._blobs_...` but created no table capable of
//! holding bytes, so the registry could pin a digest for content the system had
//! nowhere to keep. These tests exercise the storage that closes that gap.

use sqlx::PgPool;
use vestrace_application::{ArtifactContent, ArtifactRepository, RequestContext};
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::{PgArtifactRepository, PgStore};

async fn seed_workspace(pool: &PgPool, workspace_id: WorkspaceId) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("artifacts-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
}

fn context(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::new(workspace_id, PrincipalId::new())
}

fn text(body: &str) -> ArtifactContent {
    ArtifactContent {
        media_type: "text/plain".into(),
        bytes: body.as_bytes().to_vec(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn stored_content_comes_back_byte_for_byte(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let repository = PgArtifactRepository::new(PgStore::from_pool(pool));
    let context = context(workspace);
    let content = text("the model said this");

    let stored = repository
        .store(&context, "step-output", &content)
        .await
        .unwrap();
    let fetched = repository
        .fetch_content(&context, &stored.content_hash)
        .await
        .unwrap();

    assert_eq!(fetched, Some(content));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_stored_digest_describes_the_stored_bytes(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let repository = PgArtifactRepository::new(PgStore::from_pool(pool.clone()));
    let content = text("abc");

    let stored = repository
        .store(&context(workspace), "step-output", &content)
        .await
        .unwrap();

    // Computed independently of the repository, from the published SHA-256 of
    // "abc": the digest must describe the content, not merely be consistent
    // with itself.
    assert_eq!(
        stored.content_hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let on_disk: String =
        sqlx::query_scalar("SELECT content_hash FROM artifact_revisions WHERE id = $1")
            .bind(stored.revision_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(on_disk, stored.content_hash);
}

#[sqlx::test(migrations = "../../migrations")]
async fn identical_content_is_stored_once_but_yields_distinct_artifacts(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let repository = PgArtifactRepository::new(PgStore::from_pool(pool.clone()));
    let context = context(workspace);
    let content = text("identical output");

    let first = repository.store(&context, "first", &content).await.unwrap();
    let second = repository
        .store(&context, "second", &content)
        .await
        .unwrap();

    assert_eq!(first.content_hash, second.content_hash);
    assert_ne!(first.artifact_id, second.artifact_id);

    let blobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifact_blobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(blobs, 1, "identical bytes must be stored once");
    let artifacts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(artifacts, 2, "each store call is its own artifact");
}

#[sqlx::test(migrations = "../../migrations")]
async fn content_does_not_cross_a_workspace_boundary(pool: PgPool) {
    let owner = WorkspaceId::new();
    let intruder = WorkspaceId::new();
    seed_workspace(&pool, owner).await;
    seed_workspace(&pool, intruder).await;
    let repository = PgArtifactRepository::new(PgStore::from_pool(pool));
    let content = text("confidential output");

    let stored = repository
        .store(&context(owner), "step-output", &content)
        .await
        .unwrap();

    // Same digest, different workspace: dedup is per workspace precisely so
    // that holding a hash does not let one workspace read another's content.
    let leaked = repository
        .fetch_content(&context(intruder), &stored.content_hash)
        .await
        .unwrap();

    assert_eq!(leaked, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn absent_content_is_reported_as_absent_rather_than_as_a_failure(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let repository = PgArtifactRepository::new(PgStore::from_pool(pool));

    // A revision may pin a digest whose bytes live elsewhere; that is the
    // registry behaviour and it is not an error.
    let missing = repository
        .fetch_content(
            &context(workspace),
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .await
        .unwrap();

    assert_eq!(missing, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn stored_content_appears_in_the_listing_with_its_size(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let repository = PgArtifactRepository::new(PgStore::from_pool(pool));
    let context = context(workspace);
    let content = text("sized output");

    repository
        .store(&context, "step-output", &content)
        .await
        .unwrap();
    let listed = repository.list(&context, 10).await.unwrap();

    assert_eq!(listed.len(), 1);
    let revision = listed[0].latest_revision.as_ref().unwrap();
    assert_eq!(revision.byte_size, content.bytes.len() as u64);
    assert_eq!(revision.media_type, "text/plain");
}

#[sqlx::test(migrations = "../../migrations")]
async fn empty_content_is_storable_and_distinguishable_from_absent(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let repository = PgArtifactRepository::new(PgStore::from_pool(pool));
    let context = context(workspace);
    let empty = ArtifactContent {
        media_type: "text/plain".into(),
        bytes: Vec::new(),
    };

    // A model that returns nothing produced a result, and that is not the same
    // as having produced no artifact at all.
    let stored = repository.store(&context, "empty", &empty).await.unwrap();
    let fetched = repository
        .fetch_content(&context, &stored.content_hash)
        .await
        .unwrap();

    assert_eq!(stored.byte_size, 0);
    assert_eq!(fetched, Some(empty));
}
