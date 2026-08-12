#![forbid(unsafe_code)]
#![recursion_limit = "1024"]

mod commands;

use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
use commands::{
    conformance::{ConformanceArtifactArg, ConformanceLifecycleArg, ConformanceProfileArg},
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
        private_key_file: PathBuf,
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
        #[arg(long)]
        product: String,
        #[arg(long)]
        product_version: String,
        #[arg(long)]
        source_revision: String,
        #[arg(long)]
        build_digest: String,
        #[arg(long)]
        configuration_digest: String,
        #[arg(long)]
        environment_manifest: String,
        #[arg(long = "schema-version", required = true)]
        schema_versions: Vec<String>,
        #[arg(long = "profile", value_enum, required = true)]
        supported_profiles: Vec<ConformanceProfileArg>,
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
        Command::Server => commands::server::run(&config).await,
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
