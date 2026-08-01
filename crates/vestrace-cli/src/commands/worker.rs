use vestrace_infrastructure::AppConfig;
use tracing::info;

pub fn run(_config: &AppConfig) -> anyhow::Result<()> {
    info!("Starting Vestrace memory extraction background worker...");
    info!("Worker loop initialized successfully.");
    Ok(())
}
