//! Live P05-B archive authority qualification.

use sqlx::{PgPool, postgres::PgPoolOptions};

async fn qualification_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("VESTRACE_P05_TEST_DATABASE_URL") else {
        eprintln!("BLOCKED: set VESTRACE_P05_TEST_DATABASE_URL to a disposable P05 database");
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
async fn runtime_cannot_insert_head_hold_or_deletion_preparation() {
    let Some(pool) = qualification_pool().await else {
        return;
    };
    let archive_tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relname IN (\
           'managed_backup_archive_heads', 'managed_backup_restore_holds', 'managed_backup_deletion_preparations') \
           AND c.relrowsecurity AND c.relforcerowsecurity",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(archive_tables, 3);

    for statement in [
        "INSERT INTO public.managed_backup_archive_heads(backup_set_id, checkpoint_ordinal, checkpoint_digest) VALUES ('00000000-0000-0000-0000-000000000001', 0, decode(repeat('00', 32), 'hex'))",
        "INSERT INTO public.managed_backup_restore_holds(hold_id, backup_set_id) VALUES ('00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000001')",
        "INSERT INTO public.managed_backup_deletion_preparations(backup_set_id, preparation_digest) VALUES ('00000000-0000-0000-0000-000000000001', decode(repeat('00', 32), 'hex'))",
    ] {
        let error = sqlx::query(statement)
            .execute(&pool)
            .await
            .expect_err("runtime direct archive DML must be refused");
        let database = error
            .as_database_error()
            .expect("direct DML must fail in PostgreSQL");
        assert_eq!(database.code().as_deref(), Some("42501"));
    }
}
