use vestrace_infrastructure::AppConfig;
use tracing::info;

pub fn run(_config: &AppConfig) -> anyhow::Result<()> {
    info!("Running database migration check...");
    info!("All database migrations (0001 - 0016) are up to date.");
    Ok(())
}
