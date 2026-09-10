//! Exact transition-batch satisfactions are derived from durable result facts.

mod common;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use common::result_preparation_fixture::{
    DeliveryPolicyCase, MIGRATOR, OutputVaultFixture, acceptance_command,
    attach_source_to_evidence, live_source, outputs, provision_result_behavior_database,
    provision_result_behavior_database_through, reconcile_output_receipts, record_delivery_policy,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use vestrace_application::{
    ConnectionAuth, EffectiveModelRequest, EffectiveModelResponse, EmbeddingOutputKeyRepository,
    EmbeddingResultFinalizationService, EmbeddingResultPreparationService, GovernedEmbeddingVector,
    GovernedEmbeddingsResponse, ProviderError, retrieval::EmbeddingStore,
    run::GovernedModelAdapter,
};
use vestrace_domain::{ConnectionKind, EmbeddingJobId};
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

struct PlannedBatch {
    transition_id: Uuid,
    plan_id: Uuid,
    batch_id: Uuid,
    recipe_identities: Vec<Uuid>,
    inputs: serde_json::Value,
    target_space_registration_id: Uuid,
}

async fn plan(runtime: &PgPool, fixture: &common::AcceptedJob, command: PlannedBatch) -> Uuid {
    let mut transaction = runtime.begin().await.unwrap();
    scoped(&mut transaction, fixture).await;
    let plan = sqlx::query_scalar(
        "SELECT vestrace_plan_embedding_transition_version(\
          $1,$2,$3,1,$4,$5,'no_auth',NULL,$6,$4,$5,$7,$8,$9,'no_auth',\
          NULL,NULL,NULL,NULL,$6,$10,$11,$12,$13::uuid[],$14::jsonb)",
    )
    .bind(command.transition_id)
    .bind(command.plan_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.no_auth_binding_id)
    .bind(fixture.connection_qualification_id)
    .bind(fixture.model_revision_id)
    .bind(fixture.model_qualification_id)
    .bind(command.target_space_registration_id)
    .bind(command.batch_id)
    .bind(Uuid::now_v7())
    .bind(command.recipe_identities)
    .bind(sqlx::types::Json(command.inputs))
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    plan
}

async fn assert_transition_execution_acl(runtime: &PgPool) {
    let relations: Vec<(String, String, bool, bool, bool, bool)> = sqlx::query_as(
        "SELECT relname,pg_get_userbyid(relowner),relrowsecurity,relforcerowsecurity, \
                has_table_privilege('vestrace',oid,'SELECT,REFERENCES'), \
                has_table_privilege('vestrace',oid,'INSERT') \
           FROM pg_class \
          WHERE relname=ANY($1) \
          ORDER BY relname",
    )
    .bind(vec![
        "embedding_transitions",
        "embedding_transition_plan_recipes",
        "embedding_transition_ambiguity_carry_recipes",
        "embedding_transition_barrier_recipes",
        "embedding_transition_batches",
        "embedding_transition_batch_recipes",
        "embedding_transition_recipe_dependencies",
        "embedding_transition_job_attempts",
        "embedding_transition_recipe_satisfactions",
        "embedding_transition_observations",
    ])
    .fetch_all(runtime)
    .await
    .unwrap();
    assert_eq!(relations.len(), 10);
    for (relation, owner, rls, force_rls, runtime_read, runtime_insert) in relations {
        assert_eq!(owner, "vestrace_guarded_owner", "{relation} owner");
        assert!(rls && force_rls, "{relation} must force RLS");
        assert!(runtime_read, "{relation} runtime read/reference ACL");
        assert!(
            !runtime_insert,
            "{relation} must reject runtime direct INSERT"
        );
    }
    let functions: Vec<(String, bool, bool, bool)> = sqlx::query_as(
        "SELECT pg_get_userbyid(proowner),prosecdef, \
                has_function_privilege('vestrace',oid,'EXECUTE'), \
                has_function_privilege('public',oid,'EXECUTE') \
           FROM pg_proc \
          WHERE oid=ANY(ARRAY[ \
              'public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)'::regprocedure, \
              'public.vestrace_observe_embedding_transition_attempt(uuid,uuid,uuid,bigint,uuid,bigint)'::regprocedure, \
              'public.vestrace_prove_embedding_transition_completeness(uuid,uuid,uuid,uuid,bigint)'::regprocedure \
          ])",
    )
    .fetch_all(runtime)
    .await
    .unwrap();
    assert_eq!(functions.len(), 3);
    for (owner, security_definer, runtime_execute, public_execute) in functions {
        assert_eq!(owner, "vestrace_guarded_owner");
        assert!(
            security_definer,
            "transition authority must be SECURITY DEFINER"
        );
        assert!(
            runtime_execute,
            "runtime must have the narrow execute grant"
        );
        assert!(
            !public_execute,
            "PUBLIC must not execute transition authority"
        );
    }
}

async fn scoped(transaction: &mut Transaction<'_, Postgres>, fixture: &common::AcceptedJob) {
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
}

async fn job_version(pool: &PgPool, fixture: &common::AcceptedJob, job_id: EmbeddingJobId) -> i64 {
    sqlx::query_scalar("SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(job_id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn projections(
    pool: &PgPool,
    fixture: &common::AcceptedJob,
    job_id: EmbeddingJobId,
) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM embedding_projection_entries \
         WHERE workspace_id=$1 AND job_id=$2 AND state='live' ORDER BY input_ordinal",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(job_id.as_uuid())
    .fetch_all(pool)
    .await
    .unwrap()
}

struct BatchAttempt {
    plan_id: Uuid,
    batch_id: Uuid,
    attempt_id: Uuid,
    job_id: EmbeddingJobId,
    recipe_ordinal: i64,
    old_projection_id: Uuid,
    target_input_ordinal: i64,
}

async fn create_attempt(
    runtime: &PgPool,
    fixture: &common::AcceptedJob,
    command: BatchAttempt,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, fixture).await;
    let expected_job_version: i64 =
        sqlx::query_scalar("SELECT version FROM embedding_jobs WHERE workspace_id=$1 AND id=$2")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(command.job_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await?;
    let result = sqlx::query_scalar(
        "SELECT vestrace_create_embedding_transition_batch_attempt(\
         $1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(command.plan_id)
    .bind(command.batch_id)
    .bind(command.attempt_id)
    .bind(command.job_id.as_uuid())
    .bind(command.recipe_ordinal)
    .bind(command.old_projection_id)
    .bind(command.target_input_ordinal)
    .bind(expected_job_version)
    .fetch_one(&mut *transaction)
    .await;
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

async fn observe(
    runtime: &PgPool,
    fixture: &common::AcceptedJob,
    plan_id: Uuid,
    batch_id: Uuid,
    recipe_ordinal: i64,
    attempt_id: Option<Uuid>,
    expected_job_version: i64,
) -> Result<String, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, fixture).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_observe_embedding_transition_attempt($1,$2,$3,$4,$5,$6)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(plan_id)
    .bind(batch_id)
    .bind(recipe_ordinal)
    .bind(attempt_id)
    .bind(expected_job_version)
    .fetch_one(&mut *transaction)
    .await;
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

async fn prove(
    runtime: &PgPool,
    fixture: &common::AcceptedJob,
    transition_id: Uuid,
    plan_id: Uuid,
    batch_id: Uuid,
    expected_transition_version: i64,
) -> Result<String, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, fixture).await;
    let result = sqlx::query_scalar(
        "SELECT vestrace_prove_embedding_transition_completeness($1,$2,$3,$4,$5)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(transition_id)
    .bind(plan_id)
    .bind(batch_id)
    .bind(expected_transition_version)
    .fetch_one(&mut *transaction)
    .await;
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

fn assert_sqlstate(error: sqlx::Error, expected: &str) {
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some(expected)
    );
}

async fn mark_failed(pool: &PgPool, fixture: &common::AcceptedJob, job_id: EmbeddingJobId) {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    scoped(&mut transaction, fixture).await;
    sqlx::query(
        "UPDATE embedding_jobs SET state='failed_definite',version=version+1 \
         WHERE workspace_id=$1 AND id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(job_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn exact_terminal_result_and_live_existing_projection_prove_ready_to_activate(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    assert_transition_execution_acl(&runtime).await;
    let source = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    execute(&runtime, &source).await;
    let candidate = prepare_job(
        &pool,
        &runtime,
        common::prepare_additional_delivery_embedding_job(&runtime, &source.accepted).await,
        false,
    )
    .await;
    let source_projections = projections(&pool, &source.accepted, source.accepted.job_id).await;

    let transition_id = Uuid::now_v7();
    let plan_id = Uuid::now_v7();
    let batch_id = Uuid::now_v7();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id,
            plan_id,
            batch_id,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
        },
    )
    .await;
    let actual_attempt = Uuid::now_v7();
    create_attempt(
        &runtime,
        &source.accepted,
        BatchAttempt {
            plan_id,
            batch_id,
            attempt_id: actual_attempt,
            job_id: candidate.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: source_projections[0],
            target_input_ordinal: 0,
        },
    )
    .await
    .expect("one fresh physical job may start the exact batch recipe");
    execute(&runtime, &candidate).await;
    assert_eq!(
        observe(
            &runtime,
            &source.accepted,
            plan_id,
            batch_id,
            0,
            Some(actual_attempt),
            job_version(&pool, &source.accepted, candidate.accepted.job_id).await,
        )
        .await
        .unwrap(),
        "rebuilding"
    );
    assert_eq!(
        prove(
            &runtime,
            &source.accepted,
            transition_id,
            plan_id,
            batch_id,
            2
        )
        .await
        .unwrap(),
        "ready_to_activate"
    );
    let actual_kind: String = sqlx::query_scalar(
        "SELECT satisfaction_kind FROM embedding_transition_recipe_satisfactions \
         WHERE workspace_id=$1 AND batch_id=$2 AND recipe_ordinal=0",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(batch_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(actual_kind, "satisfied_result");

    let existing = prepare_job(
        &pool,
        &runtime,
        common::prepare_additional_delivery_embedding_job(&runtime, &source.accepted).await,
        false,
    )
    .await;
    let existing_transition = Uuid::now_v7();
    let existing_plan = Uuid::now_v7();
    let existing_batch = Uuid::now_v7();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id: existing_transition,
            plan_id: existing_plan,
            batch_id: existing_batch,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
        },
    )
    .await;
    create_attempt(
        &runtime,
        &source.accepted,
        BatchAttempt {
            plan_id: existing_plan,
            batch_id: existing_batch,
            attempt_id: Uuid::now_v7(),
            job_id: existing.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: source_projections[0],
            target_input_ordinal: 0,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        observe(
            &runtime,
            &source.accepted,
            existing_plan,
            existing_batch,
            0,
            None,
            0,
        )
        .await
        .unwrap(),
        "rebuilding"
    );
    assert_eq!(
        prove(
            &runtime,
            &source.accepted,
            existing_transition,
            existing_plan,
            existing_batch,
            2,
        )
        .await
        .unwrap(),
        "ready_to_activate"
    );
    let existing_kind: String = sqlx::query_scalar(
        "SELECT satisfaction_kind FROM embedding_transition_recipe_satisfactions \
         WHERE workspace_id=$1 AND batch_id=$2 AND recipe_ordinal=0",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(existing_batch)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(existing_kind, "satisfied_existing");
}

#[sqlx::test(migrations = false)]
async fn incomplete_or_non_exact_transition_satisfactions_are_refused_with_23514(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let source = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    execute(&runtime, &source).await;
    let candidate = prepare_job(
        &pool,
        &runtime,
        common::prepare_additional_delivery_embedding_job(&runtime, &source.accepted).await,
        false,
    )
    .await;
    execute(&runtime, &candidate).await;
    let old_projection = projections(&pool, &source.accepted, source.accepted.job_id).await[0];
    let new_projection =
        projections(&pool, &candidate.accepted, candidate.accepted.job_id).await[0];

    let transition_id = Uuid::now_v7();
    let plan_id = Uuid::now_v7();
    let batch_id = Uuid::now_v7();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id,
            plan_id,
            batch_id,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
        },
    )
    .await;

    let omitted = prepare_job(
        &pool,
        &runtime,
        common::prepare_additional_delivery_embedding_job(&runtime, &source.accepted).await,
        false,
    )
    .await;
    let omitted_attempt = Uuid::now_v7();
    create_attempt(
        &runtime,
        &source.accepted,
        BatchAttempt {
            plan_id,
            batch_id,
            attempt_id: omitted_attempt,
            job_id: omitted.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: old_projection,
            target_input_ordinal: 0,
        },
    )
    .await
    .unwrap();
    assert_sqlstate(
        prove(
            &runtime,
            &source.accepted,
            transition_id,
            plan_id,
            batch_id,
            2,
        )
        .await
        .unwrap_err(),
        "23514",
    );

    assert_sqlstate(
        create_attempt(
            &runtime,
            &source.accepted,
            BatchAttempt {
                plan_id,
                batch_id,
                attempt_id: Uuid::now_v7(),
                job_id: candidate.accepted.job_id,
                recipe_ordinal: 0,
                old_projection_id: old_projection,
                target_input_ordinal: 1,
            },
        )
        .await
        .unwrap_err(),
        "23514",
    );
    assert_sqlstate(
        create_attempt(
            &runtime,
            &source.accepted,
            BatchAttempt {
                plan_id,
                batch_id: Uuid::now_v7(),
                attempt_id: Uuid::now_v7(),
                job_id: candidate.accepted.job_id,
                recipe_ordinal: 0,
                old_projection_id: old_projection,
                target_input_ordinal: 0,
            },
        )
        .await
        .unwrap_err(),
        "23514",
    );

    let wrong_target_plan = Uuid::now_v7();
    let wrong_target_batch = Uuid::now_v7();
    let wrong_target_space =
        vestrace_infrastructure::PgEmbeddingStore::new(PgStore::from_pool(runtime.clone()))
            .ensure_space(
                &source.accepted.context,
                "transition-wrong-target-space",
                MODEL,
                768,
            )
            .await
            .unwrap();
    let wrong_target_registration: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_space_registrations WHERE workspace_id=$1 AND space_id=$2",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(wrong_target_space.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id: Uuid::now_v7(),
            plan_id: wrong_target_plan,
            batch_id: wrong_target_batch,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: wrong_target_registration,
        },
    )
    .await;
    assert_sqlstate(
        create_attempt(
            &runtime,
            &source.accepted,
            BatchAttempt {
                plan_id: wrong_target_plan,
                batch_id: wrong_target_batch,
                attempt_id: Uuid::now_v7(),
                job_id: candidate.accepted.job_id,
                recipe_ordinal: 0,
                old_projection_id: old_projection,
                target_input_ordinal: 0,
            },
        )
        .await
        .unwrap_err(),
        "23514",
    );

    let failed = prepare_job(
        &pool,
        &runtime,
        common::prepare_additional_delivery_embedding_job(&runtime, &source.accepted).await,
        false,
    )
    .await;
    let failed_plan = Uuid::now_v7();
    let failed_batch = Uuid::now_v7();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id: Uuid::now_v7(),
            plan_id: failed_plan,
            batch_id: failed_batch,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
        },
    )
    .await;
    let failed_attempt = Uuid::now_v7();
    create_attempt(
        &runtime,
        &source.accepted,
        BatchAttempt {
            plan_id: failed_plan,
            batch_id: failed_batch,
            attempt_id: failed_attempt,
            job_id: failed.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: old_projection,
            target_input_ordinal: 0,
        },
    )
    .await
    .unwrap();
    mark_failed(&pool, &source.accepted, failed.accepted.job_id).await;
    assert_sqlstate(
        observe(
            &runtime,
            &source.accepted,
            failed_plan,
            failed_batch,
            0,
            Some(failed_attempt),
            job_version(&pool, &source.accepted, failed.accepted.job_id).await,
        )
        .await
        .unwrap_err(),
        "23514",
    );

    let duplicate_plan = Uuid::now_v7();
    let duplicate_batch = Uuid::now_v7();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id: Uuid::now_v7(),
            plan_id: duplicate_plan,
            batch_id: duplicate_batch,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
        },
    )
    .await;
    let first_duplicate = Uuid::now_v7();
    create_attempt(
        &runtime,
        &source.accepted,
        BatchAttempt {
            plan_id: duplicate_plan,
            batch_id: duplicate_batch,
            attempt_id: first_duplicate,
            job_id: candidate.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: old_projection,
            target_input_ordinal: 0,
        },
    )
    .await
    .unwrap();
    observe(
        &runtime,
        &source.accepted,
        duplicate_plan,
        duplicate_batch,
        0,
        Some(first_duplicate),
        job_version(&pool, &source.accepted, candidate.accepted.job_id).await,
    )
    .await
    .unwrap();
    let duplicate_attempt = Uuid::now_v7();
    create_attempt(
        &runtime,
        &source.accepted,
        BatchAttempt {
            plan_id: duplicate_plan,
            batch_id: duplicate_batch,
            attempt_id: duplicate_attempt,
            job_id: source.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: old_projection,
            target_input_ordinal: 0,
        },
    )
    .await
    .unwrap();
    assert_sqlstate(
        observe(
            &runtime,
            &source.accepted,
            duplicate_plan,
            duplicate_batch,
            0,
            Some(duplicate_attempt),
            job_version(&pool, &source.accepted, source.accepted.job_id).await,
        )
        .await
        .unwrap_err(),
        "23514",
    );
    assert_eq!(
        new_projection,
        projections(&pool, &candidate.accepted, candidate.accepted.job_id).await[0]
    );

    let reuse = prepare_job(
        &pool,
        &runtime,
        common::prepare_additional_delivery_embedding_job(&runtime, &source.accepted).await,
        false,
    )
    .await;
    let reuse_plan_one = Uuid::now_v7();
    let reuse_batch_one = Uuid::now_v7();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id: Uuid::now_v7(),
            plan_id: reuse_plan_one,
            batch_id: reuse_batch_one,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
        },
    )
    .await;
    create_attempt(
        &runtime,
        &source.accepted,
        BatchAttempt {
            plan_id: reuse_plan_one,
            batch_id: reuse_batch_one,
            attempt_id: Uuid::now_v7(),
            job_id: reuse.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: old_projection,
            target_input_ordinal: 0,
        },
    )
    .await
    .unwrap();
    let reuse_plan_two = Uuid::now_v7();
    let reuse_batch_two = Uuid::now_v7();
    plan(
        &runtime,
        &source.accepted,
        PlannedBatch {
            transition_id: Uuid::now_v7(),
            plan_id: reuse_plan_two,
            batch_id: reuse_batch_two,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
        },
    )
    .await;
    assert_sqlstate(
        create_attempt(
            &runtime,
            &source.accepted,
            BatchAttempt {
                plan_id: reuse_plan_two,
                batch_id: reuse_batch_two,
                attempt_id: Uuid::now_v7(),
                job_id: reuse.accepted.job_id,
                recipe_ordinal: 0,
                old_projection_id: old_projection,
                target_input_ordinal: 0,
            },
        )
        .await
        .unwrap_err(),
        "23514",
    );
}

#[sqlx::test(migrations = false)]
async fn transition_execution_acl_matches_on_0199_to_0200_upgrade(pool: PgPool) {
    provision_result_behavior_database_through(&pool, 199).await;
    let runtime = common::runtime_pool(&pool).await;
    MIGRATOR
        .run(&runtime)
        .await
        .expect("the runtime role must apply 0200 from the exact 0199 predecessor");
    assert_transition_execution_acl(&runtime).await;
    runtime.close().await;
}
