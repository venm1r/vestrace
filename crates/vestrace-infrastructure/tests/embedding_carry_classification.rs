//! Task 7 carry-law properties exercised through the live guarded functions.

use std::sync::Arc;

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
    recipes: Vec<Uuid>,
    inputs: serde_json::Value,
}

struct PlanningTarget {
    connection_id: Uuid,
    connection_revision_id: Uuid,
    connection_qualification_id: Uuid,
    model_revision_id: Uuid,
    model_qualification_id: Uuid,
    no_auth_binding_id: Uuid,
    space_registration_id: Uuid,
}

type RecipeMapping = (i64, i64, Option<i64>, Option<i64>, String);

fn database_refusal(error: &sqlx::Error) -> (String, String) {
    let database = error
        .as_database_error()
        .expect("a guarded-function refusal must be a database error");
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
    .bind(Uuid::now_v7())
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

async fn plan_against_target(
    runtime: &PgPool,
    fixture: &AcceptedJob,
    target: &PlanningTarget,
    call: PlanCall,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;
    let result = sqlx::query_scalar(
        "SELECT vestrace_plan_embedding_transition_version(\
          $1,$2,$3,$4,$5,$6,'no_auth',NULL,$7,$8,$9,$10,$11,$12,'no_auth',\
          NULL,NULL,NULL,NULL,$13,$14,$15,$16,$17::uuid[],$18::jsonb)",
    )
    .bind(call.transition_id)
    .bind(call.plan_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(call.version)
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.no_auth_binding_id)
    .bind(target.connection_id)
    .bind(target.connection_revision_id)
    .bind(target.connection_qualification_id)
    .bind(target.model_revision_id)
    .bind(target.model_qualification_id)
    .bind(target.no_auth_binding_id)
    .bind(target.space_registration_id)
    .bind(Uuid::now_v7())
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

async fn terminal_job(
    pool: &PgPool,
    fixture: &AcceptedJob,
    snapshot_id: Uuid,
    state: &str,
    retries_unknown_embedding_job_id: Option<Uuid>,
) -> Uuid {
    let id = Uuid::now_v7();
    let mut transaction = guarded_transaction(pool, fixture).await;
    sqlx::query(
        "INSERT INTO embedding_jobs(\
         id,workspace_id,space_registration_id,kind,state,version,model_binding_snapshot_id,\
         external_effect_id,model_request_evidence_id,retries_unknown_embedding_job_id)\
         VALUES($1,$2,$3,'delivery',$4,2,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(state)
    .bind(snapshot_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(retries_unknown_embedding_job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    id
}

async fn initial_lineage(
    runtime: &PgPool,
    fixture: &AcceptedJob,
    recipes: Vec<Uuid>,
    inputs: serde_json::Value,
) -> (Uuid, Uuid) {
    let transition_id = Uuid::now_v7();
    let source_snapshot_id = Uuid::now_v7();
    plan(
        runtime,
        fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: source_snapshot_id,
            recipes,
            inputs,
        },
    )
    .await
    .unwrap();
    (transition_id, source_snapshot_id)
}

async fn acknowledge(
    runtime: &PgPool,
    fixture: &AcceptedJob,
    transition_id: Uuid,
    carry_id: Uuid,
    head_id: Uuid,
    snapshot_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await?;
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;
    let result = sqlx::query_scalar(
        "SELECT vestrace_acknowledge_carried_transition_batch_after_unknown(\
         $1,$2,$3,$4,2,$5,$6,'delivery',$7,$8,$9)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(transition_id)
    .bind(carry_id)
    .bind(head_id)
    .bind(Uuid::now_v7())
    .bind(fixture.space_registration_id)
    .bind(snapshot_id)
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

async fn independent_pool(source: &PgPool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(source.connect_options().as_ref().clone())
        .await
        .unwrap()
}

async fn independent_planning_target(
    owner: &PgPool,
    runtime: &PgPool,
    fixture: &AcceptedJob,
) -> PlanningTarget {
    let workspace_id = fixture.context.workspace_id.as_uuid();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_binding_id = Uuid::now_v7();
    let model_revision_id = Uuid::now_v7();
    let space_id = Uuid::now_v7();
    let space_registration_id = Uuid::now_v7();
    let model_id: Uuid =
        sqlx::query_scalar("SELECT model_id FROM model_revisions WHERE workspace_id=$1 AND id=$2")
            .bind(workspace_id)
            .bind(fixture.model_revision_id)
            .fetch_one(owner)
            .await
            .unwrap();
    let model_head_version: i64 = sqlx::query_scalar(
        "SELECT version FROM model_revision_heads WHERE workspace_id=$1 AND model_id=$2",
    )
    .bind(workspace_id)
    .bind(model_id)
    .fetch_one(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!("embedding-race-{connector_id}"))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) \
         VALUES($1,$2,$3,$4,$5,'active')",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_id)
    .bind(fixture.context.principal_id.as_uuid())
    .bind(format!("embedding-race-{connection_id}"))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) \
         VALUES($1,$2,'nomic-768-race',768,'text-embedding-nomic-embed-text-v1.5')",
    )
    .bind(space_id)
    .bind(workspace_id)
    .execute(owner)
    .await
    .unwrap();

    let mut governed = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard_id)
        .bind(workspace_id)
        .bind(connection_id)
        .execute(&mut *governed)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(\
         $1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1',\
         'http://127.0.0.1:1234/v1','lm-studio-local/v1','loopback_only','none',NULL,0)",
    )
    .bind(connection_revision_id)
    .bind(workspace_id)
    .bind(connection_id)
    .bind(guard_id)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
        .bind(no_auth_binding_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind(connection_revision_id)
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_revision_and_advance_head(\
         $1,$2,$3,$4,$5,$6,'embedding-model','embedding',NULL,NULL,NULL,NULL,NULL,NULL,$7)",
    )
    .bind(model_revision_id)
    .bind(workspace_id)
    .bind(model_id)
    .bind(connection_id)
    .bind(guard_id)
    .bind(connection_revision_id)
    .bind(model_head_version)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_register_embedding_space($1,$2,$3,'nomic-768-race',\
         'text-embedding-nomic-embed-text-v1.5',768)",
    )
    .bind(space_registration_id)
    .bind(workspace_id)
    .bind(space_id)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    governed.commit().await.unwrap();

    let qualification_job_id = Uuid::now_v7();
    let connection_qualification_id = Uuid::now_v7();
    let model_qualification_id = Uuid::now_v7();
    let mut seeded = guarded_transaction(owner, fixture).await;
    sqlx::query(
        "INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,\
         state,completed_at) VALUES($1,$2,$3,'q1','succeeded',NOW())",
    )
    .bind(qualification_job_id)
    .bind(workspace_id)
    .bind(connection_revision_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions(id,workspace_id,connection_revision_id,\
         qualification_job_id,profile_revision,valid_until,capabilities) \
         VALUES($1,$2,$3,$4,'q1',NOW()+INTERVAL '1 hour',ARRAY['embedding']::TEXT[])",
    )
    .bind(connection_qualification_id)
    .bind(workspace_id)
    .bind(connection_revision_id)
    .bind(qualification_job_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,\
         connection_revision_id,connection_qualification_revision_id,qualification_job_id,\
         capabilities,valid_until) \
         VALUES($1,$2,$3,$4,$5,$6,ARRAY['embedding']::TEXT[],NOW()+INTERVAL '1 hour')",
    )
    .bind(model_qualification_id)
    .bind(workspace_id)
    .bind(model_revision_id)
    .bind(connection_revision_id)
    .bind(connection_qualification_id)
    .bind(qualification_job_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    seeded.commit().await.unwrap();

    PlanningTarget {
        connection_id,
        connection_revision_id,
        connection_qualification_id,
        model_revision_id,
        model_qualification_id,
        no_auth_binding_id,
        space_registration_id,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_eligible_head_is_carried_with_its_ordered_recipe_inputs(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let recipes = vec![Uuid::now_v7(), Uuid::now_v7()];
    let inputs = serde_json::json!([[0], [0, 1]]);
    let (transition_id, source_snapshot_id) =
        initial_lineage(&runtime, &fixture, recipes.clone(), inputs.clone()).await;
    let head_id = terminal_job(
        &pool,
        &fixture,
        source_snapshot_id,
        "inconclusive_unknown",
        None,
    )
    .await;
    let target_plan_id = Uuid::now_v7();
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: target_plan_id,
            version: 2,
            snapshot_id: Uuid::now_v7(),
            recipes,
            inputs,
        },
    )
    .await
    .unwrap();

    let carry: (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT id,target_transition_plan_id,state FROM embedding_transition_ambiguity_carries \
          WHERE workspace_id=$1 AND head_embedding_job_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(head_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(carry.1, target_plan_id);
    assert_eq!(carry.2, "awaiting_acknowledgement");
    let mappings: Vec<RecipeMapping> = sqlx::query_as(
        "SELECT old_recipe_ordinal,old_input_ordinal,new_recipe_ordinal,new_input_ordinal,state \
           FROM embedding_transition_ambiguity_carry_recipes \
          WHERE workspace_id=$1 AND carry_id=$2 \
          ORDER BY old_recipe_ordinal,old_input_ordinal",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(carry.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        mappings,
        vec![
            (
                0,
                1,
                Some(0),
                Some(1),
                "awaiting_acknowledgement".to_owned()
            ),
            (
                1,
                1,
                Some(1),
                Some(1),
                "awaiting_acknowledgement".to_owned()
            ),
            (
                1,
                2,
                Some(1),
                Some(2),
                "awaiting_acknowledgement".to_owned()
            ),
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_eligible_heads_raise_the_exact_ambiguity_refusal(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let recipes = vec![Uuid::now_v7()];
    let (transition_id, source_snapshot_id) = initial_lineage(
        &runtime,
        &fixture,
        recipes.clone(),
        serde_json::json!([[0]]),
    )
    .await;
    for _ in 0..2 {
        terminal_job(
            &pool,
            &fixture,
            source_snapshot_id,
            "inconclusive_unknown",
            None,
        )
        .await;
    }
    let refusal = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            recipes,
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        database_refusal(&refusal),
        (
            "23514".to_owned(),
            "embedding transition lineage has more than one eligible ambiguity head".to_owned(),
        )
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn reordered_regrouped_or_added_survivors_raise_the_exact_refusal(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let recipes = vec![Uuid::now_v7(), Uuid::now_v7()];
    for (candidate, inputs) in [
        (
            vec![recipes[1], recipes[0]],
            serde_json::json!([[0, 1], [0]]),
        ),
        (recipes.clone(), serde_json::json!([[0], [0, 1]])),
        (
            vec![recipes[0], recipes[1], Uuid::now_v7()],
            serde_json::json!([[0], [0, 1], [0]]),
        ),
    ] {
        let (transition_id, _) = initial_lineage(
            &runtime,
            &fixture,
            recipes.clone(),
            serde_json::json!([[0, 1], [0]]),
        )
        .await;
        let refusal = plan(
            &runtime,
            &fixture,
            PlanCall {
                transition_id,
                plan_id: Uuid::now_v7(),
                version: 2,
                snapshot_id: Uuid::now_v7(),
                recipes: candidate,
                inputs,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            database_refusal(&refusal),
            (
                "23514".to_owned(),
                "embedding transition successor recipes must preserve predecessor identities and input ordinals in order".to_owned(),
            )
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn no_head_creates_no_empty_carry_or_successor_batch(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let recipes = vec![Uuid::now_v7()];
    let (transition_id, _) = initial_lineage(
        &runtime,
        &fixture,
        recipes.clone(),
        serde_json::json!([[0]]),
    )
    .await;
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            recipes,
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_transition_ambiguity_carries WHERE workspace_id=$1",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_later_target_supersedes_the_header_without_rewriting_its_recipes(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let recipes = vec![Uuid::now_v7()];
    let (transition_id, source_snapshot_id) = initial_lineage(
        &runtime,
        &fixture,
        recipes.clone(),
        serde_json::json!([[0]]),
    )
    .await;
    let head_id = terminal_job(
        &pool,
        &fixture,
        source_snapshot_id,
        "inconclusive_unknown",
        None,
    )
    .await;
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            recipes: recipes.clone(),
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let old_carry_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_transition_ambiguity_carries WHERE workspace_id=$1 AND head_embedding_job_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(head_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let old_recipes: Vec<RecipeMapping> = sqlx::query_as(
        "SELECT old_recipe_ordinal,old_input_ordinal,new_recipe_ordinal,new_input_ordinal,state \
           FROM embedding_transition_ambiguity_carry_recipes WHERE workspace_id=$1 AND carry_id=$2 \
          ORDER BY old_recipe_ordinal,old_input_ordinal",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(old_carry_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 3,
            snapshot_id: Uuid::now_v7(),
            recipes,
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let carries: Vec<(Uuid, String, Option<String>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id,state,reason,supersedes_carry_id FROM embedding_transition_ambiguity_carries \
          WHERE workspace_id=$1 AND head_embedding_job_id=$2 ORDER BY created_at,id",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(head_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(carries.len(), 2);
    assert_eq!(carries[0].0, old_carry_id);
    assert_eq!(carries[0].1, "no_longer_required");
    assert_eq!(carries[0].2.as_deref(), Some("superseded target mapping"));
    assert_eq!(carries[1].1, "awaiting_acknowledgement");
    assert_eq!(carries[1].3, Some(old_carry_id));
    let preserved: Vec<RecipeMapping> = sqlx::query_as(
        "SELECT old_recipe_ordinal,old_input_ordinal,new_recipe_ordinal,new_input_ordinal,state \
           FROM embedding_transition_ambiguity_carry_recipes WHERE workspace_id=$1 AND carry_id=$2 \
          ORDER BY old_recipe_ordinal,old_input_ordinal",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(old_carry_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(preserved, old_recipes);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_succeeded_predecessor_is_refused_as_a_successor_source(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let predecessor = terminal_job(&pool, &fixture, fixture.snapshot_id, "succeeded", None).await;
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let refusal = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,$7,2)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(fixture.snapshot_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(predecessor)
    .fetch_one(&mut *transaction)
    .await
    .unwrap_err();
    assert_eq!(
        database_refusal(&refusal),
        (
            "23514".to_owned(),
            "embedding job successor requires a terminal inconclusive predecessor at the version it read".to_owned(),
        )
    );
    transaction.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn acknowledgement_attaches_its_transition_snapshot_and_no_other_job_can_use_it(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let recipes = vec![Uuid::now_v7()];
    let (transition_id, source_snapshot_id) = initial_lineage(
        &runtime,
        &fixture,
        recipes.clone(),
        serde_json::json!([[0]]),
    )
    .await;
    let head_id = terminal_job(
        &pool,
        &fixture,
        source_snapshot_id,
        "inconclusive_unknown",
        None,
    )
    .await;
    let target_snapshot_id = Uuid::now_v7();
    let target_plan_id = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: target_snapshot_id,
            recipes,
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();
    let carry_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_transition_ambiguity_carries \
          WHERE workspace_id=$1 AND head_embedding_job_id=$2 AND state='awaiting_acknowledgement'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(head_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let effect_id: Uuid =
        sqlx::query_scalar("SELECT external_effect_id FROM embedding_jobs WHERE id=$1")
            .bind(head_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO external_effect_intents(id,workspace_id,adapter,payload) VALUES($1,$2,'carry_test','{}')")
        .bind(effect_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions(id,effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES($1,$2,$3,'unknown','receipt_recorded','carry-test',NOW())")
        .bind(Uuid::now_v7())
        .bind(effect_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let mut classified = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *classified)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_classify_embedding_transition_ambiguity_carries($1,$2,$3)")
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(transition_id)
        .bind(target_plan_id)
        .execute(&mut *classified)
        .await
        .unwrap();
    classified.commit().await.unwrap();
    let successor_id = acknowledge(
        &runtime,
        &fixture,
        transition_id,
        carry_id,
        head_id,
        target_snapshot_id,
    )
    .await
    .unwrap();
    let recorded_successor: Option<Uuid> = sqlx::query_scalar(
        "SELECT successor_embedding_job_id FROM embedding_transition_ambiguity_carries \
          WHERE workspace_id=$1 AND id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(carry_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recorded_successor, Some(successor_id));

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let refusal = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,$7,2)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(target_snapshot_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(head_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap_err();
    assert_eq!(
        database_refusal(&refusal),
        (
            "23514".to_owned(),
            "embedding job requires an ordinary binding snapshot".to_owned(),
        )
    );
    transaction.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_same_version_planners_serialize_the_carry_for_one_ambiguity_head(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let recipes = vec![Uuid::now_v7(), Uuid::now_v7()];
    let inputs = serde_json::json!([[0], [0, 1]]);
    let (transition_id, source_snapshot_id) =
        initial_lineage(&runtime, &fixture, recipes.clone(), inputs.clone()).await;
    let head_id = terminal_job(
        &pool,
        &fixture,
        source_snapshot_id,
        "inconclusive_unknown",
        None,
    )
    .await;
    let second_target = independent_planning_target(&pool, &runtime, &fixture).await;

    sqlx::query(
        "CREATE FUNCTION vestrace_test_delay_transition_plan_insert() RETURNS TRIGGER \
         LANGUAGE plpgsql AS $$ BEGIN \
         PERFORM pg_advisory_xact_lock(941207::BIGINT); PERFORM pg_sleep(1); RETURN NEW; END $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER vestrace_test_delay_transition_plan_insert \
         BEFORE INSERT ON embedding_transition_plans FOR EACH ROW \
         EXECUTE FUNCTION vestrace_test_delay_transition_plan_insert()",
    )
    .execute(&pool)
    .await
    .unwrap();

    let first_pool = independent_pool(&runtime).await;
    let second_pool = independent_pool(&runtime).await;
    let start = Arc::new(Barrier::new(2));
    let first_start = start.clone();
    let first_recipes = recipes.clone();
    let first_inputs = inputs.clone();
    let first = async {
        first_start.wait().await;
        plan(
            &first_pool,
            &fixture,
            PlanCall {
                transition_id,
                plan_id: Uuid::now_v7(),
                version: 2,
                snapshot_id: Uuid::now_v7(),
                recipes: first_recipes,
                inputs: first_inputs,
            },
        )
        .await
    };
    let second = async {
        start.wait().await;
        plan_against_target(
            &second_pool,
            &fixture,
            &second_target,
            PlanCall {
                transition_id,
                plan_id: Uuid::now_v7(),
                version: 2,
                snapshot_id: Uuid::now_v7(),
                recipes,
                inputs,
            },
        )
        .await
    };
    let (first, second) = tokio::join!(first, second);

    let successes: Vec<Uuid> = [&first, &second]
        .into_iter()
        .filter_map(|result| result.as_ref().ok().copied())
        .collect();
    let refusals: Vec<(String, String)> = [&first, &second]
        .into_iter()
        .filter_map(|result| result.as_ref().err().map(database_refusal))
        .collect();
    assert_eq!(successes.len(), 1);
    assert_eq!(
        refusals,
        vec![(
            "40001".to_owned(),
            "EMBEDDING_TRANSITION_PLAN_IDENTITY_CONFLICT".to_owned(),
        )]
    );

    let headers: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id,state FROM embedding_transition_ambiguity_carries \
          WHERE workspace_id=$1 AND head_embedding_job_id=$2 ORDER BY id",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(head_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].1, "awaiting_acknowledgement");
    let recipe_counts: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT carry_id,count(*) FROM embedding_transition_ambiguity_carry_recipes \
          WHERE workspace_id=$1 AND carry_id=$2 GROUP BY carry_id",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(headers[0].0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(recipe_counts, vec![(headers[0].0, 3)]);
    let open_headers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM embedding_transition_ambiguity_carries \
          WHERE workspace_id=$1 AND head_embedding_job_id=$2 \
            AND state='awaiting_acknowledgement'",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(head_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(open_headers, 1);
}
