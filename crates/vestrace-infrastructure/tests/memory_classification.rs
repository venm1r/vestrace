use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tokio::sync::Barrier;
use vestrace_application::retrieval::{
    EmbedMemoryHandler, EmbeddingProvider, EmbeddingStore, MemoryTextSource,
    SharedEmbeddingProvider,
};
use vestrace_application::{
    ApplicationError, EmbeddingDataPolicyGate, EmbeddingDataPolicyMode,
    EmbeddingDataPolicySettings, EventRepository, IdempotencyRepository,
    MemoryClassificationUpdate, MemoryRepository, MemoryService, OutboxHandler, OutboxRepository,
    ProvenanceRepository, ProviderEgress, RecordEventCommand, RelationRepository,
    RememberMemoryCommand, RequestContext, ReviseMemoryCommand,
};
use vestrace_domain::retrieval::ClassificationPolicy;
use vestrace_domain::trust::DataPolicy;
use vestrace_domain::{
    ActorRef, Confidence, DataDestination, DataPolicyId, EvidenceRole, Importance, MemoryKind,
    MemoryLabelVocabulary, MemoryRevision, MemorySource, MemoryWritePolicy, PrincipalId,
    Sensitivity, WorkspaceId,
    id::{EventId, MemoryId, MemoryRevisionId},
};
use vestrace_infrastructure::{
    PgEmbeddingDataPolicyDecisionRepository, PgEmbeddingStore, PgEventRepository,
    PgIdempotencyRepository, PgMemoryRepository, PgMemoryTextSource, PgOutboxRepository,
    PgProvenanceRepository, PgRelationRepository, PgStore,
};

type PgMemoryService = MemoryService<
    PgEventRepository,
    PgMemoryRepository,
    PgProvenanceRepository,
    PgRelationRepository,
    PgOutboxRepository,
    PgIdempotencyRepository,
>;

fn memory_service(
    pool: &PgPool,
    labels: impl IntoIterator<Item = &'static str>,
) -> PgMemoryService {
    let store = PgStore::from_pool(pool.clone());
    MemoryService::new(
        PgEventRepository::new(store.clone()),
        PgMemoryRepository::new(store.clone()),
        PgProvenanceRepository::new(store.clone()),
        PgRelationRepository::new(store.clone()),
        PgOutboxRepository::new(store.clone()),
        PgIdempotencyRepository::new(store),
        MemoryLabelVocabulary::new(labels).unwrap(),
    )
}

async fn context(pool: &PgPool, name: &str) -> RequestContext {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("memory-classification-{name}-{workspace_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("memory-classification-{name}"))
        .execute(pool)
        .await
        .unwrap();
    RequestContext::new(workspace_id, principal_id)
}

async fn source_event(service: &PgMemoryService, context: &RequestContext, key: &str) -> EventId {
    let id = EventId::new();
    service
        .record_event(
            context,
            RecordEventCommand {
                id,
                session_id: None,
                event_type: "observation".to_owned(),
                actor: ActorRef::User("classification-test".to_owned()),
                subject: None,
                payload: serde_json::json!({"key": key}),
                idempotency_key: format!("event-{key}"),
            },
        )
        .await
        .unwrap();
    id
}

fn remember(
    memory_id: MemoryId,
    source_event_id: EventId,
    classification: Option<&str>,
    key: &str,
) -> RememberMemoryCommand {
    RememberMemoryCommand {
        memory_id,
        kind: MemoryKind::Fact,
        content: format!("memory {key}"),
        structured: None,
        confidence: Confidence::new(0.9).unwrap(),
        importance: Importance::new(0.7).unwrap(),
        source_event_id,
        evidence_role: EvidenceRole::DirectSource,
        policy: MemoryWritePolicy::Manual,
        classification: classification.map(str::to_owned),
        idempotency_key: key.to_owned(),
    }
}

fn revise(
    memory_id: MemoryId,
    expected_revision: u32,
    source_event_id: EventId,
    classification: MemoryClassificationUpdate,
    key: &str,
) -> ReviseMemoryCommand {
    ReviseMemoryCommand {
        memory_id,
        expected_revision,
        content: format!("revision {key}"),
        structured: None,
        confidence: Confidence::new(0.8).unwrap(),
        importance: Importance::new(0.6).unwrap(),
        source_event_id,
        change_reason: Some("classification contract test".to_owned()),
        classification,
        idempotency_key: key.to_owned(),
    }
}

async fn stored_classification(pool: &PgPool, revision_id: MemoryRevisionId) -> Option<String> {
    sqlx::query_scalar("SELECT classification FROM memory_revisions WHERE id = $1")
        .bind(revision_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_stored_labels_fail_closed_in_both_embedding_readers(pool: PgPool) {
    let context = context(&pool, "malformed-reader").await;
    let service = memory_service(&pool, ["internal"]);
    let memory_id = MemoryId::new();
    let memory = service
        .remember_memory(
            &context,
            remember(
                memory_id,
                source_event(&service, &context, "malformed-reader").await,
                Some("internal"),
                "malformed-reader",
            ),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE memory_revisions SET classification = '   ' WHERE id = $1")
        .bind(memory.active_revision_id.unwrap().as_uuid())
        .execute(&pool)
        .await
        .unwrap();

    let store = PgStore::from_pool(pool);
    let delivery_error = PgMemoryTextSource::new(store.clone())
        .active_text(&context, memory_id)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        delivery_error.contains("memory_revisions.classification"),
        "{delivery_error}"
    );

    let embedding_store = PgEmbeddingStore::new(store);
    let space = embedding_store
        .ensure_space(&context, "malformed-reader", "test-model", 3)
        .await
        .unwrap();
    let backfill_error = embedding_store
        .memories_without_embedding(&context, &space, 10)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        backfill_error.contains("memory_revisions.classification"),
        "{backfill_error}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn creation_persists_declared_and_absent_labels_and_refuses_unknown_ones(pool: PgPool) {
    let context = context(&pool, "creation").await;
    let service = memory_service(&pool, ["internal", "restricted"]);

    let labelled_id = MemoryId::new();
    let labelled = service
        .remember_memory(
            &context,
            remember(
                labelled_id,
                source_event(&service, &context, "labelled").await,
                Some(" internal "),
                "labelled",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stored_classification(&pool, labelled.active_revision_id.unwrap()).await,
        Some("internal".to_owned())
    );

    let unlabelled = service
        .remember_memory(
            &context,
            remember(
                MemoryId::new(),
                source_event(&service, &context, "unlabelled").await,
                None,
                "unlabelled",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stored_classification(&pool, unlabelled.active_revision_id.unwrap()).await,
        None
    );

    let error = service
        .remember_memory(
            &context,
            remember(
                MemoryId::new(),
                source_event(&service, &context, "unknown").await,
                Some("confidential"),
                "unknown",
            ),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("confidential"), "{error}");
    assert!(error.contains("internal"), "{error}");
    assert!(error.contains("restricted"), "{error}");

    let empty = memory_service(&pool, []);
    let allowed = empty
        .remember_memory(
            &context,
            remember(
                MemoryId::new(),
                source_event(&empty, &context, "empty-absent").await,
                None,
                "empty-absent",
            ),
        )
        .await;
    assert!(allowed.is_ok());
    let refused = empty
        .remember_memory(
            &context,
            remember(
                MemoryId::new(),
                source_event(&empty, &context, "empty-labelled").await,
                Some("internal"),
                "empty-labelled",
            ),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(refused.contains("configured labels: []"), "{refused}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn revise_inherits_or_accepts_the_identical_label_and_refuses_every_change(pool: PgPool) {
    let context = context(&pool, "revise").await;
    let service = memory_service(&pool, ["internal", "restricted"]);
    let memory_id = MemoryId::new();
    service
        .remember_memory(
            &context,
            remember(
                memory_id,
                source_event(&service, &context, "create").await,
                Some("internal"),
                "create",
            ),
        )
        .await
        .unwrap();

    let inherited = service
        .revise_memory(
            &context,
            revise(
                memory_id,
                1,
                source_event(&service, &context, "inherit").await,
                MemoryClassificationUpdate::Inherit,
                "inherit",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stored_classification(&pool, inherited.active_revision_id.unwrap()).await,
        Some("internal".to_owned())
    );

    let identical = service
        .revise_memory(
            &context,
            revise(
                memory_id,
                2,
                source_event(&service, &context, "identical").await,
                MemoryClassificationUpdate::Set(" internal ".to_owned()),
                "identical",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stored_classification(&pool, identical.active_revision_id.unwrap()).await,
        Some("internal".to_owned())
    );

    for (key, classification) in [
        (
            "different",
            MemoryClassificationUpdate::Set("restricted".to_owned()),
        ),
        ("clear", MemoryClassificationUpdate::Clear),
    ] {
        let error = service
            .revise_memory(
                &context,
                revise(
                    memory_id,
                    3,
                    source_event(&service, &context, key).await,
                    classification,
                    key,
                ),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("no label transition mechanism exists"),
            "{error}"
        );
        assert!(!error.contains("DeclassificationDecision"), "{error}");
    }

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_revisions WHERE memory_id = $1")
            .bind(memory_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_idempotency_key_cannot_replay_a_different_stated_label(pool: PgPool) {
    let context = context(&pool, "idempotency").await;
    let service = memory_service(&pool, ["internal", "restricted"]);
    let event_id = source_event(&service, &context, "idempotency").await;
    service
        .remember_memory(
            &context,
            remember(MemoryId::new(), event_id, Some("internal"), "same-key"),
        )
        .await
        .unwrap();

    let error = service
        .remember_memory(
            &context,
            remember(MemoryId::new(), event_id, Some("restricted"), "same-key"),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("idempotency key reused with different request"),
        "{error}"
    );
}

#[derive(Clone)]
struct RacingMemoryRepository {
    inner: Arc<PgMemoryRepository>,
    revise_barrier: Arc<Barrier>,
}

#[async_trait]
impl MemoryRepository for RacingMemoryRepository {
    async fn save_memory_with_revision(
        &self,
        context: &RequestContext,
        memory: &vestrace_domain::Memory,
        revision: &MemoryRevision,
        source: &MemorySource,
    ) -> Result<(), ApplicationError> {
        if revision.revision_number > 1 {
            self.revise_barrier.wait().await;
        }
        self.inner
            .save_memory_with_revision(context, memory, revision, source)
            .await
    }

    async fn save_memory(
        &self,
        context: &RequestContext,
        memory: &vestrace_domain::Memory,
    ) -> Result<(), ApplicationError> {
        self.inner.save_memory(context, memory).await
    }

    async fn save_revision(
        &self,
        context: &RequestContext,
        revision: &MemoryRevision,
    ) -> Result<(), ApplicationError> {
        self.inner.save_revision(context, revision).await
    }

    async fn find_memory_by_id(
        &self,
        context: &RequestContext,
        id: MemoryId,
    ) -> Result<Option<vestrace_domain::Memory>, ApplicationError> {
        self.inner.find_memory_by_id(context, id).await
    }

    async fn find_revision_by_id(
        &self,
        context: &RequestContext,
        id: MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, ApplicationError> {
        self.inner.find_revision_by_id(context, id).await
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_labelled_revise_has_one_winner_and_one_revision_conflict(pool: PgPool) {
    let context = context(&pool, "concurrency").await;
    let store = PgStore::from_pool(pool.clone());
    let service = Arc::new(MemoryService::new(
        PgEventRepository::new(store.clone()),
        RacingMemoryRepository {
            inner: Arc::new(PgMemoryRepository::new(store.clone())),
            revise_barrier: Arc::new(Barrier::new(2)),
        },
        PgProvenanceRepository::new(store.clone()),
        PgRelationRepository::new(store.clone()),
        PgOutboxRepository::new(store.clone()),
        PgIdempotencyRepository::new(store),
        MemoryLabelVocabulary::new(["internal"]).unwrap(),
    ));
    let memory_id = MemoryId::new();
    service
        .remember_memory(
            &context,
            remember(
                memory_id,
                source_event_for(&service, &context, "race-create").await,
                Some("internal"),
                "race-create",
            ),
        )
        .await
        .unwrap();
    let source_a = source_event_for(&service, &context, "race-a").await;
    let source_b = source_event_for(&service, &context, "race-b").await;

    let first = {
        let service = Arc::clone(&service);
        let context = context.clone();
        tokio::spawn(async move {
            service
                .revise_memory(
                    &context,
                    revise(
                        memory_id,
                        1,
                        source_a,
                        MemoryClassificationUpdate::Inherit,
                        "race-a",
                    ),
                )
                .await
        })
    };
    let second = {
        let service = Arc::clone(&service);
        let context = context.clone();
        tokio::spawn(async move {
            service
                .revise_memory(
                    &context,
                    revise(
                        memory_id,
                        1,
                        source_b,
                        MemoryClassificationUpdate::Inherit,
                        "race-b",
                    ),
                )
                .await
        })
    };
    let results = [first.await.unwrap(), second.await.unwrap()];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let loser = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .unwrap();
    assert!(
        matches!(
            loser,
            ApplicationError::Domain(vestrace_domain::DomainError::RevisionConflict {
                expected: 1,
                current: 2
            })
        ),
        "unexpected loser: {loser}"
    );
    let rows = sqlx::query(
        "SELECT revision_number, classification FROM memory_revisions
         WHERE memory_id = $1 ORDER BY revision_number",
    )
    .bind(memory_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].get::<i32, _>("revision_number"), 2);
    assert_eq!(
        rows[1].get::<Option<String>, _>("classification"),
        Some("internal".to_owned())
    );
}

async fn source_event_for<E, M, P, R, O, I>(
    service: &MemoryService<E, M, P, R, O, I>,
    context: &RequestContext,
    key: &str,
) -> EventId
where
    E: EventRepository,
    M: MemoryRepository,
    P: ProvenanceRepository,
    R: RelationRepository,
    O: OutboxRepository,
    I: IdempotencyRepository,
{
    let id = EventId::new();
    service
        .record_event(
            context,
            RecordEventCommand {
                id,
                session_id: None,
                event_type: "observation".to_owned(),
                actor: ActorRef::User("classification-test".to_owned()),
                subject: None,
                payload: serde_json::json!({"key": key}),
                idempotency_key: format!("event-{key}"),
            },
        )
        .await
        .unwrap();
    id
}

struct CountingProvider(Arc<Mutex<u32>>);

#[async_trait]
impl EmbeddingProvider for CountingProvider {
    fn model(&self) -> &str {
        "must-not-be-called"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
        *self.0.lock().unwrap() += 1;
        Ok(inputs.iter().map(|_| vec![0.1, 0.2]).collect())
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_real_labelled_memory_is_refused_end_to_end_by_the_governed_provider(pool: PgPool) {
    let context = context(&pool, "headline").await;
    let service = memory_service(&pool, ["restricted"]);
    let memory_id = MemoryId::new();
    service
        .remember_memory(
            &context,
            remember(
                memory_id,
                source_event(&service, &context, "headline").await,
                Some("restricted"),
                "headline",
            ),
        )
        .await
        .unwrap();

    let store = PgStore::from_pool(pool.clone());
    let message = PgOutboxRepository::new(store.clone())
        .claim_pending(&context, 10)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.topic == "memory.created")
        .unwrap();
    let calls = Arc::new(Mutex::new(0));
    let raw: SharedEmbeddingProvider = Arc::new(CountingProvider(Arc::clone(&calls)));
    let gate = EmbeddingDataPolicyGate::new(
        EmbeddingDataPolicySettings {
            classification_policy: ClassificationPolicy::new(["internal"], true).unwrap(),
            classification: Sensitivity::Restricted,
            policy: DataPolicy::new(
                DataPolicyId::new(),
                "memory-classification-headline-v1",
                Sensitivity::Restricted,
                BTreeSet::from([DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            mode: EmbeddingDataPolicyMode::Enforce,
        },
        Arc::new(PgEmbeddingDataPolicyDecisionRepository::new(store.clone())),
    );
    let governed = gate.govern(
        raw,
        ProviderEgress::new(
            "http://localhost:12345/v1/embeddings",
            DataDestination::LocalModel,
            true,
            true,
        ),
    );
    let handler = EmbedMemoryHandler::new(
        governed,
        Arc::new(PgEmbeddingStore::new(store.clone())),
        Arc::new(PgMemoryTextSource::new(store)),
        "headline-space",
        "memory.created",
    );

    let error = handler.handle(&context, &message).await.unwrap_err();

    assert!(error.to_string().contains("restricted"), "{error}");
    assert_eq!(*calls.lock().unwrap(), 0, "the raw provider was called");
    let evidence = sqlx::query(
        "SELECT classification_labels, classification_allowed, verdict
         FROM embedding_data_policy_decisions WHERE causal_reference_id = $1",
    )
    .bind(message.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        evidence.get::<Vec<String>, _>("classification_labels"),
        vec!["restricted"]
    );
    assert!(!evidence.get::<bool, _>("classification_allowed"));
    assert_eq!(evidence.get::<String, _>("verdict"), "denied");
}
