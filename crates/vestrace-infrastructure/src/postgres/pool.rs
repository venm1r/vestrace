use std::time::Duration;

use secrecy::ExposeSecret;
use sqlx::{Row, postgres::PgPoolOptions};

use crate::{DatabaseConfig, InfrastructureError};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct PgStore {
    pub(crate) pool: sqlx::PgPool,
}

impl PgStore {
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    pub async fn connect(config: &DatabaseConfig) -> Result<Self, InfrastructureError> {
        if config.max_connections == 0 {
            return Err(InfrastructureError::configuration(
                "database.max_connections must be greater than zero".to_owned(),
            ));
        }

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect(config.url.expose_secret())
            .await?;

        Ok(Self::from_pool(pool))
    }

    pub fn from_pool(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    pub async fn migrate(&self) -> Result<(), InfrastructureError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    pub async fn migrations_are_compatible(&self) -> Result<bool, InfrastructureError> {
        let applied =
            sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&self.pool)
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
}
