use std::str::FromStr;

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{ProviderResultFinalizer, ProviderResultRepository};
use vestrace_infrastructure::PgProviderResultRepository;

#[test]
fn provider_result_public_api_is_transaction_bound_and_concrete() {
    fn assert_repository<T: ProviderResultRepository>() {}
    assert_repository::<PgProviderResultRepository>();
    let _: Option<ProviderResultFinalizer<PgProviderResultRepository>> = None;
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be PostgreSQL");
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");
    PgPoolOptions::new()
        .max_connections(2)
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

fn assert_sqlstate<T>(result: Result<T, sqlx::Error>, expected: &str, operation: &str) {
    let error = match result {
        Ok(_) => panic!("{operation} unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some(expected),
        "{operation}: {error}"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn forward_provider_result_entrypoints_refuse_absent_normalized_tuples(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let workspace_id = Uuid::now_v7();
    let ids = (0..7).map(|_| Uuid::now_v7()).collect::<Vec<_>>();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let mut framed = vec![0_u8; 4096];
    framed[..5].copy_from_slice(b"VMRF\x01");
    let prepare = sqlx::query("SELECT vestrace_prepare_provider_result($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(ids[0])
        .bind(ids[1])
        .bind(ids[2])
        .bind(ids[3])
        .bind(ids[4])
        .bind(ids[5])
        .bind(framed)
        .bind(4096_i64)
        .fetch_all(&mut *transaction)
        .await;
    assert_sqlstate(prepare, "23514", "prepare an absent normalized tuple");
    transaction.rollback().await.unwrap();

    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let witness = sqlx::query("SELECT vestrace_witness_provider_result_receipt($1,$2)")
        .bind(ids[6])
        .bind(Uuid::now_v7())
        .fetch_all(&mut *transaction)
        .await;
    assert_sqlstate(
        witness,
        "23514",
        "witness an absent preparation/receipt pair",
    );
    transaction.rollback().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn governed_artifact_revision_requires_its_exact_content_map_at_commit(pool: PgPool) {
    let workspace_id = Uuid::now_v7();
    let artifact_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace_id)
        .bind(format!("task10-artifact-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO artifacts(id,workspace_id,name) VALUES($1,$2,'provider output')")
        .bind(artifact_id)
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();

    let mut transaction = pool.begin().await.unwrap();
    let insert = sqlx::query(
        "INSERT INTO artifact_revisions(\
             id,artifact_id,workspace_id,revision_number,media_type,\
             content_hash,byte_size,storage_kind\
         ) VALUES($1,$2,$3,1,'text/plain',NULL,NULL,'governed_material')",
    )
    .bind(revision_id)
    .bind(artifact_id)
    .bind(workspace_id)
    .execute(&mut *transaction)
    .await;
    match insert {
        Ok(_) => assert_sqlstate(
            transaction.commit().await,
            "23514",
            "commit a governed revision without its typed content map",
        ),
        Err(error) => assert_sqlstate(
            Err::<(), _>(error),
            "23514",
            "insert a governed revision without its typed content map",
        ),
    }
    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifact_revisions WHERE id=$1")
        .bind(revision_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(persisted, 0);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn task10_replaces_the_old_provider_result_prepare_signature(pool: PgPool) {
    let signatures: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT \
          to_regprocedure('public.vestrace_prepare_provider_result(uuid,uuid,uuid,uuid,uuid,uuid,bytea,bigint)') IS NOT NULL, \
          to_regprocedure('public.vestrace_prepare_provider_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,bytea,bigint)') IS NULL, \
          to_regprocedure('public.vestrace_witness_provider_result_receipt(uuid,uuid)') IS NOT NULL, \
          to_regprocedure('public.vestrace_witness_provider_result_receipt(uuid,uuid,uuid,text,boolean,integer,integer)') IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(signatures, (true, true, true, true));
}
