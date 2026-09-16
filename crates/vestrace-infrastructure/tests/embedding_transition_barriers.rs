//! Task 8 barrier-law properties exercised through the live guarded schema.

use std::{sync::Arc, time::Duration};

use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use tokio::sync::Barrier;
use uuid::Uuid;

#[path = "common/mod.rs"]
mod common;

use common::*;

struct PlanCall {
    transition_id: Uuid,
    plan_id: Uuid,
    version: i64,
    snapshot_id: Uuid,
    batch_id: Option<Uuid>,
    recipes: Vec<Uuid>,
    inputs: serde_json::Value,
}

fn database_refusal(error: &sqlx::Error) -> (String, String) {
    let database = error
        .as_database_error()
        .expect("a barrier refusal must be a database error");
    (
        database.code().as_deref().unwrap().to_owned(),
        database.message().to_owned(),
    )
}

async fn plan(
    runtime: &PgPool,
    fixture: &AcceptedJob,
    call: PlanCall,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;
    let result = sqlx::query_scalar(
        "SELECT vestrace_plan_embedding_transition_version(\
          $1,$2,$3,$4,$5,$6,'no_auth',NULL,$7,$5,$6,$8,$9,$10,'no_auth',\
          NULL,NULL,NULL,NULL,$7,$11,$12,$13,$14::uuid[],$15::jsonb)",
    )
    .bind(call.transition_id)
    .bind(call.plan_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(call.version)
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.no_auth_binding_id)
    .bind(fixture.connection_qualification_id)
    .bind(fixture.model_revision_id)
    .bind(fixture.model_qualification_id)
    .bind(fixture.space_registration_id)
    .bind(call.batch_id.unwrap_or_else(Uuid::now_v7))
    .bind(call.snapshot_id)
    .bind(call.recipes)
    .bind(sqlx::types::Json(call.inputs))
    .fetch_one(&mut *transaction)
    .await;
    match result {
        Ok(id) => {
            transaction.commit().await?;
            Ok(id)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

async fn guarded_transaction<'a>(
    pool: &'a PgPool,
    fixture: &AcceptedJob,
) -> Transaction<'a, Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction
}

async fn predecessor_job(pool: &PgPool, fixture: &AcceptedJob, snapshot_id: Uuid) -> (Uuid, Uuid) {
    let id = Uuid::now_v7();
    let effect_id = Uuid::now_v7();
    let mut transaction = guarded_transaction(pool, fixture).await;
    sqlx::query(
        "INSERT INTO embedding_jobs(\
          id,workspace_id,space_registration_id,kind,state,version,model_binding_snapshot_id,\
          external_effect_id,model_request_evidence_id)\
          VALUES($1,$2,$3,'delivery','running',2,$4,$5,$6)",
    )
    .bind(id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(snapshot_id)
    .bind(effect_id)
    .bind(Uuid::now_v7())
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    (id, effect_id)
}

async fn classify(runtime: &PgPool, fixture: &AcceptedJob, transition_id: Uuid, plan_id: Uuid) {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_classify_embedding_transition_ambiguity_carries($1,$2,$3)")
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(transition_id)
        .bind(plan_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn abandon(runtime: &PgPool, fixture: &AcceptedJob, transition_id: Uuid, barrier_id: Uuid) {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_abandon_embedding_transition_barrier_candidate($1,$2,$3)")
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(transition_id)
        .bind(barrier_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn acknowledge(
    runtime: &PgPool,
    fixture: &AcceptedJob,
    transition_id: Uuid,
    carry_id: Uuid,
    head_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;
    let result = sqlx::query_scalar(
        "SELECT vestrace_acknowledge_carried_transition_batch_after_unknown(\
         $1,$2,$3,$4,3,$5,$6,'delivery',$7,$8,$9)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(transition_id)
    .bind(carry_id)
    .bind(head_id)
    .bind(Uuid::now_v7())
    .bind(fixture.space_registration_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .fetch_one(&mut *transaction)
    .await;
    match result {
        Ok(id) => {
            transaction.commit().await?;
            Ok(id)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

async fn terminal_effect(
    pool: &PgPool,
    fixture: &AcceptedJob,
    job_id: Uuid,
    effect_id: Uuid,
    state: &str,
    outcome: &str,
) {
    sqlx::query("INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) VALUES($1,$2,'barrier_test','{}')")
        .bind(effect_id).bind(fixture.context.workspace_id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(id,effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,$3,$4,'receipt_recorded','barrier-test',NOW())")
        .bind(Uuid::now_v7()).bind(effect_id).bind(fixture.context.workspace_id.as_uuid()).bind(outcome)
        .execute(pool).await.unwrap();
    let mut transaction = guarded_transaction(pool, fixture).await;
    sqlx::query(
        "UPDATE embedding_jobs SET state=$1,version=version+1 WHERE workspace_id=$2 AND id=$3",
    )
    .bind(state)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

async fn record_effect_outcome(
    pool: &PgPool,
    fixture: &AcceptedJob,
    effect_id: Uuid,
    outcome: &str,
) {
    sqlx::query("INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) VALUES($1,$2,'barrier_test','{}')")
        .bind(effect_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(id,effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,$3,$4,'receipt_recorded','barrier-test',NOW())")
        .bind(Uuid::now_v7())
        .bind(effect_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(outcome)
        .execute(pool)
        .await
        .unwrap();
}

struct OpenBarrier {
    transition_id: Uuid,
    target_plan_id: Uuid,
    predecessor_id: Uuid,
    effect_id: Uuid,
    barrier_id: Uuid,
    dedicated_batch_id: Uuid,
    recipes: Vec<Uuid>,
    inputs: serde_json::Value,
}

async fn open_barrier(pool: &PgPool, runtime: &PgPool, fixture: &AcceptedJob) -> OpenBarrier {
    let transition_id = Uuid::now_v7();
    let source_snapshot_id = Uuid::now_v7();
    let recipes = vec![Uuid::now_v7(), Uuid::now_v7()];
    let inputs = serde_json::json!([[0], [0, 1]]);
    plan(
        runtime,
        fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: source_snapshot_id,
            batch_id: None,
            recipes: recipes.clone(),
            inputs: inputs.clone(),
        },
    )
    .await
    .unwrap();
    let (predecessor_id, effect_id) = predecessor_job(pool, fixture, source_snapshot_id).await;
    let target_plan_id = plan(
        runtime,
        fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            batch_id: None,
            recipes: recipes.clone(),
            inputs: inputs.clone(),
        },
    )
    .await
    .unwrap();
    let (barrier_id, dedicated_batch_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT id,barrier_transition_batch_id FROM embedding_transition_barriers WHERE workspace_id=$1 AND target_transition_plan_id=$2 AND predecessor_embedding_job_id=$3",
    ).bind(fixture.context.workspace_id.as_uuid()).bind(target_plan_id).bind(predecessor_id).fetch_one(pool).await.unwrap();
    OpenBarrier {
        transition_id,
        target_plan_id,
        predecessor_id,
        effect_id,
        barrier_id,
        dedicated_batch_id,
        recipes,
        inputs,
    }
}

async fn assert_terminal(
    pool: &PgPool,
    fixture: &AcceptedJob,
    barrier_id: Uuid,
    state: &str,
    evidence: &str,
) {
    let actual: (String, Option<String>) = sqlx::query_as(
        "SELECT state,resolution_evidence FROM embedding_transition_barriers WHERE workspace_id=$1 AND id=$2",
    ).bind(fixture.context.workspace_id.as_uuid()).bind(barrier_id).fetch_one(pool).await.unwrap();
    assert_eq!(actual, (state.to_owned(), Some(evidence.to_owned())));
}

async fn independent_pool(source: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(source.connect_options().as_ref().clone())
        .await
        .unwrap()
}

async fn wait_until_planner_is_between_header_read_and_successor_insert(pool: &PgPool) {
    for _ in 0..50 {
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(941208::BIGINT)")
            .fetch_one(pool)
            .await
            .unwrap();
        if !acquired {
            return;
        }
        sqlx::query("SELECT pg_advisory_unlock(941208::BIGINT)")
            .execute(pool)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the planner never reached the test-local read-to-insert delay");
}

/// A nonterminal predecessor produces one durable open observation, a fresh
/// dedicated batch, and the exact old-to-new recipe/input evidence.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_nonterminal_predecessor_records_one_open_barrier_with_its_dedicated_batch_and_mapping(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let transition_id = Uuid::now_v7();
    let source_snapshot_id = Uuid::now_v7();
    let recipes = vec![Uuid::now_v7(), Uuid::now_v7()];
    let inputs = serde_json::json!([[0], [0, 1]]);
    let source_plan_id = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: source_snapshot_id,
            batch_id: None,
            recipes: recipes.clone(),
            inputs: inputs.clone(),
        },
    )
    .await
    .expect("the source transition version must be accepted");
    let (predecessor_id, _) = predecessor_job(&pool, &fixture, source_snapshot_id).await;
    let target_plan_id = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            batch_id: None,
            recipes,
            inputs,
        },
    )
    .await
    .expect("a successor plan with a running predecessor must be recorded");

    let (barrier_id, state, dedicated_batch, predecessor_batch, evidence): (Uuid, String, Uuid, Uuid, Option<String>) = sqlx::query_as(
        "SELECT id,state,barrier_transition_batch_id,predecessor_transition_batch_id,resolution_evidence \
           FROM embedding_transition_barriers WHERE workspace_id=$1 AND transition_id=$2 \
             AND target_transition_plan_id=$3 AND predecessor_embedding_job_id=$4",
    )
    .bind(fixture.context.workspace_id.as_uuid()).bind(transition_id).bind(target_plan_id).bind(predecessor_id)
    .fetch_one(&pool).await.expect("the running predecessor must have one durable barrier");
    let source_batch: Uuid = sqlx::query_scalar(
        "SELECT transition_batch_id FROM embedding_transition_plans WHERE workspace_id=$1 AND id=$2",
    ).bind(fixture.context.workspace_id.as_uuid()).bind(source_plan_id).fetch_one(&pool).await.unwrap();
    assert_eq!(state, "awaiting_predecessor_terminal");
    assert!(
        evidence.is_none(),
        "open barriers have no terminal evidence"
    );
    assert_eq!(predecessor_batch, source_batch);
    assert_ne!(
        dedicated_batch, source_batch,
        "the barrier batch is dedicated"
    );
    let mappings: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT old_recipe_ordinal,old_input_ordinal,new_recipe_ordinal,new_input_ordinal \
           FROM embedding_transition_barrier_recipes WHERE workspace_id=$1 AND barrier_id=$2 \
           ORDER BY old_recipe_ordinal,old_input_ordinal",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(barrier_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(mappings, vec![(0, 1, 0, 1), (1, 1, 1, 1), (1, 2, 1, 2)]);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn one_open_barrier_per_predecessor_lineage_is_enforced_with_a_precise_refusal(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let transition_id = Uuid::now_v7();
    let source_snapshot_id = Uuid::now_v7();
    let recipes = vec![Uuid::now_v7()];
    let source_plan_id = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: source_snapshot_id,
            batch_id: None,
            recipes: recipes.clone(),
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let (predecessor_id, _) = predecessor_job(&pool, &fixture, source_snapshot_id).await;
    let target_plan_id = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            recipes,
            batch_id: None,
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let predecessor_batch: Uuid = sqlx::query_scalar(
        "SELECT transition_batch_id FROM embedding_transition_plans WHERE workspace_id=$1 AND id=$2",
    ).bind(fixture.context.workspace_id.as_uuid()).bind(source_plan_id).fetch_one(&pool).await.unwrap();
    let mut transaction = guarded_transaction(&pool, &fixture).await;
    let second_open = sqlx::query(
        "INSERT INTO embedding_transition_barriers(\
          id,workspace_id,transition_id,target_transition_plan_id,predecessor_embedding_job_id,\
          predecessor_transition_batch_id,barrier_transition_batch_id,state)\
          VALUES($1,$2,$3,$4,$5,$6,$7,'awaiting_predecessor_terminal')",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(transition_id)
    .bind(target_plan_id)
    .bind(predecessor_id)
    .bind(predecessor_batch)
    .bind(Uuid::now_v7())
    .execute(&mut *transaction)
    .await
    .unwrap_err();
    let (state, message) = database_refusal(&second_open);
    assert_eq!(state, "23505");
    assert_eq!(
        message,
        "duplicate key value violates unique constraint \"embedding_transition_barriers_one_open_predecessor_lineage\""
    );
    transaction.rollback().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn an_unknown_terminal_predecessor_resolves_the_barrier_to_carry_with_effect_evidence(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let barrier = open_barrier(&pool, &runtime, &fixture).await;
    let mut terminalizing = guarded_transaction(&pool, &fixture).await;
    sqlx::query("UPDATE embedding_jobs SET state='inconclusive_unknown',version=version+1 WHERE workspace_id=$1 AND id=$2")
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(barrier.predecessor_id)
        .execute(&mut *terminalizing)
        .await
        .unwrap();
    terminalizing.commit().await.unwrap();
    terminal_effect(
        &pool,
        &fixture,
        barrier.predecessor_id,
        barrier.effect_id,
        "inconclusive_unknown",
        "unknown",
    )
    .await;
    classify(
        &runtime,
        &fixture,
        barrier.transition_id,
        barrier.target_plan_id,
    )
    .await;
    assert_terminal(
        &pool,
        &fixture,
        barrier.barrier_id,
        "resolved_to_carry",
        "terminal_inconclusive_unknown_effect_outcome",
    )
    .await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_succeeded_terminal_predecessor_resolves_the_barrier_as_satisfied_with_effect_evidence(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let barrier = open_barrier(&pool, &runtime, &fixture).await;
    terminal_effect(
        &pool,
        &fixture,
        barrier.predecessor_id,
        barrier.effect_id,
        "succeeded",
        "acknowledged",
    )
    .await;
    classify(
        &runtime,
        &fixture,
        barrier.transition_id,
        barrier.target_plan_id,
    )
    .await;
    assert_terminal(
        &pool,
        &fixture,
        barrier.barrier_id,
        "resolved_satisfied_existing",
        "terminal_succeeded_effect_outcome",
    )
    .await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_definite_terminal_predecessor_resolves_the_barrier_with_effect_evidence(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let barrier = open_barrier(&pool, &runtime, &fixture).await;
    terminal_effect(
        &pool,
        &fixture,
        barrier.predecessor_id,
        barrier.effect_id,
        "failed_definite",
        "failed",
    )
    .await;
    classify(
        &runtime,
        &fixture,
        barrier.transition_id,
        barrier.target_plan_id,
    )
    .await;
    assert_terminal(
        &pool,
        &fixture,
        barrier.barrier_id,
        "resolved_definite",
        "terminal_definite_effect_outcome",
    )
    .await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn candidate_abandonment_terminalizes_only_the_current_barrier_as_no_longer_required(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let barrier = open_barrier(&pool, &runtime, &fixture).await;
    abandon(
        &runtime,
        &fixture,
        barrier.transition_id,
        barrier.barrier_id,
    )
    .await;
    assert_terminal(
        &pool,
        &fixture,
        barrier.barrier_id,
        "no_longer_required",
        "candidate_abandoned",
    )
    .await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_stale_target_with_still_required_overlap_is_superseded_into_one_linked_current_barrier(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let barrier = open_barrier(&pool, &runtime, &fixture).await;
    let successor_plan_id = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: barrier.transition_id,
            plan_id: Uuid::now_v7(),
            version: 3,
            snapshot_id: Uuid::now_v7(),
            batch_id: None,
            recipes: barrier.recipes.clone(),
            inputs: barrier.inputs.clone(),
        },
    )
    .await
    .unwrap();
    assert_terminal(
        &pool,
        &fixture,
        barrier.barrier_id,
        "superseded",
        "target_version_superseded",
    )
    .await;
    let successor: (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT id,supersedes_barrier_id,state FROM embedding_transition_barriers WHERE workspace_id=$1 AND target_transition_plan_id=$2",
    ).bind(fixture.context.workspace_id.as_uuid()).bind(successor_plan_id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        successor,
        (
            successor.0,
            barrier.barrier_id,
            "awaiting_predecessor_terminal".to_owned()
        )
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn an_open_dedicated_barrier_batch_is_refused_while_a_sibling_batch_is_admitted(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let barrier = open_barrier(&pool, &runtime, &fixture).await;
    let blocked = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: barrier.transition_id,
            plan_id: Uuid::now_v7(),
            version: 3,
            snapshot_id: Uuid::now_v7(),
            batch_id: Some(barrier.dedicated_batch_id),
            recipes: barrier.recipes.clone(),
            inputs: barrier.inputs.clone(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        database_refusal(&blocked),
        (
            "23514".to_owned(),
            "embedding transition barrier batch is not dispatchable".to_owned()
        )
    );
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: barrier.transition_id,
            plan_id: Uuid::now_v7(),
            version: 3,
            snapshot_id: Uuid::now_v7(),
            batch_id: Some(Uuid::now_v7()),
            recipes: barrier.recipes,
            inputs: barrier.inputs,
        },
    )
    .await
    .expect("the sibling transition batch in this workspace is admitted");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn acknowledgement_refuses_the_open_barriers_dedicated_batch_with_the_exact_refusal(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let transition_id = Uuid::now_v7();
    let source_snapshot_id = Uuid::now_v7();
    let recipes = vec![Uuid::now_v7()];
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: source_snapshot_id,
            batch_id: None,
            recipes: recipes.clone(),
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let (head_id, _) = predecessor_job(&pool, &fixture, source_snapshot_id).await;
    let mut transition = guarded_transaction(&pool, &fixture).await;
    sqlx::query("UPDATE embedding_jobs SET state='inconclusive_unknown',version=version+1 WHERE workspace_id=$1 AND id=$2")
        .bind(fixture.context.workspace_id.as_uuid()).bind(head_id).execute(&mut *transition).await.unwrap();
    transition.commit().await.unwrap();
    let target_plan_id = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            batch_id: None,
            recipes,
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let (carry_id, carry_batch, barrier_batch): (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT carry.id,carry.successor_transition_batch_id,barrier.barrier_transition_batch_id \
         FROM embedding_transition_ambiguity_carries AS carry \
         JOIN embedding_transition_barriers AS barrier \
           ON barrier.workspace_id=carry.workspace_id \
          AND barrier.predecessor_embedding_job_id=carry.head_embedding_job_id \
          AND barrier.target_transition_plan_id=carry.target_transition_plan_id \
         WHERE carry.workspace_id=$1 AND carry.target_transition_plan_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(target_plan_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        carry_batch, barrier_batch,
        "the acknowledgement names the barrier-owned dedicated batch"
    );
    let refusal = acknowledge(&runtime, &fixture, transition_id, carry_id, head_id)
        .await
        .unwrap_err();
    assert_eq!(
        database_refusal(&refusal),
        (
            "23514".to_owned(),
            "embedding transition barrier batch is not dispatchable".to_owned()
        )
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn classifier_and_supersession_serialize_to_one_current_barrier_without_recreating_the_stale_target(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let barrier = open_barrier(&pool, &runtime, &fixture).await;
    let old_mappings: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT old_recipe_ordinal,old_input_ordinal,new_recipe_ordinal,new_input_ordinal \
         FROM embedding_transition_barrier_recipes WHERE workspace_id=$1 AND barrier_id=$2 \
         ORDER BY old_recipe_ordinal,old_input_ordinal",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(barrier.barrier_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    // This runs after the planner has read the old header and before it inserts
    // the replacement.  Two independent runtime pools make the race real.
    sqlx::query(
        "CREATE FUNCTION vestrace_test_delay_barrier_successor_insert() RETURNS TRIGGER \
         LANGUAGE plpgsql AS $$ BEGIN \
         PERFORM pg_advisory_xact_lock(941208::BIGINT); PERFORM pg_sleep(1); RETURN NEW; END $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER vestrace_test_delay_barrier_successor_insert \
         BEFORE INSERT ON embedding_transition_barriers FOR EACH ROW \
         EXECUTE FUNCTION vestrace_test_delay_barrier_successor_insert()",
    )
    .execute(&pool)
    .await
    .unwrap();

    let planner_pool = independent_pool(&runtime).await;
    let classifier_pool = independent_pool(&runtime).await;
    let start = Arc::new(Barrier::new(2));
    let planner_start = start.clone();
    let planner_recipes = barrier.recipes.clone();
    let planner_inputs = barrier.inputs.clone();
    let planner = async {
        planner_start.wait().await;
        plan(
            &planner_pool,
            &fixture,
            PlanCall {
                transition_id: barrier.transition_id,
                plan_id: Uuid::now_v7(),
                version: 3,
                snapshot_id: Uuid::now_v7(),
                batch_id: None,
                recipes: planner_recipes,
                inputs: planner_inputs,
            },
        )
        .await
    };
    let classifier = async {
        start.wait().await;
        wait_until_planner_is_between_header_read_and_successor_insert(&classifier_pool).await;
        record_effect_outcome(&pool, &fixture, barrier.effect_id, "unknown").await;
        classify(
            &classifier_pool,
            &fixture,
            barrier.transition_id,
            barrier.target_plan_id,
        )
        .await;
    };
    let (planner, ()) = tokio::join!(planner, classifier);
    let _successor_plan_id =
        planner.expect("the current-version planner must win its guarded insertion");

    let chain: Vec<(Uuid, Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT id,supersedes_barrier_id,state,barrier_transition_batch_id \
         FROM embedding_transition_barriers WHERE workspace_id=$1 AND predecessor_embedding_job_id=$2 ORDER BY created_at,id",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(barrier.predecessor_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        chain.len(),
        2,
        "the stale classifier may not recreate an obsolete third header"
    );
    assert_eq!(
        chain[0],
        (
            barrier.barrier_id,
            None,
            "superseded".to_owned(),
            barrier.dedicated_batch_id
        )
    );
    assert_eq!(chain[1].1, Some(barrier.barrier_id));
    assert_eq!(chain[1].2, "awaiting_predecessor_terminal");
    let current_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_transition_barriers WHERE workspace_id=$1 AND predecessor_embedding_job_id=$2 AND state='awaiting_predecessor_terminal'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(barrier.predecessor_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        current_count, 1,
        "the race leaves exactly one current barrier"
    );
    let successor_mappings: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT old_recipe_ordinal,old_input_ordinal,new_recipe_ordinal,new_input_ordinal \
         FROM embedding_transition_barrier_recipes WHERE workspace_id=$1 AND barrier_id=$2 \
         ORDER BY old_recipe_ordinal,old_input_ordinal",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(chain[1].0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        successor_mappings, old_mappings,
        "the race cannot omit or remap a recipe"
    );
    assert_ne!(
        chain[1].3, barrier.dedicated_batch_id,
        "the successor has a fresh dedicated batch"
    );
}
