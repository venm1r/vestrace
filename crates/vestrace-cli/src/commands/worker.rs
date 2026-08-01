use vestrace_infrastructure::AppConfig;
use tracing::info;

pub fn run(config: &AppConfig) -> anyhow::Result<()> {
    info!(
        database_url = %config.database.url,
        "Starting Vestrace memory extraction background worker..."
    );
    info!("Worker loop initialized successfully.");
    Ok(())
}
