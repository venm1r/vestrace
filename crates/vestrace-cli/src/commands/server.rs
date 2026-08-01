use std::sync::Arc;

use anyhow::anyhow;
use tracing_subscriber::EnvFilter;
use vestrace_http::{AppState, build_router};
use vestrace_infrastructure::{AppConfig, LogFormat, ObservabilityConfig, PgStore};

pub async fn run(config: &AppConfig) -> anyhow::Result<()> {
    init_tracing(&config.observability)?;
    tracing::info!(bind = %config.http.bind, "starting vestrace server");

    let store = PgStore::connect(&config.database)
        .await
        .map_err(|_| anyhow!("database is unavailable"))?;
    store
        .migrate()
        .await
        .map_err(|_| anyhow!("database migrations are unavailable"))?;

    let router = build_router(AppState::new(Arc::new(store)));
    let listener = tokio::net::TcpListener::bind(config.http.bind)
        .await
        .map_err(|_| anyhow!("HTTP listener is unavailable"))?;
    axum::serve(listener, router)
        .await
        .map_err(|_| anyhow!("HTTP server stopped unexpectedly"))
}

fn init_tracing(config: &ObservabilityConfig) -> anyhow::Result<()> {
    let filter = EnvFilter::try_new(&config.log_filter)
        .map_err(|_| anyhow!("invalid observability log filter"))?;
    let result = match config.format {
        LogFormat::Text => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init(),
        LogFormat::Json => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .json()
            .try_init(),
    };

    result.map_err(|_| anyhow!("failed to initialize tracing"))
}
