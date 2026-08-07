use std::sync::Arc;

use anyhow::anyhow;
use tokio::signal;
use tracing::Subscriber;
use tracing_subscriber::{
    EnvFilter,
    filter::{self, FilterExt},
    fmt::MakeWriter,
    prelude::*,
};
use vestrace_application::{MemoryService, RetrievalService, RunCommandService, RunService};
use vestrace_http::{AppState, MetricsRegistry, build_router};
use vestrace_infrastructure::{
    AppConfig, LogFormat, ObservabilityConfig, PgAgentRepository, PgEvaluationRepository,
    PgEventRepository, PgExecutionHistoryRepository, PgIdempotencyRepository, PgMemoryRepository,
    PgModelExecutionRepository, PgModelRepository, PgOutboxRepository, PgProvenanceRepository,
    PgProviderRepository, PgRelationRepository, PgRetrievalJournal, PgRoutingDecisionRepository,
    PgRunCommandCommitter, PgRunEventStore, PgRunRepository, PgSkillRepository, PgStore,
    PgTextRetriever, PgWorkflowRepository,
};

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
    match store.migrations_are_compatible().await {
        Ok(true) => {}
        Ok(false) => return Err(anyhow!("database migration history is incompatible")),
        Err(_) => return Err(anyhow!("database migration verification is unavailable")),
    }

    let run_repository = Arc::new(PgRunRepository::new(store.clone()));
    let run_service = Arc::new(RunService::new(run_repository));
    let run_command_service = Arc::new(RunCommandService::new(
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunCommandCommitter::new(store.clone())),
    ));

    let pool = store.pool().clone();
    let memory_service = MemoryService::new(
        PgEventRepository::new(pool.clone()),
        PgMemoryRepository::new(pool.clone()),
        PgProvenanceRepository::new(pool.clone()),
        PgRelationRepository::new(pool.clone()),
        PgOutboxRepository::new(pool.clone()),
        PgIdempotencyRepository::new(pool.clone()),
    );
    let memory_use_cases: vestrace_application::SharedMemoryUseCases = Arc::new(memory_service);

    let text_retriever: vestrace_application::SharedTextRetriever =
        Arc::new(PgTextRetriever::new(pool.clone()));
    let retrieval_journal: vestrace_application::SharedRetrievalJournal =
        Arc::new(PgRetrievalJournal::new(pool.clone()));
    let retrieval_service = Arc::new(RetrievalService::new(text_retriever, retrieval_journal));

    let provider_repository: vestrace_application::SharedProviderRepository =
        Arc::new(PgProviderRepository::new(pool.clone()));
    let model_repository: vestrace_application::SharedModelRepository =
        Arc::new(PgModelRepository::new(pool.clone()));
    let agent_repository: vestrace_application::SharedAgentRepository =
        Arc::new(PgAgentRepository::new(pool.clone()));
    let skill_repository: vestrace_application::SharedSkillRepository =
        Arc::new(PgSkillRepository::new(pool.clone()));
    let routing_decision_repository: vestrace_application::SharedRoutingDecisionRepository =
        Arc::new(PgRoutingDecisionRepository::new(pool.clone()));
    let model_execution_repository: vestrace_application::SharedModelExecutionRepository =
        Arc::new(PgModelExecutionRepository::new(pool.clone()));
    let execution_history_repository: vestrace_application::SharedExecutionHistoryRepository =
        Arc::new(PgExecutionHistoryRepository::new(pool.clone()));
    let workflow_repository: vestrace_application::SharedWorkflowRepository =
        Arc::new(PgWorkflowRepository::new(pool.clone()));
    let evaluation_repository: vestrace_application::SharedEvaluationRepository =
        Arc::new(PgEvaluationRepository::new(pool));
    let metrics_registry = Arc::new(MetricsRegistry::new());

    let router = build_router(AppState::new(
        Arc::new(store),
        run_service,
        run_command_service,
        memory_use_cases,
        retrieval_service,
        provider_repository,
        model_repository,
        agent_repository,
        skill_repository,
        routing_decision_repository,
        model_execution_repository,
        execution_history_repository,
        workflow_repository,
        evaluation_repository,
        metrics_registry,
    ));
    let listener = tokio::net::TcpListener::bind(config.http.bind)
        .await
        .map_err(|_| anyhow!("HTTP listener is unavailable"))?;
    tracing::info!("HTTP server listening on {}", config.http.bind);

    let server = axum::serve(listener, router);

    tokio::select! {
        result = server => {
            result.map_err(|_| anyhow!("HTTP server stopped unexpectedly"))
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received, stopping server");
            Ok(())
        }
    }
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

fn init_tracing(config: &ObservabilityConfig) -> anyhow::Result<()> {
    let result = match config.format {
        LogFormat::Text => build_text_subscriber(config, std::io::stderr)?.try_init(),
        LogFormat::Json => build_json_subscriber(config, std::io::stderr)?.try_init(),
    };

    result.map_err(|_| anyhow!("failed to initialize tracing"))
}

fn build_text_subscriber<W>(
    config: &ObservabilityConfig,
    writer: W,
) -> anyhow::Result<impl Subscriber + Send + Sync>
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let filter = output_filter(config)?;
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_filter(filter);
    Ok(tracing_subscriber::registry().with(layer))
}

fn build_json_subscriber<W>(
    config: &ObservabilityConfig,
    writer: W,
) -> anyhow::Result<impl Subscriber + Send + Sync>
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let filter = output_filter(config)?;
    let layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(writer)
        .with_filter(filter);
    Ok(tracing_subscriber::registry().with(layer))
}

fn output_filter<S>(
    config: &ObservabilityConfig,
) -> anyhow::Result<impl tracing_subscriber::layer::Filter<S> + use<S>>
where
    S: Subscriber,
{
    let user_filter = EnvFilter::try_new(&config.log_filter)
        .map_err(|_| anyhow!("invalid observability log filter"))?;
    Ok(filter::filter_fn(|metadata| {
        matches!(
            metadata.target().split("::").next(),
            Some(
                "vestrace"
                    | "vestrace_cli"
                    | "vestrace_http"
                    | "vestrace_infrastructure"
                    | "vestrace_application"
                    | "vestrace_domain"
                    | "vestrace_integration_tests"
            )
        )
    })
    .and(user_filter))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Write},
        sync::{Arc, Mutex},
    };

    use tracing::Level;
    use tracing_subscriber::fmt::MakeWriter;

    use super::{LogFormat, ObservabilityConfig};

    const SQL_SECRET: &str = "SELECT 'db-statement-secret'";
    const CONNECTION_SECRET: &str =
        "postgres://secret-user:secret-password@database.internal/vestrace";
    const ALLOWED_MESSAGE: &str = "allowed vestrace event";

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl SharedWriter {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> MakeWriter<'writer> for SharedWriter {
        type Writer = Self;

        fn make_writer(&'writer self) -> Self::Writer {
            self.clone()
        }
    }

    fn verbose_config(format: LogFormat) -> ObservabilityConfig {
        ObservabilityConfig {
            log_filter: "trace".to_owned(),
            format,
        }
    }

    fn emit_allowed_and_dependency_events() {
        tracing::info!(target: "vestrace_cli", ALLOWED_MESSAGE);
        tracing::event!(
            target: "sqlx::query",
            Level::TRACE,
            db.statement = SQL_SECRET,
            db.connection_string = CONNECTION_SECRET,
            "dependency query event"
        );
    }

    fn assert_output_is_clamped(output: &str) {
        assert!(output.contains(ALLOWED_MESSAGE), "{output}");
        assert!(!output.contains("dependency query event"), "{output}");
        assert!(!output.contains(SQL_SECRET), "{output}");
        assert!(!output.contains(CONNECTION_SECRET), "{output}");
        assert!(!output.contains("secret-password"), "{output}");
    }

    #[test]
    fn verbose_text_filter_cannot_enable_dependency_events() {
        let writer = SharedWriter::default();
        let subscriber =
            super::build_text_subscriber(&verbose_config(LogFormat::Text), writer.clone()).unwrap();

        tracing::subscriber::with_default(subscriber, emit_allowed_and_dependency_events);

        assert_output_is_clamped(&writer.contents());
    }

    #[test]
    fn verbose_json_filter_cannot_enable_dependency_events() {
        let writer = SharedWriter::default();
        let subscriber =
            super::build_json_subscriber(&verbose_config(LogFormat::Json), writer.clone()).unwrap();

        tracing::subscriber::with_default(subscriber, emit_allowed_and_dependency_events);

        let output = writer.contents();
        assert_output_is_clamped(&output);
        let event: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert!(event.is_object(), "{event}");
    }
}
