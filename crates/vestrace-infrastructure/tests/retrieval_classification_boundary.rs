//! Composed PostgreSQL proof for the production retrieval classification boundary.

use std::sync::Arc;

use sqlx::PgPool;
use vestrace_application::{RetrievalRequest, RetrievalService};
use vestrace_domain::{
    MemoryStatus, TimePerspective, WorkspaceId,
    id::{MemoryId, MemoryRevisionId},
    retrieval::{ClassificationPolicy, WithholdingReason},
};
use vestrace_infrastructure::{
    PgCorpusGenerationResolver, PgRetrievalJournal, PgRevisionHydrator, PgStore, PgTextRetriever,
};

mod common;

const EMBEDDING_SPACE_NAME: &str = "retrieval-classification";
const EMBEDDING_MODEL_NAME: &str = common::result_preparation_fixture::RESULT_MODEL;

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

    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision_id.as_uuid())
        .bind(memory_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    (memory_id, revision_id)
}

#[sqlx::test(migrations = false)]
async fn postgres_retrieval_withholds_inadmissible_content_without_hiding_the_gap(pool: PgPool) {
    let mut corpus = common::canonical_memory_fixture::new(&pool, EMBEDDING_SPACE_NAME).await;
    let context = corpus.accepted.context.clone();
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
    corpus
        .publish_memories(&pool, &[internal_memory, restricted_memory])
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

#[derive(Clone, Copy)]
enum BetweenDiscoveryAndHydration {
    EraseSource,
    ReplaceGeneration,
    DeleteMemory,
    AdvanceRevision,
}

struct MutateBeforeFinalHydration {
    runtime: PgPool,
    delegate: PgRevisionHydrator,
    source: uuid::Uuid,
    memory: MemoryId,
    registration: uuid::Uuid,
    mutation: BetweenDiscoveryAndHydration,
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl vestrace_application::retrieval::RevisionHydrator for MutateBeforeFinalHydration {
    async fn hydrate(
        &self,
        _: &vestrace_application::RequestContext,
        _: &[vestrace_domain::retrieval::RevisionRef],
    ) -> Result<
        Vec<vestrace_domain::retrieval::HydratedRevision>,
        vestrace_application::ApplicationError,
    > {
        panic!("canonical service must invoke final hydration with its normalized generation pin")
    }
    async fn hydrate_for_retrieval(
        &self,
        context: &vestrace_application::RequestContext,
        request: &vestrace_application::NormalizedRetrievalRequest,
        references: &[vestrace_domain::retrieval::RevisionRef],
    ) -> Result<
        Vec<vestrace_domain::retrieval::HydratedRevision>,
        vestrace_application::ApplicationError,
    > {
        assert_eq!(
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert_eq!(
            references.len(),
            1,
            "the finder must discover the candidate before the mutation"
        );
        assert_eq!(references[0].memory_id, self.memory);
        let mut tx = self.runtime.begin().await.unwrap();
        common::result_preparation_fixture::scoped(&mut tx, context.workspace_id.as_uuid()).await;
        match self.mutation {
            BetweenDiscoveryAndHydration::EraseSource => {
                sqlx::query("SELECT vestrace_propagate_embedding_source_erasure($1,$2,$3)")
                    .bind(uuid::Uuid::now_v7())
                    .bind(context.workspace_id.as_uuid())
                    .bind(self.source)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            BetweenDiscoveryAndHydration::ReplaceGeneration => {
                let version:i64=sqlx::query_scalar("SELECT guard_version FROM embedding_index_generation_guards WHERE workspace_id=$1 AND space_registration_id=$2")
                    .bind(context.workspace_id.as_uuid()).bind(self.registration).fetch_one(&mut *tx).await.unwrap();
                let next = uuid::Uuid::now_v7();
                sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,$4)")
                    .bind(next)
                    .bind(context.workspace_id.as_uuid())
                    .bind(self.registration)
                    .bind(version)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                sqlx::query("SELECT vestrace_publish_embedding_generation($1,$2,$3,$4)")
                    .bind(context.workspace_id.as_uuid())
                    .bind(self.registration)
                    .bind(next)
                    .bind(version)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            BetweenDiscoveryAndHydration::DeleteMemory => {
                sqlx::query("UPDATE memories SET status='deleted' WHERE workspace_id=$1 AND id=$2")
                    .bind(context.workspace_id.as_uuid())
                    .bind(self.memory.as_uuid())
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            BetweenDiscoveryAndHydration::AdvanceRevision => {
                let revision = MemoryRevisionId::new();
                sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance,classification) VALUES($1,$2,$3,2,'volcano unrepresented replacement secret',1,0.5,'internal')")
                    .bind(revision.as_uuid()).bind(self.memory.as_uuid()).bind(context.workspace_id.as_uuid()).execute(&mut *tx).await.unwrap();
                sqlx::query(
                    "UPDATE memories SET active_revision_id=$1 WHERE workspace_id=$2 AND id=$3",
                )
                .bind(revision.as_uuid())
                .bind(context.workspace_id.as_uuid())
                .bind(self.memory.as_uuid())
                .execute(&mut *tx)
                .await
                .unwrap();
            }
        }
        tx.commit().await.unwrap();
        vestrace_application::retrieval::RevisionHydrator::hydrate_for_retrieval(
            &self.delegate,
            context,
            request,
            references,
        )
        .await
    }
}

async fn final_hydration_race(pool: PgPool, mutation: BetweenDiscoveryAndHydration) {
    let mut corpus = common::canonical_memory_fixture::new(&pool, EMBEDDING_SPACE_NAME).await;
    let context = corpus.accepted.context.clone();
    let (memory, revision) = seed_memory(
        &pool,
        context.workspace_id,
        "volcano discovered secret",
        "internal",
    )
    .await;
    let source = corpus.publish_revision(&pool, revision.as_uuid()).await;
    // Current retrieval admits Active memories. Publish the source event and
    // its provenance before activation so the real deferred invariant applies.
    let event = uuid::Uuid::now_v7();
    let mut activation = corpus.runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut activation, context.workspace_id.as_uuid())
        .await;
    sqlx::query("INSERT INTO events(id,workspace_id,event_type,actor,payload) VALUES($1,$2,'memory.observed',$3,$4)")
        .bind(event).bind(context.workspace_id.as_uuid())
        .bind(serde_json::json!({"principal_id":context.principal_id}))
        .bind(serde_json::json!({"memory_id":memory,"revision_id":revision}))
        .execute(&mut *activation).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(id,memory_id,workspace_id,event_id,role) VALUES($1,$2,$3,$4,'primary')")
        .bind(uuid::Uuid::now_v7()).bind(memory.as_uuid()).bind(context.workspace_id.as_uuid()).bind(event)
        .execute(&mut *activation).await.unwrap();
    sqlx::query("UPDATE memories SET status='active' WHERE workspace_id=$1 AND id=$2")
        .bind(context.workspace_id.as_uuid())
        .bind(memory.as_uuid())
        .execute(&mut *activation)
        .await
        .unwrap();
    activation.commit().await.unwrap();
    corpus.capture().await;
    let store = PgStore::from_pool(corpus.runtime.clone());
    let boundary = Arc::new(MutateBeforeFinalHydration {
        runtime: corpus.runtime.clone(),
        delegate: PgRevisionHydrator::new(store.clone()),
        source,
        memory,
        registration: corpus.accepted.space_registration_id,
        mutation,
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let service = RetrievalService::new(
        Arc::new(PgTextRetriever::new(store.clone())),
        Arc::new(PgRetrievalJournal::new(store.clone())),
    )
    .with_hydration(
        boundary.clone(),
        ClassificationPolicy::new(["internal"], false).unwrap(),
        "final-hydration-race-v1",
    )
    .with_corpus_generation_resolver(
        Arc::new(PgCorpusGenerationResolver::new(store)),
        EMBEDDING_SPACE_NAME,
        EMBEDDING_MODEL_NAME,
    );
    let result = service
        .search(
            &context,
            RetrievalRequest::new(context.workspace_id, "volcano")
                .with_allowed_statuses(vec![MemoryStatus::Candidate]),
        )
        .await;
    assert_eq!(boundary.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    match mutation {
        BetweenDiscoveryAndHydration::EraseSource
        | BetweenDiscoveryAndHydration::ReplaceGeneration => {
            let error =
                result.expect_err("revoked or replaced canonical pin must fail final hydration");
            assert!(
                error.to_string().contains("canonical generation"),
                "{error}"
            );
            assert!(!error.to_string().contains("discovered secret"));
        }
        BetweenDiscoveryAndHydration::DeleteMemory
        | BetweenDiscoveryAndHydration::AdvanceRevision => {
            let result = result.expect("ineligible exact revision is safely withheld");
            assert!(result.candidates.is_empty());
            assert_eq!(result.withheld.len(), 1);
            assert_eq!(result.withheld[0].memory_id, memory);
            assert_eq!(result.withheld[0].revision_id, revision);
            assert_eq!(
                result.withheld[0].reason,
                WithholdingReason::RevisionNotFound
            );
            let journal: serde_json::Value =
                sqlx::query_scalar("SELECT parameters FROM retrieval_runs WHERE id=$1")
                    .bind(result.run_id.as_uuid())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let safe =
                serde_json::json!({"withheld":result.withheld,"journal":journal}).to_string();
            assert!(!safe.contains("discovered secret"));
            assert!(!safe.contains("unrepresented replacement secret"));
        }
    }
}

#[sqlx::test(migrations = false)]
async fn source_erased_after_discovery_is_not_rehydrated(pool: PgPool) {
    final_hydration_race(pool, BetweenDiscoveryAndHydration::EraseSource).await;
}
#[sqlx::test(migrations = false)]
async fn generation_replaced_after_discovery_is_not_rehydrated(pool: PgPool) {
    final_hydration_race(pool, BetweenDiscoveryAndHydration::ReplaceGeneration).await;
}
#[sqlx::test(migrations = false)]
async fn memory_deleted_after_discovery_is_safely_withheld(pool: PgPool) {
    final_hydration_race(pool, BetweenDiscoveryAndHydration::DeleteMemory).await;
}
#[sqlx::test(migrations = false)]
async fn revision_advanced_after_discovery_does_not_supply_unrepresented_content(pool: PgPool) {
    final_hydration_race(pool, BetweenDiscoveryAndHydration::AdvanceRevision).await;
}
