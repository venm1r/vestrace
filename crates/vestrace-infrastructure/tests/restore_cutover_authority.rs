//! Live P05-C restore/cutover authority qualification.

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
async fn restore_attempt_authority_is_force_rls_and_supervisor_only() {
    let Some(pool) = qualification_pool().await else {
        return;
    };
    let guarded_relations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname='public' AND c.relname IN ('managed_restore_attempts','managed_restore_events') \
           AND c.relrowsecurity AND c.relforcerowsecurity",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(guarded_relations, 2);

    let guarded_owners: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         JOIN pg_roles r ON r.oid=c.relowner WHERE n.nspname='public' \
         AND c.relname IN ('managed_restore_attempts','managed_restore_events') \
         AND r.rolname='vestrace_guarded_owner'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(guarded_owners, 2);

    let runtime_dml: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname='public' AND c.relname IN ('managed_restore_attempts','managed_restore_events') \
           AND has_table_privilege('vestrace',c.oid,'INSERT,UPDATE,DELETE'))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!runtime_dml);

    for statement in [
        "INSERT INTO public.managed_restore_events(attempt_id,event_kind) VALUES ('00000000-0000-0000-0000-000000000000','forged')",
        "UPDATE public.managed_restore_attempts SET state='released'",
        "DELETE FROM public.managed_restore_events",
    ] {
        let error = sqlx::query(statement).execute(&pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|error| error.code()),
            Some("42501".into()),
            "raw runtime DML must be rejected: {statement}"
        );
    }

    let append_only_trigger: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_restore_events'::regclass \
         AND tgname='managed_restore_events_append_only' AND NOT tgisinternal)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(append_only_trigger);

    for procedure in [
        "vestrace_prepare_restore_attempt",
        "vestrace_record_source_freeze",
        "vestrace_record_target_initialized",
        "vestrace_record_source_resume_prepared",
        "vestrace_record_restore_safety_event",
        "vestrace_list_restore_archive_objects",
        "vestrace_release_restore_hold",
    ] {
        let executable: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace \
             WHERE n.nspname='public' AND p.proname=$1 \
               AND has_function_privilege('vestrace_safety_supervisor',p.oid,'EXECUTE') \
               AND NOT has_function_privilege('vestrace',p.oid,'EXECUTE'))",
        )
        .bind(procedure)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(executable, "{procedure} must be supervisor-only");
    }
}
