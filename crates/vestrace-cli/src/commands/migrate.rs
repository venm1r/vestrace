use anyhow::anyhow;
use tracing::info;
use vestrace_infrastructure::{AppConfig, PgStore};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow!("database is unavailable"))?;

    store
        .migrate()
        .await
        .map_err(|_| anyhow!("database migrations are unavailable"))?;

    match store.migrations_are_compatible().await {
        Ok(true) => {
            info!("database migrations applied and verified");
            Ok(())
        }
        Ok(false) => anyhow::bail!("database migration history is incompatible"),
        Err(_) => Err(anyhow!("database migration verification is unavailable")),
    }
}
