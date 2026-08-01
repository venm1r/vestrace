use std::time::{Duration, Instant};
use vestrace_application::{HealthRepository, RequestContext, TransactionManager, UnitOfWork};
use vestrace_domain::{PrincipalId, WorkspaceId};

use secrecy::SecretString;
use vestrace_infrastructure::{
    DatabaseConfig, InfrastructureErrorKind, PgStore, PgTransactionManager,
};

async fn single_connection_pool(source: &sqlx::PgPool) -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(source.connect_options().as_ref().clone())
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn scoped_transaction_sets_database_context(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let mut tx = store.begin_scoped(&ctx).await.unwrap();
    let workspace: String = sqlx::query_scalar("SELECT current_setting('vestrace.workspace_id')")
        .fetch_one(tx.connection())
        .await
        .unwrap();

    assert_eq!(workspace, ctx.workspace_id.to_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn scoped_transaction_sets_principal_database_context(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let mut tx = store.begin_scoped(&ctx).await.unwrap();
    let principal: Option<String> =
        sqlx::query_scalar("SELECT current_setting('vestrace.principal_id', true)")
            .fetch_one(tx.connection())
            .await
            .unwrap();

    assert_eq!(principal, Some(ctx.principal_id.to_string()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn committed_transaction_scope_does_not_leak_on_connection_reuse(pool: sqlx::PgPool) {
    let pool = single_connection_pool(&pool).await;
    let store = PgStore::from_pool(pool.clone());
    let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let mut tx = store.begin_scoped(&ctx).await.unwrap();
    let scoped_backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(tx.connection())
        .await
        .unwrap();

    tx.commit().await.unwrap();

    let mut connection = pool.acquire().await.unwrap();
    let reused_backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let workspace: Option<String> =
        sqlx::query_scalar("SELECT NULLIF(current_setting('vestrace.workspace_id', true), '')")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
    let principal: Option<String> =
        sqlx::query_scalar("SELECT NULLIF(current_setting('vestrace.principal_id', true), '')")
            .fetch_one(&mut *connection)
            .await
            .unwrap();

    assert_eq!(reused_backend, scoped_backend);
    assert_eq!(workspace, None);
    assert_eq!(principal, None);

    drop(connection);
    pool.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn rolled_back_transaction_scope_does_not_leak_on_connection_reuse(pool: sqlx::PgPool) {
    let pool = single_connection_pool(&pool).await;
    let store = PgStore::from_pool(pool.clone());
    let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let mut tx = store.begin_scoped(&ctx).await.unwrap();
    let scoped_backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(tx.connection())
        .await
        .unwrap();

    tx.rollback().await.unwrap();

    let mut connection = pool.acquire().await.unwrap();
    let reused_backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let workspace: Option<String> =
        sqlx::query_scalar("SELECT NULLIF(current_setting('vestrace.workspace_id', true), '')")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
    let principal: Option<String> =
        sqlx::query_scalar("SELECT NULLIF(current_setting('vestrace.principal_id', true), '')")
            .fetch_one(&mut *connection)
            .await
            .unwrap();

    assert_eq!(reused_backend, scoped_backend);
    assert_eq!(workspace, None);
    assert_eq!(principal, None);

    drop(connection);
    pool.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn application_transaction_manager_commits_through_object_safe_ports(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let manager: Box<dyn TransactionManager> = Box::new(PgTransactionManager::new(store));
    let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

    let unit_of_work: Box<dyn UnitOfWork> = manager.begin(&ctx).await.unwrap();
    unit_of_work.commit().await.unwrap();
}

#[sqlx::test]
async fn store_migrate_applies_embedded_migrations(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool.clone());

    store.migrate().await.unwrap();

    let table_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name IN \
         ('workspaces', 'principals', 'roles', 'capabilities', \
          'principal_roles', 'role_capabilities')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let extensions: Vec<String> = sqlx::query_scalar(
        "SELECT extname FROM pg_extension WHERE extname IN ('vector', 'pg_trgm') ORDER BY extname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(table_count, 6);
    assert_eq!(extensions, vec!["pg_trgm", "vector"]);
}

#[tokio::test]
async fn connect_honors_max_connections_and_acquisition_timeout() {
    let config = DatabaseConfig {
        url: SecretString::from(std::env::var("DATABASE_URL").unwrap()),
        max_connections: 1,
    };
    let store = PgStore::connect(&config).await.unwrap();
    let first_context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let second_context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let first_transaction = store.begin_scoped(&first_context).await.unwrap();

    let started = Instant::now();
    let error = match store.begin_scoped(&second_context).await {
        Ok(transaction) => {
            transaction.rollback().await.unwrap();
            panic!("second transaction unexpectedly acquired the only pooled connection");
        }
        Err(error) => error,
    };

    assert_eq!(error.kind(), InfrastructureErrorKind::Database);
    assert!(started.elapsed() < Duration::from_secs(10));

    first_transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn connect_rejects_zero_max_connections() {
    let config = DatabaseConfig {
        url: SecretString::from(std::env::var("DATABASE_URL").unwrap()),
        max_connections: 0,
    };

    let error = match PgStore::connect(&config).await {
        Ok(_) => panic!("zero-sized database pool was unexpectedly accepted"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), InfrastructureErrorKind::Configuration);
}

#[sqlx::test(migrations = "../../migrations")]
async fn health_check_accepts_reachable_database_with_compatible_migrations(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);

    assert!(HealthRepository::check(&store).await.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn health_check_rejects_migration_checksum_mismatch(pool: sqlx::PgPool) {
    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let store = PgStore::from_pool(pool);

    let error = HealthRepository::check(&store).await.unwrap_err();

    assert!(matches!(
        error,
        vestrace_application::ApplicationError::Unavailable(_)
    ));
    assert!(!error.to_string().contains("checksum"));
    assert!(!error.to_string().contains("_sqlx_migrations"));
}

async fn assert_health_is_opaquely_unavailable(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let error = HealthRepository::check(&store).await.unwrap_err();

    assert!(matches!(
        error,
        vestrace_application::ApplicationError::Unavailable(ref message)
            if message == "database is not ready"
    ));
    let message = error.to_string();
    assert_eq!(message, "unavailable: database is not ready");
    assert!(!message.contains("_sqlx_migrations"));
    assert!(!message.contains("checksum"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn health_check_rejects_missing_migration(pool: sqlx::PgPool) {
    sqlx::query(
        "DELETE FROM _sqlx_migrations \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn health_check_rejects_unsuccessful_migration(pool: sqlx::PgPool) {
    sqlx::query(
        "UPDATE _sqlx_migrations SET success = false \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn health_check_rejects_extra_migration(pool: sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO _sqlx_migrations \
         (version, description, success, checksum, execution_time) \
         VALUES (9999, 'unrecognized migration', true, decode('00', 'hex'), 0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(pool).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn health_check_rejects_same_count_version_mismatch(pool: sqlx::PgPool) {
    sqlx::query(
        "UPDATE _sqlx_migrations SET version = -1 \
         WHERE version = (SELECT min(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(pool).await;
}
