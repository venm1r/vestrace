use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use config::{Environment, File, FileFormat};
use secrecy::SecretString;
use serde::Deserialize;
use vestrace_application::RestorationStage;
use vestrace_domain::{
    DataDestination, QualificationLifecycle, Sensitivity, conformance::QualificationProfile,
};

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:3000";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_LOG_FORMAT: &str = "text";
const DEFAULT_POLICY_ENGINE: &str = "deny-all";
const DEFAULT_POLICY_VERSION: &str = "deny-all-v1";
const DEFAULT_POLICY_RISK_CEILING: &str = "low";
const MIN_ADMIN_TOKEN_LENGTH: usize = 32;

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
    recovery: Option<FileRecoveryConfig>,
    policy: Option<FilePolicyConfig>,
    auth: Option<FileAuthConfig>,
    secrets: Option<FileSecretsConfig>,
    model: Option<FileModelConfig>,
    workspaces: Option<Vec<uuid::Uuid>>,
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

impl AppConfig {
    /// A digest of the settings that decide how this deployment behaves.
    ///
    /// # Why this is an explicit list rather than a serialization
    ///
    /// A qualification bundle binds to a `target_digest` computed over the
    /// manifest's identity, and `QualificationBaseline::matches_bundle` refuses
    /// a bundle whose digest moved — that is how "a material change invalidates
    /// the baseline" is enforced. Which means this digest decides what counts as
    /// a material change, and getting it from `serde` would mean whatever
    /// happened to be `Serialize` that week.
    ///
    /// It is also why secrets are not filtered out but never reached: the master
    /// key and the admin token are in `AppConfig`, and a digest built by walking
    /// the struct would one day include them in something published beside a
    /// release. Every field below is named on purpose, and adding a setting that
    /// changes behaviour means adding it here.
    ///
    /// What is deliberately absent: bind address and log filter (they change
    /// where output goes, not what the system does), and every credential.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        let mut field = |name: &str, value: &str| {
            hasher.update(name.as_bytes());
            hasher.update([0u8]);
            hasher.update(value.as_bytes());
            hasher.update([0u8]);
        };

        field(
            "database.max_connections",
            &self.database.max_connections.to_string(),
        );
        field("policy.engine", &format!("{:?}", self.policy.engine));
        field("policy.version", &self.policy.version);
        field(
            "policy.risk_ceiling",
            &format!("{:?}", self.policy.risk_ceiling),
        );
        for capability in &self.policy.capabilities {
            field("policy.capability", capability);
        }
        for (capability, stage) in &self.policy.capability_restoration {
            field(
                "policy.capability_restoration",
                &format!("{capability}={stage:?}"),
            );
        }
        match &self.policy.data {
            None => field("policy.data", "absent"),
            Some(data) => {
                field("policy.data.mode", &format!("{:?}", data.mode));
                field(
                    "policy.data.classification",
                    &format!("{:?}", data.classification),
                );
                field(
                    "policy.data.maximum_sensitivity",
                    &format!("{:?}", data.maximum_sensitivity),
                );
                for destination in &data.allowed_destinations {
                    field(
                        "policy.data.allowed_destination",
                        &format!("{destination:?}"),
                    );
                }
                field(
                    "policy.data.required_capability",
                    data.required_capability.as_deref().unwrap_or("absent"),
                );
            }
        }
        field("auth.enabled", &self.auth.is_enabled().to_string());
        field("secrets.key_version", self.secrets.effective_key_version());
        field(
            "secrets.configured",
            &self.secrets.master_key.is_some().to_string(),
        );
        field("model.enabled", &self.model.enabled.to_string());
        field("model.base_url", &self.model.base_url);
        field("model.name", &self.model.model_name);
        field("embedding.enabled", &self.embedding.enabled.to_string());
        field("embedding.base_url", &self.embedding.base_url);
        field("embedding.model", &self.embedding.model_name);
        field("embedding.space", &self.embedding.space_name);
        field(
            "qualification.enabled",
            &self.qualification.enabled.to_string(),
        );
        field("recovery.enabled", &self.recovery.enabled.to_string());
        for adapter in self.effects.configured() {
            field("effects.adapter", &adapter.name);
            field("effects.dispatch_url", &adapter.dispatch_url);
            field("effects.read_back_url", &adapter.read_back_url);
        }
        for workspace in &self.workspaces {
            field("workspace", &workspace.to_string());
        }

        format!("sha256:{:x}", hasher.finalize())
    }

    /// Existing release-identity name retained for callers that bind the
    /// configuration fingerprint into qualification evidence.
    pub fn identity_digest(&self) -> String {
        self.fingerprint()
    }

    /// What this deployment is running against, in a form a human can read.
    ///
    /// Not a digest: the environment manifest is meant to be looked at when
    /// somebody asks what a qualification was performed on.
    pub fn environment_summary(&self) -> String {
        let adapters: Vec<&str> = self
            .effects
            .configured()
            .into_iter()
            .map(|adapter| adapter.name.as_str())
            .collect();
        format!(
            "postgres; policy={:?}; auth={}; secrets={}; model={}; embedding={}; effects=[{}]; \
             workspaces={}",
            self.policy.engine,
            self.auth.is_enabled(),
            if self.secrets.master_key.is_some() {
                "local-file"
            } else {
                "none"
            },
            if self.model.enabled {
                self.model.model_name.as_str()
            } else {
                "disabled"
            },
            if self.embedding.enabled {
                self.embedding.model_name.as_str()
            } else {
                "disabled"
            },
            adapters.join(","),
            self.workspaces.len()
        )
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub observability: ObservabilityConfig,
    pub qualification: QualificationConfig,
    pub recovery: RecoveryConfig,
    pub policy: PolicyConfig,
    /// Absent by default: a deployment must opt in to authentication.
    #[serde(default)]
    pub auth: AuthConfig,
    /// Absent by default: without a master key there is no secret storage, and
    /// no plaintext fallback.
    #[serde(default)]
    pub secrets: SecretsConfig,
    /// Disabled by default: an agent step then fails rather than pretending to
    /// have run.
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub effects: EffectsConfig,
    /// The workspaces this process serves. Every run/lease/work-queue query is
    /// workspace-scoped, so a background process has to be told which
    /// workspaces to sweep and poll; there is no cross-workspace scan.
    #[serde(default)]
    pub workspaces: Vec<uuid::Uuid>,
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

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRecoveryConfig {
    enabled: Option<bool>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePolicyConfig {
    engine: Option<PolicyEngineKind>,
    version: Option<String>,
    capabilities: Option<Vec<String>>,
    capability_restoration: Option<BTreeMap<String, RestorationStage>>,
    data: Option<FileDataPolicyConfig>,
    risk_ceiling: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDataPolicyConfig {
    mode: Option<DataPolicyMode>,
    classification: Option<Sensitivity>,
    maximum_sensitivity: Option<Sensitivity>,
    allowed_destinations: Option<BTreeSet<DataDestination>>,
    required_capability: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileAuthConfig {
    admin_token: Option<String>,
    admin_workspace: Option<uuid::Uuid>,
    admin_principal: Option<uuid::Uuid>,
}

/// Interim single-administrator authentication.
///
/// While no real identity provider exists, one shared bearer token maps to one
/// configured administrator. When `admin_token` is absent the server keeps its
/// previous behaviour of trusting identity headers, which is not authentication
/// and is reported as such.
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub admin_token: Option<SecretString>,
    pub admin_workspace: Option<uuid::Uuid>,
    pub admin_principal: Option<uuid::Uuid>,
}

impl AuthConfig {
    pub fn is_enabled(&self) -> bool {
        self.admin_token.is_some()
    }
}

impl fmt::Debug for AuthConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The token must never reach a log line, a panic message or a response.
        formatter
            .debug_struct("AuthConfig")
            .field(
                "admin_token",
                &self.admin_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("admin_workspace", &self.admin_workspace)
            .field("admin_principal", &self.admin_principal)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSecretsConfig {
    master_key: Option<String>,
    key_version: Option<String>,
}

/// The master key that wraps each workspace's data key.
///
/// Absent by default, and its absence disables secret storage rather than
/// falling back to storing plaintext. A store that silently degrades to
/// unencrypted is worse than no store, because callers will trust it.
///
/// The key is held here as a `SecretString` and is read from the environment,
/// which for the current deployment means a `.env` file. That is
/// local-development custody: anything that can read the process environment
/// can read the key. It is documented as such in
/// [`crate::crypto`] and is refused by production crypto qualification.
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct SecretsConfig {
    pub master_key: Option<SecretString>,
    /// Names the key generation, so material wrapped under an older key can be
    /// recognised after a rotation instead of silently failing to open.
    pub key_version: Option<String>,
}

impl SecretsConfig {
    pub fn is_enabled(&self) -> bool {
        self.master_key.is_some()
    }

    /// The version to record for newly wrapped material.
    pub fn effective_key_version(&self) -> &str {
        self.key_version.as_deref().unwrap_or("v1")
    }
}

impl fmt::Debug for SecretsConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretsConfig")
            .field(
                "master_key",
                &self.master_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("key_version", &self.key_version)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileModelConfig {
    enabled: Option<bool>,
    base_url: Option<String>,
    model_name: Option<String>,
    secret_name: Option<String>,
    max_tokens: Option<u32>,
}

/// The model an agent-assigned run step invokes.
///
/// Disabled by default. When disabled, an agent step **fails** rather than
/// reporting success — see `ExecuteStepHandler`. That is deliberate: a step
/// that was supposed to call a model and did not has not been performed.
///
/// No API key here. The credential is a per-workspace secret resolved through
/// the secret store at invocation time, so `secret_name` names it rather than
/// carrying it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model_name: String,
    /// Name of the workspace secret holding the provider API key. Its purpose
    /// is always `provider-api-key`.
    pub secret_name: String,
    pub max_tokens: Option<u32>,
}

/// The embedding model that gives retrieval a vector channel.
///
/// Separate from [`ModelConfig`] on purpose: embedding and completion are
/// different models with different widths and different failure modes, and a
/// deployment routinely wants a small local embedder beside a large remote
/// completion model.
#[derive(Clone, Debug, Deserialize)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model_name: String,
    /// The embedding space these vectors belong to. Changing the model without
    /// changing the space is refused rather than silently mixing geometries.
    pub space_name: String,
    /// Optional workspace secret holding the provider key. A local endpoint
    /// usually needs none.
    pub secret_name: Option<String>,
}

/// One configured external effect adapter.
///
/// The destination lives here and not in a request. A caller names the adapter;
/// where it points is a deployment decision, because a surface that let a
/// request choose the URL would be an outbound proxy for everything the process
/// can reach.
#[derive(Clone, Debug, Deserialize)]
pub struct EffectAdapterConfig {
    /// The name a caller uses, and the name recorded on every intent and
    /// receipt.
    pub name: String,
    pub dispatch_url: String,
    /// Where to ask what happened. Required: an adapter that cannot be asked
    /// leaves an unknown outcome unknown forever, and the adapter contract
    /// refuses to let it claim otherwise.
    pub read_back_url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct EffectsConfig {
    /// Adapters from a configuration file, where a list is expressible.
    #[serde(default)]
    pub adapters: Vec<EffectAdapterConfig>,
    /// One adapter from the environment.
    ///
    /// Container deployments configure everything through environment
    /// variables, and an indexed list does not survive that translation. One
    /// webhook covers the case a deployment actually has; more than one needs a
    /// file, and saying so beats an env syntax that silently drops the second
    /// entry.
    #[serde(default)]
    pub webhook: Option<EffectAdapterConfig>,
}

impl EffectsConfig {
    /// Every adapter this deployment configured, from either source.
    pub fn configured(&self) -> Vec<&EffectAdapterConfig> {
        self.webhook.iter().chain(self.adapters.iter()).collect()
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            model_name: String::new(),
            space_name: "default".to_string(),
            secret_name: None,
        }
    }
}

/// Which authorization engine the HTTP surface uses.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyEngineKind {
    /// Deny every governed request. The safe default while no capability-grant
    /// store exists.
    #[default]
    DenyAll,
    /// Allow a fixed set of capabilities named in configuration. A stopgap for
    /// deployments without grant persistence; see
    /// `ConfiguredCapabilityPolicyEngine` for what it does not provide.
    ConfiguredCapabilities,
    /// Decide every request from the durable capability grants issued to the
    /// requesting principal.
    ///
    /// This is the real model: subject-scoped, expiring, revocable, budgeted and
    /// risk-ceilinged, evaluated by the domain kernel that CAP-002..CAP-014
    /// verify. A deployment selecting it must issue grants — a principal with
    /// none is denied everything, which is the correct behaviour and will look
    /// like a broken deployment to anybody expecting the configured list.
    CapabilityGrants,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PolicyConfig {
    pub engine: PolicyEngineKind,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Stages are separate from the configured capability set so a missing
    /// declaration remains an observable `CapabilityNotDeclared` decision.
    #[serde(default)]
    pub capability_restoration: BTreeMap<String, RestorationStage>,
    /// The model-call data boundary. Absent is permitted only while model
    /// execution itself is disabled; enabling a model requires all of these
    /// decisions to be operator-authored.
    #[serde(default)]
    pub data: Option<DataPolicyConfig>,
    pub risk_ceiling: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataPolicyMode {
    Enforce,
    Observe,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DataPolicyConfig {
    pub mode: DataPolicyMode,
    /// A conservative floor applied to every run objective. It is a channel
    /// declaration, not a claim that the individual objective was classified.
    pub classification: Sensitivity,
    pub maximum_sensitivity: Sensitivity,
    pub allowed_destinations: BTreeSet<DataDestination>,
    /// Parsed only so startup can explicitly refuse this unsafe configuration.
    /// The worker has a workspace identity here, not the originating subject.
    #[serde(default)]
    pub required_capability: Option<String>,
}

/// Startup recovery is workspace-scoped like every other query in the system.
/// The workspaces it sweeps come from the top-level `workspaces` list, so
/// recovery and run polling can never drift onto different sets.
#[derive(Clone, Debug, Deserialize)]
pub struct RecoveryConfig {
    pub enabled: bool,
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
            .set_default("qualification.enabled", false)?
            .set_default("recovery.enabled", false)?
            .set_default("policy.engine", DEFAULT_POLICY_ENGINE)?
            .set_default("policy.version", DEFAULT_POLICY_VERSION)?
            .set_default("policy.risk_ceiling", DEFAULT_POLICY_RISK_CEILING)?;

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
                .try_parsing(true)
                // `workspaces` is the one list-valued setting, and container
                // deployments can only supply configuration through the
                // environment, so it accepts a comma-separated list there.
                .list_separator(",")
                .with_list_parse_key("workspaces")
                .with_list_parse_key("policy.capabilities")
                .with_list_parse_key("policy.data.allowed_destinations"),
        );

        if let Some(http_bind) = overrides.http_bind {
            builder = builder.set_override("http.bind", http_bind.to_string())?;
        }

        let config: Self = builder.build()?.try_deserialize()?;
        if config.auth.is_enabled() {
            // A short shared secret is guessable, and an authenticated identity
            // that is not configured cannot be attributed to anyone.
            let token_len = config
                .auth
                .admin_token
                .as_ref()
                .map(|token| secrecy::ExposeSecret::expose_secret(token).len())
                .unwrap_or_default();
            if token_len < MIN_ADMIN_TOKEN_LENGTH {
                return Err(config::ConfigError::Message(format!(
                    "auth.admin_token must be at least {MIN_ADMIN_TOKEN_LENGTH} characters"
                )));
            }
            if config.auth.admin_workspace.is_none() || config.auth.admin_principal.is_none() {
                return Err(config::ConfigError::Message(
                    "auth.admin_workspace and auth.admin_principal are required when auth.admin_token is set"
                        .into(),
                ));
            }
        }
        if config.secrets.is_enabled() {
            // Validated at load rather than at first use: a malformed master key
            // must stop the process from starting, not surface as a failure the
            // first time someone tries to read a credential.
            let key = config
                .secrets
                .master_key
                .as_ref()
                .map(|key| secrecy::ExposeSecret::expose_secret(key).to_string())
                .unwrap_or_default();
            if crate::crypto::MasterKey::from_base64(&key, config.secrets.effective_key_version())
                .is_err()
            {
                // Says what shape is required, never what was supplied.
                return Err(config::ConfigError::Message(
                    "secrets.master_key must be 32 bytes encoded as base64".into(),
                ));
            }
        }
        if config.model.enabled {
            // Each of these is required to make a call and to attribute its
            // cost; a blank one would surface as a runtime failure on the first
            // step rather than at start-up.
            for (label, value) in [
                ("model.base_url", &config.model.base_url),
                ("model.model_name", &config.model.model_name),
                ("model.secret_name", &config.model.secret_name),
            ] {
                if value.trim().is_empty() {
                    return Err(config::ConfigError::Message(format!(
                        "{label} is required when model.enabled is true"
                    )));
                }
            }
            if config.policy.data.is_none() {
                return Err(config::ConfigError::Message(
                    "policy.data.mode, policy.data.classification, policy.data.maximum_sensitivity, and policy.data.allowed_destinations are required when model.enabled is true"
                        .into(),
                ));
            }
            if !config.secrets.is_enabled() {
                // The provider credential lives in the secret store, so model
                // execution without secret storage could never authenticate.
                return Err(config::ConfigError::Message(
                    "secrets.master_key is required when model.enabled is true, because the                      provider credential is resolved from the secret store"
                        .into(),
                ));
            }
        }
        if config
            .policy
            .data
            .as_ref()
            .and_then(|data| data.required_capability.as_ref())
            .is_some()
        {
            // A worker request context carries a workspace-derived principal,
            // not the subject that created the run. Evaluating a subject grant
            // here would authorize the wrong party, so this configuration is
            // refused rather than answered inaccurately.
            return Err(config::ConfigError::Message(
                "policy.data.required_capability cannot be configured because model-step execution does not carry the originating principal"
                    .into(),
            ));
        }
        if config.policy.engine == PolicyEngineKind::ConfiguredCapabilities
            && config.policy.capabilities.is_empty()
        {
            return Err(config::ConfigError::Message(
                "policy.capabilities must name at least one capability when policy.engine is configured-capabilities"
                    .into(),
            ));
        }
        if config.recovery.enabled && config.workspaces.is_empty() {
            return Err(config::ConfigError::Message(
                "workspaces must name at least one workspace when recovery.enabled is true".into(),
            ));
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vestrace_application::RestorationStage;
    use vestrace_domain::{DataDestination, Sensitivity};

    use super::{AppConfig, DataPolicyMode};

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

    fn temp_config(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "vestrace-{name}-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn startup_recovery_is_disabled_and_unscoped_by_default() {
        let path = temp_config("recovery-default", "[database]\nmax_connections = 10\n");

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert!(!config.recovery.enabled);
                assert!(config.workspaces.is_empty());
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn startup_recovery_workspaces_are_explicit() {
        let path = temp_config(
            "recovery-explicit",
            "workspaces = ['3f1d2c4e-0000-4000-8000-000000000001']\n\n[database]\nmax_connections = 10\n\n[recovery]\nenabled = true\n",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert!(config.recovery.enabled);
                assert_eq!(
                    config.workspaces,
                    vec![uuid::Uuid::parse_str("3f1d2c4e-0000-4000-8000-000000000001").unwrap()]
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    /// Enabling recovery without naming a workspace would silently recover
    /// nothing, which is the failure mode this whole gate exists to remove.
    #[test]
    fn enabled_startup_recovery_requires_at_least_one_workspace() {
        let path = temp_config(
            "recovery-unscoped",
            "[database]\nmax_connections = 10\n\n[recovery]\nenabled = true\n",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();

                assert!(
                    error.to_string().contains("workspaces"),
                    "unexpected error: {error}"
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    /// Container deployments can only configure through the environment, so the
    /// one list-valued setting has to be reachable there.
    #[test]
    fn workspaces_can_be_supplied_as_a_comma_separated_environment_list() {
        let path = temp_config(
            "recovery-env",
            "[database]
max_connections = 10
",
        );

        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                (
                    "VESTRACE_WORKSPACES",
                    Some(
                        "3f1d2c4e-0000-4000-8000-000000000001,3f1d2c4e-0000-4000-8000-000000000002",
                    ),
                ),
                ("VESTRACE_RECOVERY__ENABLED", Some("true")),
            ],
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert!(config.recovery.enabled);
                assert_eq!(
                    config.workspaces,
                    vec![
                        uuid::Uuid::parse_str("3f1d2c4e-0000-4000-8000-000000000001").unwrap(),
                        uuid::Uuid::parse_str("3f1d2c4e-0000-4000-8000-000000000002").unwrap(),
                    ]
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn policy_defaults_to_deny_all() {
        let path = temp_config(
            "policy-default",
            "[database]
max_connections = 10
",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert_eq!(config.policy.engine, super::PolicyEngineKind::DenyAll);
                assert!(config.policy.capabilities.is_empty());
            },
        );
        let _ = fs::remove_file(path);
    }

    /// Selecting the configured engine without naming a capability would deny
    /// everything anyway, which is the deny-all engine under a misleading name.
    #[test]
    fn configured_policy_engine_requires_capabilities() {
        let path = temp_config(
            "policy-empty",
            "[database]
max_connections = 10

[policy]
engine = 'configured-capabilities'
",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();

                assert!(
                    error.to_string().contains("policy.capabilities"),
                    "unexpected error: {error}"
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn policy_capabilities_can_be_supplied_as_an_environment_list() {
        let path = temp_config(
            "policy-env",
            "[database]
max_connections = 10
",
        );

        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                ("VESTRACE_POLICY__ENGINE", Some("configured-capabilities")),
                (
                    "VESTRACE_POLICY__CAPABILITIES",
                    Some("memory.read,execution.read"),
                ),
                ("VESTRACE_POLICY__RISK_CEILING", Some("medium")),
            ],
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert_eq!(
                    config.policy.engine,
                    super::PolicyEngineKind::ConfiguredCapabilities
                );
                assert_eq!(
                    config.policy.capabilities,
                    vec!["memory.read".to_owned(), "execution.read".to_owned()]
                );
                assert_eq!(config.policy.risk_ceiling, "medium");
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn capability_restoration_stages_are_typed_configuration() {
        let path = temp_config(
            "capability-restoration",
            "[database]
max_connections = 10

[policy]
engine = 'configured-capabilities'
capabilities = ['audit.read', 'memory.write']

[policy.capability_restoration]
'audit.read' = 'diagnostics-read-only'
'memory.write' = 'internal-deterministic-writes'
",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert_eq!(
                    config.policy.capability_restoration["audit.read"],
                    RestorationStage::DiagnosticsReadOnly
                );
                assert_eq!(
                    config.policy.capability_restoration["memory.write"],
                    RestorationStage::InternalDeterministicWrites
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn unknown_capability_restoration_stage_is_rejected() {
        let path = temp_config(
            "capability-restoration-unknown-stage",
            "[database]
max_connections = 10

[policy]
engine = 'configured-capabilities'
capabilities = ['audit.read']

[policy.capability_restoration]
'audit.read' = 'quietly-restore-everything'
",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();

                assert!(
                    error.to_string().contains("quietly-restore-everything"),
                    "unexpected error: {error}"
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn authentication_is_absent_by_default() {
        let path = temp_config(
            "auth-default",
            "[database]
max_connections = 10
",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert!(!config.auth.is_enabled());
            },
        );
        let _ = fs::remove_file(path);
    }

    /// A short shared secret is guessable, so the server refuses to start
    /// rather than offering weak authentication that looks like protection.
    #[test]
    fn a_short_administrator_token_is_rejected() {
        let path = temp_config(
            "auth-short",
            "[database]
max_connections = 10
",
        );

        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                ("VESTRACE_AUTH__ADMIN_TOKEN", Some("too-short")),
                (
                    "VESTRACE_AUTH__ADMIN_WORKSPACE",
                    Some("3f1d2c4e-0000-4000-8000-000000000001"),
                ),
                (
                    "VESTRACE_AUTH__ADMIN_PRINCIPAL",
                    Some("3f1d2c4e-0000-4000-8000-000000000002"),
                ),
            ],
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();
                assert!(
                    error.to_string().contains("admin_token"),
                    "unexpected error: {error}"
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    /// An authenticated request must be attributable to a configured identity.
    #[test]
    fn an_administrator_token_without_an_identity_is_rejected() {
        let path = temp_config(
            "auth-no-identity",
            "[database]
max_connections = 10
",
        );

        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                (
                    "VESTRACE_AUTH__ADMIN_TOKEN",
                    Some("a-sufficiently-long-development-token-0001"),
                ),
            ],
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();
                assert!(
                    error.to_string().contains("admin_workspace"),
                    "unexpected error: {error}"
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    /// The token must never be rendered, because Debug output reaches logs.
    #[test]
    fn the_administrator_token_is_redacted_in_debug_output() {
        let path = temp_config(
            "auth-redaction",
            "[database]
max_connections = 10
",
        );

        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                (
                    "VESTRACE_AUTH__ADMIN_TOKEN",
                    Some("super-secret-development-token-000000001"),
                ),
                (
                    "VESTRACE_AUTH__ADMIN_WORKSPACE",
                    Some("3f1d2c4e-0000-4000-8000-000000000001"),
                ),
                (
                    "VESTRACE_AUTH__ADMIN_PRINCIPAL",
                    Some("3f1d2c4e-0000-4000-8000-000000000002"),
                ),
            ],
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();
                let rendered = format!("{:?}", config.auth);

                assert!(config.auth.is_enabled());
                assert!(!rendered.contains("super-secret"), "{rendered}");
                assert!(rendered.contains("REDACTED"), "{rendered}");
            },
        );
        let _ = fs::remove_file(path);
    }

    /// No master key means no secret storage. It must never mean "store it in
    /// the clear instead".
    #[test]
    fn secret_storage_is_absent_by_default() {
        let path = temp_config(
            "secrets-default",
            "[database]
max_connections = 10
",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();

                assert!(!config.secrets.is_enabled());
            },
        );
        let _ = fs::remove_file(path);
    }

    /// A key of the wrong length must stop the process at start-up, not at the
    /// first attempt to read a credential.
    #[test]
    fn a_malformed_master_key_is_rejected_at_load() {
        let path = temp_config(
            "secrets-malformed",
            "[database]
max_connections = 10
",
        );

        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                // Valid base64, but 16 bytes rather than 32.
                (
                    "VESTRACE_SECRETS__MASTER_KEY",
                    Some("AAAAAAAAAAAAAAAAAAAAAA=="),
                ),
            ],
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();
                assert!(
                    error.to_string().contains("master_key"),
                    "unexpected error: {error}"
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_well_formed_master_key_loads_and_is_never_rendered() {
        let path = temp_config(
            "secrets-valid",
            "[database]
max_connections = 10
",
        );

        // 32 bytes of 0x2a.
        let key = "KioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKio=";
        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                ("VESTRACE_SECRETS__MASTER_KEY", Some(key)),
                ("VESTRACE_SECRETS__KEY_VERSION", Some("v2")),
            ],
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();
                let rendered = format!("{:?}", config.secrets);

                assert!(config.secrets.is_enabled());
                assert_eq!(config.secrets.effective_key_version(), "v2");
                assert!(!rendered.contains(key), "{rendered}");
                assert!(rendered.contains("REDACTED"), "{rendered}");
            },
        );
        let _ = fs::remove_file(path);
    }

    /// Without an explicit version, wrapped material still has to be labelled
    /// with something stable so a later rotation can tell generations apart.
    #[test]
    fn the_key_version_defaults_rather_than_being_absent() {
        assert_eq!(
            super::SecretsConfig::default().effective_key_version(),
            "v1"
        );
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

    #[test]
    fn an_enabled_model_requires_an_explicit_data_policy() {
        let path = temp_config(
            "model-without-data-policy",
            "[database]\nmax_connections = 10\n\n[model]\nenabled = true\nbase_url = 'http://localhost:12345/v1'\nmodel_name = 'prism-ml/bonsai-27b'\nsecret_name = 'lm-studio'\n",
        );

        temp_env::with_vars(
            [(
                "VESTRACE_DATABASE__URL",
                Some("postgres://localhost/vestrace"),
            )],
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();
                let message = error.to_string();
                for key in [
                    "policy.data.mode",
                    "policy.data.classification",
                    "policy.data.maximum_sensitivity",
                    "policy.data.allowed_destinations",
                ] {
                    assert!(message.contains(key), "{key} missing from {message:?}");
                }
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_required_data_capability_is_refused_at_startup() {
        let path = temp_config(
            "data-policy-required-capability",
            "[database]\nmax_connections = 10\n\n[policy.data]\nmode = 'enforce'\nclassification = 'confidential'\nmaximum_sensitivity = 'confidential'\nallowed_destinations = ['local_model']\nrequired_capability = 'model.invoke'\n",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let error = AppConfig::load_from(Some(&path)).unwrap_err();
                let message = error.to_string();
                assert!(
                    message.contains("policy.data.required_capability"),
                    "{message}"
                );
                assert!(message.contains("cannot be configured"), "{message}");
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn every_data_policy_field_changes_the_configuration_identity() {
        let path = temp_config(
            "data-policy-fingerprint",
            "[database]\nmax_connections = 10\n\n[policy.data]\nmode = 'enforce'\nclassification = 'confidential'\nmaximum_sensitivity = 'confidential'\nallowed_destinations = ['local_model']\n",
        );

        temp_env::with_var(
            "VESTRACE_DATABASE__URL",
            Some("postgres://localhost/vestrace"),
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();
                let baseline = config.fingerprint();

                let mut changed = config.clone();
                changed.policy.data.as_mut().unwrap().mode = DataPolicyMode::Observe;
                assert_ne!(baseline, changed.fingerprint(), "mode was not hashed");

                let mut changed = config.clone();
                changed.policy.data.as_mut().unwrap().classification = Sensitivity::Restricted;
                assert_ne!(
                    baseline,
                    changed.fingerprint(),
                    "classification was not hashed"
                );

                let mut changed = config.clone();
                changed.policy.data.as_mut().unwrap().maximum_sensitivity = Sensitivity::Restricted;
                assert_ne!(
                    baseline,
                    changed.fingerprint(),
                    "maximum_sensitivity was not hashed"
                );

                let mut changed = config.clone();
                changed
                    .policy
                    .data
                    .as_mut()
                    .unwrap()
                    .allowed_destinations
                    .insert(DataDestination::RemoteProvider);
                assert_ne!(
                    baseline,
                    changed.fingerprint(),
                    "allowed_destinations was not hashed"
                );

                let mut changed = config.clone();
                changed.policy.data.as_mut().unwrap().required_capability =
                    Some("model.invoke".into());
                assert_ne!(
                    baseline,
                    changed.fingerprint(),
                    "required_capability was not hashed"
                );
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn data_policy_destinations_can_be_supplied_as_an_environment_list() {
        let path = temp_config(
            "data-policy-environment-list",
            "[database]\nmax_connections = 10\n",
        );

        temp_env::with_vars(
            [
                (
                    "VESTRACE_DATABASE__URL",
                    Some("postgres://localhost/vestrace"),
                ),
                ("VESTRACE_POLICY__DATA__MODE", Some("observe")),
                (
                    "VESTRACE_POLICY__DATA__CLASSIFICATION",
                    Some("confidential"),
                ),
                (
                    "VESTRACE_POLICY__DATA__MAXIMUM_SENSITIVITY",
                    Some("restricted"),
                ),
                (
                    "VESTRACE_POLICY__DATA__ALLOWED_DESTINATIONS",
                    Some("local_model,remote_provider"),
                ),
            ],
            || {
                let config = AppConfig::load_from(Some(&path)).unwrap();
                let data = config.policy.data.unwrap();
                assert_eq!(data.mode, DataPolicyMode::Observe);
                assert_eq!(data.classification, Sensitivity::Confidential);
                assert_eq!(data.maximum_sensitivity, Sensitivity::Restricted);
                assert_eq!(
                    data.allowed_destinations,
                    [DataDestination::LocalModel, DataDestination::RemoteProvider]
                        .into_iter()
                        .collect()
                );
            },
        );
        let _ = fs::remove_file(path);
    }
}
