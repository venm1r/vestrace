use sqlx::Row;
use vestrace_application::{ApplicationError, HealthRepository};

use super::PgStore;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[async_trait::async_trait]
impl HealthRepository for PgStore {
    async fn check(&self) -> Result<(), ApplicationError> {
        match migrations_are_compatible(self).await {
            Ok(true) => Ok(()),
            Ok(false) | Err(_) => Err(ApplicationError::Unavailable(
                "database is not ready".to_owned(),
            )),
        }
    }
}

async fn migrations_are_compatible(store: &PgStore) -> Result<bool, sqlx::Error> {
    let applied =
        sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&store.pool)
            .await?;
    let expected: Vec<_> = MIGRATOR.iter().collect();

    if applied.len() != expected.len() {
        return Ok(false);
    }

    for (row, migration) in applied.iter().zip(expected) {
        let version: i64 = row.try_get("version")?;
        let success: bool = row.try_get("success")?;
        let checksum: Vec<u8> = row.try_get("checksum")?;

        if version != migration.version
            || !success
            || checksum.as_slice() != migration.checksum.as_ref()
        {
            return Ok(false);
        }
    }

    Ok(true)
}
