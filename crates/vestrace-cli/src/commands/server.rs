use std::sync::Arc;

use anyhow::anyhow;
use tracing::Subscriber;
use tracing_subscriber::{
    EnvFilter,
    filter::{self, FilterExt},
    fmt::MakeWriter,
    prelude::*,
};
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
