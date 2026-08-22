#![forbid(unsafe_code)]
#![recursion_limit = "1024"]

mod commands;

use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
use commands::{
    conformance::{
        ConformanceArtifactArg, ConformanceLifecycleArg, ConformanceProfileArg,
        FaultSuiteIsolationArg,
    },
    rebuild::RebuildTarget,
    schema::SchemaFormat,
};
use vestrace_infrastructure::{AppConfig, ConfigOverrides};

#[derive(Debug, Parser)]
#[command(name = "vestrace", about = "Vestrace service operations")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[arg(long, global = true)]
    http_bind: Option<SocketAddr>,

    #[command(subcommand)]
    command: Command,
}

// Keep the direct Clap command shape during stabilization; parsing remains unchanged.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
enum Command {
    Server,
    Worker,
    Mcp,
    Migrate,
    Doctor,
    Plan {
        #[arg(long = "finding-id", required = true)]
        finding_ids: Vec<uuid::Uuid>,
    },
    Repair {
        #[arg(long)]
        plan_id: uuid::Uuid,
        #[arg(long)]
        current_state_ref: String,
    },
    Rebuild {
        #[arg(value_enum)]
        target: RebuildTarget,
    },
    Schema {
        #[arg(value_enum)]
        format: SchemaFormat,
    },
    Conformance {
        #[command(subcommand)]
        action: ConformanceAction,
    },
}

#[derive(Clone, Debug, Subcommand)]
enum ConformanceAction {
    List {
        #[arg(value_enum)]
        profile: Option<ConformanceProfileArg>,
    },
    Check {
        #[arg(value_enum)]
        profile: Option<ConformanceProfileArg>,
        #[arg(long)]
        json: bool,
    },
    /// Run the destructive external-effect qualification against an ephemeral
    /// deployment and persist exactly what it observed.
    FaultSuite {
        /// Path to the separately built fault-scenario program.
        #[arg(long)]
        program: PathBuf,
        /// Digest of the exact deployment target being observed.
        #[arg(long)]
        target_digest: String,
        /// This program aborts processes mid-transaction, so the CLI exposes
        /// only the isolation the scenario can prove.
        #[arg(long, value_enum)]
        isolation: FaultSuiteIsolationArg,
    },
    Bundle {
        #[arg(long, value_enum)]
        profile: ConformanceProfileArg,
        #[arg(long, value_enum)]
        lifecycle: Option<ConformanceLifecycleArg>,
        #[arg(long)]
        persist: bool,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        target_manifest: Option<String>,
        #[arg(long)]
        source_revision: Option<String>,
        #[arg(long)]
        build_digest: Option<String>,
        #[arg(long)]
        configuration_digest: Option<String>,
        #[arg(long)]
        environment_manifest: Option<String>,
        #[arg(long)]
        target_manifest_file: Option<PathBuf>,
        #[arg(long)]
        suite_version: String,
        #[arg(long = "known-limitation")]
        known_limitations: Vec<String>,
        #[arg(long)]
        post_incident_evidence_file: Option<PathBuf>,
    },
    /// Ask the v1.0 release gate whether this exact build in this exact
    /// environment may be released.
    ///
    /// The gate names every evidence source it does not have, so a release
    /// that cannot be made says which evidence is missing rather than failing
    /// for a reason nobody recorded.
    Release {
        #[arg(long)]
        manifest_file: PathBuf,
        #[arg(long)]
        bundle_file: PathBuf,
        #[arg(long, value_enum)]
        profile: ConformanceProfileArg,
        /// Collect runtime qualification from the configured database.
        ///
        /// Without it the gate reports runtime evidence as missing, which is
        /// what it is: nobody looked.
        #[arg(long)]
        runtime_evidence: bool,
        /// Collect crypto adapter qualification from a mounted secret store.
        #[arg(long)]
        crypto_evidence: bool,
        /// Requires `--crypto-evidence`: naming a store without asking to
        /// collect from it is a request this build cannot honour silently.
        #[arg(long, requires = "crypto_evidence")]
        key_store_root: Option<PathBuf>,
        #[arg(long, requires = "crypto_evidence")]
        key_id: Option<String>,
        #[arg(long, default_value = "v1")]
        key_version: String,
        #[arg(long, default_value = "release")]
        key_scope: String,
        /// Load target-bound persisted fault-suite observations and let the
        /// current evaluator derive their release verdict.
        #[arg(long)]
        fault_suite_evidence: Option<uuid::Uuid>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Verify {
        #[arg(long, value_enum)]
        profile: ConformanceProfileArg,
        #[arg(long, value_enum)]
        lifecycle: ConformanceLifecycleArg,
        #[arg(long)]
        target_manifest_file: PathBuf,
        #[arg(long)]
        bundle_file: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        public_key_file: Option<PathBuf>,
        #[arg(long)]
        require_signature: bool,
        #[arg(long)]
        trusted_signer: Option<String>,
        #[arg(long)]
        trusted_key_provider: Option<String>,
        #[arg(long)]
        trusted_key_id: Option<String>,
        #[arg(long)]
        trusted_key_version: Option<String>,
        #[arg(long)]
        trusted_key_scope: Option<String>,
        #[arg(long)]
        require_trusted_signer: bool,
    },
    Sign {
        #[arg(long, value_enum)]
        artifact: ConformanceArtifactArg,
        #[arg(long)]
        artifact_file: PathBuf,
        #[arg(long)]
        private_key_file: Option<PathBuf>,
        #[arg(long)]
        key_store_root: Option<PathBuf>,
        #[arg(long)]
        signer_identity: String,
        #[arg(long, default_value = "local-file")]
        key_provider: String,
        #[arg(long)]
        key_id: String,
        #[arg(long, default_value = "v1")]
        key_version: String,
        #[arg(long, default_value = "release")]
        key_scope: String,
        #[arg(long)]
        output: PathBuf,
    },
    VerifySignature {
        #[arg(long, value_enum)]
        artifact: ConformanceArtifactArg,
        #[arg(long)]
        artifact_file: PathBuf,
        #[arg(long)]
        public_key_file: PathBuf,
        #[arg(long)]
        trusted_signer: Option<String>,
        #[arg(long)]
        trusted_key_provider: Option<String>,
        #[arg(long)]
        trusted_key_id: Option<String>,
        #[arg(long)]
        trusted_key_version: Option<String>,
        #[arg(long)]
        trusted_key_scope: Option<String>,
        #[arg(long)]
        require_trusted_signer: bool,
    },
    Manifest {
        #[arg(long, default_value = "manifest-v1")]
        manifest_version: String,
        #[arg(long, default_value = "vestrace")]
        product: String,
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        product_version: String,
        /// The revision this binary was built from. Compiled in when the build
        /// sets `VESTRACE_SOURCE_REVISION`; the manifest refuses to be written
        /// without one, because a qualification that cannot say what it
        /// qualified is a certificate for nothing in particular.
        #[arg(long)]
        source_revision: Option<String>,
        /// Defaults to a digest of the running executable.
        #[arg(long)]
        build_digest: Option<String>,
        /// Defaults to a digest of the effective configuration.
        #[arg(long)]
        configuration_digest: Option<String>,
        /// Defaults to a summary of what this deployment runs against.
        #[arg(long)]
        environment_manifest: Option<String>,
        #[arg(long = "schema-version", required = true)]
        schema_versions: Vec<String>,
        #[arg(long = "profile", value_enum, required = true)]
        supported_profiles: Vec<ConformanceProfileArg>,
        /// Where to read the configuration whose digest is taken. Defaults to
        /// the same resolution every other command uses.
        #[arg(long)]
        config_for_identity: Option<PathBuf>,
        #[arg(long = "feature")]
        optional_features: Vec<String>,
        #[arg(long = "storage-backend")]
        storage_backends: Vec<String>,
        #[arg(long = "crypto-provider")]
        crypto_providers: Vec<String>,
        #[arg(long = "model-provider")]
        model_provider_adapters: Vec<String>,
        #[arg(long = "external-effect-adapter")]
        external_effect_adapters: Vec<String>,
        #[arg(long = "federation-capability")]
        federation_capabilities: Vec<String>,
        #[arg(long = "known-limitation")]
        known_limitations: Vec<String>,
        #[arg(long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Schema { format } = &cli.command {
        return commands::schema::run(format);
    }

    if let Command::Conformance { action } = &cli.command {
        return commands::conformance::run(
            action.clone(),
            cli.config.as_deref(),
            ConfigOverrides {
                http_bind: cli.http_bind,
            },
        )
        .await;
    }

    match &cli.command {
        Command::Plan { finding_ids } => return commands::operator::plan(finding_ids),
        Command::Repair {
            plan_id,
            current_state_ref,
        } => return commands::operator::repair(*plan_id, current_state_ref),
        _ => {}
    }

    let config = AppConfig::load_from_with_overrides(
        cli.config.as_deref(),
        ConfigOverrides {
            http_bind: cli.http_bind,
        },
    )?;

    match cli.command {
        // Process identity is created once at the command boundary and injected
        // into dispatch. Storage must record the process that actually exists,
        // never fabricate an owner when the call reaches the repository.
        Command::Server => {
            commands::server::run(&config, vestrace_domain::id::WorkerId::new()).await
        }
        Command::Worker => commands::worker::run(&config).await,
        Command::Mcp => commands::mcp::run(&config).await,
        Command::Migrate => commands::migrate::run(&config).await,
        Command::Doctor => commands::doctor::run(&config).await,
        Command::Plan { .. } | Command::Repair { .. } => unreachable!(),
        Command::Rebuild { target } => commands::rebuild::run(&config, &target).await,
        Command::Schema { .. } => unreachable!(),
        Command::Conformance { .. } => unreachable!(),
    }
}
