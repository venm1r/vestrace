//! Provisioner-backed P05 supervisor ACL qualification.

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
async fn supervisor_has_only_guarded_functions_and_no_safety_table_privilege() {
    let Some(pool) = qualification_pool().await else {
        return;
    };
    let flags: (bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT rolsuper, rolbypassrls, rolcreaterole, rolcreatedb, rolreplication \
         FROM pg_roles WHERE rolname = 'vestrace_safety_supervisor'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(flags, (false, false, false, false, false));

    let guarded_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
         WHERE n.nspname = 'public' \
           AND p.proname IN ('vestrace_initialize_installation_safety', 'vestrace_register_database_generation') \
           AND has_function_privilege('vestrace_safety_supervisor', p.oid, 'EXECUTE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(guarded_count, 2);

    for table in [
        "installation_safety_state",
        "installation_safety_generations",
        "installation_safety_journal_events",
    ] {
        let has_dml: bool = sqlx::query_scalar(
            "SELECT has_table_privilege('vestrace_safety_supervisor', $1::regclass, 'SELECT,INSERT,UPDATE,DELETE')",
        )
        .bind(format!("public.{table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!has_dml, "supervisor has direct DML on {table}");
    }
}
