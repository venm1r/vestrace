use std::time::Duration;

use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;

use crate::{DatabaseConfig, InfrastructureError};

const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct PgStore {
    pub(super) pool: sqlx::PgPool,
}

impl PgStore {
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
        sqlx::migrate!("../../migrations").run(&self.pool).await?;
        Ok(())
    }
}
