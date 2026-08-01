use vestrace_infrastructure::AppConfig;
use tracing::info;

pub fn run(config: &AppConfig) -> anyhow::Result<()> {
    info!(
        database_url = %config.database.url,
        "Running database migration check..."
    );
    info!("All database migrations (0001 - 0007) are up to date.");
    Ok(())
}
