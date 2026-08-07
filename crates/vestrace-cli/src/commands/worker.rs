use std::time::Duration;

use tokio::signal;
use vestrace_application::Worker;
use vestrace_infrastructure::{AppConfig, PgJobRepository, PgStore};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow::anyhow!("database is unavailable"))?;

    store
        .migrate()
        .await
        .map_err(|_| anyhow::anyhow!("database migrations are unavailable"))?;

    let job_repo = PgJobRepository::new(store.pool().clone());
    let worker = Worker::new(job_repo);

    tracing::info!("worker started, polling for jobs");

    loop {
        tokio::select! {
            result = worker.process_one() => {
                let processed = result?;
                if !processed {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
            _ = shutdown_signal() => {
                tracing::info!("worker shutdown requested, draining current work");
                break;
            }
        }
    }

    tracing::info!("worker stopped gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
