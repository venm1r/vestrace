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

// --- The Rust half: the path a product actually takes to reach the authority
// above. Every test to this point drove the SQL function directly, which proves
// the transaction and nothing about whether anything calls it.

/// A vault that records what it was asked to do and refuses everything the
/// erasure sequence does not need. Erasure is the only thing under test here.
#[derive(Default)]
struct ErasureVaultCounters {
    prepare_calls: AtomicUsize,
    erase_calls: AtomicUsize,
}

struct CountingErasureVault {
    counters: Arc<ErasureVaultCounters>,
}

impl vestrace_application::MaterialKeyVault for CountingErasureVault {
    fn create_if_absent(
        &self,
        _key_id: vestrace_domain::MaterialKeyId,
        _nonce: vestrace_domain::IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, vestrace_application::VaultError> {
        Err(vestrace_application::VaultError::Unavailable)
    }

    fn unwrap(
        &self,
        _key_id: vestrace_domain::MaterialKeyId,
        _use_dek: &mut dyn FnMut(&vestrace_domain::ZeroizingDek),
    ) -> Result<(), vestrace_application::VaultError> {
        Err(vestrace_application::VaultError::Unavailable)
    }

    fn prepare_erasure(
        &self,
        _key_id: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, vestrace_application::VaultError> {
        self.counters.prepare_calls.fetch_add(1, Ordering::SeqCst);
        Ok(vestrace_application::FenceReceipt::from_uuid(Uuid::now_v7()))
    }

    fn erase(
        &self,
        _key_id: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, vestrace_application::VaultError> {
        self.counters.erase_calls.fetch_add(1, Ordering::SeqCst);
        Ok(vestrace_domain::ErasureReceipt::from_uuid(Uuid::now_v7()))
    }
}

type TestErasureService = vestrace_application::embedding::EmbeddingErasureService<
    vestrace_infrastructure::postgres::PgEmbeddingErasureRepository,
    vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry,
    vestrace_infrastructure::embedding_index::FlatEmbeddingIndex,
>;

fn erasure_service(
    runtime: &PgPool,
    registry: Arc<vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry>,
) -> TestErasureService {
    vestrace_application::embedding::EmbeddingErasureService::new(
        Arc::new(
            vestrace_infrastructure::postgres::PgEmbeddingErasureRepository::new(
                PgStore::from_pool(runtime.clone()),
            ),
        ),
        registry,
    )
}

/// Builds one local index for a space, at a chosen epoch, and installs it.
///
/// The snapshot is deliberately synthetic: the registry is a cache keyed by
/// workspace and registration, and what this test needs from it is an entry at
/// a known epoch, not a faithful generation.
fn install_index_at_epoch(
    registry: &vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry,
    workspace: vestrace_domain::WorkspaceId,
    registration: Uuid,
    epoch: u64,
) -> vestrace_domain::embedding::CanonicalGenerationSnapshot {
    use vestrace_application::embedding::index::{EmbeddingIndexBuilder, EmbeddingIndexFactory};
    use vestrace_infrastructure::embedding_index::{
        FlatEmbeddingIndexFactory, IndexLimits, IndexMemoryBudget, IndexVector,
    };

    let space = vestrace_domain::embedding::EmbeddingSpaceKey::canonical(
        workspace,
        "erasure",
        vestrace_domain::embedding::CanonicalEmbeddingSpace {
            model_revision_id: vestrace_domain::ModelRevisionId::new(),
            model_qualification_revision_id: vestrace_domain::ModelQualificationRevisionId::new(),
            adapter_profile_revision: "q1".into(),
            request_shape_revision_id: Uuid::now_v7(),
            returned_model: "model".into(),
            encoding_format: "float".into(),
            dimensions: 2,
        },
    )
    .expect("a canonical space key");
    let snapshot = vestrace_domain::embedding::CanonicalGenerationSnapshot::new(
        space,
        vestrace_domain::CorpusGenerationId::new(),
        epoch,
        1,
        1,
        10,
        1,
    )
    .expect("a canonical generation snapshot");

    let factory = FlatEmbeddingIndexFactory::new(IndexMemoryBudget::new(1 << 20));
    let mut builder = factory
        .begin(
            snapshot.clone(),
            registration,
            IndexLimits {
                max_members: 8,
                max_bytes: 1 << 20,
            },
        )
        .expect("a builder");
    builder
        .push(IndexVector {
            projection_id: Uuid::now_v7(),
            projection_ordinal: 1,
            material_id: vestrace_domain::ContentMaterialId::new(),
            values: zeroize::Zeroizing::new(vec![1.0_f32, 0.0]),
        })
        .expect("one vector");
    let index = builder.finish().expect("a finished index");
    vestrace_application::embedding::index::EmbeddingIndexRegistryPort::install(
        registry,
        Arc::new(index),
    )
    .expect("install");
    snapshot
}

/// The product path reaches the same authority the tests above drove by hand,
/// and carries back the preparation the two-phase erasure continues from.
#[sqlx::test(migrations = false)]
async fn the_service_propagates_and_returns_the_preparation(pool: PgPool) {
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

    let registry =
        Arc::new(vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry::new());
    let service = erasure_service(&runtime, registry);
    let propagated = service
        .prepare_source(
            &job.accepted.context,
            vestrace_domain::ContentMaterialId::from_uuid(material),
        )
        .await
        .expect("the service must propagate a source a corpus was computed from")
        .expect("a source with dependents is not left to the ordinary path");

    let recorded: (Uuid, Uuid, i64) = sqlx::query_as(
        "SELECT id, material_erasure_preparation_id, dependent_projection_count \
           FROM embedding_erasure_propagations WHERE workspace_id=$1 AND source_material_id=$2",
    )
    .bind(workspace)
    .bind(material)
    .fetch_one(&pool)
    .await
    .expect("the propagation the service made must be recorded");
    assert_eq!(propagated.propagation_id(), recorded.0);
    assert_eq!(propagated.preparation().id(), recorded.1);
    assert_eq!(
        propagated.retired_vector_materials().len(),
        usize::try_from(recorded.2).unwrap(),
        "one ciphertext per retired projection"
    );
    assert_eq!(
        i64::try_from(propagated.dependent_projection_count()).unwrap(),
        recorded.2
    );
    assert!(
        propagated.dependent_projection_count() > 0,
        "an executed job's source has dependents"
    );

    // The preparation must be usable by the ordinary two-phase authority: it
    // names the same key that authority would have fenced.
    let key: Uuid = sqlx::query_scalar(
        "SELECT material_key_id FROM material_erasure_preparations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace)
    .bind(recorded.1)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(propagated.preparation().material_key_id().as_uuid(), key);
    runtime.close().await;
}

/// A material no corpus was computed from is left to the ordinary authority.
/// Interposing on it would record an embedding propagation for an erasure that
/// has nothing to do with embeddings, and refuse a material whose state the
/// ordinary path is entitled to judge.
#[sqlx::test(migrations = false)]
async fn a_material_with_no_dependents_is_left_to_the_ordinary_path(pool: PgPool) {
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

    let unrelated = live_source(&pool, &job.accepted).await;
    let registry =
        Arc::new(vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry::new());
    let service = erasure_service(&runtime, registry);
    let propagated = service
        .prepare_source(&job.accepted.context, unrelated)
        .await
        .expect("a material with no dependents is not a refusal");
    assert!(
        propagated.is_none(),
        "nothing embedded depends on it, so the ordinary path owns it"
    );

    let propagations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_erasure_propagations \
          WHERE workspace_id=$1 AND source_material_id=$2",
    )
    .bind(workspace)
    .bind(unrelated.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(propagations, 0, "and it recorded no propagation");

    // A material that is not in this workspace at all is also not this
    // module's refusal to make.
    let absent = service
        .prepare_source(
            &job.accepted.context,
            vestrace_domain::ContentMaterialId::new(),
        )
        .await
        .expect("an absent material is deferred, not refused here");
    assert!(absent.is_none());
    runtime.close().await;
}

/// Step 4: the local index of an invalidated space is dropped after the
/// invalidation committed, and an index at or past the committed epoch is not.
#[sqlx::test(migrations = false)]
async fn reconciliation_drops_only_the_indexes_the_erasure_invalidated(pool: PgPool) {
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
    let workspace = job.accepted.context.workspace_id;
    let material = a_source_of(&pool, &job.accepted).await;
    let registration = job.accepted.space_registration_id;

    // The epoch the guard stands at now. The propagation advances it, which is
    // what makes an index installed here stale rather than any assumption about
    // where the epoch started.
    let epoch_before: i64 = sqlx::query_scalar(
        "SELECT generation_epoch FROM embedding_index_generation_guards           WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace.as_uuid())
    .bind(registration)
    .fetch_one(&pool)
    .await
    .expect("the executed job must have a generation guard");
    let epoch_before = u64::try_from(epoch_before).unwrap().max(1);

    let registry =
        Arc::new(vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry::new());
    let stale = install_index_at_epoch(&registry, workspace, registration, epoch_before);
    // An unrelated space keeps its index: the sweep is per-space, not a flush.
    let untouched_registration = Uuid::now_v7();
    let untouched =
        install_index_at_epoch(&registry, workspace, untouched_registration, epoch_before);

    let service = erasure_service(&runtime, registry.clone());
    service
        .prepare_source(
            &job.accepted.context,
            vestrace_domain::ContentMaterialId::from_uuid(material),
        )
        .await
        .expect("propagation")
        .expect("a source with dependents");

    use vestrace_application::embedding::index::EmbeddingIndexRegistryPort;
    assert!(
        EmbeddingIndexRegistryPort::get(registry.as_ref(), &stale, registration).is_none(),
        "the invalidated space must not keep answering from its old index"
    );
    assert!(
        EmbeddingIndexRegistryPort::get(registry.as_ref(), &untouched, untouched_registration)
            .is_some(),
        "a space this erasure did not touch keeps its index"
    );

    // The pass is idempotent: replaying it removes nothing further and reports
    // the same spaces.
    let first = service
        .reconcile_one(&job.accepted.context)
        .await
        .expect("a replayed pass");
    let second = service
        .reconcile_one(&job.accepted.context)
        .await
        .expect("a replayed pass");
    assert_eq!(first, second);
    assert!(first >= 1, "the erased space carries a committed epoch");
    assert!(
        EmbeddingIndexRegistryPort::get(registry.as_ref(), &untouched, untouched_registration)
            .is_some(),
        "replaying the pass still does not touch an unaffected space"
    );

    // A newly built index at the committed epoch survives the next pass, which
    // is why no cursor is needed.
    let committed_epoch: i64 = sqlx::query_scalar(
        "SELECT generation_epoch FROM embedding_index_generation_guards \
          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace.as_uuid())
    .bind(registration)
    .fetch_one(&pool)
    .await
    .unwrap();
    let fresh = install_index_at_epoch(
        &registry,
        workspace,
        registration,
        u64::try_from(committed_epoch).unwrap(),
    );
    service
        .reconcile_one(&job.accepted.context)
        .await
        .expect("a pass after a rebuild");
    assert!(
        EmbeddingIndexRegistryPort::get(registry.as_ref(), &fresh, registration).is_some(),
        "an index at the committed epoch is not older than the invalidation"
    );
    runtime.close().await;
}

/// Wired into the two-phase authority, erasing such a source takes the
/// propagating path and still finishes through the ordinary vault sequence.
#[sqlx::test(migrations = false)]
async fn wired_material_erasure_propagates_before_it_destroys(pool: PgPool) {
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

    let registry =
        Arc::new(vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry::new());
    let counters = Arc::new(ErasureVaultCounters::default());
    let service = vestrace_application::MaterialErasureService::new(
        vestrace_infrastructure::postgres::PgMaterialErasureRepository::new(PgStore::from_pool(
            runtime.clone(),
        )),
        CountingErasureVault {
            counters: Arc::clone(&counters),
        },
    )
    .with_embedding_propagation(Arc::new(erasure_service(&runtime, registry)));

    let vectors: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_projection_entries AS entry            JOIN embedding_projection_source_dependencies AS dependency              ON dependency.workspace_id=entry.workspace_id             AND dependency.projection_id=entry.id           WHERE entry.workspace_id=$1 AND dependency.source_material_id=$2",
    )
    .bind(workspace)
    .bind(material)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(vectors > 0, "the source must have vectors computed from it");

    let receipt = service
        .erase_content(
            &job.accepted.context,
            vestrace_domain::ContentMaterialId::from_uuid(material),
        )
        .await
        .expect("a wired erasure of an embedded source must complete");

    // One vault sequence per vector, then one for the source. Erasing the
    // source and leaving its embeddings would be the failure this path exists
    // to prevent, so the count is the claim.
    let expected = usize::try_from(vectors).unwrap() + 1;
    assert_eq!(counters.prepare_calls.load(Ordering::SeqCst), expected);
    assert_eq!(counters.erase_calls.load(Ordering::SeqCst), expected);

    let (recorded_receipt, propagations): (Option<Uuid>, i64) = sqlx::query_as(
        "SELECT (SELECT erasure_receipt FROM material_erasure_preparations \
                  WHERE workspace_id=$1 AND content_material_id=$2), \
                (SELECT count(*) FROM embedding_erasure_propagations \
                  WHERE workspace_id=$1 AND source_material_id=$2)",
    )
    .bind(workspace)
    .bind(material)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recorded_receipt, Some(receipt.as_uuid()));
    assert_eq!(
        propagations, 1,
        "the erasure went through propagation rather than around it"
    );

    // The blocker that made this erasure unlawful is terminal, and it went
    // terminal as part of the propagation rather than by being ignored.
    let nonterminal: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM material_erasure_blockers \
          WHERE workspace_id=$1 AND content_material_id=$2 AND state='nonterminal'",
    )
    .bind(workspace)
    .bind(material)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(nonterminal, 0);

    // A replay is the same erasure, not a second one: the vault is not called
    // again and the propagation is still one.
    let replay = service
        .erase_content(
            &job.accepted.context,
            vestrace_domain::ContentMaterialId::from_uuid(material),
        )
        .await
        .expect("a replay");
    assert_eq!(replay, receipt);
    assert_eq!(counters.prepare_calls.load(Ordering::SeqCst), expected);
    assert_eq!(counters.erase_calls.load(Ordering::SeqCst), expected);
    runtime.close().await;
}

/// Phase two, end to end: the ciphertext is destroyed, and what a later build
/// would draw from this corpus is exactly what the erasure left behind.
///
/// The membership claim is asserted through the predicate
/// `vestrace_capture_embedding_generation` selects members with -- a live
/// projection over a live material -- rather than by calling that function.
/// It cannot be called here: it refuses any space that is not canonically
/// evidenced, and this fixture's world registers a legacy space. Driving it
/// would need a world that both executed a real delivery job and carries q1
/// structural evidence for that same registration, which no fixture builds
/// today. So this proves the corpus a capture would see, and not the capture.
#[sqlx::test(migrations = false)]
async fn after_erasure_the_corpus_a_build_would_draw_holds_only_what_remains(pool: PgPool) {
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
    let registration = job.accepted.space_registration_id;

    let registry =
        Arc::new(vestrace_infrastructure::embedding_index::EmbeddingIndexRegistry::new());
    let counters = Arc::new(ErasureVaultCounters::default());
    let service = vestrace_application::MaterialErasureService::new(
        vestrace_infrastructure::postgres::PgMaterialErasureRepository::new(PgStore::from_pool(
            runtime.clone(),
        )),
        CountingErasureVault {
            counters: Arc::clone(&counters),
        },
    )
    .with_embedding_propagation(Arc::new(erasure_service(&runtime, registry)));
    service
        .erase_content(
            &job.accepted.context,
            vestrace_domain::ContentMaterialId::from_uuid(material),
        )
        .await
        .expect("the full two-phase erasure");

    // The ciphertext is gone, not merely marked: the material has left Live and
    // the vault was asked to destroy its key exactly once.
    let material_state: String =
        sqlx::query_scalar("SELECT state FROM content_materials WHERE workspace_id=$1 AND id=$2")
            .bind(workspace)
            .bind(material)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(material_state, "live", "phase two must leave Live behind");
    // And so has every vector computed from it: a retired projection's
    // ciphertext is content of the erased source in another representation.
    let live_vectors: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_erasure_revoked_members AS revoked            JOIN content_materials AS vector              ON vector.workspace_id=revoked.workspace_id             AND vector.id=revoked.vector_material_id           WHERE revoked.workspace_id=$1 AND vector.state='live'",
    )
    .bind(workspace)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        live_vectors, 0,
        "no vector of an erased source may still be Live"
    );
    assert!(
        counters.erase_calls.load(Ordering::SeqCst) > 1,
        "the source and each of its vectors reached the vault"
    );

    // Not one projection the erasure revoked is still a member a build could
    // draw. This is the capture's own selection, run against the world the
    // erasure left.
    let revoked_still_drawable: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_projection_entries AS p \
           JOIN content_materials AS m ON m.workspace_id=p.workspace_id AND m.id=p.material_id \
           JOIN embedding_erasure_revoked_members AS revoked \
             ON revoked.workspace_id=p.workspace_id AND revoked.projection_entry_id=p.id \
          WHERE p.workspace_id=$1 AND p.space_registration_id=$2 \
            AND p.state='live' AND m.state='live'",
    )
    .bind(workspace)
    .bind(registration)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        revoked_still_drawable, 0,
        "a projection of an erased source may not remain drawable"
    );

    // And the corpus counter agrees with that selection exactly, which is the
    // equality the capture refuses to proceed without. A capture here would
    // therefore succeed and would hold only the remaining members.
    let (drawable, live_member_count): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM embedding_projection_entries AS p \
                   JOIN content_materials AS m \
                     ON m.workspace_id=p.workspace_id AND m.id=p.material_id \
                  WHERE p.workspace_id=$1 AND p.space_registration_id=$2 \
                    AND p.state='live' AND m.state='live'), \
                (SELECT live_member_count FROM embedding_space_corpus_states \
                  WHERE workspace_id=$1 AND space_registration_id=$2)",
    )
    .bind(workspace)
    .bind(registration)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        drawable, live_member_count,
        "the corpus counter must equal what a build would draw, or no capture can proceed"
    );
    runtime.close().await;
}

// The second sealed candidate uses the guarded preparation/binding SQL chain.
// Ciphertext is opaque to this provenance boundary; these probes never decrypt it.
async fn bound_revision_candidate(
    runtime: &PgPool,
    workspace: Uuid,
    revision: Uuid,
) -> (Uuid, Uuid) {
    let intent = Uuid::now_v7();
    let material = Uuid::now_v7();
    let mut ciphertext = vec![0x51_u8; 4096];
    ciphertext[..5].copy_from_slice(b"VMRF\x01");
    let mut tx = runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut tx, workspace).await;
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'memory_revision',$6,0)")
        .bind(intent).bind(workspace).bind(material).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(revision)
        .execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(intent)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(intent)
        .bind(Uuid::now_v7())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,4096)")
        .bind(intent)
        .bind(Uuid::now_v7())
        .bind(ciphertext)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent)
        .bind(Uuid::now_v7())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (intent, material)
}

async fn unprojected_revision_source(pool: &PgPool) -> (PgPool, Uuid, Uuid, Uuid) {
    provision_result_behavior_database(pool).await;
    let runtime = common::runtime_pool(pool).await;
    let accepted = common::prepare_delivery_embedding_job(pool, &runtime).await;
    let workspace = accepted.context.workspace_id.as_uuid();
    let memory = Uuid::now_v7();
    let revision = Uuid::now_v7();
    sqlx::query("INSERT INTO memories(id,workspace_id,kind,status,state_revision) VALUES($1,$2,'fact','candidate',1)")
        .bind(memory).bind(workspace).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO memory_revisions(id,workspace_id,memory_id,revision_number,content,confidence,importance) VALUES($1,$2,$3,1,'source whose erasure must prevent republication',1,1)")
        .bind(revision).bind(workspace).bind(memory).execute(pool).await.unwrap();
    sqlx::query("UPDATE memories SET active_revision_id=$1 WHERE id=$2")
        .bind(revision)
        .bind(memory)
        .execute(pool)
        .await
        .unwrap();
    let vault = OutputVaultFixture::new();
    let materializer = vestrace_infrastructure::postgres::PgGovernedContentMaterializer::new(
        PgStore::from_pool(runtime.clone()),
        Arc::new(vault.vault(accepted.context.workspace_id)),
        Arc::new(ContentMaterialCodec::new()),
    );
    let source = materializer
        .materialize_revision(&accepted.context, revision)
        .await
        .unwrap()
        .unwrap();
    let dependencies: i64 = sqlx::query_scalar("SELECT count(*) FROM embedding_projection_source_dependencies WHERE workspace_id=$1 AND source_material_id=$2")
        .bind(workspace).bind(source.material_id).fetch_one(pool).await.unwrap();
    assert_eq!(
        dependencies, 0,
        "generic erasure must be tested without a projection"
    );
    (runtime, workspace, revision, source.material_id)
}

async fn wait_for_owner_lock(pool: &PgPool, pid: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND locktype='advisory' AND NOT granted)")
                .bind(pid).fetch_one(pool).await.unwrap();
            if waiting { return; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.expect("the competing operation must wait on the common revision owner lock");
}

#[sqlx::test(migrations = false)]
async fn unprojected_erasure_wins_before_revision_republication(pool: PgPool) {
    let (runtime, workspace, revision, source) = unprojected_revision_source(&pool).await;
    let (intent, candidate) = bound_revision_candidate(&runtime, workspace, revision).await;
    let mut erasing = runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut erasing, workspace).await;
    sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
        .bind(source)
        .execute(&mut *erasing)
        .await
        .unwrap();
    let mut publishing = runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut publishing, workspace).await;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *publishing)
        .await
        .unwrap();
    let publication = tokio::spawn(async move {
        let result = sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
            .bind(intent)
            .execute(&mut *publishing)
            .await;
        publishing.rollback().await.unwrap();
        result
    });
    wait_for_owner_lock(&pool, pid).await;
    erasing.commit().await.unwrap();
    let error = publication
        .await
        .unwrap()
        .expect_err("erasure committed while ciphertext was already sealed");
    let database = error.as_database_error().unwrap();
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert!(database.message().contains("requires an eligible revision"));
    let state: String = sqlx::query_scalar("SELECT state FROM content_materials WHERE id=$1")
        .bind(candidate)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        state, "prepared",
        "the losing candidate was never made Live"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn revision_publication_wins_before_unprojected_erasure(pool: PgPool) {
    let (runtime, workspace, revision, source) = unprojected_revision_source(&pool).await;
    let (intent, candidate) = bound_revision_candidate(&runtime, workspace, revision).await;
    let mut publishing = runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut publishing, workspace).await;
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(intent)
        .execute(&mut *publishing)
        .await
        .unwrap();
    let mut erasing = runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut erasing, workspace).await;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *erasing)
        .await
        .unwrap();
    let erasure = tokio::spawn(async move {
        sqlx::query("SELECT * FROM vestrace_prepare_content_material_erasure($1)")
            .bind(source)
            .execute(&mut *erasing)
            .await
            .unwrap();
        erasing.commit().await.unwrap();
    });
    wait_for_owner_lock(&pool, pid).await;
    publishing.commit().await.unwrap();
    erasure.await.unwrap();
    let states: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id,state FROM content_materials WHERE id=ANY($1) ORDER BY id")
            .bind(vec![source, candidate])
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(states.contains(&(candidate, "live".to_owned())));
    assert!(states.contains(&(source, "erasure_prepared".to_owned())));
    // A third candidate after the erase cannot resurrect the same revision.
    let (later, _) = bound_revision_candidate(&runtime, workspace, revision).await;
    let mut tx = runtime.begin().await.unwrap();
    common::result_preparation_fixture::scoped(&mut tx, workspace).await;
    let error = sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(later)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    tx.rollback().await.unwrap();
    runtime.close().await;
}
