use tracing::info;
use vestrace_infrastructure::AppConfig;

pub fn run(_config: &AppConfig) -> anyhow::Result<()> {
    info!("Starting Vestrace memory extraction background worker...");
    info!("Worker loop initialized successfully.");
    Ok(())
}
