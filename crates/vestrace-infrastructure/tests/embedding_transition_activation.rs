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
    /// Defaults to the fixture's own qualification; a canonical activation
    /// target names the revision its canonical space was registered against.
    target_model_qualification_revision_id: Option<Uuid>,
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
    .bind(
        command
            .target_model_qualification_revision_id
            .unwrap_or(fixture.model_qualification_id),
    )
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

/// One transition batch carried all the way to `ready_to_activate` by the real
/// authorities: two executed delivery jobs, a planned single-recipe batch, an
/// attempt started against the exact old projection, and a completeness proof.
///
/// Extracted so the qualification below can mutate a predicate against a world
/// that reached this state the way production does. A hand-seeded batch would
/// prove nothing about a rule whose whole subject is the agreement between the
/// recipes and the satisfactions the execution path writes.
struct ProvenBatch {
    source: PreparedJob,
    source_projections: Vec<Uuid>,
    batch_id: Uuid,
}

async fn proven_batch(pool: &PgPool, runtime: &PgPool) -> ProvenBatch {
    let source = prepare_job(
        pool,
        runtime,
        common::prepare_delivery_embedding_job(pool, runtime).await,
        true,
    )
    .await;
    execute(runtime, &source).await;
    let candidate = prepare_job(
        pool,
        runtime,
        common::prepare_additional_delivery_embedding_job(runtime, &source.accepted).await,
        false,
    )
    .await;
    let source_projections = projections(pool, &source.accepted, source.accepted.job_id).await;

    let transition_id = Uuid::now_v7();
    let plan_id = Uuid::now_v7();
    let batch_id = Uuid::now_v7();
    plan(
        runtime,
        &source.accepted,
        PlannedBatch {
            transition_id,
            plan_id,
            batch_id,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: source.accepted.space_registration_id,
            target_model_qualification_revision_id: None,
        },
    )
    .await;
    let actual_attempt = Uuid::now_v7();
    create_attempt(
        runtime,
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
    execute(runtime, &candidate).await;
    assert_eq!(
        observe(
            runtime,
            &source.accepted,
            plan_id,
            batch_id,
            0,
            Some(actual_attempt),
            job_version(pool, &source.accepted, candidate.accepted.job_id).await,
        )
        .await
        .unwrap(),
        "rebuilding"
    );
    assert_eq!(
        prove(
            runtime,
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
    ProvenBatch {
        source,
        source_projections,
        batch_id,
    }
}

#[sqlx::test(migrations = false)]
async fn exact_terminal_result_and_live_existing_projection_prove_ready_to_activate(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    assert_transition_execution_acl(&runtime).await;
    let fixture = proven_batch(&pool, &runtime).await;
    let source = fixture.source;
    let source_projections = fixture.source_projections;
    let batch_id = fixture.batch_id;
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
            target_model_qualification_revision_id: None,
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
            target_model_qualification_revision_id: None,
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
            target_model_qualification_revision_id: None,
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
            target_model_qualification_revision_id: None,
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
            target_model_qualification_revision_id: None,
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
            target_model_qualification_revision_id: None,
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
            target_model_qualification_revision_id: None,
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

// ---------------------------------------------------------------------------
// Task 7: activation.
//
// The invariant these cover is that an embedding qualification head may only
// move when this transaction has already written the exact activation receipt
// that authorizes the move.  The receipt is a durable row, not a session flag,
// so nothing can assert its way past the guard.

async fn seed_qualification_head(pool: &PgPool, fixture: &common::AcceptedJob) {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    scoped(&mut transaction, fixture).await;
    sqlx::query(
        "INSERT INTO model_qualification_heads(workspace_id,model_revision_id,\
         current_qualification_revision_id,version) VALUES($1,$2,$3,1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.model_revision_id)
    .bind(fixture.model_qualification_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

/// Registers one canonical space bound to the fixture's exact model and
/// qualification, then points the head at it.  The head's deferred
/// consistency trigger demands exactly this tuple, so a head that names an
/// active space can only be built this way.
async fn audit_event(pool: &PgPool, fixture: &common::AcceptedJob) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,\
         resource_id,payload,created_at) \
         VALUES($1,$2,$3,'embedding.transition.activated','embedding_transition',$1,'{}',NOW())",
    )
    .bind(id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.context.principal_id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    id
}

#[allow(clippy::too_many_arguments)]
async fn activate(
    runtime: &PgPool,
    fixture: &common::AcceptedJob,
    transition_id: Uuid,
    plan_id: Uuid,
    batch_id: Uuid,
    expected_transition_version: i64,
    expected_head_version: i64,
    audit: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    scoped(&mut transaction, fixture).await;
    let result =
        sqlx::query_scalar("SELECT vestrace_activate_embedding_transition($1,$2,$3,$4,$5,$6,$7)")
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(transition_id)
            .bind(plan_id)
            .bind(batch_id)
            .bind(expected_transition_version)
            .bind(expected_head_version)
            .bind(audit)
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

async fn head_tuple(pool: &PgPool, fixture: &common::AcceptedJob) -> (Uuid, Option<Uuid>, i64) {
    sqlx::query_as(
        "SELECT current_qualification_revision_id,active_space_registration_id,version \
         FROM model_qualification_heads WHERE workspace_id=$1 AND model_revision_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.model_revision_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn assert_refusal(error: sqlx::Error, expected_state: &str, expected_message: &str) {
    let database = error
        .as_database_error()
        .expect("a database refusal was expected");
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

async fn receipt_count(pool: &PgPool, fixture: &common::AcceptedJob) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM embedding_transition_activation_receipts WHERE workspace_id=$1",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The guard, exercised directly.
///
/// A head that names no active space may still move: nothing is live behind
/// it.  The moment it names one, every further move needs its receipt - and
/// not even the guarded owner, the role that owns every P03/P04 relation, may
/// skip that.  Both halves are proven in one transaction, which also lets the
/// established state exist without satisfying the deferred canonical-space
/// validator that only runs at commit.
#[sqlx::test(migrations = false)]
async fn embedding_head_cannot_advance_without_its_exact_activation_receipt(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let source = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    seed_qualification_head(&pool, &source.accepted).await;

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    scoped(&mut transaction, &source.accepted).await;

    // No active space yet, so this move is unguarded.
    sqlx::query(
        "UPDATE model_qualification_heads SET active_space_registration_id=$3,version=version+1          WHERE workspace_id=$1 AND model_revision_id=$2",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(source.accepted.model_revision_id)
    .bind(source.accepted.space_registration_id)
    .execute(&mut *transaction)
    .await
    .expect("a head that names no active space may adopt one");

    // The head is now established, so the next move demands its receipt.
    let error = sqlx::query(
        "UPDATE model_qualification_heads SET current_qualification_revision_id=$3,         version=version+1 WHERE workspace_id=$1 AND model_revision_id=$2",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(source.accepted.model_revision_id)
    .bind(Uuid::now_v7())
    .execute(&mut *transaction)
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding qualification head advance requires its exact activation receipt",
    );
    transaction.rollback().await.unwrap();

    // Nothing survived the refused transaction.
    let (qualification, space, version) = head_tuple(&pool, &source.accepted).await;
    assert_eq!(
        (qualification, space, version),
        (source.accepted.model_qualification_id, None, 1)
    );
    assert_eq!(receipt_count(&pool, &source.accepted).await, 0);
    runtime.close().await;
}

/// A first head is unconstrained: no live corpus precedes it, so no transition
/// could exist to prove anything about it.
#[sqlx::test(migrations = false)]
async fn a_first_embedding_head_needs_no_activation_receipt(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let source = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    seed_qualification_head(&pool, &source.accepted).await;
    let (qualification, space, version) = head_tuple(&pool, &source.accepted).await;
    assert_eq!(
        (qualification, space, version),
        (source.accepted.model_qualification_id, None, 1)
    );
    assert_eq!(receipt_count(&pool, &source.accepted).await, 0);
    runtime.close().await;
}

/// Activation refuses a transition that was never proven complete, and leaves
/// both the head and the receipt table untouched.
#[sqlx::test(migrations = false)]
async fn activation_refuses_an_unproven_transition_and_leaves_the_head(pool: PgPool) {
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
    seed_qualification_head(&pool, &source.accepted).await;

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
            target_model_qualification_revision_id: None,
        },
    )
    .await;
    let audit = audit_event(&pool, &source.accepted).await;
    let error = activate(
        &runtime,
        &source.accepted,
        transition_id,
        plan_id,
        batch_id,
        1,
        1,
        audit,
    )
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding transition activation requires a proven ready transition",
    );

    let (qualification, space, version) = head_tuple(&pool, &source.accepted).await;
    assert_eq!(
        (qualification, space, version),
        (source.accepted.model_qualification_id, None, 1)
    );
    assert_eq!(receipt_count(&pool, &source.accepted).await, 0);
    runtime.close().await;
}

/// Malformed argument tuples are refused before any lock is taken.
#[sqlx::test(migrations = false)]
async fn activation_refuses_malformed_arguments(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let source = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    let audit = audit_event(&pool, &source.accepted).await;
    let error = activate(
        &runtime,
        &source.accepted,
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        0,
        1,
        audit,
    )
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "22023",
        "embedding transition activation arguments are malformed",
    );
    runtime.close().await;
}

/// One transition proven complete over the fixture's legacy space.  The
/// bijection is real: an exact terminal result satisfies the single recipe.
struct ProvenTransition {
    transition_id: uuid::Uuid,
    plan_id: uuid::Uuid,
    batch_id: uuid::Uuid,
    /// The version the transition carries once `prove` has advanced it.
    proven_version: i64,
}

async fn prove_one_transition(
    pool: &PgPool,
    runtime: &PgPool,
    source: &PreparedJob,
) -> ProvenTransition {
    prove_one_transition_onto(
        pool,
        runtime,
        source,
        source.accepted.space_registration_id,
        None,
    )
    .await
}

async fn prove_one_transition_onto(
    pool: &PgPool,
    runtime: &PgPool,
    source: &PreparedJob,
    target_space: Uuid,
    target_qualification: Option<Uuid>,
) -> ProvenTransition {
    // Task 6 requires the physical rebuild job to live in the batch's target
    // space, so the candidate is accepted into that space rather than the
    // source's.
    let mut accepted =
        common::prepare_additional_delivery_embedding_job(runtime, &source.accepted).await;
    accepted.space_registration_id = target_space;
    let candidate = prepare_job(pool, runtime, accepted, false).await;
    let source_projections = projections(pool, &source.accepted, source.accepted.job_id).await;
    let transition_id = Uuid::now_v7();
    let plan_id = Uuid::now_v7();
    let batch_id = Uuid::now_v7();
    plan(
        runtime,
        &source.accepted,
        PlannedBatch {
            transition_id,
            plan_id,
            batch_id,
            recipe_identities: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
            target_space_registration_id: target_space,
            target_model_qualification_revision_id: target_qualification,
        },
    )
    .await;
    let attempt = Uuid::now_v7();
    create_attempt(
        runtime,
        &source.accepted,
        BatchAttempt {
            plan_id,
            batch_id,
            attempt_id: attempt,
            job_id: candidate.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: source_projections[0],
            target_input_ordinal: 0,
        },
    )
    .await
    .expect("one fresh physical job may start the exact batch recipe");
    execute(runtime, &candidate).await;
    observe(
        runtime,
        &source.accepted,
        plan_id,
        batch_id,
        0,
        Some(attempt),
        job_version(pool, &source.accepted, candidate.accepted.job_id).await,
    )
    .await
    .expect("the exact terminal result must satisfy its recipe");
    // Observing the satisfier already advanced the transition, so completeness
    // must be proven against whatever version the database now holds.
    let observed_version: i64 = sqlx::query_scalar(
        "SELECT version FROM embedding_transitions WHERE workspace_id=$1 AND id=$2",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(transition_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let state = prove(
        runtime,
        &source.accepted,
        transition_id,
        plan_id,
        batch_id,
        observed_version,
    )
    .await
    .expect("an exactly satisfied batch must prove ready to activate");
    assert_eq!(state, "ready_to_activate");
    let proven_version: i64 = sqlx::query_scalar(
        "SELECT version FROM embedding_transitions WHERE workspace_id=$1 AND id=$2",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(transition_id)
    .fetch_one(pool)
    .await
    .unwrap();
    ProvenTransition {
        transition_id,
        plan_id,
        batch_id,
        proven_version,
    }
}

/// A proven transition is still not activatable onto a legacy space.  The head
/// may only ever point at a canonical registration bound to its exact
/// qualification, and activation says so exactly rather than letting the
/// deferred head-consistency trigger fail opaquely at commit.
#[sqlx::test(migrations = false)]
async fn activation_refuses_a_non_canonical_target_space(pool: PgPool) {
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
    seed_qualification_head(&pool, &source.accepted).await;
    let proven = prove_one_transition(&pool, &runtime, &source).await;
    let audit = audit_event(&pool, &source.accepted).await;

    let error = activate(
        &runtime,
        &source.accepted,
        proven.transition_id,
        proven.plan_id,
        proven.batch_id,
        proven.proven_version,
        1,
        audit,
    )
    .await
    .unwrap_err();
    assert_refusal(
        error,
        "23514",
        "embedding transition activation requires a canonical target space",
    );

    let (qualification, space, version) = head_tuple(&pool, &source.accepted).await;
    assert_eq!(
        (qualification, space, version),
        (source.accepted.model_qualification_id, None, 1)
    );
    assert_eq!(receipt_count(&pool, &source.accepted).await, 0);
    runtime.close().await;
}

/// A stale transition version is a conflict, not a policy refusal, and nothing
/// moves.
#[sqlx::test(migrations = false)]
async fn activation_refuses_a_stale_transition_version(pool: PgPool) {
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
    seed_qualification_head(&pool, &source.accepted).await;
    let proven = prove_one_transition(&pool, &runtime, &source).await;
    let audit = audit_event(&pool, &source.accepted).await;

    let error = activate(
        &runtime,
        &source.accepted,
        proven.transition_id,
        proven.plan_id,
        proven.batch_id,
        proven.proven_version + 1,
        1,
        audit,
    )
    .await
    .unwrap_err();
    assert_refusal(error, "40001", "embedding transition version is stale");

    let (_, space, version) = head_tuple(&pool, &source.accepted).await;
    assert_eq!((space, version), (None, 1));
    assert_eq!(receipt_count(&pool, &source.accepted).await, 0);
    runtime.close().await;
}

/// Seeds the exact q1 structural evidence `vestrace_assert_canonical_embedding_space`
/// demands, for the shared delivery fixture's own world, then registers one
/// canonical space through the real guarded authority.
///
/// The evidence chain is not faked past its own guard: the registration still
/// goes through `vestrace_register_canonical_embedding_space`, which asserts
/// every join below.  What is seeded here is the durable evidence a real q1
/// qualification would have left behind.
async fn register_canonical_space(
    pool: &PgPool,
    runtime: &PgPool,
    fixture: &common::AcceptedJob,
) -> (Uuid, Uuid) {
    let workspace = fixture.context.workspace_id.as_uuid();
    let qualification_job: Uuid = sqlx::query_scalar(
        "SELECT qualification_job_id FROM model_qualification_revisions \
         WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace)
    .bind(fixture.model_qualification_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let wire_model: String = sqlx::query_scalar(
        "SELECT wire_model_id FROM model_revisions WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace)
    .bind(fixture.model_revision_id)
    .fetch_one(pool)
    .await
    .unwrap();

    let canonical_qualification = Uuid::now_v7();
    let probe_effect = Uuid::now_v7();
    let evidence_root = Uuid::now_v7();
    let evidence_check = Uuid::now_v7();
    let target_binding = Uuid::now_v7();

    // external_effect_intents predates the P03 guarded ownership and the
    // guarded owner holds no privilege on it, so it is written before the role
    // switch rather than under that role.
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) \
         VALUES($1,$2,'local','{}'::jsonb)",
    )
    .bind(probe_effect)
    .bind(workspace)
    .execute(pool)
    .await
    .expect("one probe external effect intent");

    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture).await;
    // model_qualification_revisions is immutable P03 evidence, so the shared
    // fixture's 'embedding' capability cannot be widened in place.  A second
    // qualification revision over the same job and model states the
    // 'embeddings' request capability the canonical assertion reads, and the
    // transition targets that revision.
    sqlx::query(
        "INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,         connection_revision_id,connection_qualification_revision_id,qualification_job_id,         capabilities,valid_until)          SELECT $1,workspace_id,model_revision_id,connection_revision_id,         connection_qualification_revision_id,qualification_job_id,         ARRAY['embedding','embeddings']::TEXT[],NOW()+INTERVAL '1 hour'          FROM model_qualification_revisions WHERE workspace_id=$2 AND id=$3",
    )
    .bind(canonical_qualification)
    .bind(workspace)
    .bind(fixture.model_qualification_id)
    .execute(&mut *owner)
    .await
    .expect("one further qualification revision stating the embeddings capability");
    sqlx::query(
        "INSERT INTO qualification_target_bindings(id,workspace_id,qualification_job_id,\
         connection_id,connection_revision_id,branch,no_auth_binding_revision_id,\
         embedding_model_revision_id) VALUES($1,$2,$3,$4,$5,'no_auth',$6,$7)",
    )
    .bind(target_binding)
    .bind(workspace)
    .bind(qualification_job)
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.no_auth_binding_id)
    .bind(fixture.model_revision_id)
    .execute(&mut *owner)
    .await
    .expect("one q1 target binding naming the embedding model revision");
    sqlx::query(
        "INSERT INTO model_request_evidence_roots(id,workspace_id,external_effect_id,\
         request_kind,binding_snapshot_id,qualification_target_binding_id,cause_kind,cause_id) \
         VALUES($1,$2,$3,'embeddings',NULL,$4,'qualification_probe',$5)",
    )
    .bind(evidence_root)
    .bind(workspace)
    .bind(probe_effect)
    .bind(target_binding)
    .bind(qualification_job)
    .execute(&mut *owner)
    .await
    .expect("one embeddings evidence root rooted at the q1 probe");
    sqlx::query(
        "INSERT INTO model_request_evidence_checks(id,workspace_id,evidence_root_id,status) \
         VALUES($1,$2,$3,'complete')",
    )
    .bind(evidence_check)
    .bind(workspace)
    .bind(evidence_root)
    .execute(&mut *owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_probe_results(id,workspace_id,qualification_job_id,\
         probe_ordinal,result,external_effect_id,model_request_evidence_id) \
         VALUES($1,$2,$3,'90','pass',$4,$5)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace)
    .bind(qualification_job)
    .bind(probe_effect)
    .bind(evidence_root)
    .execute(&mut *owner)
    .await
    .expect("the passing embeddings probe at ordinal 90");
    sqlx::query(
        "INSERT INTO provider_dispatch_causes(external_effect_id,workspace_id,\
         model_request_evidence_id,model_request_evidence_check_id,cause_kind,\
         qualification_job_id,qualification_target_binding_id,qualification_probe_ordinal) \
         VALUES($1,$2,$3,$4,'qualification_probe',$5,$6,'90')",
    )
    .bind(probe_effect)
    .bind(workspace)
    .bind(evidence_root)
    .bind(evidence_check)
    .bind(qualification_job)
    .bind(target_binding)
    .execute(&mut *owner)
    .await
    .expect("the dispatch cause binding the probe to its evidence");
    sqlx::query(
        "INSERT INTO qualification_q1_mre_sources(evidence_root_id,workspace_id,probe_ordinal,\
         message_layout,tool_choice,parallel_tool_calls,response_format,stream,stream_include_usage) \
         VALUES($1,$2,'90','plain_text','none',false,'none',false,false)",
    )
    .bind(evidence_root)
    .bind(workspace)
    .execute(&mut *owner)
    .await
    .expect("the q1 source describing the embeddings probe shape");
    owner.commit().await.unwrap();

    let shape = Uuid::now_v7();
    let registration = Uuid::now_v7();
    let mut governed = runtime.begin().await.unwrap();
    scoped(&mut governed, fixture).await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision($1,$2,1,'embeddings',false,ARRAY[]::TEXT[])",
    )
    .bind(shape)
    .bind(workspace)
    .fetch_one(&mut *governed)
    .await
    .expect("one embeddings request shape revision");
    let registered: Uuid = sqlx::query_scalar(
        "SELECT vestrace_register_canonical_embedding_space($1,$2,'activation-canonical',$3,$4,$5,$6,'float',768)",
    )
    .bind(registration)
    .bind(workspace)
    .bind(fixture.model_revision_id)
    .bind(canonical_qualification)
    .bind(shape)
    .bind(&wire_model)
    .fetch_one(&mut *governed)
    .await
    .expect("the real guarded authority must accept a fully evidenced canonical space");
    governed.commit().await.unwrap();

    // vestrace_register_canonical_embedding_space writes the registration
    // alone; the corpus state and generation guard that every result path
    // reads are seeded here so the canonical space behaves like a real one.
    let mut owner = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *owner)
        .await
        .unwrap();
    scoped(&mut owner, fixture).await;
    sqlx::query(
        "INSERT INTO embedding_space_corpus_states(workspace_id,space_registration_id)          VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(registered)
    .execute(&mut *owner)
    .await
    .expect("one canonical corpus state");
    sqlx::query(
        "INSERT INTO embedding_index_generation_guards(workspace_id,space_registration_id)          VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(registered)
    .execute(&mut *owner)
    .await
    .expect("one canonical generation guard");
    owner.commit().await.unwrap();

    (registered, canonical_qualification)
}

/// Probe: the seeded q1 evidence chain is complete enough for the real
/// guarded authority to register a canonical space.
#[sqlx::test(migrations = false)]
async fn canonical_registration_probe(pool: PgPool) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;
    let source = prepare_job(
        &pool,
        &runtime,
        common::prepare_delivery_embedding_job(&pool, &runtime).await,
        true,
    )
    .await;
    let (registration, _qualification) =
        register_canonical_space(&pool, &runtime, &source.accepted).await;
    let kind: String = sqlx::query_scalar(
        "SELECT registration_kind FROM embedding_space_registrations WHERE workspace_id=$1 AND id=$2",
    )
    .bind(source.accepted.context.workspace_id.as_uuid())
    .bind(registration)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind, "canonical");
    runtime.close().await;
}

const BIJECTION_AUTHORITY: &str =
    "public.vestrace_validate_embedding_transition_bijection(uuid,uuid,boolean)";

/// The half of the bijection that can actually fire, and the edit that disables
/// exactly it.
///
/// The rule is stated as two `EXISTS` joined by `OR` under one message: no
/// recipe without a satisfaction, and no satisfaction without a recipe. Those
/// are two rules sharing a refusal rather than one rule in two parts, and only
/// the first is reachable -- the satisfactions table's own keys make an orphan
/// unconstructible, so the second half never runs. Prefixing the first `EXISTS`
/// with `FALSE AND` leaves the second standing, which is what lets them be told
/// apart.
const BIJECTION_NEEDLE: &str = "    IF EXISTS(\n        SELECT 1\n          FROM embedding_transition_batch_recipes AS recipe\n          LEFT JOIN embedding_transition_recipe_satisfactions AS satisfaction";
const BIJECTION_MUTATION: &str = "    IF FALSE AND EXISTS(\n        SELECT 1\n          FROM embedding_transition_batch_recipes AS recipe\n          LEFT JOIN embedding_transition_recipe_satisfactions AS satisfaction";

/// Read one authority's definition, owner, ACL and runtime reachability.
///
/// Duplicated per suite rather than shared: Rust test binaries share code only
/// through `tests/common/mod.rs`, which is outside this package's change scope.
/// Safe in the one way that matters -- every caller installs, reads back, and
/// compares all four values, so a copy that drifted could not pass quietly.
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

async fn install_authority(pool: &PgPool, definition: &str) {
    sqlx::raw_sql(definition)
        .execute(pool)
        .await
        .expect("the authority definition is installable");
}

fn refusal(error: sqlx::Error) -> (String, String) {
    let database = error.as_database_error().expect("a database refusal");
    (
        database
            .code()
            .map(|code| code.into_owned())
            .unwrap_or_default(),
        database.message().to_owned(),
    )
}

struct PartialBatch {
    source: PreparedJob,
    transition_id: Uuid,
    plan_id: Uuid,
    batch_id: Uuid,
}

/// A batch planned with two recipes and carried to a terminal result for one of
/// them, left one satisfier short of complete.
///
/// Built through the real authorities rather than seeded, because the rule
/// under test is about the agreement between what the plan asked for and what
/// the execution path delivered. Taking a satisfaction away afterwards is not
/// an option and should not be: `embedding_transition_recipe_satisfactions`
/// carries `vestrace_reject_p03_immutable_mutation`, which accepts guarded
/// inserts and nothing else, so even the guarded owner cannot delete one. The
/// shortfall has to be arranged the only way production could reach it, by
/// never satisfying the second recipe.
async fn partly_satisfied_batch(pool: &PgPool, runtime: &PgPool) -> PartialBatch {
    let source = prepare_job(
        pool,
        runtime,
        common::prepare_delivery_embedding_job(pool, runtime).await,
        true,
    )
    .await;
    execute(runtime, &source).await;
    let candidate = prepare_job(
        pool,
        runtime,
        common::prepare_additional_delivery_embedding_job(runtime, &source.accepted).await,
        false,
    )
    .await;
    let source_projections = projections(pool, &source.accepted, source.accepted.job_id).await;

    let transition_id = Uuid::now_v7();
    let plan_id = Uuid::now_v7();
    let batch_id = Uuid::now_v7();
    plan(
        runtime,
        &source.accepted,
        PlannedBatch {
            transition_id,
            plan_id,
            batch_id,
            recipe_identities: vec![Uuid::now_v7(), Uuid::now_v7()],
            // Two constraints meet here. Each recipe's own input ordinals
            // must be zero-based and contiguous, and no two recipes in a
            // batch may bind the same target input ordinal -- so the second
            // recipe declares {0,1} and binds 1.
            inputs: serde_json::json!([[0], [0, 1]]),
            target_space_registration_id: source.accepted.space_registration_id,
            target_model_qualification_revision_id: None,
        },
    )
    .await;
    let attempt = Uuid::now_v7();
    create_attempt(
        runtime,
        &source.accepted,
        BatchAttempt {
            plan_id,
            batch_id,
            attempt_id: attempt,
            job_id: candidate.accepted.job_id,
            recipe_ordinal: 0,
            old_projection_id: source_projections[0],
            target_input_ordinal: 0,
        },
    )
    .await
    .expect("one fresh physical job may start the first batch recipe");
    execute(runtime, &candidate).await;
    assert_eq!(
        observe(
            runtime,
            &source.accepted,
            plan_id,
            batch_id,
            0,
            Some(attempt),
            job_version(pool, &source.accepted, candidate.accepted.job_id).await,
        )
        .await
        .unwrap(),
        "rebuilding"
    );
    // The second recipe is bound and never answered. Binding it matters:
    // `vestrace_validate_embedding_transition_bijection` checks for an unbound
    // recipe before it counts satisfiers, so a recipe left without an attempt
    // would be refused by that earlier clause and this qualification would be
    // observing the wrong predicate.
    let unanswered = prepare_job(
        pool,
        runtime,
        common::prepare_additional_delivery_embedding_job(runtime, &source.accepted).await,
        false,
    )
    .await;
    create_attempt(
        runtime,
        &source.accepted,
        BatchAttempt {
            plan_id,
            batch_id,
            attempt_id: Uuid::now_v7(),
            job_id: unanswered.accepted.job_id,
            recipe_ordinal: 1,
            old_projection_id: source_projections[1],
            // Distinct from the first recipe's: the batch holds one recipe per
            // target input ordinal.
            target_input_ordinal: 1,
        },
    )
    .await
    .expect("the second recipe may be bound by an attempt that never finishes");

    PartialBatch {
        source,
        transition_id,
        plan_id,
        batch_id,
    }
}

/// Try to write a satisfaction for a recipe ordinal the batch does not have.
async fn orphan_satisfaction(
    pool: &PgPool,
    fixture: &common::AcceptedJob,
    batch_id: Uuid,
) -> Result<(), (String, String)> {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    scoped(&mut transaction, fixture).await;
    let written = sqlx::query(
        "INSERT INTO embedding_transition_recipe_satisfactions(\
           id,workspace_id,batch_id,recipe_ordinal,satisfaction_kind,attempt_id,\
           satisfying_projection_id,satisfying_material_id,terminal_job_id,terminal_job_version) \
         SELECT gen_random_uuid(),workspace_id,batch_id,recipe_ordinal+64,'satisfied_existing',NULL,\
           satisfying_projection_id,satisfying_material_id,terminal_job_id,terminal_job_version \
           FROM embedding_transition_recipe_satisfactions \
          WHERE workspace_id=$1 AND batch_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(batch_id)
    .execute(&mut *transaction)
    .await;
    match written {
        Ok(_) => transaction.commit().await.map_err(refusal),
        Err(error) => {
            transaction.rollback().await.unwrap();
            Err(refusal(error))
        }
    }
}

async fn batch_standing(
    pool: &PgPool,
    fixture: &common::AcceptedJob,
    batch_id: Uuid,
) -> (String, i64, i64) {
    let workspace = fixture.context.workspace_id.as_uuid();
    let state: String = sqlx::query_scalar(
        "SELECT state FROM embedding_transition_batches WHERE workspace_id=$1 AND id=$2",
    )
    .bind(workspace)
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let recipes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_transition_batch_recipes \
          WHERE workspace_id=$1 AND batch_id=$2",
    )
    .bind(workspace)
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let satisfactions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_transition_recipe_satisfactions \
          WHERE workspace_id=$1 AND batch_id=$2",
    )
    .bind(workspace)
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .unwrap();
    (state, recipes, satisfactions)
}

/// Mutation qualification: one half of the one-satisfier rule is what stands
/// between a two-recipe batch and a completeness proof it has not earned; the
/// other half cannot fire at all.
///
/// The rule reads as a bijection and is written as two `EXISTS` under a single
/// message, which makes it look like one predicate doing two jobs. Running it
/// one half short says otherwise.
///
/// **No recipe without a satisfier** is load-bearing. Disabled,
/// `vestrace_prove_embedding_transition_completeness` accepts a batch whose
/// second recipe nothing ever answered and moves it to `ready_to_activate`.
/// Nothing else objects, at any boundary, and the batch then stands as an
/// activation candidate on the strength of half its own plan.
///
/// **No satisfier without a recipe** never gets the chance. The satisfactions
/// table refuses an orphan out of its own keys -- the recipe foreign key on
/// `(workspace_id,batch_id,recipe_ordinal)`, and the uniqueness of a satisfying
/// projection within a batch, which is the one this probe's row meets first.
/// The refusal is identical with the mutation installed and without it. That
/// half of the predicate is unreachable, and which key happens to catch a
/// given orphan is an accident of how the row was built; that a key catches it
/// before the rule ever runs is not.
///
/// Worth knowing about a rule one might otherwise trust to be doing both jobs,
/// and not a thing a reader could settle without holding the schema and the
/// function side by side and being right about the order.
#[sqlx::test(migrations = false)]
async fn mutating_the_one_satisfier_rule_proves_a_batch_whose_second_recipe_nothing_answers(
    pool: PgPool,
) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let (original, owner, acl, runtime_execute) = authority_state(&pool, BIJECTION_AUTHORITY).await;
    assert!(
        original.contains(BIJECTION_NEEDLE),
        "the predicate this qualification mutates is no longer in the authority; \
         the mutation would silently test nothing: {original}"
    );

    // Green before: one satisfier short, and the proof is refused.
    let before = partly_satisfied_batch(&pool, &runtime).await;
    assert_eq!(
        batch_standing(&pool, &before.source.accepted, before.batch_id).await,
        ("rebuilding".into(), 2, 1)
    );
    let (state, message) = prove(
        &runtime,
        &before.source.accepted,
        before.transition_id,
        before.plan_id,
        before.batch_id,
        2,
    )
    .await
    .map_err(refusal)
    .expect_err("a batch one satisfier short must not prove complete");
    assert_eq!(state, "23514");
    assert!(
        message.contains("requires exactly one satisfier per recipe"),
        "the refusal must be the rule's own rather than an earlier check: {message}"
    );
    assert_eq!(
        batch_standing(&pool, &before.source.accepted, before.batch_id).await,
        ("rebuilding".into(), 2, 1),
        "a refused proof moves nothing"
    );

    // The other half, before the mutation: refused by the schema, and never by
    // the rule. The table's own keys make an orphan unconstructible -- the
    // recipe foreign key on (workspace_id,batch_id,recipe_ordinal), and, first
    // for this particular attempt, the uniqueness of a satisfying projection
    // within a batch. Which key catches it is an accident of how the row is
    // built; that a key catches it before the rule ever runs is not.
    let (orphan_state, orphan_message) =
        orphan_satisfaction(&pool, &before.source.accepted, before.batch_id)
            .await
            .expect_err("a satisfaction for an absent recipe must be refused");
    assert_eq!(
        orphan_state, "23505",
        "and by an integrity key rather than by the rule: {orphan_message}"
    );
    assert!(
        orphan_message.contains("embedding_transition_recipe_s"),
        "the refusal must name the constraint that caught it: {orphan_message}"
    );

    install_authority(
        &pool,
        &original.replace(BIJECTION_NEEDLE, BIJECTION_MUTATION),
    )
    .await;
    let (mutated, mutated_owner, mutated_acl, mutated_execute) =
        authority_state(&pool, BIJECTION_AUTHORITY).await;
    assert_ne!(mutated, original, "the mutation must actually be installed");
    assert_eq!(
        mutated_owner, owner,
        "the mutation must not change the owner"
    );
    assert_eq!(mutated_acl, acl, "nor the access control list");
    assert_eq!(mutated_execute, runtime_execute, "nor its reachability");

    // Red, in its own world so nothing here can be explained by the probe above.
    let during = partly_satisfied_batch(&pool, &runtime).await;
    assert_eq!(
        prove(
            &runtime,
            &during.source.accepted,
            during.transition_id,
            during.plan_id,
            during.batch_id,
            2,
        )
        .await
        .expect("with the half disabled nothing else refuses an unanswered recipe"),
        "ready_to_activate"
    );
    assert_eq!(
        batch_standing(&pool, &during.source.accepted, during.batch_id).await,
        ("ready_to_activate".into(), 2, 1),
        "the unsafe state is a batch proven ready on the strength of half its own plan"
    );

    // Unchanged: the orphan half was never what refused an orphan.
    let (still_state, _) = orphan_satisfaction(&pool, &before.source.accepted, before.batch_id)
        .await
        .expect_err("the foreign key refuses an orphan with or without the rule");
    assert_eq!(still_state, orphan_state);

    // Restore, byte-exactly, and prove it.
    install_authority(&pool, &original).await;
    let (restored, restored_owner, restored_acl, restored_execute) =
        authority_state(&pool, BIJECTION_AUTHORITY).await;
    assert_eq!(
        restored, original,
        "the definition must be restored exactly"
    );
    assert_eq!(restored_owner, owner);
    assert_eq!(restored_acl, acl);
    assert_eq!(restored_execute, runtime_execute);

    // Green after, on a third world, by the same refusal.
    let after = partly_satisfied_batch(&pool, &runtime).await;
    let (after_state, after_message) = prove(
        &runtime,
        &after.source.accepted,
        after.transition_id,
        after.plan_id,
        after.batch_id,
        2,
    )
    .await
    .map_err(refusal)
    .expect_err("the rule must refuse again");
    assert_eq!(after_state, state);
    assert_eq!(after_message, message);
    assert_eq!(
        batch_standing(&pool, &after.source.accepted, after.batch_id).await,
        ("rebuilding".into(), 2, 1)
    );

    runtime.close().await;
}

const ACTIVATION_AUTHORITY: &str =
    "public.vestrace_activate_embedding_transition(uuid,uuid,uuid,uuid,bigint,bigint,uuid)";

/// The completion-blocker adoption gate, and the edit that disables exactly it.
///
/// `unadopted` counts the batch's `satisfied_result` satisfactions whose result
/// preparation has no row in `embedding_result_credential_blocker_adoptions`.
/// The edit disables the comparison and leaves the count, so the query still
/// runs and only the decision changes.
const ADOPTION_NEEDLE: &str = "    IF unadopted > 0 THEN";
const ADOPTION_MUTATION: &str = "    IF FALSE THEN";

/// One world carried as far as activation can currently be carried: a canonical
/// space that received a real delivery job's outputs, a proven transition onto
/// it, and a Ready current generation standing at the moment of the attempt.
///
/// The order matters and was learned by being refused. The transition has to be
/// proven *before* the generation is published, because proving it runs a
/// rebuild job whose results publish into the same space and stale whatever
/// generation was standing -- so a generation published first is no longer
/// current by the time activation looks.
struct ActivationWorld {
    source: PreparedJob,
    proven: ProvenTransition,
}

async fn activation_world(pool: &PgPool, runtime: &PgPool) -> ActivationWorld {
    let mut accepted = common::prepare_delivery_embedding_job(pool, runtime).await;
    let (registration, qualification) = register_canonical_space(pool, runtime, &accepted).await;
    accepted.space_registration_id = registration;
    let source = prepare_job(pool, runtime, accepted, true).await;
    execute(runtime, &source).await;
    seed_qualification_head(pool, &source.accepted).await;

    let proven =
        prove_one_transition_onto(pool, runtime, &source, registration, Some(qualification)).await;

    let workspace = source.accepted.context.workspace_id.as_uuid();
    let generation = Uuid::now_v7();
    let mut tx = runtime.begin().await.unwrap();
    scoped(&mut tx, &source.accepted).await;
    let version: i64 = sqlx::query_scalar(
        "SELECT guard_version FROM embedding_index_generation_guards \
          WHERE workspace_id=$1 AND space_registration_id=$2",
    )
    .bind(workspace)
    .bind(registration)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("SELECT * FROM vestrace_capture_embedding_generation($1,$2,$3,$4)")
        .bind(generation)
        .bind(workspace)
        .bind(registration)
        .bind(version)
        .execute(&mut *tx)
        .await
        .expect("the canonical corpus must capture");
    sqlx::query("SELECT * FROM vestrace_publish_embedding_generation($1,$2,$3,$4)")
        .bind(workspace)
        .bind(registration)
        .bind(generation)
        .bind(version)
        .execute(&mut *tx)
        .await
        .expect("the captured generation must publish Ready");
    tx.commit()
        .await
        .expect("the published generation must stand");

    ActivationWorld { source, proven }
}

async fn attempt_activation(
    pool: &PgPool,
    runtime: &PgPool,
    world: &ActivationWorld,
) -> Result<Uuid, sqlx::Error> {
    let audit = audit_event(pool, &world.source.accepted).await;
    activate(
        runtime,
        &world.source.accepted,
        world.proven.transition_id,
        world.proven.plan_id,
        world.proven.batch_id,
        world.proven.proven_version,
        1,
        audit,
    )
    .await
}

/// Mutation qualification: the completion-blocker adoption gate is the last
/// thing standing between a proven transition and the qualification head.
///
/// This is the first run in this repository to carry an activation attempt past
/// its earlier guards. Reaching it took the scope amendment recorded for Task
/// 14 and two fixture repairs -- the shared delivery fixture declared a model
/// its own model revision contradicted, and handed `memory_embeddings` to
/// `vestrace` under a comment claiming to mirror production, which migration
/// 0197 and the real provisioner both contradict. Until both were repaired, the
/// attempt stopped at "requires a canonical target space" and this predicate
/// was unreachable.
///
/// With the gate in place the attempt is refused by its own message, the head
/// does not move, and no receipt is written. That much is the qualification.
///
/// With the comparison disabled the refusal **moves**, and where it moves is a
/// product defect this run is the first thing in the repository to reach. The
/// activation proceeds to its own final write --
/// `UPDATE embedding_transitions SET state='activated'` at 0201 line 244 -- and
/// is refused there by `vestrace_guard_embedding_transition_header`, which
/// migration 0200 lines 427-430 defines to permit exactly two moves:
/// `planned -> rebuilding` and `rebuilding -> ready_to_activate`. There is no
/// permitted move into `activated`, although 0188 line 42 lists `activated`
/// among the legal states and 0201 is the authority written to reach it.
///
/// So `vestrace_activate_embedding_transition` cannot succeed anywhere, under
/// any fixture, in production included: migration 0201 added activation and did
/// not extend 0200's header guard to admit it. Every `unwrap_err` on this
/// authority in this repository has this as its final cause, and no test before
/// this one got close enough to see it, because the earlier barriers -- the
/// shared fixture's contradictory model, and its misassignment of
/// `memory_embeddings` -- stopped every attempt long before.
///
/// This test therefore records the gate's qualification honestly as **outcome
/// two**: the rule holds under mutation, but not because of the gate, and the
/// mechanism that holds it is broken rather than protective. Repairing the
/// header guard is a product change and is not made here.
#[sqlx::test(migrations = false)]
async fn mutating_the_completion_blocker_gate_lets_an_unadopted_transition_take_the_head(
    pool: PgPool,
) {
    provision_result_behavior_database(&pool).await;
    let runtime = common::runtime_pool(&pool).await;

    let (original, owner, acl, runtime_execute) =
        authority_state(&pool, ACTIVATION_AUTHORITY).await;
    assert!(
        original.contains(ADOPTION_NEEDLE),
        "the predicate this qualification mutates is no longer in the authority; \
         the mutation would silently test nothing: {original}"
    );

    // Green before: refused by this gate, by name, with nothing moved.
    let before = activation_world(&pool, &runtime).await;
    let error = attempt_activation(&pool, &runtime, &before)
        .await
        .expect_err("an unadopted completion blocker must refuse the activation");
    assert_refusal(
        error,
        "23514",
        "embedding transition activation requires complete completion-blocker adoption",
    );
    let (_, space, version) = head_tuple(&pool, &before.source.accepted).await;
    assert_eq!(
        (space, version),
        (None, 1),
        "a refused activation leaves the head where it was"
    );
    assert_eq!(receipt_count(&pool, &before.source.accepted).await, 0);

    install_authority(&pool, &original.replace(ADOPTION_NEEDLE, ADOPTION_MUTATION)).await;
    let (mutated, mutated_owner, mutated_acl, mutated_execute) =
        authority_state(&pool, ACTIVATION_AUTHORITY).await;
    assert_ne!(mutated, original, "the mutation must actually be installed");
    assert_eq!(
        mutated_owner, owner,
        "the mutation must not change the owner"
    );
    assert_eq!(mutated_acl, acl, "nor the access control list");
    assert_eq!(mutated_execute, runtime_execute, "nor its reachability");

    // Red, in its own world -- and the refusal moves somewhere that has no
    // business being able to refuse it. See this test's note.
    let during = activation_world(&pool, &runtime).await;
    let error = attempt_activation(&pool, &runtime, &during)
        .await
        .expect_err("the transition header guard refuses every move into 'activated'");
    assert_refusal(
        error,
        "23514",
        "embedding transition permits only guarded progress",
    );
    let (_, moved_space, moved_version) = head_tuple(&pool, &during.source.accepted).await;
    assert_eq!(
        (moved_space, moved_version),
        (None, 1),
        "so the head does not move even with the gate disabled"
    );
    assert_eq!(receipt_count(&pool, &during.source.accepted).await, 0);

    // Restore, byte-exactly, and prove it.
    install_authority(&pool, &original).await;
    let (restored, restored_owner, restored_acl, restored_execute) =
        authority_state(&pool, ACTIVATION_AUTHORITY).await;
    assert_eq!(
        restored, original,
        "the definition must be restored exactly"
    );
    assert_eq!(restored_owner, owner);
    assert_eq!(restored_acl, acl);
    assert_eq!(restored_execute, runtime_execute);

    // Green after, on a third world, by the same refusal.
    let after = activation_world(&pool, &runtime).await;
    let error = attempt_activation(&pool, &runtime, &after)
        .await
        .expect_err("the gate must refuse again");
    assert_refusal(
        error,
        "23514",
        "embedding transition activation requires complete completion-blocker adoption",
    );
    let (_, after_space, after_version) = head_tuple(&pool, &after.source.accepted).await;
    assert_eq!((after_space, after_version), (None, 1));
    assert_eq!(receipt_count(&pool, &after.source.accepted).await, 0);

    runtime.close().await;
}
