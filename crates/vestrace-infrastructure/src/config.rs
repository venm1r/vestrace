use std::{fmt, net::SocketAddr, path::Path};

use config::{Environment, File, FileFormat};
use secrecy::SecretString;
use serde::Deserialize;

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:3000";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_LOG_FILTER: &str = "info";

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub observability: ObservabilityConfig,
}

#[derive(Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: SecretString,
    pub max_connections: u32,
}

impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct HttpConfig {
    pub bind: SocketAddr,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObservabilityConfig {
    pub log_filter: String,
}

#[derive(Clone, Debug, Default)]
pub struct ConfigOverrides {
    pub http_bind: Option<SocketAddr>,
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        Self::load_from_with_overrides(None, ConfigOverrides::default())
    }

    pub fn load_from(path: Option<&Path>) -> Result<Self, config::ConfigError> {
        Self::load_from_with_overrides(path, ConfigOverrides::default())
    }

    pub fn load_from_with_overrides(
        path: Option<&Path>,
        overrides: ConfigOverrides,
    ) -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder()
            .set_default("database.max_connections", DEFAULT_DATABASE_MAX_CONNECTIONS)?
            .set_default("http.bind", DEFAULT_HTTP_BIND)?
            .set_default("observability.log_filter", DEFAULT_LOG_FILTER)?;

        if let Some(path) = path {
            builder = builder.add_source(File::from(path).format(FileFormat::Toml).required(true));
        }

        builder = builder.add_source(
            Environment::with_prefix("VESTRACE")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true),
        );

        if let Some(http_bind) = overrides.http_bind {
            builder = builder.set_override("http.bind", http_bind.to_string())?;
        }

        builder.build()?.try_deserialize()
    }
}
