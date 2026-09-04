//! PostgreSQL contract tests for the guarded q1 target/lifecycle authority.

use std::{str::FromStr, time::Duration};

use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_lifecycle_is_forward_migrated_and_runtime_has_only_execute(pool: PgPool) {
    let target_columns: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM information_schema.columns
          WHERE table_schema = 'public'
            AND table_name = 'qualification_target_bindings'
            AND column_name IN ('chat_model_revision_id', 'embedding_model_revision_id')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(target_columns, 2);

    for signature in [
        "vestrace_request_qualification_job(uuid,uuid,uuid,uuid,uuid,text,text,uuid,uuid,uuid,bigint,uuid,uuid,uuid)",
        "vestrace_record_qualification_probe_result(uuid,uuid,uuid,text,text,uuid,uuid)",
        "vestrace_cancel_qualification_job(uuid,uuid)",
        "vestrace_recover_qualification_dispatch_unknown(uuid,uuid,text)",
        "vestrace_finalize_qualification_job(uuid,uuid,uuid,uuid,uuid)",
        "vestrace_create_qualification_q1_mre_source(uuid,uuid,text,text,text,boolean,text,boolean,boolean,text)",
        "vestrace_lock_provider_dispatch_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid)",
    ] {
        let row = sqlx::query(
            "SELECT pg_get_userbyid(proowner) AS owner,
                    has_function_privilege('vestrace', to_regprocedure($1), 'EXECUTE') AS executable,
                    EXISTS (
                        SELECT 1 FROM aclexplode(proacl)
                         WHERE grantee = 0 AND privilege_type = 'EXECUTE'
                    ) AS public_executable
               FROM pg_proc WHERE oid = to_regprocedure($1)",
        )
        .bind(signature)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("owner"), "vestrace_guarded_owner");
        assert!(row.get::<bool, _>("executable"));
        assert!(!row.get::<bool, _>("public_executable"));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_cannot_bypass_the_guarded_qualification_tables(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let error = sqlx::query(
        "INSERT INTO qualification_jobs
             (id, workspace_id, connection_revision_id, profile_revision, state)
         VALUES ($1,$2,$3,'openai-chat-completions-v1/q1','requested')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await
    .expect_err("runtime must not insert a qualification job directly");
    let database = error
        .as_database_error()
        .expect("direct runtime DML must return a database error");
    assert_eq!(database.code().as_deref(), Some("42501"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn q1_mre_source_is_closed_and_runtime_cannot_write_it_directly(pool: PgPool) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
           FROM information_schema.columns
          WHERE table_schema='public'
            AND table_name='qualification_q1_mre_sources'
          ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        [
            "evidence_root_id",
            "workspace_id",
            "probe_ordinal",
            "message_layout",
            "tool_choice",
            "parallel_tool_calls",
            "response_format",
            "stream",
            "stream_include_usage",
            "assistant_tool_call_id",
            "created_at",
        ]
    );

    let runtime = runtime_pool(&pool).await;
    let error = sqlx::query(
        "INSERT INTO qualification_q1_mre_sources(
             evidence_root_id,workspace_id,probe_ordinal,message_layout,tool_choice,
             parallel_tool_calls,response_format,stream,stream_include_usage
         ) VALUES ($1,$2,'20','plain_text','none',false,'none',false,false)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await
    .expect_err("runtime must not write q1 MRE source directly");
    assert_eq!(
        error
            .as_database_error()
            .expect("direct q1 source DML must be a database error")
            .code()
            .as_deref(),
        Some("42501")
    );
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("frozen runtime database URL must be available for RLS tests");
    PgConnectOptions::from_str(&url).expect("runtime URL must be a PostgreSQL URL");
    let authority = url
        .split_once("://")
        .expect("runtime URL must include a scheme")
        .1;
    let credentials = authority
        .rsplit_once('@')
        .expect("runtime URL must include credentials")
        .0;
    let (username, password) = credentials
        .split_once(':')
        .expect("runtime URL must include a password");
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(username)
                .password(password),
        )
        .await
        .expect("restricted runtime role must connect to the SQLx test database")
}
