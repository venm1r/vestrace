//! Production execution for governed delivery and rebuild embedding jobs.

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
    AcceptDeliveryOutputs, ApplicationError, ConnectionAuth, EffectiveModelRequest,
    EffectiveModelResponse, EmbeddingOutputKeyRepository, EmbeddingResultFinalizationService,
    EmbeddingResultPreparationService, GovernedEmbeddingVector, GovernedEmbeddingsResponse,
    ProviderError, RequestContext,
    embedding::{EmbeddingExecutionOutcome, EmbeddingExecutor},
    run::GovernedModelAdapter,
};
use vestrace_domain::{ConnectionKind, EmbeddingJobId, embedding::EmbeddingJobKind};
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

impl LoopbackEmbeddingAdapter {
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
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
        match request {
            EffectiveModelRequest::Embeddings(_) => {}
            _ => {
                return Err(ProviderError::InvalidResponse(
                    "non-embedding request".into(),
                ));
            }
        }
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

struct ExecutorFixture {
    runtime: PgPool,
    accepted: common::AcceptedJob,
    vault: OutputVaultFixture,
    output_count: i64,
}

async fn fixture(pool: &PgPool, kind: EmbeddingJobKind) -> ExecutorFixture {
    let runtime = common::runtime_pool(pool).await;
    let accepted = common::prepare_delivery_embedding_job(pool, &runtime).await;
    let sources = [
        live_source(&runtime, &accepted).await,
        live_source(&runtime, &accepted).await,
        live_source(&runtime, &accepted).await,
    ];
    common::make_dispatchable(pool, &runtime, &accepted).await;
    for (ordinal, source) in sources.into_iter().enumerate() {
        attach_source_to_evidence(pool, &accepted, source, 8 + ordinal as i64).await;
    }
    let output_set = outputs();
    let receipt_id = Uuid::now_v7();
    let mut command = acceptance_command(&accepted, receipt_id, output_set.clone());
    command.acceptance.kind = kind;
    PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(runtime.clone()))
        .accept_delivery_outputs(&accepted.context, command)
        .await
        .expect("delivery/rebuild output acceptance must establish one exact result chain");
    let vault = OutputVaultFixture::new();
    reconcile_output_receipts(&runtime, &accepted, &output_set, &vault).await;
    record_delivery_policy(&runtime, receipt_id, DeliveryPolicyCase::ExactAllowed).await;
    ExecutorFixture {
        runtime,
        accepted,
        vault,
        output_count: i64::try_from(output_set.len()).unwrap(),
    }
}

async fn execute(
    fixture: &ExecutorFixture,
    adapter: Arc<LoopbackEmbeddingAdapter>,
) -> EmbeddingExecutionOutcome {
    let store = PgStore::from_pool(fixture.runtime.clone());
    let dispatch = Arc::new(common::dispatching_repository(&fixture.runtime, None));
    let vault = Arc::new(fixture.vault.vault(fixture.accepted.context.workspace_id));
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
    EmbeddingExecutor::new(
        dispatch,
        preparation,
        finalization,
        adapter,
        vestrace_domain::WorkerId::new(),
    )
    .execute(&fixture.accepted.context, fixture.accepted.job_id)
    .await
    .expect("the accepted governed embedding job must execute")
}

async fn job_state(pool: &PgPool, context: &RequestContext, job_id: EmbeddingJobId) -> String {
    sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
        .bind(context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn live_projection_count(
    pool: &PgPool,
    context: &RequestContext,
    job_id: EmbeddingJobId,
) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM embedding_projection_entries \
         WHERE workspace_id=$1 AND job_id=$2 AND state='live'",
    )
    .bind(context.workspace_id.as_uuid())
    .bind(job_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn delivery_and_rebuild_each_call_the_provider_once_and_publish_through_the_result_chain(
    pool: PgPool,
) {
    provision_result_behavior_database(&pool).await;
    let delivery = fixture(&pool, EmbeddingJobKind::Delivery).await;
    let rebuild = fixture(&pool, EmbeddingJobKind::Rebuild).await;
    let delivery_provider = Arc::new(LoopbackEmbeddingAdapter::default());
    let rebuild_provider = Arc::new(LoopbackEmbeddingAdapter::default());

    assert_eq!(
        execute(&delivery, delivery_provider.clone()).await,
        EmbeddingExecutionOutcome::Succeeded
    );
    assert_eq!(delivery_provider.calls(), 1);
    assert_eq!(
        job_state(&pool, &delivery.accepted.context, delivery.accepted.job_id).await,
        "succeeded"
    );
    assert_eq!(
        live_projection_count(&pool, &delivery.accepted.context, delivery.accepted.job_id).await,
        delivery.output_count
    );

    assert_eq!(
        execute(&rebuild, rebuild_provider.clone()).await,
        EmbeddingExecutionOutcome::Succeeded
    );
    assert_eq!(rebuild_provider.calls(), 1);
    assert_eq!(
        job_state(&pool, &rebuild.accepted.context, rebuild.accepted.job_id).await,
        "succeeded"
    );
    assert_eq!(
        live_projection_count(&pool, &rebuild.accepted.context, rebuild.accepted.job_id).await,
        rebuild.output_count
    );
}

#[sqlx::test(migrations = false)]
async fn mismatched_delivery_or_rebuild_result_acceptance_is_refused(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let rebuild = fixture(&pool, EmbeddingJobKind::Rebuild).await;
    let mut mismatched: AcceptDeliveryOutputs =
        acceptance_command(&rebuild.accepted, Uuid::now_v7(), outputs());
    mismatched.acceptance.kind = EmbeddingJobKind::Delivery;
    let error = PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(rebuild.runtime.clone()))
        .accept_delivery_outputs(&rebuild.accepted.context, mismatched)
        .await
        .expect_err("a delivery acceptance must not replay against a rebuild job");
    assert!(matches!(
        error,
        ApplicationError::Policy(ref code) if code == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
    ));
}
