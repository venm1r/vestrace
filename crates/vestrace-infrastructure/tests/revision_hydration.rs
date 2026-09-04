//! Database-backed tests for revision hydration.
//!
//! # What these can and cannot show
//!
//! `sqlx::test` connects as the database owner, so row level security is **not**
//! exercised here — a workspace boundary that these tests appear to prove is
//! proved by the `WHERE workspace_id = $1` predicate alone, not by the policy.
//! RET-004 remains open for exactly that reason. What they do show is the
//! property no in-memory test can: that the query returns the revision that was
//! asked for, against a table that also holds a newer one.

use sqlx::PgPool;
use vestrace_application::RequestContext;
use vestrace_application::retrieval::RevisionHydrator;
use vestrace_domain::retrieval::RevisionRef;
use vestrace_domain::{
    PrincipalId, WorkspaceId,
    id::{MemoryId, MemoryRevisionId},
};
use vestrace_infrastructure::{PgRevisionHydrator, PgStore};

struct Fixture {
    context: RequestContext,
    memory_id: MemoryId,
    first: MemoryRevisionId,
    latest: MemoryRevisionId,
}

/// A memory with two revisions: an older one and the current one.
async fn seed(pool: &PgPool, classification: Option<&str>) -> Fixture {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let memory_id = MemoryId::new();
    let first = MemoryRevisionId::new();
    let latest = MemoryRevisionId::new();

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

    // Left as `candidate`: the schema enforces that an active memory has at
    // least one source (MEM-005, as a trigger), and these tests are about
    // resolving a reference, not about the source graph. The status is asserted
    // below so the join and the decode are still covered.
    sqlx::query(
        "INSERT INTO memories (id, workspace_id, kind, status, state_revision) \
         VALUES ($1, $2, 'fact', 'candidate', 2)",
    )
    .bind(memory_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .execute(pool)
    .await
    .expect("memory");

    for (id, number, content) in [
        (first, 1, "the original statement"),
        (latest, 2, "the corrected statement"),
    ] {
        sqlx::query(
            "INSERT INTO memory_revisions \
             (id, memory_id, workspace_id, revision_number, content, confidence, importance, classification) \
             VALUES ($1, $2, $3, $4, $5, 1.0, 0.5, $6)",
        )
        .bind(id.as_uuid())
        .bind(memory_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(number)
        .bind(content)
        .bind(classification)
        .execute(pool)
        .await
        .expect("revision");
    }

    Fixture {
        context: RequestContext::new(workspace_id, principal_id),
        memory_id,
        first,
        latest,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_older_revision_is_returned_rather_than_the_current_one(pool: PgPool) {
    // The property RET-002 names: asking for revision 1 while revision 2 exists
    // must return revision 1. Substituting the latest is how a caller asking
    // about the past is served the present, with the reference in hand as
    // apparent proof that it did not happen.
    let fixture = seed(&pool, None).await;
    let hydrator = PgRevisionHydrator::new(PgStore::from_pool(pool));

    let hydrated = hydrator
        .hydrate(
            &fixture.context,
            &[RevisionRef {
                memory_id: fixture.memory_id,
                revision_id: fixture.first,
            }],
        )
        .await
        .expect("hydration succeeds");

    assert_eq!(hydrated.len(), 1);
    assert_eq!(hydrated[0].revision_id, fixture.first);
    assert_eq!(hydrated[0].revision_number, 1);
    assert_eq!(hydrated[0].content, "the original statement");
    // The status comes from the memory, which the revision row does not carry;
    // a reader needs it to tell retired content from current content.
    assert_eq!(
        hydrated[0].memory_status,
        vestrace_domain::MemoryStatus::Candidate
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_reference_resolves_to_its_own_revision(pool: PgPool) {
    let fixture = seed(&pool, None).await;
    let hydrator = PgRevisionHydrator::new(PgStore::from_pool(pool));

    let hydrated = hydrator
        .hydrate(
            &fixture.context,
            &[
                RevisionRef {
                    memory_id: fixture.memory_id,
                    revision_id: fixture.first,
                },
                RevisionRef {
                    memory_id: fixture.memory_id,
                    revision_id: fixture.latest,
                },
            ],
        )
        .await
        .expect("hydration succeeds");

    assert_eq!(hydrated.len(), 2);
    let first = hydrated
        .iter()
        .find(|r| r.revision_id == fixture.first)
        .expect("the older revision");
    let latest = hydrated
        .iter()
        .find(|r| r.revision_id == fixture.latest)
        .expect("the current revision");
    assert_eq!(first.content, "the original statement");
    assert_eq!(latest.content, "the corrected statement");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_revision_yields_no_row_rather_than_a_substitute(pool: PgPool) {
    let fixture = seed(&pool, None).await;
    let hydrator = PgRevisionHydrator::new(PgStore::from_pool(pool));

    let hydrated = hydrator
        .hydrate(
            &fixture.context,
            &[RevisionRef {
                memory_id: fixture.memory_id,
                // A revision of this memory that does not exist.
                revision_id: MemoryRevisionId::new(),
            }],
        )
        .await
        .expect("hydration succeeds");

    assert!(
        hydrated.is_empty(),
        "an unknown revision resolved to something"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_revision_of_a_different_memory_is_not_returned(pool: PgPool) {
    // The two `ANY` predicates are independent, so a revision id from one
    // reference paired with a memory id from another satisfies both. The
    // adapter drops pairings nobody asked for; without that a caller could
    // hydrate a revision by naming any memory they happen to know.
    let fixture = seed(&pool, None).await;
    let hydrator = PgRevisionHydrator::new(PgStore::from_pool(pool));

    let hydrated = hydrator
        .hydrate(
            &fixture.context,
            &[RevisionRef {
                memory_id: MemoryId::new(),
                revision_id: fixture.first,
            }],
        )
        .await
        .expect("hydration succeeds");

    assert!(hydrated.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_reference_from_another_workspace_resolves_to_nothing(pool: PgPool) {
    // Proved here by the query predicate, not by row level security: this test
    // connects as the owner. See the module header.
    let fixture = seed(&pool, None).await;
    let stranger = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let hydrator = PgRevisionHydrator::new(PgStore::from_pool(pool));

    let hydrated = hydrator
        .hydrate(
            &stranger,
            &[RevisionRef {
                memory_id: fixture.memory_id,
                revision_id: fixture.first,
            }],
        )
        .await
        .expect("hydration succeeds");

    assert!(hydrated.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_stored_classification_travels_with_the_revision(pool: PgPool) {
    // The classification has to arrive for the policy to have anything to
    // decide on. A hydrator that dropped it would make every revision look
    // unassessed, which a lenient policy admits.
    let fixture = seed(&pool, Some("restricted")).await;
    let hydrator = PgRevisionHydrator::new(PgStore::from_pool(pool));

    let hydrated = hydrator
        .hydrate(
            &fixture.context,
            &[RevisionRef {
                memory_id: fixture.memory_id,
                revision_id: fixture.first,
            }],
        )
        .await
        .expect("hydration succeeds");

    assert_eq!(hydrated.len(), 1);
    assert_eq!(hydrated[0].classification.as_deref(), Some("restricted"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_stored_classification_fails_hydration_closed(pool: PgPool) {
    let fixture = seed(&pool, Some("internal")).await;
    sqlx::query("UPDATE memory_revisions SET classification = ' internal ' WHERE id = $1")
        .bind(fixture.first.as_uuid())
        .execute(&pool)
        .await
        .expect("corrupt stored classification");
    let hydrator = PgRevisionHydrator::new(PgStore::from_pool(pool));

    let error = hydrator
        .hydrate(
            &fixture.context,
            &[RevisionRef {
                memory_id: fixture.memory_id,
                revision_id: fixture.first,
            }],
        )
        .await
        .expect_err("malformed classification must not reach policy admission");

    assert!(error.to_string().contains("must be trimmed"), "{error}");
}
