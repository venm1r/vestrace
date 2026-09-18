use std::str::FromStr;

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const P04_GUARDED_TABLES: [&str; 12] = [
    "embedding_space_registrations",
    "embedding_corpus_generations",
    "embedding_jobs",
    "embedding_transitions",
    "embedding_transition_plans",
    "embedding_transition_plan_recipes",
    "model_binding_snapshot_scopes",
    "embedding_transition_ambiguity_carries",
    "embedding_transition_ambiguity_carry_recipes",
    "embedding_transition_barriers",
    "embedding_transition_barrier_recipes",
    "embedding_corpus_generation_members",
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

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn governed_run_step_input_runtime_direct_attempt_dml_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let has_insert_privilege: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('vestrace', 'public.run_step_execution_attempts', 'INSERT')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !has_insert_privilege,
        "the direct write refusal must be caused by an absent runtime INSERT privilege"
    );
    let refusal = sqlx::query(
        "INSERT INTO run_step_execution_attempts (
            id, workspace_id, run_id, step_id, model_binding_snapshot_id,
            input_material_intent_id, input_content_material_id, input_material_key_id,
            input_intent_nonce, input_prepared_attachment_id, external_effect_id,
            model_request_evidence_id
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await
    .expect_err("the runtime must not insert governed Run-step attempts directly");
    eprintln!("runtime direct attempt DML refusal: {refusal}");
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("42501")
    );
    runtime.close().await;
}

/// Every table the guarded owner holds must refuse runtime DML, including one
/// nobody remembered to declare.
///
/// The two matrices that already exist name their tables: 26 P02 names in
/// `runtime_role_cannot_write_directly` and 38 P03 names beside them. Both
/// prove that the tables they name are refused, and neither can notice a table
/// that exists and is in no list — `every_p03_table_is_guarded_forced_rls_and_has_a_nonempty_acl`
/// filters the catalog through the same declaration, so a guarded table added
/// by a later migration and forgotten would escape the ACL assertion and the
/// refusal matrix together.
///
/// This one declares nothing. It asks the catalog which tables the guarded
/// owner holds and refuses all of them, so the cost of forgetting is a red
/// test rather than a silent write path.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn every_guarded_owner_table_refuses_runtime_dml_including_undeclared_ones(pool: PgPool) {
    let tables: Vec<(String, String)> = sqlx::query_as(
        "
        SELECT class.relname, attribute.attname
          FROM pg_class AS class
          JOIN pg_namespace AS namespace ON namespace.oid = class.relnamespace
          JOIN pg_attribute AS attribute
            ON attribute.attrelid = class.oid
           AND attribute.attnum = 1
           AND NOT attribute.attisdropped
         WHERE namespace.nspname = 'public'
           AND class.relkind = 'r'
           AND pg_get_userbyid(class.relowner) = 'vestrace_guarded_owner'
         ORDER BY class.relname
        ",
    )
    .fetch_all(&pool)
    .await
    .expect("the guarded owner's table set must be readable from the catalog");

    // 26 P02 names plus 38 P03 names are declared elsewhere. Fewer than that
    // means this query stopped finding what it is meant to police, which would
    // make the loop below vacuous.
    assert!(
        tables.len() >= 64,
        "expected at least the 64 declared guarded tables, found {}: {:?}",
        tables.len(),
        tables.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    for expected in P04_GUARDED_TABLES {
        assert!(
            tables.iter().any(|(table, _)| table == expected),
            "the catalog-derived guarded table set omitted P04 table {expected}: {:?}",
            tables.iter().map(|(table, _)| table).collect::<Vec<_>>()
        );
    }

    let runtime = runtime_pool(&pool).await;
    for (table, first_column) in &tables {
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
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert!(
                !has_privilege,
                "{verb} on {table} must be refused because the runtime role lacks that table privilege"
            );
            let refusal = sqlx::query(&statement)
                .execute(&runtime)
                .await
                .expect_err(&format!(
                    "the runtime role unexpectedly executed {verb} on guarded table {table}"
                ));
            let database = refusal
                .as_database_error()
                .unwrap_or_else(|| panic!("{verb} on {table} returned a non-database error"));
            assert_eq!(
                database.code().as_deref(),
                Some("42501"),
                "{verb} on {table} must be refused by the table ACL, got {}: {}",
                database.code().as_deref().unwrap_or("no SQLSTATE"),
                database.message()
            );
        }
    }
    runtime.close().await;
}
