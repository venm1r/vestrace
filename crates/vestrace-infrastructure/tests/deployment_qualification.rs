//! Deployment qualification against a completely migrated database.
//!
//! These assert properties of a finished deployment: that the applied ledger
//! equals the embedded migrator, and that the health check refuses when it does
//! not. A database bounded at the historical prefix cannot answer either
//! question -- it is already incompatible, so a refusal proves nothing.

use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use vestrace_application::HealthRepository;
use vestrace_infrastructure::PgStore;

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn deployment_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("VESTRACE_P05_TEST_DATABASE_URL") else {
        eprintln!(
            "BLOCKED: set VESTRACE_P05_TEST_DATABASE_URL to a disposable database \
             taken through the three-phase P05 route"
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
async fn health_check_accepts_reachable_database_with_compatible_migrations() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let store = PgStore::from_pool(pool);
    assert!(HealthRepository::check(&store).await.is_ok());
}

#[tokio::test]
async fn health_check_rejects_migration_checksum_mismatch() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let store = PgStore::from_pool(pool.clone());
    assert!(HealthRepository::check(&store).await.is_ok());

    let original_checksum: Vec<u8> = sqlx::query_scalar(
        "SELECT checksum FROM _sqlx_migrations WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let error = HealthRepository::check(&store).await.unwrap_err();

    assert!(matches!(
        error,
        vestrace_application::ApplicationError::Unavailable(_)
    ));
    assert!(!error.to_string().contains("checksum"));
    assert!(!error.to_string().contains("_sqlx_migrations"));

    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = $1 \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .bind(original_checksum)
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn deployment_qualification_reports_runtime_role_and_compatible_migrations() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let expected_role: String = sqlx::query_scalar("SELECT current_user")
        .fetch_one(&pool)
        .await
        .unwrap();
    let store = PgStore::from_pool(pool);

    let evidence = store.deployment_qualification_evidence().await.unwrap();

    assert!(evidence.migration_history_compatible);
    assert_eq!(evidence.runtime_role, expected_role);
    assert!(!evidence.runtime_role.trim().is_empty());
}

#[tokio::test]
async fn deployment_qualification_reports_incompatible_migrations_without_mutating_them() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let original_checksum: Vec<u8> = sqlx::query_scalar(
        "SELECT checksum FROM _sqlx_migrations WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let store = PgStore::from_pool(pool.clone());

    let evidence = store.deployment_qualification_evidence().await.unwrap();
    let checksum: Vec<u8> = sqlx::query_scalar(
        "SELECT checksum FROM _sqlx_migrations WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(!evidence.migration_history_compatible);
    assert_eq!(checksum, vec![0]);

    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = $1 \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .bind(original_checksum)
    .execute(&pool)
    .await
    .unwrap();
}

async fn assert_health_is_opaquely_unavailable(pool: &sqlx::PgPool) {
    let store = PgStore::from_pool(pool.clone());
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

#[tokio::test]
async fn health_check_rejects_missing_migration() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let store = PgStore::from_pool(pool.clone());
    assert!(HealthRepository::check(&store).await.is_ok());

    let row = sqlx::query(
        "SELECT version, description, success, checksum, execution_time \
         FROM _sqlx_migrations WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let version: i64 = row.get("version");
    let description: String = row.get("description");
    let success: bool = row.get("success");
    let checksum: Vec<u8> = row.get("checksum");
    let execution_time: i64 = row.get("execution_time");

    sqlx::query(
        "DELETE FROM _sqlx_migrations \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(&pool).await;

    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(version)
    .bind(description)
    .bind(success)
    .bind(checksum)
    .bind(execution_time)
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn health_check_rejects_unsuccessful_migration() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let store = PgStore::from_pool(pool.clone());
    assert!(HealthRepository::check(&store).await.is_ok());

    sqlx::query(
        "UPDATE _sqlx_migrations SET success = false \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(&pool).await;

    sqlx::query(
        "UPDATE _sqlx_migrations SET success = true \
         WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn health_check_rejects_extra_migration() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let store = PgStore::from_pool(pool.clone());
    assert!(HealthRepository::check(&store).await.is_ok());

    sqlx::query(
        "INSERT INTO _sqlx_migrations \
         (version, description, success, checksum, execution_time) \
         VALUES (9999, 'unrecognized migration', true, decode('00', 'hex'), 0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(&pool).await;

    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 9999")
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn health_check_rejects_same_count_version_mismatch() {
    let _guard = DB_LOCK.lock().await;
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let store = PgStore::from_pool(pool.clone());
    assert!(HealthRepository::check(&store).await.is_ok());

    let min_version: i64 = sqlx::query_scalar("SELECT min(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();

    sqlx::query(
        "UPDATE _sqlx_migrations SET version = -1 \
         WHERE version = (SELECT min(version) FROM _sqlx_migrations)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_health_is_opaquely_unavailable(&pool).await;

    sqlx::query("UPDATE _sqlx_migrations SET version = $1 WHERE version = -1")
        .bind(min_version)
        .execute(&pool)
        .await
        .unwrap();
}
