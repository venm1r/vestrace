use std::{
    fmt, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use config::{Environment, File, FileFormat};
use secrecy::SecretString;
use serde::Deserialize;
use vestrace_domain::{QualificationLifecycle, conformance::QualificationProfile};

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:3000";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_LOG_FORMAT: &str = "text";

// These fields are consumed by Serde only: the validated TOML text is then
// passed to the normal config source so precedence remains centralized.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    database: Option<FileDatabaseConfig>,
    http: Option<FileHttpConfig>,
    observability: Option<FileObservabilityConfig>,
    qualification: Option<FileQualificationConfig>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDatabaseConfig {
    url: Option<String>,
    max_connections: Option<u32>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileHttpConfig {
    bind: Option<SocketAddr>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileObservabilityConfig {
    log_filter: Option<String>,
    format: Option<LogFormat>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileQualificationConfig {
    enabled: Option<bool>,
    manifest_file: Option<PathBuf>,
    profile: Option<QualificationProfile>,
    lifecycle: Option<QualificationLifecycle>,
    suite_version: Option<String>,
    output: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub observability: ObservabilityConfig,
    pub qualification: QualificationConfig,
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
    pub format: LogFormat,
}

#[derive(Clone, Debug, Deserialize)]
pub struct QualificationConfig {
    pub enabled: bool,
    pub manifest_file: Option<PathBuf>,
    pub profile: Option<QualificationProfile>,
    pub lifecycle: Option<QualificationLifecycle>,
    pub suite_version: Option<String>,
    pub output: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Text,
    Json,
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
            .set_default("observability.log_filter", DEFAULT_LOG_FILTER)?
            .set_default("observability.format", DEFAULT_LOG_FORMAT)?
            .set_default("qualification.enabled", false)?;

        if let Some(path) = path {
            let contents = fs::read_to_string(path).map_err(|error| {
                config::ConfigError::Message(format!(
                    "failed to read configuration file {}: {error}",
                    path.display()
                ))
            })?;
            let file = File::from_str(&contents, FileFormat::Toml);
            let file_config: FileConfig = config::Config::builder()
                .add_source(file)
                .build()?
                .try_deserialize()?;
            if file_config
                .database
                .as_ref()
                .is_some_and(|database| database.url.is_some())
            {
                return Err(config::ConfigError::Message(
                    "database.url must be supplied through VESTRACE_DATABASE__URL, not a configuration file"
                        .into(),
                ));
            }

            builder = builder.add_source(File::from_str(&contents, FileFormat::Toml));
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::AppConfig;

    #[test]
    fn qualification_is_disabled_by_default() {
        let path = std::env::temp_dir().join(format!(
            "vestrace-config-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, "[database]\nmax_connections = 10\n").unwrap();

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert!(!config.qualification.enabled);
                assert!(config.qualification.manifest_file.is_none());
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn qualification_configuration_is_typed_and_explicit() {
        let path = std::env::temp_dir().join(format!(
            "vestrace-qualification-config-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            "[database]\nmax_connections = 10\n\n[qualification]\nenabled = true\nmanifest_file = 'capability-manifest.json'\nprofile = 'core'\nlifecycle = 'deployment'\nsuite_version = 'runtime-v1'\noutput = 'qualification.json'\n",
        )
        .unwrap();

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert!(config.qualification.enabled);
                assert_eq!(
                    config
                        .qualification
                        .manifest_file
                        .unwrap()
                        .to_string_lossy(),
                    "capability-manifest.json"
                );
                assert_eq!(
                    config.qualification.profile,
                    Some(vestrace_domain::conformance::QualificationProfile::Core)
                );
                assert_eq!(
                    config.qualification.lifecycle,
                    Some(vestrace_domain::QualificationLifecycle::Deployment)
                );
                assert_eq!(
                    config.qualification.suite_version.as_deref(),
                    Some("runtime-v1")
                );
            },
        );
        let _ = fs::remove_file(path);
    }
}
