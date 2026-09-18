//! Live P05 safety-authority catalog qualification.

use sqlx::{PgPool, postgres::PgPoolOptions};

async fn qualification_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("VESTRACE_P05_TEST_DATABASE_URL") else {
        eprintln!(
            "BLOCKED: set VESTRACE_P05_TEST_DATABASE_URL to a disposable provisioned P05 database"
        );
        return None;
    };
    Some(
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn installed_authority_is_force_rls_and_runtime_cannot_write_it_directly() {
    let Some(pool) = qualification_pool().await else {
        return;
    };
    let force_rls_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' \
           AND c.relname IN ('installation_safety_state', 'installation_safety_generations', 'installation_safety_journal_events') \
           AND c.relrowsecurity AND c.relforcerowsecurity",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(force_rls_count, 3);

    let runtime_dml: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' \
           AND c.relname IN ('installation_safety_state', 'installation_safety_generations', 'installation_safety_journal_events') \
           AND has_table_privilege('vestrace', c.oid, 'INSERT,UPDATE,DELETE'))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!runtime_dml);

    let verifier_is_guarded: bool = sqlx::query_scalar(
        "SELECT NOT has_function_privilege('vestrace_safety_supervisor', \
         'public.vestrace_safety_ed25519_verify(bytea,bytea,bytea)'::regprocedure, 'EXECUTE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(verifier_is_guarded);
}
