//! What erasing a source does to the corpora computed from it.
//!
//! The subject is `vestrace_propagate_embedding_source_erasure`: one guarded
//! transaction that makes every dependent projection unavailable, advances the
//! affected corpora, revokes their current generations, stales transitions
//! built on the affected recipes, and only then lets the ordinary two-phase
//! material erasure begin.
//!
//! The world these tests erase from is built the way the activation suite
//! builds one -- a real delivery job executed through the real result chain --
//! because a projection seeded by hand would not carry the source dependency
//! that is the whole subject here.

//! Exact transition-batch satisfactions are derived from durable result facts.

mod common;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use common::result_preparation_fixture::{
    DeliveryPolicyCase, OutputVaultFixture, acceptance_command, attach_source_to_evidence,
    live_source, outputs, provision_result_behavior_database, reconcile_output_receipts,
    record_delivery_policy,
};
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    ConnectionAuth, EffectiveModelRequest, EffectiveModelResponse, EmbeddingOutputKeyRepository,
    EmbeddingResultFinalizationService, EmbeddingResultPreparationService, GovernedEmbeddingVector,
    GovernedEmbeddingsResponse, ProviderError, run::GovernedModelAdapter,
};
use vestrace_domain::ConnectionKind;
use vestrace_infrastructure::{
    crypto::ContentMaterialCodec,
    postgres::{
        EmbeddingOutputHmacCommitter, PgEmbeddingOutputKeyRepository,
        PgEmbeddingResultFinalizationRepository, PgEmbeddingResultRepository, PgStore,
    },
};

const MODEL: &str = "text-embedding-nomic-embed-text-v1.5";
const OUTPUT_COUNT: usize = 2;

#[derive(Default)]
struct LoopbackEmbeddingAdapter {
    calls: AtomicUsize,
}

#[async_trait]
impl GovernedModelAdapter for LoopbackEmbeddingAdapter {
    async fn execute(
        &self,
        _kind: ConnectionKind,
        _runtime_base_url: &str,
        _auth: ConnectionAuth,
        request: EffectiveModelRequest,
    ) -> Result<EffectiveModelResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(matches!(request, EffectiveModelRequest::Embeddings(_)));
        let data = (0..OUTPUT_COUNT)
            .map(|ordinal| {
                GovernedEmbeddingVector::from_provider_components(ordinal, vec![1.0; 768])
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EffectiveModelResponse::Embeddings(
            GovernedEmbeddingsResponse::new(MODEL, MODEL.into(), data, OUTPUT_COUNT)?,
        ))
    }
}

struct PreparedJob {
    accepted: common::AcceptedJob,
    vault: OutputVaultFixture,
}

async fn prepare_job(
    owner: &PgPool,
    runtime: &PgPool,
    accepted: common::AcceptedJob,
    initialize_policy: bool,
) -> PreparedJob {
    let sources = [
        live_source(runtime, &accepted).await,
        live_source(runtime, &accepted).await,
        live_source(runtime, &accepted).await,
    ];
    common::make_dispatchable_with_policy(owner, runtime, &accepted, initialize_policy).await;
    for (ordinal, source) in sources.into_iter().enumerate() {
        attach_source_to_evidence(owner, &accepted, source, 8 + ordinal as i64).await;
    }
    let output_set = outputs();
    let receipt_id = Uuid::now_v7();
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(
            &accepted.context,
            acceptance_command(&accepted, receipt_id, output_set.clone()),
        )
        .await
        .expect("the result chain must accept the physical transition job");
    let vault = OutputVaultFixture::new();
    reconcile_output_receipts(runtime, &accepted, &output_set, &vault).await;
    record_delivery_policy(runtime, receipt_id, DeliveryPolicyCase::ExactAllowed).await;
    PreparedJob { accepted, vault }
}

async fn execute(runtime: &PgPool, job: &PreparedJob) {
    let store = PgStore::from_pool(runtime.clone());
    let dispatch = Arc::new(common::dispatching_repository(runtime, None));
    let vault = Arc::new(job.vault.vault(job.accepted.context.workspace_id));
    let preparation = Arc::new(EmbeddingResultPreparationService::new(
        Arc::new(PgEmbeddingResultRepository::new(
            store.clone(),
            dispatch.clone(),
        )),
        vault.clone(),
        Arc::new(ContentMaterialCodec::new()),
    ));
    let finalization = Arc::new(EmbeddingResultFinalizationService::new(
        Arc::new(PgEmbeddingResultFinalizationRepository::new(store)),
        vault,
        Arc::new(EmbeddingOutputHmacCommitter::new()),
    ));
    let adapter = Arc::new(LoopbackEmbeddingAdapter::default());
    let outcome = vestrace_application::embedding::EmbeddingExecutor::new(
        dispatch,
        preparation,
        finalization,
        adapter.clone(),
        vestrace_domain::WorkerId::new(),
    )
    .execute(&job.accepted.context, job.accepted.job_id)
    .await
    .expect("the exact physical job result must finalize");
    assert_eq!(
        outcome,
        vestrace_application::embedding::EmbeddingExecutionOutcome::Succeeded
    );
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
}

/// One source material this world's projections were actually computed from.
async fn a_source_of(pool: &PgPool, fixture: &common::AcceptedJob) -> Uuid {
    sqlx::query_scalar(
        "SELECT source_material_id FROM embedding_projection_source_dependencies \
          WHERE workspace_id=$1 ORDER BY source_ordinal LIMIT 1",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(pool)
    .await
    .expect("the executed job must have recorded its source dependencies")
}

async fn propagate(
    runtime: &PgPool,
    fixture: &common::AcceptedJob,
    propagation: Uuid,
    material: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let mut scoped = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *scoped)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.context.principal_id.to_string())
        .fetch_one(&mut *scoped)
        .await
        .unwrap();
    let result = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_propagate_embedding_source_erasure($1,$2,$3)",
    )
    .bind(propagation)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(material)
    .fetch_one(&mut *scoped)
    .await;
    if result.is_ok() {
        scoped.commit().await.unwrap();
    }
    result
}

/// Erasing a source revokes every generation holding a vector computed from it,
/// advances the corpus, and leaves the guard pointing at nothing rather than at
/// a generation whose members are about to stop existing.
#[sqlx::test(migrations = false)]
async fn erasing_a_source_revokes_the_generations_computed_from_it(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    execute(&runtime, &job).await;
    let workspace = job.accepted.context.workspace_id.as_uuid();
    let material = a_source_of(&pool, &job.accepted).await;

    let before: (i64, i64) = sqlx::query_as(
        "SELECT corpus_revision, live_member_count FROM embedding_space_corpus_states \
          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace)
    .bind(job.accepted.space_registration_id)
    .fetch_one(&pool)
    .await
    .expect("the executed job must have a corpus state");

    let propagation = Uuid::now_v7();
    let preparation = propagate(&runtime, &job.accepted, propagation, material)
        .await
        .expect("a Live source with dependents must propagate");

    let recorded: (i64, i64, i64, Uuid) = sqlx::query_as(
        "SELECT dependent_projection_count, revoked_generation_count, staled_transition_count, \
                material_erasure_preparation_id \
           FROM embedding_erasure_propagations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace)
    .bind(propagation)
    .fetch_one(&pool)
    .await
    .expect("the propagation must be recorded");
    assert_eq!(
        recorded.3, preparation,
        "the recorded preparation is the one returned"
    );

    let after: (i64, i64) = sqlx::query_as(
        "SELECT corpus_revision, live_member_count FROM embedding_space_corpus_states \
          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace)
    .bind(job.accepted.space_registration_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after.0,
        before.0 + 1,
        "the corpus revision must advance exactly once for the affected space"
    );
    assert!(
        after.1 <= before.1,
        "erasure never raises the live member count"
    );

    // Nothing that held a vector of this source may still be current.
    let live_holding: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_corpus_generations AS generation \
           JOIN embedding_corpus_generation_members AS member \
             ON member.workspace_id=generation.workspace_id \
            AND member.corpus_generation_id=generation.id \
           JOIN embedding_projection_source_dependencies AS dependency \
             ON dependency.workspace_id=member.workspace_id \
            AND dependency.projection_id=member.embedding_projection_entry_id \
          WHERE generation.workspace_id=$1 AND dependency.source_material_id=$2 \
            AND generation.state IN ('building','ready')",
    )
    .bind(workspace)
    .bind(material)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        live_holding, 0,
        "no building or ready generation may still hold a vector of an erased source"
    );
    runtime.close().await;
}

/// The invalidation event reaches the one append-only stream index workers
/// consume, under the cause that names its preparation.
#[sqlx::test(migrations = false)]
async fn the_invalidation_reaches_the_corpus_change_stream(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    execute(&runtime, &job).await;
    let workspace = job.accepted.context.workspace_id.as_uuid();
    let material = a_source_of(&pool, &job.accepted).await;

    let propagation = Uuid::now_v7();
    let preparation = propagate(&runtime, &job.accepted, propagation, material)
        .await
        .expect("propagation");

    let events: Vec<(String, Uuid, i64, i64)> = sqlx::query_as(
        "SELECT cause, material_erasure_preparation_id, before_corpus_revision, \
                after_corpus_revision \
           FROM embedding_index_rebuild_events \
          WHERE workspace_id=$1 AND material_erasure_preparation_id=$2",
    )
    .bind(workspace)
    .bind(preparation)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!events.is_empty(), "an affected space must be invalidated");
    for event in &events {
        assert_eq!(event.0, "material_erasure");
        assert_eq!(event.1, preparation);
        assert_eq!(
            event.3,
            event.2 + 1,
            "the event must name the revision it advanced to"
        );
    }
    runtime.close().await;
}

/// Propagating twice is the same fact, not a second advance. A replay that
/// advanced the corpus again would invalidate a generation for a source already
/// gone from it.
#[sqlx::test(migrations = false)]
async fn a_replayed_propagation_advances_nothing_further(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    execute(&runtime, &job).await;
    let workspace = job.accepted.context.workspace_id.as_uuid();
    let material = a_source_of(&pool, &job.accepted).await;

    let first = propagate(&runtime, &job.accepted, Uuid::now_v7(), material)
        .await
        .expect("the first propagation");
    let revision_after_first: i64 = sqlx::query_scalar(
        "SELECT corpus_revision FROM embedding_space_corpus_states \
          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace)
    .bind(job.accepted.space_registration_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let replay = propagate(&runtime, &job.accepted, Uuid::now_v7(), material)
        .await
        .expect("a replay returns the same preparation");
    assert_eq!(
        replay, first,
        "a replay must return its one prior preparation"
    );

    let revision_after_replay: i64 = sqlx::query_scalar(
        "SELECT corpus_revision FROM embedding_space_corpus_states \
          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace)
    .bind(job.accepted.space_registration_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision_after_replay, revision_after_first);

    let propagations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_erasure_propagations WHERE workspace_id=$1",
    )
    .bind(workspace)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(propagations, 1, "one source takes one propagation");
    runtime.close().await;
}

/// A material that is not Live has nothing lawful to propagate, and a material
/// that does not exist is not an erasure target.
#[sqlx::test(migrations = false)]
async fn only_an_exact_live_source_may_propagate(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    execute(&runtime, &job).await;

    let error = propagate(&runtime, &job.accepted, Uuid::now_v7(), Uuid::now_v7())
        .await
        .expect_err("an absent material is not an erasure target");
    let code = error
        .as_database_error()
        .and_then(|database| database.code())
        .map(|code| code.to_string())
        .unwrap_or_default();
    assert_eq!(
        code, "23514",
        "expected the exact-Live refusal, got {error}"
    );
    runtime.close().await;
}

/// The runtime role reads propagation records and writes none of them.
#[sqlx::test(migrations = false)]
async fn the_runtime_role_cannot_write_propagation_records_directly(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let job = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    let workspace = job.accepted.context.workspace_id;

    for statement in [
        "INSERT INTO embedding_erasure_propagations(id,workspace_id,source_material_id,\
         material_erasure_preparation_id,dependent_projection_count,revoked_generation_count,\
         staled_transition_count) VALUES(gen_random_uuid(),$1,gen_random_uuid(),\
         gen_random_uuid(),0,0,0)",
        "DELETE FROM embedding_erasure_revoked_members WHERE workspace_id=$1",
    ] {
        let mut scoped = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(workspace.to_string())
            .fetch_one(&mut *scoped)
            .await
            .unwrap();
        let error = sqlx::query(statement)
            .bind(workspace.as_uuid())
            .execute(&mut *scoped)
            .await
            .expect_err("the runtime role must not write propagation records");
        let code = error
            .as_database_error()
            .and_then(|database| database.code())
            .map(|code| code.to_string())
            .unwrap_or_default();
        assert_eq!(
            code, "42501",
            "expected insufficient_privilege, got {error}"
        );
    }
    runtime.close().await;
}
