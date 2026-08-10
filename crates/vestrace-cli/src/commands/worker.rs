use std::sync::Arc;
use std::time::Duration;

use tokio::signal;
use vestrace_application::run::{
    AdvanceRunHandler, ExecuteStepHandler, ResumeRunHandler, RunWorkHandlerRegistry, RunWorker,
    RunWorkerConfig, SystemClock,
};
use vestrace_application::{RequestContext, Worker};
use vestrace_domain::id::{PrincipalId, WorkerId, WorkspaceId};
use vestrace_infrastructure::{
    AppConfig, PgJobRepository, PgRunLeasePort, PgStore, PgWorkQueuePort, PostgresRunStore,
};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow::anyhow!("database is unavailable"))?;

    store
        .migrate()
        .await
        .map_err(|_| anyhow::anyhow!("database migrations are unavailable"))?;

    let job_repo = PgJobRepository::new(store.pool().clone());
    let job_worker = Worker::new(job_repo);

    let run_store = Arc::new(PostgresRunStore::new(&store));
    let lease_port = Arc::new(PgRunLeasePort::new(&store));
    let queue_port = Arc::new(PgWorkQueuePort::new(&store));
    let clock = Arc::new(SystemClock::new());

    let run_worker_config = RunWorkerConfig {
        worker_id: WorkerId::new(),
        poll_interval: Duration::from_millis(500),
        lease_ttl: Duration::from_secs(60),
        heartbeat_interval: Duration::from_secs(20),
        max_concurrency: 1,
    };

    let mut registry = RunWorkHandlerRegistry::new();
    registry.register(Arc::new(AdvanceRunHandler::new(
        run_store.clone(),
        queue_port.clone(),
        clock.clone(),
    )));
    registry.register(Arc::new(ResumeRunHandler::new(
        run_store.clone(),
        clock.clone(),
    )));
    registry.register(Arc::new(ExecuteStepHandler::new(
        run_store.clone(),
        clock.clone(),
    )));

    let run_worker = Arc::new(RunWorker::new(
        run_worker_config,
        run_store,
        lease_port,
        queue_port,
        clock,
        registry,
    )?);

    let run_context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

    tracing::info!("worker started, polling for jobs and run work items");

    let shutdown_notify = Arc::new(tokio::sync::Notify::new());
    let shutdown_clone = shutdown_notify.clone();

    tokio::spawn(async move {
        shutdown_signal().await;
        shutdown_clone.notify_waiters();
    });

    loop {
        tokio::select! {
            result = job_worker.process_one() => {
                let processed = result?;
                if !processed {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
            result = run_worker.run_once(&run_context) => {
                match result {
                    Ok(true) => {}
                    Ok(false) => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "run worker iteration failed");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
            _ = shutdown_notify.notified() => {
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
