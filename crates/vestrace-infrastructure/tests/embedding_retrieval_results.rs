//! A retrieval attempt owns its fence, and a fence is cashed at most once.
//!
//! The property under test is that a stored answer can always be traced to the
//! exact generation it answered.  A generation that moves while an attempt is
//! in flight closes that attempt as changed rather than letting its answer
//! land, and only such a change may be followed by exactly one successor.

mod common;

use common::result_preparation_fixture::provision_result_behavior_database;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

async fn scoped(transaction: &mut Transaction<'_, Postgres>, workspace: Uuid) {
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
}

fn assert_refusal(error: sqlx::Error, expected_state: &str, expected_message: &str) {
    let database = error.as_database_error().expect("a database refusal");
    assert_eq!(
        database.code().as_deref(),
        Some(expected_state),
        "unexpected sqlstate; message was {:?}",
        database.message()
    );
    assert!(
        database.message().contains(expected_message),
        "expected refusal {expected_message:?}, got {:?}",
        database.message()
    );
}

struct Attempt {
    workspace: Uuid,
    space: Uuid,
    job: Uuid,
    generation: Uuid,
    corpus: common::canonical_memory_fixture::Corpus,
    references: Vec<(Uuid, Uuid, Uuid, Uuid)>,
}

async fn attempt(pool: &PgPool, _runtime: &PgPool) -> Attempt {
    use std::sync::Arc;
    use vestrace_infrastructure::postgres::*;
    let mut corpus =
        common::canonical_memory_fixture::new_in_database(pool, "result-canonical").await;
    let workspace = corpus.accepted.context.workspace_id.as_uuid();
    for content in ["first canonical answer", "second canonical answer"] {
        let memory = Uuid::now_v7();
        let revision = Uuid::now_v7();
        sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)").bind(memory).bind(workspace).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,$4,1,0.5)").bind(revision).bind(memory).bind(workspace).bind(content).execute(pool).await.unwrap();
        sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
            .bind(revision)
            .bind(memory)
            .execute(pool)
            .await
            .unwrap();
        corpus.publish_revision(pool, revision).await;
    }
    let generation = corpus.capture().await.as_uuid();
    let mut tx = corpus.runtime.begin().await.unwrap();
    scoped(&mut tx, workspace).await;
    let references: Vec<(Uuid,Uuid,Uuid,Uuid)>=sqlx::query_as("SELECT DISTINCT ON(revision_id) projection_id,source_material_id,memory_id,revision_id FROM vestrace_resolve_embedding_memory_references($1,$2,NULL) ORDER BY revision_id,projection_id")
        .bind(workspace).bind(generation).fetch_all(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(references.len(), 2);
    assert_ne!(references[0].2, references[1].2);
    assert_ne!(references[0].3, references[1].3);
    let store = PgStore::from_pool(corpus.runtime.clone());
    let vault = Arc::new(corpus.vault.vault(corpus.accepted.context.workspace_id));
    let source = PgGovernedContentMaterializer::new(
        store.clone(),
        vault.clone(),
        Arc::new(vestrace_infrastructure::crypto::ContentMaterialCodec::new()),
    )
    .materialize_bytes(
        &corpus.accepted.context,
        "retrieval_query",
        Uuid::now_v7(),
        b"canonical answer",
    )
    .await
    .unwrap();
    let job = PgGovernedEmbeddingJobFactory::new(
        store.clone(),
        Arc::new(PgEmbeddingJobRepository::new(store)),
        Arc::new(PgModelRequestEvidenceRepository::new(vault)),
        vestrace_application::EffectiveRequestLimits::new(8, 1, 2048).unwrap(),
    )
    .create_retrieval_job(
        &corpus.accepted.context,
        corpus.accepted.space_registration_id,
        source,
        GovernedEmbeddingJobPurpose {
            kind: vestrace_domain::embedding::EmbeddingJobKind::RetrievalQuery,
            subject_id: Uuid::now_v7(),
            cause: "canonical-result-probe",
            action: "embedding.job.retrieval_accepted",
            summary: "query canonical result fixture",
            detail: serde_json::json!({}),
        },
        generation,
    )
    .await
    .unwrap()
    .as_uuid();
    Attempt {
        workspace,
        space: corpus.accepted.space_registration_id,
        job,
        generation,
        corpus,
        references,
    }
}

/// The provider response is durably acknowledged; the test controls the next
/// terminal transaction to probe malformed references and concurrent invalidation.
struct HeldQueryResponse;
#[async_trait::async_trait]
impl vestrace_application::embedding::EmbeddingRetrievalSink for HeldQueryResponse {
    async fn complete(
        &self,
        _: &vestrace_application::RequestContext,
        _: vestrace_domain::EmbeddingJobId,
        _: vestrace_application::embedding::QueryEmbedding,
    ) -> Result<(), vestrace_application::ApplicationError> {
        Ok(())
    }
}
struct QueryAdapter(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl vestrace_application::run::GovernedModelAdapter for QueryAdapter {
    async fn execute(
        &self,
        _: vestrace_domain::ConnectionKind,
        _: &str,
        _: vestrace_application::ConnectionAuth,
        request: vestrace_application::EffectiveModelRequest,
    ) -> Result<vestrace_application::EffectiveModelResponse, vestrace_application::ProviderError>
    {
        assert!(matches!(
            request,
            vestrace_application::EffectiveModelRequest::Embeddings(_)
        ));
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(vestrace_application::EffectiveModelResponse::Embeddings(
            vestrace_application::GovernedEmbeddingsResponse::new(
                common::result_preparation_fixture::RESULT_MODEL,
                common::result_preparation_fixture::RESULT_MODEL.into(),
                vec![
                    vestrace_application::GovernedEmbeddingVector::from_provider_components(
                        0,
                        vec![1.0; 768],
                    )?,
                ],
                1,
            )?,
        ))
    }
}
async fn dispatch_query(a: &Attempt) {
    use std::sync::Arc;
    use vestrace_infrastructure::postgres::*;
    let store = PgStore::from_pool(a.corpus.runtime.clone());
    let vault = Arc::new(a.corpus.vault.vault(a.corpus.accepted.context.workspace_id));
    let gate = Arc::new(vestrace_application::EmbeddingDataPolicyGate::new(
        vestrace_application::EmbeddingDataPolicySettings {
            classification_policy: vestrace_domain::retrieval::ClassificationPolicy::new(
                Vec::<String>::new(),
                true,
            )
            .unwrap(),
            classification: vestrace_domain::Sensitivity::Internal,
            policy: vestrace_domain::trust::DataPolicy::new(
                vestrace_domain::DataPolicyId::new(),
                "result-query-policy",
                vestrace_domain::Sensitivity::Internal,
                std::collections::BTreeSet::from([vestrace_domain::DataDestination::LocalModel]),
                None,
            )
            .unwrap(),
            mode: vestrace_application::EmbeddingDataPolicyMode::Enforce,
        },
        Arc::new(PgEmbeddingDataPolicyDecisionRepository::new(store.clone())),
    ));
    let dispatch = Arc::new(
        PgProviderDispatchRepository::new(
            Arc::new(PgInstallationMutationPermit::new(store.clone())),
            Arc::new(PgModelRequestEvidenceRepository::new(vault.clone())),
            Arc::new(PgExternalEffectRepository::new(store.clone())),
            Arc::new(common::UnusedCredentialLeases),
            Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
            Arc::new(PgGovernedMutationRepository::new(store.clone())),
            Arc::new(common::AllowEmbeddingPolicy),
        )
        .with_embedding_policy(gate),
    );
    let preparation = Arc::new(
        vestrace_application::EmbeddingResultPreparationService::new(
            Arc::new(PgEmbeddingResultRepository::new(
                store.clone(),
                dispatch.clone(),
            )),
            vault.clone(),
            Arc::new(vestrace_infrastructure::crypto::ContentMaterialCodec::new()),
        ),
    );
    let finalization = Arc::new(
        vestrace_application::EmbeddingResultFinalizationService::new(
            Arc::new(PgEmbeddingResultFinalizationRepository::new(store)),
            vault,
            Arc::new(EmbeddingOutputHmacCommitter::new()),
        ),
    );
    let adapter = Arc::new(QueryAdapter(std::sync::atomic::AtomicUsize::new(0)));
    let outcome = vestrace_application::embedding::EmbeddingExecutor::new(
        dispatch,
        preparation,
        finalization,
        adapter.clone(),
        vestrace_domain::WorkerId::new(),
    )
    .with_retrieval_sink(Arc::new(HeldQueryResponse))
    .execute(
        &a.corpus.accepted.context,
        vestrace_domain::EmbeddingJobId::from_uuid(a.job),
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        vestrace_application::embedding::EmbeddingExecutionOutcome::Succeeded
    );
    assert_eq!(adapter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// Accepts one retrieval_query job over its own effect and evidence root.
async fn accept_retrieval_job(pool: &PgPool, runtime: &PgPool, base: &common::AcceptedJob) -> Uuid {
    let workspace = base.context.workspace_id.as_uuid();
    let effect = Uuid::now_v7();
    let evidence = Uuid::now_v7();
    let job = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) VALUES($1,$2,'local','{}'::jsonb)")
        .bind(effect)
        .bind(workspace)
        .execute(pool)
        .await
        .unwrap();
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, workspace).await;
    sqlx::query(
        "INSERT INTO model_request_evidence_roots\
         (id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) \
         VALUES($1,$2,$3,'embeddings',$4,'embedding_job',$5)",
    )
    .bind(evidence)
    .bind(workspace)
    .bind(effect)
    .bind(base.snapshot_id)
    .bind(job)
    .execute(&mut *owner)
    .await
    .unwrap();
    owner.commit().await.unwrap();

    let mut accept = runtime.begin().await.unwrap();
    scoped(&mut accept, workspace).await;
    let accepted: Uuid = sqlx::query_scalar(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'retrieval_query',$4,$5,$6,NULL,NULL::BIGINT)",
    )
    .bind(job)
    .bind(workspace)
    .bind(base.space_registration_id)
    .bind(base.snapshot_id)
    .bind(effect)
    .bind(evidence)
    .fetch_one(&mut *accept)
    .await
    .expect("the runtime role accepts one retrieval_query job");
    accept.commit().await.unwrap();
    assert_eq!(accepted, job);
    job
}

async fn accept_fence(runtime: &PgPool, a: &Attempt, request: Uuid) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_accept_embedding_retrieval_attempt($1,$2,$3,$4,$5,NOW()+INTERVAL '30 seconds')",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(request)
    .bind(a.space)
    .bind(a.generation)
    .fetch_one(&mut *transaction)
    .await;
    finish(transaction, result).await
}

async fn finalize(
    runtime: &PgPool,
    a: &Attempt,
    fence: Uuid,
    references: &[(Uuid, Uuid, i64, f64)],
) -> Result<Uuid, sqlx::Error> {
    if job_state(runtime, a).await == "requested" {
        dispatch_query(a).await;
    }
    let projections: Vec<Uuid> = references
        .iter()
        .map(|r| {
            a.references
                .iter()
                .find(|p| p.2 == r.0 && p.3 == r.1)
                .map(|p| p.0)
                .unwrap_or_else(Uuid::now_v7)
        })
        .collect();
    let sources: Vec<Uuid> = references
        .iter()
        .map(|r| {
            a.references
                .iter()
                .find(|p| p.2 == r.0 && p.3 == r.1)
                .map(|p| p.1)
                .unwrap_or_else(Uuid::now_v7)
        })
        .collect();
    let memories: Vec<Uuid> = references.iter().map(|r| r.0).collect();
    let revisions: Vec<Uuid> = references.iter().map(|r| r.1).collect();
    let ranks: Vec<i64> = references.iter().map(|r| r.2).collect();
    let scores: Vec<f64> = references.iter().map(|r| r.3).collect();
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(fence)
    .bind(projections)
    .bind(sources)
    .bind(memories)
    .bind(revisions)
    .bind(ranks)
    .bind(scores)
    .fetch_one(&mut *transaction)
    .await;
    finish(transaction, result).await
}

async fn observe_change(
    runtime: &PgPool,
    a: &Attempt,
    fence: Uuid,
    reason: &str,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_observe_embedding_retrieval_generation_change($1,$2,$3,$4)",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(fence)
    .bind(reason)
    .fetch_one(&mut *transaction)
    .await;
    finish(transaction, result).await
}

async fn authorize_retry(
    runtime: &PgPool,
    a: &Attempt,
    successor: Uuid,
    successor_request: Uuid,
    key: &str,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, a.workspace).await;
    let result =
        sqlx::query_scalar("SELECT vestrace_authorize_embedding_retrieval_retry($1,$2,$3,$4,$5)")
            .bind(a.workspace)
            .bind(a.job)
            .bind(successor)
            .bind(successor_request)
            .bind(key)
            .fetch_one(&mut *transaction)
            .await;
    finish(transaction, result).await
}

async fn finish(
    transaction: Transaction<'_, Postgres>,
    result: Result<Uuid, sqlx::Error>,
) -> Result<Uuid, sqlx::Error> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

/// Moves the space on by capturing and publishing a second generation, which
/// is what a rebuild does underneath an in-flight attempt.  The guard cannot be
/// nudged by hand: its own validator demands a consistent Ready snapshot.
async fn advance_generation(pool: &PgPool, runtime: &PgPool, a: &Attempt) {
    let current: i64 = sqlx::query_scalar(
        "SELECT guard_version FROM embedding_index_generation_guards          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(a.workspace)
    .bind(a.space)
    .fetch_one(pool)
    .await
    .unwrap();
    let next = Uuid::now_v7();
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, a.workspace).await;
    sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,$4)")
        .bind(next)
        .bind(a.workspace)
        .bind(a.space)
        .bind(current)
        .execute(&mut *governed)
        .await
        .expect("a successor generation may be captured");
    sqlx::query("SELECT vestrace_publish_embedding_generation($1,$2,$3,$4)")
        .bind(a.workspace)
        .bind(a.space)
        .bind(next)
        .bind(current)
        .execute(&mut *governed)
        .await
        .expect("the successor generation must publish Ready");
    governed.commit().await.unwrap();
}

async fn counts(pool: &PgPool, a: &Attempt) -> (i64, i64) {
    let results: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_retrieval_results WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(a.workspace)
    .bind(a.job)
    .fetch_one(pool)
    .await
    .unwrap();
    let changes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_retrieval_generation_changes \
         WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(a.workspace)
    .bind(a.job)
    .fetch_one(pool)
    .await
    .unwrap();
    (results, changes)
}

async fn job_state(pool: &PgPool, a: &Attempt) -> String {
    let mut transaction = pool.begin().await.unwrap();
    scoped(&mut transaction, a.workspace).await;
    let state =
        sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(a.workspace)
            .bind(a.job)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    transaction.commit().await.unwrap();
    state
}

/// Query-first order: the attempt cashes its fence while the pinned generation
/// is still current, and the answer becomes terminal.  A guard that moves
/// afterwards does not retract it.
#[sqlx::test(migrations = false)]
async fn a_result_lands_while_its_pinned_generation_is_current(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .expect("one attempt is admitted against the current generation");

    // The fence pinned exactly the generation that was current, not merely
    // some generation: this is what makes a stored answer traceable.
    let pinned: Uuid = sqlx::query_scalar(
        "SELECT generation_id FROM embedding_retrieval_fences WHERE workspace_id=$1 AND id=$2",
    )
    .bind(a.workspace)
    .bind(fence)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pinned, a.generation);

    let first = (a.references[0].2, a.references[0].3, 0_i64, 0.75_f64);
    let second = (a.references[1].2, a.references[1].3, 2_i64, 0.25_f64);
    finalize(&runtime, &a, fence, &[first, second])
        .await
        .expect("the pinned generation is still current, so the answer is terminal");

    assert_eq!(counts(&pool, &a).await, (1, 0));
    assert_eq!(job_state(&pool, &a).await, "succeeded");

    // The references are stored in order, and carry nothing but references.
    let stored: Vec<(i32, Uuid, Uuid, i32, f64)> = sqlx::query_as(
        "SELECT r.ordinal,r.memory_id,r.revision_id,r.rank,r.score \
         FROM embedding_retrieval_result_references r \
         JOIN embedding_retrieval_results s ON s.workspace_id=r.workspace_id AND s.id=r.result_id \
         WHERE s.workspace_id=$1 AND s.job_id=$2 ORDER BY r.ordinal",
    )
    .bind(a.workspace)
    .bind(a.job)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        vec![
            (0, first.0, first.1, 0, 0.75),
            (1, second.0, second.1, 2, 0.25)
        ]
    );

    // Staling afterwards leaves the terminal answer exactly as it was.
    advance_generation(&pool, &runtime, &a).await;
    assert_eq!(counts(&pool, &a).await, (1, 0));
    runtime.close().await;
}

/// Change-first order: the guard moves while the attempt is in flight, so the
/// fence can no longer be cashed and no result row is written at all.
#[sqlx::test(migrations = false)]
async fn a_generation_that_moves_first_refuses_the_result_and_stores_nothing(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();

    dispatch_query(&a).await;
    advance_generation(&pool, &runtime, &a).await;

    let error = finalize(
        &runtime,
        &a,
        fence,
        &[(a.references[0].2, a.references[0].3, 0, 1.0)],
    )
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval result requires its exact pinned generation",
    );
    assert_eq!(counts(&pool, &a).await, (0, 0));

    observe_change(&runtime, &a, fence, "stale")
        .await
        .expect("the attempt closes as a confirmed generation change");
    assert_eq!(counts(&pool, &a).await, (0, 1));
    assert_eq!(job_state(&pool, &a).await, "failed_definite");
    runtime.close().await;
}

/// A job carries a result or a generation change, never both, in either order.
#[sqlx::test(migrations = false)]
async fn a_result_and_a_generation_change_exclude_each_other(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();
    finalize(
        &runtime,
        &a,
        fence,
        &[(a.references[0].2, a.references[0].3, 0, 0.5)],
    )
    .await
    .unwrap();
    let error = observe_change(&runtime, &a, fence, "stale")
        .await
        .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval attempt already produced a terminal result",
    );
    assert_eq!(counts(&pool, &a).await, (1, 0));

    let b = attempt(&pool, &runtime).await;
    let other = accept_fence(&runtime, &b, Uuid::now_v7()).await.unwrap();
    observe_change(&runtime, &b, other, "replaced")
        .await
        .unwrap();
    let error = finalize(
        &runtime,
        &b,
        other,
        &[(b.references[0].2, b.references[0].3, 0, 0.5)],
    )
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval result requires completed query dispatch authority",
    );
    assert_eq!(counts(&pool, &b).await, (0, 1));
    runtime.close().await;
}

/// One fence per job and one logical request identity per fence.
#[sqlx::test(migrations = false)]
async fn a_job_takes_exactly_one_fence(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();
    let error = accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23505")
    );
    runtime.close().await;
}

/// One authorized successor: a replay under the same key returns it, and a
/// different key against the same predecessor conflicts.
#[sqlx::test(migrations = false)]
async fn one_confirmed_change_authorizes_exactly_one_successor(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();

    // No confirmed change yet.
    let successor = Uuid::now_v7();
    let error = authorize_retry(&runtime, &a, successor, Uuid::now_v7(), "key-1")
        .await
        .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding retrieval retry requires a confirmed generation change",
    );

    observe_change(&runtime, &a, fence, "corpus_changed")
        .await
        .unwrap();

    let request = Uuid::now_v7();
    let first = authorize_retry(&runtime, &a, successor, request, "key-1")
        .await
        .expect("a confirmed change authorizes one successor");
    assert_eq!(first, successor);

    // Same key replays the same successor.
    let replay = authorize_retry(&runtime, &a, successor, request, "key-1")
        .await
        .expect("the same key replays its own successor");
    assert_eq!(replay, successor);

    // A different key against the same predecessor conflicts.
    let error = authorize_retry(&runtime, &a, Uuid::now_v7(), Uuid::now_v7(), "key-2")
        .await
        .unwrap_err();
    assert_refusal(
        error,
        "23505",
        "embedding retrieval retry predecessor already has its successor",
    );

    let edges: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_retrieval_retry_edges WHERE workspace_id=$1",
    )
    .bind(a.workspace)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(edges, 1);
    runtime.close().await;
}

/// The runtime role reaches the retrieval authority only through its guarded
/// functions, and never writes these tables directly.
#[sqlx::test(migrations = false)]
async fn the_runtime_role_cannot_write_retrieval_tables_directly(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let rows: Vec<(String, String, bool, bool, bool, bool)> = sqlx::query_as(
        "SELECT relname,pg_get_userbyid(relowner),relrowsecurity,relforcerowsecurity, \
                has_table_privilege('vestrace',oid,'SELECT,REFERENCES'), \
                has_table_privilege('vestrace',oid,'INSERT') \
           FROM pg_class WHERE relname=ANY($1) ORDER BY relname",
    )
    .bind(vec![
        "embedding_retrieval_fences",
        "embedding_retrieval_generation_changes",
        "embedding_retrieval_result_references",
        "embedding_retrieval_results",
        "embedding_retrieval_retry_edges",
    ])
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert_eq!(rows.len(), 5);
    for (relation, owner, rls, force, read, insert) in rows {
        assert_eq!(owner, "vestrace_guarded_owner", "{relation} owner");
        assert!(rls && force, "{relation} must force RLS");
        assert!(read, "{relation} runtime read/reference ACL");
        assert!(!insert, "{relation} must refuse runtime INSERT");
    }
    runtime.close().await;
}

// --- What migration 0205 opened, and the two reads the worker makes through
// the fence it opened them for.

/// Claims one bounded batch of dispatch work as the runtime role.
async fn claim_dispatch(runtime: &PgPool, workspace: Uuid, owner: &str) -> Vec<Uuid> {
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, workspace).await;
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT job_id FROM vestrace_claim_embedding_work($1,'dispatch',$2,16)")
            .bind(workspace)
            .bind(owner)
            .fetch_all(&mut *governed)
            .await
            .expect("a dispatch claim");
    governed.commit().await.unwrap();
    rows.into_iter().map(|row| row.0).collect()
}

/// A retrieval query is claimable exactly when it has a live fence.
///
/// Before the fence there is no pinned generation, so dispatching would embed a
/// query against a corpus nothing had agreed on; after the deadline nobody is
/// waiting, so the provider call would be paid for an answer with no reader.
#[sqlx::test(migrations = false)]
async fn only_a_live_fence_makes_a_retrieval_query_claimable(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;

    let unfenced = claim_dispatch(&runtime, a.workspace, "worker:unfenced").await;
    assert!(
        !unfenced.contains(&a.job),
        "a retrieval query with no fence pins no generation and must not be claimable"
    );

    accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .expect("the fence");
    let fenced = claim_dispatch(&runtime, a.workspace, "worker:fenced").await;
    assert!(
        fenced.contains(&a.job),
        "a fenced retrieval query is exactly the work a worker holding a local index must take"
    );

    // Past its deadline it stops being claimable, and the claim is not merely
    // hidden by the one just taken: a fresh owner sees the same refusal.
    // Both tables are guarded, so the clock is moved as the guarded owner. No
    // product path expires a fence in place; this is a test reaching past the
    // authority on purpose, to observe what the claim does on the other side of
    // a deadline it cannot otherwise wait for.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, a.workspace).await;
    sqlx::query(
        "UPDATE embedding_retrieval_fences SET deadline=now()-INTERVAL '1 second'           WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(a.workspace)
    .bind(a.job)
    .execute(&mut *owner)
    .await
    .expect("an expired fence");
    sqlx::query("DELETE FROM embedding_job_work_claims WHERE workspace_id=$1 AND job_id=$2")
        .bind(a.workspace)
        .bind(a.job)
        .execute(&mut *owner)
        .await
        .expect("the claim just taken is cleared so a fresh owner may try");
    owner.commit().await.unwrap();
    let expired = claim_dispatch(&runtime, a.workspace, "worker:expired").await;
    assert!(
        !expired.contains(&a.job),
        "past its deadline nobody is waiting for the answer"
    );
    runtime.close().await;
}

/// The worker reads back exactly the generation the fence pinned, in the shape
/// the local registry keys an index by.
#[sqlx::test(migrations = false)]
async fn the_worker_reads_back_the_generation_the_fence_pinned(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7())
        .await
        .expect("the fence");

    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(a.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );
    let (read_fence, snapshot) = repository
        .pinned_attempt(&context, vestrace_domain::EmbeddingJobId::from_uuid(a.job))
        .await
        .expect("the pinned attempt of an accepted retrieval query");

    assert_eq!(read_fence, fence, "the fence a job owns is the one it took");
    assert_eq!(
        snapshot.generation_id.as_uuid(),
        a.generation,
        "the snapshot names the generation the fence pinned"
    );
    assert_eq!(snapshot.workspace_id.as_uuid(), a.workspace);
    assert!(
        snapshot.space.is_canonical(),
        "only a canonical space can carry a Ready generation to answer from"
    );
    // Every number comes from the generation itself, so a snapshot assembled
    // from two sources cannot name a pair that never existed together.
    let stored: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT generation_epoch, captured_guard_version, corpus_revision, \
                built_through_projection_ordinal, member_count \
           FROM embedding_corpus_generations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(a.workspace)
    .bind(a.generation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(i64::try_from(snapshot.generation_epoch).unwrap(), stored.0);
    assert_eq!(i64::try_from(snapshot.guard_version).unwrap(), stored.1 + 1);
    assert_ne!(i64::try_from(snapshot.guard_version).unwrap(), stored.1);
    assert_eq!(i64::try_from(snapshot.corpus_revision).unwrap(), stored.2);
    assert_eq!(
        i64::try_from(snapshot.built_through_projection_ordinal).unwrap(),
        stored.3
    );
    assert_eq!(i64::try_from(snapshot.member_count).unwrap(), stored.4);
    snapshot
        .validate()
        .expect("what the registry is keyed by must be a valid canonical snapshot");
    runtime.close().await;
}

// --- The whole path, once: a question asked against a pinned generation and an
// answer read back from the durable result.
//
// Everything before this proves a piece. This proves they are the same path.
// The one piece it does not rebuild is the local index, because an index is a
// cache and `embedding_index_builds` proves how one is built; what is under
// test here is what retrieval does with a generation, a query and an index that
// already exists.

const E2E_MODEL: &str = "text-embedding-nomic-embed-text-v1.5";
const E2E_OUTPUT_COUNT: usize = 2;

#[derive(Default)]
struct E2eAdapter {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl vestrace_application::run::GovernedModelAdapter for E2eAdapter {
    async fn execute(
        &self,
        _kind: vestrace_domain::ConnectionKind,
        _runtime_base_url: &str,
        _auth: vestrace_application::ConnectionAuth,
        request: vestrace_application::EffectiveModelRequest,
    ) -> Result<vestrace_application::EffectiveModelResponse, vestrace_application::ProviderError>
    {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(
            request,
            vestrace_application::EffectiveModelRequest::Embeddings(_)
        ));
        let data = (0..E2E_OUTPUT_COUNT)
            .map(|ordinal| {
                vestrace_application::GovernedEmbeddingVector::from_provider_components(
                    ordinal,
                    vec![1.0; 768],
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(vestrace_application::EffectiveModelResponse::Embeddings(
            vestrace_application::GovernedEmbeddingsResponse::new(
                E2E_MODEL,
                E2E_MODEL.into(),
                data,
                E2E_OUTPUT_COUNT,
            )?,
        ))
    }
}

/// One active memory whose current content becomes a governed source material.
///
/// This is the link the whole answer hangs on. A projection names a material,
/// a material's intent names what owns it, and only a `memory_revision` owner
/// lets a hit be reported as a memory at all -- so a fixture that seeded any
/// other owner would prove the search and nothing about the answer.
async fn e2e_memory_source(
    pool: &PgPool,
    runtime: &PgPool,
    accepted: &common::AcceptedJob,
) -> (Uuid, Uuid, Uuid) {
    let workspace = accepted.context.workspace_id;
    let memory = Uuid::now_v7();
    let revision = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO memories(id,workspace_id,kind,status,state_revision) \
         VALUES($1,$2,'fact','candidate',1)",
    )
    .bind(memory)
    .bind(workspace.as_uuid())
    .execute(pool)
    .await
    .expect("memory");
    sqlx::query(
        "INSERT INTO memory_revisions(id,memory_id,workspace_id,revision_number,content,\
         confidence,importance) VALUES($1,$2,$3,1,'the answer a query should find',1.0,0.5)",
    )
    .bind(revision)
    .bind(memory)
    .bind(workspace.as_uuid())
    .execute(pool)
    .await
    .expect("revision");
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision)
        .bind(memory)
        .execute(pool)
        .await
        .expect("active revision");

    let vault_fixture = common::result_preparation_fixture::OutputVaultFixture::new();
    let materializer = vestrace_infrastructure::postgres::PgGovernedContentMaterializer::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
        std::sync::Arc::new(vault_fixture.vault(workspace)),
        std::sync::Arc::new(vestrace_infrastructure::crypto::ContentMaterialCodec::new()),
    );
    let source = materializer
        .materialize_revision(&accepted.context, revision)
        .await
        .expect("materialization must not fail")
        .expect("an intact revision is not a blocker");
    (memory, revision, source.material_id)
}

/// Executes the fixture's delivery job with the memory-owned material among its
/// sources, so the projections it publishes depend on a memory revision.
async fn e2e_publish_projections(
    pool: &PgPool,
    runtime: &PgPool,
    accepted: &common::AcceptedJob,
    memory_source: Uuid,
    additional_memory_source: Option<Uuid>,
) {
    use common::result_preparation_fixture::{
        DeliveryPolicyCase, OutputVaultFixture, acceptance_command, attach_source_to_evidence,
        live_source, outputs, reconcile_output_receipts, record_delivery_policy,
    };
    use vestrace_application::EmbeddingOutputKeyRepository;

    let sources = [
        vestrace_domain::ContentMaterialId::from_uuid(memory_source),
        live_source(runtime, accepted).await,
    ];
    common::make_dispatchable_with_policy(pool, runtime, accepted, true).await;
    for (ordinal, source) in sources.into_iter().enumerate() {
        attach_source_to_evidence(pool, accepted, source, 8 + ordinal as i64).await;
    }
    if let Some(source) = additional_memory_source {
        attach_source_to_evidence(
            pool,
            accepted,
            vestrace_domain::ContentMaterialId::from_uuid(source),
            10,
        )
        .await;
    }
    let output_set = outputs();
    let receipt_id = Uuid::now_v7();
    vestrace_infrastructure::postgres::PgEmbeddingOutputKeyRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    )
    .accept_delivery_outputs(
        &accepted.context,
        acceptance_command(accepted, receipt_id, output_set.clone()),
    )
    .await
    .expect("the result chain must accept the delivery job");
    let vault = OutputVaultFixture::new();
    reconcile_output_receipts(runtime, accepted, &output_set, &vault).await;
    record_delivery_policy(runtime, receipt_id, DeliveryPolicyCase::ExactAllowed).await;

    let store = vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone());
    let dispatch = std::sync::Arc::new(common::dispatching_repository(runtime, None));
    let output_vault = std::sync::Arc::new(vault.vault(accepted.context.workspace_id));
    let preparation = std::sync::Arc::new(
        vestrace_application::EmbeddingResultPreparationService::new(
            std::sync::Arc::new(
                vestrace_infrastructure::postgres::PgEmbeddingResultRepository::new(
                    store.clone(),
                    dispatch.clone(),
                ),
            ),
            output_vault.clone(),
            std::sync::Arc::new(vestrace_infrastructure::crypto::ContentMaterialCodec::new()),
        ),
    );
    let finalization = std::sync::Arc::new(
        vestrace_application::EmbeddingResultFinalizationService::new(
            std::sync::Arc::new(
                vestrace_infrastructure::postgres::PgEmbeddingResultFinalizationRepository::new(
                    store,
                ),
            ),
            output_vault,
            std::sync::Arc::new(
                vestrace_infrastructure::postgres::EmbeddingOutputHmacCommitter::new(),
            ),
        ),
    );
    let adapter = std::sync::Arc::new(E2eAdapter::default());
    let outcome = vestrace_application::embedding::EmbeddingExecutor::new(
        dispatch,
        preparation,
        finalization,
        adapter.clone(),
        vestrace_domain::WorkerId::new(),
    )
    .execute(&accepted.context, accepted.job_id)
    .await
    .expect("the delivery job must finalize");
    assert_eq!(
        outcome,
        vestrace_application::embedding::EmbeddingExecutionOutcome::Succeeded
    );
    assert_eq!(adapter.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// A published projection resolves to the memory revision it was computed from.
///
/// This is the link every retrieval answer hangs on and the one that fails
/// silently. A projection names the material it came from, that material's
/// intent names what owns it, and only a `memory_revision` owner lets a hit be
/// reported as a memory at all. Get the join wrong and retrieval returns an
/// empty answer rather than a wrong one -- indistinguishable, from outside,
/// from a corpus that genuinely matched nothing.
///
/// The projections here are real: a delivery job executed through the whole
/// result chain, with one of its sources materialized from a memory revision by
/// the same materializer the on-write route uses.
#[sqlx::test(migrations = false)]
async fn a_published_projection_resolves_to_its_memory_revision(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    let mut corpus = common::canonical_memory_fixture::new(&pool, "memory-resolution").await;
    let runtime = corpus.runtime.clone();
    let workspace = corpus.accepted.context.workspace_id;

    let (memory, revision, memory_source) =
        e2e_memory_source(&pool, &runtime, &corpus.accepted).await;
    e2e_publish_projections(&pool, &runtime, &corpus.accepted, memory_source, None).await;

    let generation = corpus.capture().await.as_uuid();

    // Two projections, and only one of them was computed from a memory.
    let all: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM embedding_projection_entries WHERE workspace_id=$1 ORDER BY id",
    )
    .bind(workspace.as_uuid())
    .fetch_all(&pool)
    .await
    .expect("the executed job published projections");
    assert_eq!(all.len(), E2E_OUTPUT_COUNT);

    let of_memory: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT dependency.projection_id \
           FROM embedding_projection_source_dependencies AS dependency \
          WHERE dependency.workspace_id=$1 AND dependency.source_material_id=$2",
    )
    .bind(workspace.as_uuid())
    .bind(memory_source)
    .fetch_all(&pool)
    .await
    .expect("the dependency the delivery recorded");
    assert!(
        !of_memory.is_empty(),
        "the delivery must have recorded the memory-owned source as a dependency"
    );

    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );
    let resolved = repository
        .resolve_members(&corpus.accepted.context, generation, &all)
        .await
        .expect("resolution must not fail");

    assert_eq!(
        resolved.len(),
        of_memory.len(),
        "exactly the projections computed from a memory resolve; a projection whose source is \
         some other governed input has no memory to name"
    );
    for member in &resolved {
        assert!(
            of_memory.contains(&member.projection_id),
            "only a projection that depends on the memory-owned source may resolve"
        );
        assert_eq!(member.memory_id, memory, "the memory the projection means");
        assert_eq!(member.revision_id, revision, "and its exact revision");
    }

    // An unknown projection resolves to nothing rather than to something.
    let absent = repository
        .resolve_members(&corpus.accepted.context, generation, &[Uuid::now_v7()])
        .await
        .expect("an unknown projection is not a failure");
    assert!(absent.is_empty());

    // That a projection stops resolving once its source leaves Live is not
    // asserted here. Taking a material out of Live is the two-phase erasure's
    // job and the schema refuses any shortcut to it -- `Live material requires
    // its exact Bound promotion` -- so proving it needs the vault sequence,
    // which `embedding_erasure_propagation` already drives. What that suite
    // does not do is read the answer afterwards.
    runtime.close().await;
}

/// The read model reports what the durable record holds, and nothing it does
/// not.
///
/// Driven through the real guarded functions rather than by inserting rows,
/// because the view's job is to state the outcome of those functions. Rows
/// written by hand would prove that the SELECT reads its own columns and
/// nothing about whether those columns mean what the view says they mean.
#[sqlx::test(migrations = false)]
async fn the_attempt_view_reports_what_the_durable_record_holds(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let request = Uuid::now_v7();
    let fence = accept_fence(&runtime, &a, request).await.unwrap();

    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(a.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );
    let job_id = vestrace_domain::EmbeddingJobId::from_uuid(a.job);

    // An accepted attempt that has not finished: pinned, and undecided.
    let view = repository
        .attempt_view(&context, job_id)
        .await
        .expect("an accepted attempt is readable")
        .expect("an accepted attempt exists");
    assert_eq!(view.job_id.as_uuid(), a.job);
    assert_eq!(view.request_id.as_uuid(), request);
    assert_eq!(view.space_registration_id, a.space);
    assert_eq!(view.generation_id, a.generation);
    assert_eq!(
        view.reference_count, None,
        "no terminal result is absent, not zero"
    );
    assert_eq!(view.degradation_reason, None);
    assert_eq!(view.generation_changed_reason, None);
    assert!(!view.retry_available, "nothing has been declined yet");
    assert_eq!(view.predecessor_job_id, None);
    assert_eq!(view.successor_job_id, None);

    // What it reports about the generation is what the fence pinned, not what
    // the generation happens to be now.
    let (pinned_epoch, pinned_members): (i64, i64) = sqlx::query_as(
        "SELECT generation_epoch, member_count FROM embedding_retrieval_fences \
          WHERE workspace_id=$1 AND id=$2",
    )
    .bind(a.workspace)
    .bind(fence)
    .fetch_one(&pool)
    .await
    .expect("the fence this attempt took");
    assert_eq!(view.generation_epoch, pinned_epoch as u64);
    assert_eq!(view.generation_member_count, pinned_members as u64);

    // A job that exists but was never admitted as an attempt has no view. The
    // fence is what makes a job an attempt, so a job with none has not declined
    // and has not answered -- it has not started, and a zeroed view would say
    // otherwise.
    let base = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let unfenced = accept_retrieval_job(&pool, &runtime, &base).await;
    let unfenced_context = vestrace_application::RequestContext::new(
        base.context.workspace_id,
        vestrace_domain::PrincipalId::new(),
    );
    assert!(
        repository
            .attempt_view(
                &unfenced_context,
                vestrace_domain::EmbeddingJobId::from_uuid(unfenced)
            )
            .await
            .expect("a job with no fence is not a failure")
            .is_none(),
        "a job that took no fence must not read back as a retrieval that found nothing"
    );

    // And a job in another workspace is absent rather than readable. The view
    // is scoped by the request context, not by the identity the caller states.
    let elsewhere = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::new(),
        vestrace_domain::PrincipalId::new(),
    );
    assert!(
        repository
            .attempt_view(&elsewhere, job_id)
            .await
            .expect("a foreign attempt is not a failure")
            .is_none(),
        "an attempt must not be readable from another workspace"
    );

    runtime.close().await;
}

/// A terminal result reads back as its count, and a declined attempt reads back
/// as its closed reason and the successor it earned.
#[sqlx::test(migrations = false)]
async fn a_terminal_attempt_reads_back_as_its_outcome_and_its_lineage(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );

    // One attempt that answered.
    let answered = attempt(&pool, &runtime).await;
    let answered_fence = accept_fence(&runtime, &answered, Uuid::now_v7())
        .await
        .unwrap();
    // The reference table names identities and does not resolve them; what a
    // reference means is `resolve_members`' job, proven in its own test.
    let (memory, revision) = (answered.references[0].2, answered.references[0].3);
    finalize(
        &runtime,
        &answered,
        answered_fence,
        &[(memory, revision, 0, 0.75)],
    )
    .await
    .expect("a result lands on a current generation");

    let context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(answered.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let view = repository
        .attempt_view(
            &context,
            vestrace_domain::EmbeddingJobId::from_uuid(answered.job),
        )
        .await
        .unwrap()
        .expect("an answered attempt exists");
    assert_eq!(
        view.reference_count,
        Some(1),
        "the count of what answered, not the references themselves"
    );
    assert_eq!(
        view.degradation_reason, None,
        "an attempt that answered did not degrade"
    );
    assert!(!view.retry_available);

    // And one that was declined, then retried.
    let declined = attempt(&pool, &runtime).await;
    let declined_fence = accept_fence(&runtime, &declined, Uuid::now_v7())
        .await
        .unwrap();
    observe_change(&runtime, &declined, declined_fence, "corpus_changed")
        .await
        .expect("a generation change is terminal");
    let declined_context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(declined.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    let declined_id = vestrace_domain::EmbeddingJobId::from_uuid(declined.job);

    let view = repository
        .attempt_view(&declined_context, declined_id)
        .await
        .unwrap()
        .expect("a declined attempt exists");
    assert_eq!(
        view.degradation_reason,
        Some("retrieval_generation_changed")
    );
    assert_eq!(
        view.generation_changed_reason,
        Some(vestrace_domain::embedding::RetrievalGenerationChangedReason::CorpusChanged)
    );
    assert!(
        view.retry_available,
        "a confirmed change with no successor is the one case a retry may follow"
    );
    assert_eq!(view.reference_count, None);

    let successor = Uuid::now_v7();
    authorize_retry(
        &runtime,
        &declined,
        successor,
        Uuid::now_v7(),
        "attempt-view-key",
    )
    .await
    .expect("a confirmed change authorizes one successor");

    let view = repository
        .attempt_view(&declined_context, declined_id)
        .await
        .unwrap()
        .expect("the predecessor is still readable");
    assert_eq!(
        view.successor_job_id.map(|id| id.as_uuid()),
        Some(successor),
        "the predecessor names the one successor it earned"
    );
    assert!(
        !view.retry_available,
        "a predecessor has exactly one successor; once spent, the retry is gone"
    );

    runtime.close().await;
}

/// The retry queue holds exactly the attempts a confirmed change left
/// retryable, and drops each one the moment its successor is authorized.
///
/// This is the route by which an HTTP caller learns a predecessor job id at
/// all, so what it must never do is name an attempt that the single read then
/// says is not retryable. Both are driven here against the same rows.
#[sqlx::test(migrations = false)]
async fn the_retry_queue_holds_only_unspent_confirmed_changes(pool: PgPool) {
    use vestrace_application::embedding::EmbeddingRetrievalRepository;

    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let repository = vestrace_infrastructure::postgres::PgEmbeddingRetrievalRepository::new(
        vestrace_infrastructure::postgres::PgStore::from_pool(runtime.clone()),
    );

    // One workspace holding three attempts in three different conditions.
    let base = common::prepare_delivery_embedding_job(&pool, &runtime).await;
    let workspace = base.context.workspace_id;
    let context =
        vestrace_application::RequestContext::new(workspace, vestrace_domain::PrincipalId::new());

    // Nothing has declined yet.
    let empty = repository
        .attempts_awaiting_retry(&context)
        .await
        .expect("an empty queue is an answer");
    assert!(empty.attempts.is_empty());
    assert!(!empty.truncated);

    let declined = attempt(&pool, &runtime).await;
    let declined_fence = accept_fence(&runtime, &declined, Uuid::now_v7())
        .await
        .unwrap();
    observe_change(&runtime, &declined, declined_fence, "revoked")
        .await
        .expect("a generation change is terminal");
    let declined_context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(declined.workspace),
        vestrace_domain::PrincipalId::new(),
    );

    // An attempt that answered is not in the queue: it has nothing to decide.
    let answered = attempt(&pool, &runtime).await;
    let answered_fence = accept_fence(&runtime, &answered, Uuid::now_v7())
        .await
        .unwrap();
    finalize(
        &runtime,
        &answered,
        answered_fence,
        &[(answered.references[0].2, answered.references[0].3, 0, 0.5)],
    )
    .await
    .expect("a result lands on a current generation");

    // Each `attempt` builds its own workspace, so the queue is read per
    // workspace -- which is itself the scoping claim worth making.
    let queue = repository
        .attempts_awaiting_retry(&declined_context)
        .await
        .expect("the declined workspace has a queue");
    assert_eq!(
        queue.attempts.len(),
        1,
        "exactly the one confirmed change is waiting"
    );
    assert!(!queue.truncated);
    let waiting = &queue.attempts[0];
    assert_eq!(waiting.job_id.as_uuid(), declined.job);
    assert!(waiting.retry_available);
    assert_eq!(
        waiting.generation_changed_reason,
        Some(vestrace_domain::embedding::RetrievalGenerationChangedReason::Revoked)
    );

    // The queue and the single read describe the same attempt identically.
    let single = repository
        .attempt_view(&declined_context, waiting.job_id)
        .await
        .unwrap()
        .expect("the queued attempt is readable on its own");
    assert_eq!(
        &single, waiting,
        "the queue must not describe an attempt differently from the read an \
         operator confirms against"
    );

    // The answered workspace's queue is empty.
    let answered_context = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(answered.workspace),
        vestrace_domain::PrincipalId::new(),
    );
    assert!(
        repository
            .attempts_awaiting_retry(&answered_context)
            .await
            .unwrap()
            .attempts
            .is_empty(),
        "an attempt that answered has no decision waiting on it"
    );

    // Authorizing the successor empties the queue for all time: a predecessor
    // has exactly one successor, so the edge that appears is permanent.
    authorize_retry(
        &runtime,
        &declined,
        Uuid::now_v7(),
        Uuid::now_v7(),
        "queue-key",
    )
    .await
    .expect("a confirmed change authorizes one successor");
    assert!(
        repository
            .attempts_awaiting_retry(&declined_context)
            .await
            .unwrap()
            .attempts
            .is_empty(),
        "a spent retry leaves the queue"
    );

    runtime.close().await;
}

/// The signature whose primary predicate this qualification mutates.
const RETRY_AUTHORITY: &str =
    "public.vestrace_authorize_embedding_retrieval_retry(uuid,uuid,uuid,uuid,text)";

/// The one-successor lookup, and the smallest edit that disables it.
///
/// `AND FALSE` rather than a deleted block: the mutation has to be a *predicate*
/// change, so that what is being qualified is the rule and not the shape of the
/// function around it. It also restores by exact string, which is what makes
/// the byte-equality check afterwards meaningful.
const ONE_SUCCESSOR_NEEDLE: &str = "WHERE workspace_id=target_workspace AND predecessor_job_id=target_predecessor\n     FOR UPDATE";
const ONE_SUCCESSOR_MUTATION: &str = "WHERE workspace_id=target_workspace AND predecessor_job_id=target_predecessor AND FALSE\n     FOR UPDATE";

/// What the catalogue holds for a function: its definition and its posture.
///
/// Both, because a mutation that restored the text and lost the owner would
/// leave a SECURITY DEFINER function running as somebody else -- which is the
/// defect this package already found once, in the provisioner's hand-back.
async fn authority_state(pool: &PgPool, signature: &str) -> (String, String, Option<String>, bool) {
    sqlx::query_as(
        "SELECT pg_get_functiondef(oid), pg_get_userbyid(proowner), \
                array_to_string(proacl,'|'), \
                has_function_privilege('vestrace',oid,'EXECUTE') \
           FROM pg_proc WHERE oid=$1::regprocedure",
    )
    .bind(signature)
    .fetch_one(pool)
    .await
    .expect("the authority is in the catalogue")
}

/// Install one definition, leaving everything about the function except its
/// body exactly as it was.
///
/// Run as the connected superuser rather than as the guarded owner: the public
/// schema belongs to the runtime role and the guarded owner holds no CREATE on
/// it, so the owner cannot replace its own function. `CREATE OR REPLACE` keeps
/// the existing owner and ACL, and the caller checks that -- because a
/// SECURITY DEFINER function that changed hands would run the mutated body as
/// somebody else, and the observation would then be about the role rather than
/// about the predicate.
async fn install_authority(pool: &PgPool, definition: &str) {
    sqlx::raw_sql(definition)
        .execute(pool)
        .await
        .expect("the authority definition is installable");
}

/// Drive the one-successor rule and say how it was refused, or that it was not.
///
/// Returns the SQLSTATE and message of the refusal a second, differently-keyed
/// authorization earns. The point of naming both is that a mutation may move
/// the refusal to another defence rather than removing it, and those two
/// outcomes are not the same result.
async fn second_key_refusal(pool: &PgPool, runtime: &PgPool) -> Result<Uuid, (String, String)> {
    let a = attempt(pool, runtime).await;
    let fence = accept_fence(runtime, &a, Uuid::now_v7()).await.unwrap();
    observe_change(runtime, &a, fence, "revoked").await.unwrap();
    authorize_retry(runtime, &a, Uuid::now_v7(), Uuid::now_v7(), "key-1")
        .await
        .expect("a confirmed change authorizes its first successor");
    match authorize_retry(runtime, &a, Uuid::now_v7(), Uuid::now_v7(), "key-2").await {
        Ok(id) => Ok(id),
        Err(error) => {
            let database = error.as_database_error().expect("a database refusal");
            Err((
                database
                    .code()
                    .map(|code| code.into_owned())
                    .unwrap_or_default(),
                database.message().to_owned(),
            ))
        }
    }
}

/// Mutation qualification: one predicate, one run, restored byte-exactly.
///
/// The rule under test is that a predecessor has one successor for all time.
/// Two independent defences enforce it -- this function's own lookup for an
/// existing edge, and the table's primary key. Disabling the lookup is what
/// tells them apart: if the guarded refusal simply disappeared, the rule would
/// rest on a message; if some other defence catches it, the rule is layered and
/// the qualification says at which boundary.
///
/// The restore is checked on the definition *and* the posture, and the rule is
/// re-driven afterwards, because a qualification that left the authority
/// subtly different would make every later test in this suite a test of the
/// mutation.
#[sqlx::test(migrations = false)]
async fn mutating_the_one_successor_lookup_moves_the_refusal_and_restores_exactly(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let (original, owner, acl, runtime_execute) = authority_state(&pool, RETRY_AUTHORITY).await;
    assert!(
        original.contains(ONE_SUCCESSOR_NEEDLE),
        "the predicate this qualification mutates is no longer in the authority; \
         the mutation would silently test nothing: {original}"
    );

    // Green before: the guarded refusal, by its own message.
    let before = second_key_refusal(&pool, &runtime).await;
    let (before_state, before_message) = before.expect_err("a second key must be refused");
    assert_eq!(before_state, "23505");
    assert!(
        before_message.contains("predecessor already has its successor"),
        "{before_message}"
    );

    // Mutate exactly one predicate.
    install_authority(
        &pool,
        &original.replace(ONE_SUCCESSOR_NEEDLE, ONE_SUCCESSOR_MUTATION),
    )
    .await;
    let (mutated, mutated_owner, mutated_acl, mutated_execute) =
        authority_state(&pool, RETRY_AUTHORITY).await;
    assert_ne!(mutated, original, "the mutation must actually be installed");
    // Only the body may differ. A mutation that also moved the owner would be
    // two mutations, and the one that mattered would be the one nobody chose.
    assert_eq!(
        mutated_owner, owner,
        "the mutation must not change the owner"
    );
    assert_eq!(mutated_acl, acl, "nor the access control list");
    assert_eq!(
        mutated_execute, runtime_execute,
        "nor whether the runtime role may call it"
    );

    // Red: the rule still holds, and the qualification records where.
    let (mutated_state, mutated_message) = second_key_refusal(&pool, &runtime)
        .await
        .expect_err("a second successor must not become reachable");
    assert_eq!(
        mutated_state, "23505",
        "the table's primary key is the independent defence, and it is still a \
         unique violation"
    );
    assert!(
        !mutated_message.contains("predecessor already has its successor"),
        "with the lookup disabled the guarded message cannot be what refused; \
         an unchanged message would mean the mutation never took: {mutated_message}"
    );
    assert!(
        mutated_message.contains("embedding_retrieval_retry_edges"),
        "the refusal now names the constraint rather than the rule, which is the \
         different boundary this run exists to record: {mutated_message}"
    );

    // Restore, byte-exactly, and prove it.
    install_authority(&pool, &original).await;
    let (restored, restored_owner, restored_acl, restored_execute) =
        authority_state(&pool, RETRY_AUTHORITY).await;
    assert_eq!(
        restored, original,
        "the definition must be restored exactly"
    );
    assert_eq!(restored_owner, owner, "and so must its owner");
    assert_eq!(restored_acl, acl, "and its access control list");
    assert_eq!(
        restored_execute, runtime_execute,
        "and the runtime role's ability to call it"
    );

    // Green after: the guarded refusal is back, by its own message.
    let (after_state, after_message) = second_key_refusal(&pool, &runtime)
        .await
        .expect_err("a second key must be refused again");
    assert_eq!(after_state, before_state);
    assert_eq!(after_message, before_message);

    runtime.close().await;
}

const FINALIZE_AUTHORITY: &str = "public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],uuid[],uuid[],bigint[],double precision[])";

/// The pinned-generation revalidation, and the edit that disables all of it.
///
/// All six clauses, not one: they raise a single refusal between them, so the
/// primary predicate is the revalidation itself. Disabling one clause would
/// qualify the redundancy inside it; disabling the predicate asks the question
/// the qualification exists for -- whether anything else stands between a moved
/// corpus and a stored answer.
const FENCE_NEEDLE: &str = "    IF guard_row.current_generation_id IS DISTINCT FROM fence_row.generation_id\n       OR guard_row.guard_version <> fence_row.guard_version\n       OR generation_row.state <> 'ready'\n       OR generation_row.generation_epoch <> fence_row.generation_epoch\n       OR generation_row.corpus_revision <> fence_row.corpus_revision\n       OR generation_row.member_count <> fence_row.member_count THEN";
const FENCE_MUTATION: &str = "    IF FALSE THEN";

/// Move the corpus under an accepted attempt, then try to land its answer.
///
/// Returns the refusal, or the result identity if one was stored. A stored
/// result here is the unsafe state itself: an answer attributed to a generation
/// that no longer exists, which is exactly what the fence is for.
async fn land_on_a_moved_generation(
    pool: &PgPool,
    runtime: &PgPool,
) -> (Attempt, Result<Uuid, (String, String)>) {
    let a = attempt(pool, runtime).await;
    let fence = accept_fence(runtime, &a, Uuid::now_v7()).await.unwrap();
    dispatch_query(&a).await;
    advance_generation(pool, runtime, &a).await;
    let landed = finalize(
        runtime,
        &a,
        fence,
        &[(a.references[0].2, a.references[0].3, 0, 0.5)],
    )
    .await;
    let outcome = match landed {
        Ok(id) => Ok(id),
        Err(error) => {
            let database = error.as_database_error().expect("a database refusal");
            Err((
                database
                    .code()
                    .map(|code| code.into_owned())
                    .unwrap_or_default(),
                database.message().to_owned(),
            ))
        }
    };
    (a, outcome)
}

/// Removing the first generation fence remains contained by the additional snapshot guard.
/// This records the independent refusal boundary and restores the original authority.
#[sqlx::test(migrations = false)]
async fn mutating_the_pinned_generation_fence_is_contained_by_additional_snapshot_guard(
    pool: PgPool,
) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let (original, owner, acl, runtime_execute) = authority_state(&pool, FINALIZE_AUTHORITY).await;
    assert!(
        original.contains(FENCE_NEEDLE),
        "the predicate this qualification mutates is no longer in the authority; \
         the mutation would silently test nothing: {original}"
    );

    // Green before: the fence refuses, by its own message, and stores nothing.
    let (before_attempt, before) = land_on_a_moved_generation(&pool, &runtime).await;
    let (before_state, before_message) =
        before.expect_err("a moved generation must refuse its answer");
    assert_eq!(before_state, "23514");
    assert!(
        before_message.contains("requires its exact pinned generation"),
        "{before_message}"
    );
    assert_eq!(
        counts(&pool, &before_attempt).await,
        (0, 0),
        "a refused answer leaves neither a result nor a change"
    );

    install_authority(&pool, &original.replace(FENCE_NEEDLE, FENCE_MUTATION)).await;
    let (mutated, mutated_owner, mutated_acl, mutated_execute) =
        authority_state(&pool, FINALIZE_AUTHORITY).await;
    assert_ne!(mutated, original, "the mutation must actually be installed");
    assert_eq!(
        mutated_owner, owner,
        "the mutation must not change the owner"
    );
    assert_eq!(mutated_acl, acl, "nor the access control list");
    assert_eq!(mutated_execute, runtime_execute, "nor its reachability");

    // The additional corpus/epoch/watermark guard still refuses the replacement.
    let (mutated_attempt, landed) = land_on_a_moved_generation(&pool, &runtime).await;

    // Restore, byte-exactly, and prove it.
    install_authority(&pool, &original).await;
    let (restored, restored_owner, restored_acl, restored_execute) =
        authority_state(&pool, FINALIZE_AUTHORITY).await;
    assert_eq!(
        restored, original,
        "the definition must be restored exactly"
    );
    assert_eq!(restored_owner, owner);
    assert_eq!(restored_acl, acl);
    assert_eq!(restored_execute, runtime_execute);

    let (state, message) = landed
        .expect_err("the additional snapshot guard independently refuses the replaced generation");
    assert_eq!(state, "23514");
    assert_eq!(
        message,
        "embedding retrieval result requires its exact pinned generation"
    );
    assert_eq!(counts(&pool, &mutated_attempt).await, (0, 0));

    // Green after: the same refusal, and nothing stored.
    let (after_attempt, after) = land_on_a_moved_generation(&pool, &runtime).await;
    let (after_state, after_message) = after.expect_err("the fence must refuse again");
    assert_eq!(after_state, before_state);
    assert_eq!(after_message, before_message);
    assert_eq!(counts(&pool, &after_attempt).await, (0, 0));

    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn unchecked_result_overloads_are_retired(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    for signature in [
        "vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)",
        "vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])",
    ] {
        let (definition, owner, _, executable) = authority_state(&pool, signature).await;
        assert_eq!(owner, "vestrace_guarded_owner");
        assert!(
            !executable,
            "unchecked overload remains executable: {signature}"
        );
        assert!(
            definition.contains("retired"),
            "unchecked body must refuse even its owner"
        );
    }
    let version: i32 = sqlx::query_scalar("SELECT column_default::integer FROM information_schema.columns WHERE table_name='embedding_retrieval_results' AND column_name='provenance_version'")
        .fetch_one(&pool).await.expect("old empty results have explicit historical provenance");
    assert_eq!(version, 0);
}

#[sqlx::test(migrations = false)]
async fn canonical_reference_entrypoints_require_workspace_context(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    // Owner bypasses runtime ACLs, so each SECURITY DEFINER body must reject
    // missing context itself even when every caller-supplied UUID is absent.
    for statement in [
        "SELECT * FROM vestrace_resolve_embedding_memory_references($1,$2,ARRAY[]::uuid[])",
        "SELECT vestrace_accept_embedding_retrieval_attempt($1,$2,$2,$2,$2,NOW())",
        "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$2,ARRAY[]::uuid[],ARRAY[]::uuid[],ARRAY[]::uuid[],ARRAY[]::uuid[],ARRAY[]::bigint[],ARRAY[]::float8[])",
    ] {
        let mut transaction = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT set_config('vestrace.workspace_id','',true)")
            .execute(&mut *transaction)
            .await
            .unwrap();
        let error = sqlx::query(statement)
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .execute(&mut *transaction)
            .await
            .expect_err("context is mandatory independently of RLS");
        assert_refusal(error, "42501", "workspace context is required");
        transaction.rollback().await.unwrap();
    }
}

#[sqlx::test(migrations = false)]
async fn distinct_projections_of_one_revision_cannot_form_two_result_references(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();
    dispatch_query(&a).await;
    let mut tx = runtime.begin().await.unwrap();
    scoped(&mut tx, a.workspace).await;
    let members:Vec<(Uuid,Uuid,Uuid,Uuid)>=sqlx::query_as("SELECT projection_id,source_material_id,memory_id,revision_id FROM vestrace_resolve_embedding_memory_references($1,$2,NULL) WHERE revision_id=$3 ORDER BY projection_id")
        .bind(a.workspace).bind(a.generation).bind(a.references[0].3).fetch_all(&mut *tx).await.unwrap();
    assert_eq!(
        members.len(),
        2,
        "the governed delivery really published two projections of one revision"
    );
    assert_ne!(members[0].0, members[1].0);
    let result = sqlx::query_scalar(
        "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(fence)
    .bind(members.iter().map(|m| m.0).collect::<Vec<_>>())
    .bind(members.iter().map(|m| m.1).collect::<Vec<_>>())
    .bind(members.iter().map(|m| m.2).collect::<Vec<_>>())
    .bind(members.iter().map(|m| m.3).collect::<Vec<_>>())
    .bind(vec![0_i64, 1])
    .bind(vec![0.75_f64, 0.5])
    .fetch_one(&mut *tx)
    .await;
    assert_refusal(
        finish(tx, result).await.unwrap_err(),
        "23514",
        "embedding retrieval result requires unique canonical memory references",
    );
    assert_eq!(counts(&pool, &a).await, (0, 0));
    assert_eq!(job_state(&runtime, &a).await, "running");
}

async fn finalize_wrong_memory(
    runtime: &PgPool,
    a: &Attempt,
    fence: Uuid,
) -> Result<Uuid, sqlx::Error> {
    dispatch_query(a).await;
    let reference = a.references[0];
    let mut tx = runtime.begin().await.unwrap();
    scoped(&mut tx, a.workspace).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(a.workspace)
    .bind(a.job)
    .bind(fence)
    .bind(vec![reference.0])
    .bind(vec![reference.1])
    .bind(vec![Uuid::now_v7()])
    .bind(vec![reference.3])
    .bind(vec![0_i64])
    .bind(vec![0.75_f64])
    .fetch_one(&mut *tx)
    .await;
    finish(tx, result).await
}

#[sqlx::test(migrations = false)]
async fn mutating_memory_provenance_makes_a_wrong_memory_reference_persist(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let needle = "target_memory_ids[position] IS DISTINCT FROM resolved.memory_id";
    let before = authority_state(&pool, FINALIZE_AUTHORITY).await;
    assert_eq!(before.0.matches(needle).count(), 1);
    let baseline = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &baseline, Uuid::now_v7())
        .await
        .unwrap();
    assert_refusal(
        finalize_wrong_memory(&runtime, &baseline, fence)
            .await
            .unwrap_err(),
        "23514",
        "embedding retrieval result requires exact canonical memory provenance",
    );
    assert_eq!(counts(&pool, &baseline).await, (0, 0));
    install_authority(&pool, &before.0.replace(needle, "FALSE")).await;
    let mutant = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &mutant, Uuid::now_v7())
        .await
        .unwrap();
    let result = finalize_wrong_memory(&runtime, &mutant, fence).await;
    // Restore before inspecting the mutation outcome, including its owner and grants.
    install_authority(&pool, &before.0).await;
    assert_eq!(authority_state(&pool, FINALIZE_AUTHORITY).await, before);
    result.expect("without exact memory ownership validation a forged memory identity persists");
    assert_eq!(counts(&pool, &mutant).await, (1, 0));
    let stored:Uuid=sqlx::query_scalar("SELECT reference.memory_id FROM embedding_retrieval_result_references reference JOIN embedding_retrieval_results result ON result.workspace_id=reference.workspace_id AND result.id=reference.result_id WHERE result.workspace_id=$1 AND result.job_id=$2")
        .bind(mutant.workspace).bind(mutant.job).fetch_one(&pool).await.unwrap();
    assert_ne!(stored, mutant.references[0].2);
    let restored = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &restored, Uuid::now_v7())
        .await
        .unwrap();
    assert_refusal(
        finalize_wrong_memory(&runtime, &restored, fence)
            .await
            .unwrap_err(),
        "23514",
        "embedding retrieval result requires exact canonical memory provenance",
    );
    assert_eq!(counts(&pool, &restored).await, (0, 0));
}

#[sqlx::test(migrations = false)]
async fn ambiguous_revision_owners_remain_ambiguous_after_owner_deletion(pool: PgPool) {
    let mut corpus =
        common::canonical_memory_fixture::new(&pool, "ambiguous-revision-owners").await;
    let runtime = corpus.runtime.clone();
    let workspace = corpus.accepted.context.workspace_id.as_uuid();
    let (_, _, first_source) = e2e_memory_source(&pool, &runtime, &corpus.accepted).await;
    let (second_memory, second_revision, second_source) =
        e2e_memory_source(&pool, &runtime, &corpus.accepted).await;
    // Delivery acceptance captures every evidenced source for every output.
    // Thus both revisions are authentic owners of these aggregate projections.
    e2e_publish_projections(
        &pool,
        &runtime,
        &corpus.accepted,
        first_source,
        Some(second_source),
    )
    .await;
    let generation = corpus.capture().await.as_uuid();
    for stage in ["both_present", "one_soft_deleted", "one_physically_deleted"] {
        let mut tx = runtime.begin().await.unwrap();
        scoped(&mut tx, workspace).await;
        let error =
            sqlx::query("SELECT * FROM vestrace_resolve_embedding_memory_references($1,$2,NULL)")
                .bind(workspace)
                .bind(generation)
                .fetch_all(&mut *tx)
                .await
                .expect_err(
                    "owner eligibility must never choose the surviving owner of an aggregate",
                );
        assert_refusal(error, "23514", "owner is ambiguous");
        tx.rollback().await.unwrap();
        match stage {
            "both_present" => {
                sqlx::query("UPDATE memories SET status='deleted' WHERE workspace_id=$1 AND id=$2")
                    .bind(workspace)
                    .bind(second_memory)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "one_soft_deleted" => {
                let mut deletion = pool.begin().await.unwrap();
                scoped(&mut deletion, workspace).await;
                sqlx::query("DELETE FROM memory_revisions WHERE workspace_id=$1 AND id=$2")
                    .bind(workspace)
                    .bind(second_revision)
                    .execute(&mut *deletion)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM memories WHERE workspace_id=$1 AND id=$2")
                    .bind(workspace)
                    .bind(second_memory)
                    .execute(&mut *deletion)
                    .await
                    .unwrap();
                deletion.commit().await.unwrap();
            }
            _ => {}
        }
    }
    let still_live: i64 = sqlx::query_scalar("SELECT count(*) FROM content_materials WHERE workspace_id=$1 AND id=ANY($2) AND state='live'")
        .bind(workspace).bind(vec![first_source,second_source]).fetch_one(&pool).await.unwrap();
    assert_eq!(
        still_live, 2,
        "this probes owner deletion, not material erasure"
    );
    runtime.close().await;
}

const REFERENCE_ARGUMENTS: &str =
    "$4::uuid[],$5::uuid[],$6::uuid[],$7::uuid[],$8::bigint[],$9::double precision[]";

async fn finalize_arguments_in(
    tx: &mut Transaction<'_, Postgres>,
    a: &Attempt,
    fence: Uuid,
    arguments: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$3,{arguments})"
    ))
    .bind(a.workspace)
    .bind(a.job)
    .bind(fence)
    .bind(a.references.iter().map(|r| r.0).collect::<Vec<_>>())
    .bind(a.references.iter().map(|r| r.1).collect::<Vec<_>>())
    .bind(a.references.iter().map(|r| r.2).collect::<Vec<_>>())
    .bind(a.references.iter().map(|r| r.3).collect::<Vec<_>>())
    .bind(vec![0_i64, 2])
    .bind(vec![0.75_f64, 0.25])
    .fetch_one(&mut **tx)
    .await
}

async fn terminal_branch_in(
    tx: &mut Transaction<'_, Postgres>,
    a: &Attempt,
    fence: Uuid,
    result: bool,
) -> Result<Uuid, sqlx::Error> {
    if result {
        finalize_arguments_in(tx, a, fence, REFERENCE_ARGUMENTS).await
    } else {
        sqlx::query_scalar(
            "SELECT vestrace_observe_embedding_retrieval_generation_change($1,$2,$3,'replaced')",
        )
        .bind(a.workspace)
        .bind(a.job)
        .bind(fence)
        .fetch_one(&mut **tx)
        .await
    }
}

#[sqlx::test(migrations = false)]
async fn terminal_branches_exclude_each_other_in_both_blocked_commit_orders(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    for result_first in [true, false] {
        let a = attempt(&pool, &runtime).await;
        let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();
        dispatch_query(&a).await;
        let mut winner = runtime.begin().await.unwrap();
        scoped(&mut winner, a.workspace).await;
        let winner_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *winner)
            .await
            .unwrap();
        let mut loser = runtime.begin().await.unwrap();
        scoped(&mut loser, a.workspace).await;
        let loser_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *loser)
            .await
            .unwrap();
        assert_ne!(winner_pid, loser_pid);
        terminal_branch_in(&mut winner, &a, fence, result_first)
            .await
            .unwrap();
        let waiting = async {
            let outcome = terminal_branch_in(&mut loser, &a, fence, !result_first).await;
            finish(loser, outcome).await
        };
        tokio::pin!(waiting);
        let blocked=tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                tokio::select! {
                    premature=&mut waiting=>panic!("competing terminal transaction did not wait for the winner: {premature:?}"),
                    _=tokio::time::sleep(std::time::Duration::from_millis(10))=> {
                        let blockers:Vec<i32>=sqlx::query_scalar("SELECT pg_blocking_pids($1)").bind(loser_pid).fetch_one(&pool).await.unwrap();
                        if blockers.contains(&winner_pid) { break; }
                    }
                }
            }
        }).await;
        // Release the winner even if the blocking assertion times out.
        winner.commit().await.unwrap();
        blocked.expect(
            "the second connection must demonstrably block on the first terminal transaction",
        );
        let failure = tokio::time::timeout(std::time::Duration::from_secs(5), &mut waiting)
            .await
            .expect("the losing terminal transaction must finish without a deadlock")
            .unwrap_err();
        assert_eq!(
            failure.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
        if result_first {
            assert!(
                failure
                    .to_string()
                    .contains("already produced a terminal result")
            );
            assert_eq!(counts(&pool, &a).await, (1, 0));
            assert_eq!(job_state(&runtime, &a).await, "succeeded");
        } else {
            assert!(
                failure.to_string().contains("exact unfinished job"),
                "{failure}"
            );
            assert_eq!(counts(&pool, &a).await, (0, 1));
            assert_eq!(job_state(&runtime, &a).await, "failed_definite");
        }
    }
}

#[sqlx::test(migrations = false)]
async fn malformed_references_roll_back_before_a_valid_finish(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let a = attempt(&pool, &runtime).await;
    let fence = accept_fence(&runtime, &a, Uuid::now_v7()).await.unwrap();
    dispatch_query(&a).await;
    // Fixed SQL expressions are test cases, never caller input. Each retains
    // all bind parameters so malformed-array checks exercise PostgreSQL itself.
    let cases = [
        (
            "$4::uuid[]",
            "CASE WHEN cardinality($4::uuid[])>0 THEN NULL::uuid[] ELSE $4::uuid[] END",
            "22023",
        ),
        ("$4::uuid[]", "ARRAY[NULL::uuid,($4::uuid[])[2]]", "22023"),
        ("$5::uuid[]", "($5::uuid[])[1:1]", "22023"),
        (
            "$5::uuid[]",
            "ARRAY[gen_random_uuid(),($5::uuid[])[2]]",
            "23514",
        ),
        ("$6::uuid[]", "ARRAY[NULL::uuid,($6::uuid[])[2]]", "23514"),
        (
            "$7::uuid[]",
            "ARRAY[gen_random_uuid(),($7::uuid[])[2]]",
            "23514",
        ),
        (
            "$8::bigint[]",
            "ARRAY[-1::bigint,($8::bigint[])[2]]",
            "22023",
        ),
        (
            "$8::bigint[]",
            "ARRAY[($8::bigint[])[1],($8::bigint[])[1]]",
            "22023",
        ),
        (
            "$8::bigint[]",
            "ARRAY[NULL::bigint,($8::bigint[])[2]]",
            "22023",
        ),
        (
            "$9::double precision[]",
            "ARRAY['NaN'::float8,($9::float8[])[2]]",
            "22023",
        ),
        (
            "$9::double precision[]",
            "ARRAY['Infinity'::float8,($9::float8[])[2]]",
            "22023",
        ),
    ];
    for (needle, replacement, expected) in cases {
        let mut tx = runtime.begin().await.unwrap();
        scoped(&mut tx, a.workspace).await;
        let result = finalize_arguments_in(
            &mut tx,
            &a,
            fence,
            &REFERENCE_ARGUMENTS.replace(needle, replacement),
        )
        .await;
        let error = finish(tx, result).await.expect_err(replacement);
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some(expected),
            "{replacement}: {error}"
        );
        assert_eq!(counts(&pool, &a).await, (0, 0), "{replacement}");
        assert_eq!(job_state(&runtime, &a).await, "running");
    }
    let mut tx = runtime.begin().await.unwrap();
    scoped(&mut tx, a.workspace).await;
    let result = finalize_arguments_in(&mut tx, &a, fence, REFERENCE_ARGUMENTS).await;
    finish(tx, result)
        .await
        .expect("the exact same acknowledged attempt remains eligible after every rejected probe");
    assert_eq!(counts(&pool, &a).await, (1, 0));
    assert_eq!(job_state(&runtime, &a).await, "succeeded");
}
