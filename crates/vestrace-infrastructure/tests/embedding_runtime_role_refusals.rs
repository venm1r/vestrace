use std::str::FromStr;

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const P04_GUARDED_TABLES: [&str; 19] = [
    "embedding_space_registrations",
    "embedding_corpus_generations",
    "embedding_jobs",
    "embedding_job_material_intents",
    "embedding_job_termination_receipts",
    "embedding_transitions",
    "embedding_transition_plans",
    "embedding_transition_plan_recipes",
    "model_binding_snapshot_scopes",
    "embedding_transition_ambiguity_carries",
    "embedding_transition_ambiguity_carry_recipes",
    "embedding_transition_barriers",
    "embedding_transition_barrier_recipes",
    "embedding_corpus_generation_members",
    "embedding_job_credential_completion_blockers",
    "embedding_result_credential_blocker_adoptions",
    "embedding_result_key_binding_receipts",
    "embedding_job_result_publications",
    "embedding_index_rebuild_events",
];

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");

    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .expect("runtime must connect to the SQLx test database")
}

fn assert_exact_check_violation<T>(result: Result<T, sqlx::Error>, message: &str) {
    let error = match result {
        Ok(_) => panic!("R0 RED: the runtime role forged an embedding-space registration"),
        Err(error) => error,
    };
    let database = error
        .as_database_error()
        .unwrap_or_else(|| panic!("R0 registration refusal was not a database error: {error}"));
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert_eq!(database.message(), message);
}

async fn register(
    runtime: &PgPool,
    workspace_id: Uuid,
    space_id: Uuid,
    name: &str,
    model: &str,
    dimensions: i32,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let result = sqlx::query_scalar("SELECT vestrace_register_embedding_space($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(space_id)
        .bind(name)
        .bind(model)
        .bind(dimensions)
        .fetch_one(&mut *transaction)
        .await;
    transaction.rollback().await.unwrap();
    result
}

async fn assert_runtime_dml_is_refused(runtime: &PgPool, owner: &PgPool, table: &str) {
    let first_column: String = sqlx::query_scalar(
        "SELECT attribute.attname
           FROM pg_attribute AS attribute
           JOIN pg_class AS class ON class.oid = attribute.attrelid
           JOIN pg_namespace AS namespace ON namespace.oid = class.relnamespace
          WHERE namespace.nspname = 'public'
            AND class.relname = $1
            AND attribute.attnum > 0
            AND NOT attribute.attisdropped
          ORDER BY attribute.attnum
          LIMIT 1",
    )
    .bind(table)
    .fetch_one(owner)
    .await
    .unwrap();
    for (verb, statement) in [
        (
            "INSERT",
            format!("INSERT INTO public.{table} DEFAULT VALUES"),
        ),
        (
            "UPDATE",
            format!("UPDATE public.{table} SET {first_column} = {first_column}"),
        ),
        ("DELETE", format!("DELETE FROM public.{table}")),
    ] {
        let has_privilege: bool =
            sqlx::query_scalar("SELECT has_table_privilege('vestrace', $1, $2)")
                .bind(format!("public.{table}"))
                .bind(verb)
                .fetch_one(owner)
                .await
                .unwrap();
        assert!(
            !has_privilege,
            "runtime role must lack {verb} table privilege on {table} before its direct DML is attempted"
        );

        let error = sqlx::query(&statement)
            .execute(runtime)
            .await
            .expect_err(&format!(
                "runtime role unexpectedly executed {verb} on {table}"
            ));
        assert_eq!(
            error
                .as_database_error()
                .and_then(|database| database.code())
                .as_deref(),
            Some("42501"),
            "{verb} on {table} must be refused by its absent table privilege: {error}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_registration_requires_a_matching_legacy_embedding_space(pool: PgPool) {
    let workspace_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace_id)
        .bind(format!("r0-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();

    let runtime = runtime_pool(&pool).await;
    let missing_space_id = Uuid::now_v7();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&runtime)
        .await
        .unwrap();

    assert_exact_check_violation(
        register(
            &runtime,
            workspace_id,
            missing_space_id,
            "forged",
            "wire-model",
            2,
        )
        .await,
        "embedding space registration requires a matching legacy embedding space",
    );

    let legacy_space_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO embedding_spaces(id,workspace_id,name,dimensions,model) VALUES($1,$2,'real',2,'wire-model')",
    )
    .bind(legacy_space_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    for (name, model, dimensions) in [
        ("wrong-name", "wire-model", 2),
        ("real", "wrong-model", 2),
        ("real", "wire-model", 3),
    ] {
        assert_exact_check_violation(
            register(
                &runtime,
                workspace_id,
                legacy_space_id,
                name,
                model,
                dimensions,
            )
            .await,
            "embedding space registration requires a matching legacy embedding space",
        );
    }

    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_p04_guarded_table_refuses_runtime_insert_update_and_delete(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    for table in P04_GUARDED_TABLES {
        assert_runtime_dml_is_refused(&runtime, &pool, table).await;
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_cannot_execute_embedding_pre_dispatch_gate_directly(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let gate = "public.vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)";
    let executable: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('vestrace', $1::regprocedure, 'EXECUTE')",
    )
    .bind(gate)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !executable,
        "the routing-only embedding pre-dispatch gate must not be a runtime command"
    );

    let refusal =
        sqlx::query("SELECT public.vestrace_lock_embedding_job_pre_dispatch_gate($1,$2,$3)")
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .bind(false)
            .execute(&runtime)
            .await
            .expect_err("the runtime role must not call the embedding pre-dispatch gate directly");
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("42501"),
        "direct embedding pre-dispatch gate execution must fail before argument validation: {refusal}"
    );
    runtime.close().await;
}
