//! Transition-version planning is an all-or-nothing guarded write.

use std::collections::BTreeMap;

use sqlx::{PgPool, Postgres, Transaction};
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

type EvidenceNodes = (
    Vec<String>,
    Vec<Uuid>,
    Vec<Option<i64>>,
    Vec<Option<String>>,
);

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

async fn replay_embedding_evidence(
    transaction: &mut Transaction<'_, Postgres>,
    fixture: &AcceptedJob,
    snapshot_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let (node_kinds, node_ids, node_versions, node_safe_ordinals): EvidenceNodes = sqlx::query_as(
        "SELECT array_agg(reference_kind ORDER BY ordinal), \
                array_agg(reference_id ORDER BY ordinal), \
                array_agg(reference_version ORDER BY ordinal), \
                array_agg(safe_ordinal ORDER BY ordinal) \
           FROM model_request_evidence_nodes \
          WHERE workspace_id=$1 AND evidence_root_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.evidence_id)
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query_scalar(
        "SELECT vestrace_create_model_request_evidence(\
            $1,$2,$3,'embeddings',$4,NULL,'embedding_job',$5,$6,$7,$8,$9)",
    )
    .bind(fixture.evidence_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(snapshot_id)
    .bind(fixture.job_id.as_uuid())
    .bind(node_kinds)
    .bind(node_ids)
    .bind(node_versions)
    .bind(node_safe_ordinals)
    .fetch_one(&mut **transaction)
    .await
}

/// A first plan is a whole immutable transition version: the target binding is
/// copied from the stated tuple, its transition snapshot is scoped, and its
/// ordered recipes are written in the same transaction.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_first_transition_version_writes_its_plan_recipes_and_scoped_snapshot(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let transition_id = Uuid::now_v7();
    let plan_id = Uuid::now_v7();
    let snapshot_id = Uuid::now_v7();
    let recipes = vec![Uuid::now_v7(), Uuid::now_v7()];

    let planned = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id,
            version: 1,
            snapshot_id,
            recipes: recipes.clone(),
            inputs: serde_json::json!([[0, 1], [0]]),
        },
    )
    .await
    .expect("the guarded planner must persist a first version");

    assert_eq!(planned, plan_id);
    let persisted: (i64, Vec<Uuid>, String, Option<Uuid>) = sqlx::query_as(
        "SELECT plan.version, array_agg(recipe.recipe_identity ORDER BY recipe.recipe_ordinal), \
                scope.scope, scope.transition_plan_id \
           FROM embedding_transition_plans AS plan \
           JOIN embedding_transition_plan_recipes AS recipe \
             ON recipe.workspace_id=plan.workspace_id AND recipe.transition_plan_id=plan.id \
           JOIN model_binding_snapshot_scopes AS scope \
             ON scope.workspace_id=plan.workspace_id AND scope.snapshot_id=$2 \
          WHERE plan.workspace_id=$1 AND plan.id=$3 \
          GROUP BY plan.version, scope.scope, scope.transition_plan_id",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(snapshot_id)
    .bind(plan_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        persisted,
        (1, recipes, "transition".to_owned(), Some(plan_id))
    );
}

fn database_refusal(error: &sqlx::Error) -> (String, String) {
    let database = error
        .as_database_error()
        .expect("a guarded planner refusal must be a database error");
    (
        database
            .code()
            .as_deref()
            .expect("the refusal has SQLSTATE")
            .to_owned(),
        database.message().to_owned(),
    )
}

fn live_function_bodies() -> BTreeMap<&'static str, &'static str> {
    const MIGRATIONS: &[&str] = &[
        include_str!("../../../migrations/0178_model_revisions_and_binding_snapshots.sql"),
        include_str!("../../../migrations/0184_provider_dispatch_and_result_contract.sql"),
        include_str!("../../../migrations/0186_provider_execution_wiring.sql"),
        include_str!("../../../migrations/0187_embedding_jobs_and_corpus_generations.sql"),
        include_str!("../../../migrations/0188_embedding_transitions.sql"),
    ];
    let mut functions = BTreeMap::new();
    for migration in MIGRATIONS {
        let mut remaining = *migration;
        while let Some((start, _prefix)) = [
            "CREATE OR REPLACE FUNCTION vestrace_",
            "CREATE OR REPLACE FUNCTION public.vestrace_",
        ]
        .iter()
        .filter_map(|prefix| remaining.find(prefix).map(|start| (start, *prefix)))
        .min_by_key(|(start, _)| *start)
        {
            remaining = &remaining[start + "CREATE OR REPLACE FUNCTION ".len()..];
            let header_end = remaining
                .find("AS $$")
                .expect("each guarded SQL function has a dollar-quoted body");
            let header = &remaining[..header_end];
            let name = header
                .split_once('(')
                .expect("function header has arguments")
                .0
                .trim_start_matches("public.");
            let body = &remaining[header_end + "AS $$".len()..];
            let body_end = body
                .find("$$;")
                .expect("each SQL body has its own dollar terminator");
            functions.insert(name, &body[..body_end]);
            remaining = &body[body_end + "$$;".len()..];
        }
    }
    functions
}

#[test]
fn snapshot_callers_are_a_closed_world_with_the_recorded_dispositions() {
    let functions = live_function_bodies();
    let caller_dispositions = BTreeMap::from([
        ("vestrace_create_run_model_binding_snapshot", "creator"),
        ("vestrace_accept_embedding_job", "fenced"),
        ("vestrace_create_model_request_evidence", "fenced"),
        ("vestrace_reserve_run_step_execution_attempt", "run-bound"),
        ("vestrace_lock_provider_dispatch_routing", "open"),
        ("vestrace_try_admit_provider_dispatch", "open"),
        ("vestrace_lock_embedding_job_recovery_authority", "carry"),
        ("vestrace_lock_run_step_attempt_recovery_authority", "carry"),
    ]);
    assert_eq!(caller_dispositions.len(), 8);
    for (name, disposition) in caller_dispositions {
        let body = functions
            .get(name)
            .unwrap_or_else(|| panic!("{name} is absent from the live migration set"));
        match disposition {
            "creator" => {
                assert!(body.contains("target_workspace_id, target_snapshot_id, 'ordinary', NULL"))
            }
            "fenced" => assert!(body.contains("vestrace_snapshot_scope")),
            "run-bound" => assert!(body.contains("run_model_binding_snapshots")),
            "open" => assert!(!body.contains("vestrace_snapshot_scope")),
            "carry" => assert!(body.contains("model_binding_snapshot_id")),
            _ => unreachable!(),
        }
    }
}

async fn plan_counts(pool: &PgPool, workspace_id: Uuid) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM embedding_transition_plans WHERE workspace_id=$1), \
                (SELECT count(*) FROM embedding_transition_plan_recipes WHERE workspace_id=$1), \
                (SELECT count(*) FROM model_binding_snapshot_scopes WHERE workspace_id=$1), \
                (SELECT count(*) FROM model_binding_snapshots WHERE workspace_id=$1)",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn planning_conflicts_atomically_and_is_workspace_bound(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let transition = Uuid::now_v7();
    let plan_id = Uuid::now_v7();
    let recipes = vec![Uuid::now_v7()];
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: transition,
            plan_id,
            version: 1,
            snapshot_id: Uuid::now_v7(),
            recipes: recipes.clone(),
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();

    let conflict = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: transition,
            plan_id,
            version: 1,
            snapshot_id: Uuid::now_v7(),
            recipes,
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .expect_err("the same plan identity with another tuple is a conflict");
    assert_eq!(
        database_refusal(&conflict),
        (
            "40001".to_owned(),
            "EMBEDDING_TRANSITION_PLAN_IDENTITY_CONFLICT".to_owned()
        )
    );

    let before = plan_counts(&pool, fixture.context.workspace_id.as_uuid()).await;
    let refused = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: Uuid::now_v7(),
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: Uuid::now_v7(),
            recipes: vec![],
            inputs: serde_json::json!([]),
        },
    )
    .await
    .expect_err("a refusal must not leave a half-written plan or snapshot");
    assert_eq!(
        database_refusal(&refused),
        (
            "23514".to_owned(),
            "embedding transition plan requires at least one recipe".to_owned()
        )
    );
    assert_eq!(
        plan_counts(&pool, fixture.context.workspace_id.as_uuid()).await,
        before
    );

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let crossed = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_plan_embedding_transition_version(\
          $1,$2,$3,1,$4,$5,'no_auth',NULL,$6,$4,$5,$7,$8,$9,'no_auth',\
          NULL,NULL,NULL,NULL,$6,$10,$11,$12,$13::uuid[],$14::jsonb)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.no_auth_binding_id)
    .bind(fixture.connection_qualification_id)
    .bind(fixture.model_revision_id)
    .bind(fixture.model_qualification_id)
    .bind(fixture.space_registration_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(vec![Uuid::now_v7()])
    .bind(sqlx::types::Json(serde_json::json!([[0]])))
    .fetch_one(&mut *transaction)
    .await
    .expect_err("a caller cannot plan in another workspace");
    assert_eq!(database_refusal(&crossed).0, "22023");
    transaction.rollback().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn successor_recipes_are_exactly_the_predecessor_identities_in_order(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let predecessor = vec![Uuid::now_v7(), Uuid::now_v7()];

    let accepted_transition = Uuid::now_v7();
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: accepted_transition,
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: Uuid::now_v7(),
            recipes: predecessor.clone(),
            inputs: serde_json::json!([[0], [0, 1]]),
        },
    )
    .await
    .unwrap();
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: accepted_transition,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            recipes: predecessor.clone(),
            inputs: serde_json::json!([[0], [0, 1]]),
        },
    )
    .await
    .expect("an exact ordered successor is accepted");

    for (candidate, inputs) in [
        (
            vec![predecessor[1], predecessor[0]],
            serde_json::json!([[0, 1], [0]]),
        ),
        (vec![predecessor[0]], serde_json::json!([[0]])),
        (
            vec![predecessor[0], predecessor[1], Uuid::now_v7()],
            serde_json::json!([[0], [0, 1], [0]]),
        ),
        (
            vec![predecessor[0], Uuid::now_v7()],
            serde_json::json!([[0], [0, 1]]),
        ),
        (vec![], serde_json::json!([])),
    ] {
        let transition = Uuid::now_v7();
        plan(
            &runtime,
            &fixture,
            PlanCall {
                transition_id: transition,
                plan_id: Uuid::now_v7(),
                version: 1,
                snapshot_id: Uuid::now_v7(),
                recipes: predecessor.clone(),
                inputs: serde_json::json!([[0], [0, 1]]),
            },
        )
        .await
        .unwrap();
        let refusal = plan(
            &runtime,
            &fixture,
            PlanCall {
                transition_id: transition,
                plan_id: Uuid::now_v7(),
                version: 2,
                snapshot_id: Uuid::now_v7(),
                recipes: candidate,
                inputs,
            },
        )
        .await
        .expect_err("a reordered, dropped, added, regrouped, or empty successor is refused");
        assert_eq!(database_refusal(&refusal), ("23514".to_owned(), "embedding transition successor recipes must preserve predecessor identities and input ordinals in order".to_owned()));
    }
}

/// Recipe identity alone is not the fixed persisted ordered wire structure.  A caller that moves
/// an input ordinal while retaining the same recipe id must be refused before a
/// successor plan is materialized.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn successor_recipes_preserve_their_ordered_input_ordinals(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let transition_id = Uuid::now_v7();
    let recipes = vec![Uuid::now_v7(), Uuid::now_v7()];

    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: Uuid::now_v7(),
            recipes: recipes.clone(),
            inputs: serde_json::json!([[0, 1], [0]]),
        },
    )
    .await
    .expect("the predecessor plan is valid");

    let refusal = plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id,
            plan_id: Uuid::now_v7(),
            version: 2,
            snapshot_id: Uuid::now_v7(),
            recipes,
            inputs: serde_json::json!([[0], [0, 1]]),
        },
    )
    .await
    .expect_err("a successor may not reuse recipe identities with regrouped inputs");
    assert_eq!(
        database_refusal(&refusal),
        (
            "23514".to_owned(),
            "embedding transition successor recipes must preserve predecessor identities and input ordinals in order".to_owned()
        )
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn ordinary_consumers_refuse_transition_and_scopeless_snapshots(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    make_dispatchable(&pool, &runtime, &fixture).await;

    let mut ordinary = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *ordinary)
        .await
        .unwrap();
    assert_eq!(
        replay_embedding_evidence(&mut ordinary, &fixture, fixture.snapshot_id)
            .await
            .expect("an ordinary snapshot remains usable by evidence creation"),
        fixture.evidence_id
    );
    ordinary.commit().await.unwrap();

    let transition_snapshot = Uuid::now_v7();
    plan(
        &runtime,
        &fixture,
        PlanCall {
            transition_id: Uuid::now_v7(),
            plan_id: Uuid::now_v7(),
            version: 1,
            snapshot_id: transition_snapshot,
            recipes: vec![Uuid::now_v7()],
            inputs: serde_json::json!([[0]]),
        },
    )
    .await
    .unwrap();

    let mut transition = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transition)
        .await
        .unwrap();
    let refused = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,NULL,NULL::BIGINT)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(transition_snapshot)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .fetch_one(&mut *transition)
    .await
    .expect_err("ordinary embedding acceptance cannot attach a transition snapshot");
    assert_eq!(
        database_refusal(&refused),
        (
            "23514".to_owned(),
            "embedding job requires an ordinary binding snapshot".to_owned()
        )
    );
    transition.rollback().await.unwrap();

    let mut transition_evidence = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transition_evidence)
        .await
        .unwrap();
    let refused =
        replay_embedding_evidence(&mut transition_evidence, &fixture, transition_snapshot)
            .await
            .expect_err("model request evidence cannot attach a transition snapshot");
    assert_eq!(
        database_refusal(&refused),
        (
            "23514".to_owned(),
            "model request evidence requires an ordinary binding snapshot".to_owned()
        )
    );
    transition_evidence.rollback().await.unwrap();

    let mut scopeless = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *scopeless)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *scopeless)
        .await
        .unwrap();
    let no_scope = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,\
         connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,\
          branch,no_auth_binding_revision_id) SELECT $1,source_row.workspace_id,source_row.connection_id,source_row.connection_revision_id,source_row.connection_qualification_revision_id,\
                source_row.model_revision_id,source_row.model_qualification_revision_id,source_row.branch,source_row.no_auth_binding_revision_id FROM model_binding_snapshots source_row WHERE source_row.workspace_id=$2 AND source_row.id=$3",
    ).bind(no_scope).bind(fixture.context.workspace_id.as_uuid()).bind(fixture.snapshot_id)
     .execute(&mut *scopeless).await.unwrap();
    let refused = replay_embedding_evidence(&mut scopeless, &fixture, no_scope)
        .await
        .expect_err("model request evidence fails closed for a scopeless snapshot");
    assert_eq!(
        database_refusal(&refused),
        (
            "23514".to_owned(),
            "model request evidence requires an ordinary binding snapshot".to_owned()
        )
    );
    scopeless.rollback().await.unwrap();

    let mut scopeless_acceptance = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *scopeless_acceptance)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *scopeless_acceptance)
        .await
        .unwrap();
    let no_scope = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,\
         connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,\
          branch,no_auth_binding_revision_id) SELECT $1,source_row.workspace_id,source_row.connection_id,source_row.connection_revision_id,source_row.connection_qualification_revision_id,\
                source_row.model_revision_id,source_row.model_qualification_revision_id,source_row.branch,source_row.no_auth_binding_revision_id FROM model_binding_snapshots source_row WHERE source_row.workspace_id=$2 AND source_row.id=$3",
    )
    .bind(no_scope)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.snapshot_id)
    .execute(&mut *scopeless_acceptance)
    .await
    .unwrap();
    let refused = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,NULL,NULL::BIGINT)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.space_registration_id)
    .bind(no_scope)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .fetch_one(&mut *scopeless_acceptance)
    .await
    .expect_err("a snapshot with no positive scope is fail-closed");
    assert_eq!(
        database_refusal(&refused),
        (
            "23514".to_owned(),
            "embedding job requires an ordinary binding snapshot".to_owned()
        )
    );
    scopeless_acceptance.rollback().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn snapshot_scope_is_deferred_to_commit_and_can_be_completed_in_the_same_transaction(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    for complete_scope in [false, true] {
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
        let snapshot = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,\
             connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,\
              branch,no_auth_binding_revision_id) SELECT $1,source_row.workspace_id,source_row.connection_id,source_row.connection_revision_id,source_row.connection_qualification_revision_id,\
                    source_row.model_revision_id,source_row.model_qualification_revision_id,source_row.branch,source_row.no_auth_binding_revision_id FROM model_binding_snapshots source_row WHERE source_row.workspace_id=$2 AND source_row.id=$3",
        ).bind(snapshot).bind(fixture.context.workspace_id.as_uuid()).bind(fixture.snapshot_id)
         .execute(&mut *transaction).await.unwrap();
        if complete_scope {
            sqlx::query("INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)")
                .bind(fixture.context.workspace_id.as_uuid()).bind(snapshot).execute(&mut *transaction).await.unwrap();
            transaction
                .commit()
                .await
                .expect("inserting snapshot then scope must commit");
        } else {
            let refusal = transaction
                .commit()
                .await
                .expect_err("the deferred invariant fires at COMMIT, not INSERT");
            assert_eq!(
                database_refusal(&refusal),
                (
                    "23514".to_owned(),
                    "model binding snapshot requires a durable scope".to_owned()
                )
            );
        }
    }
}
