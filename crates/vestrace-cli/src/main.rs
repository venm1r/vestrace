#![forbid(unsafe_code)]
#![recursion_limit = "1024"]

mod commands;

use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
use commands::{rebuild::RebuildTarget, schema::SchemaFormat};
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
    Rebuild {
        #[arg(value_enum)]
        target: RebuildTarget,
    },
    Schema {
        #[arg(value_enum)]
        format: SchemaFormat,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Schema { format } = &cli.command {
        return commands::schema::run(format);
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
        Command::Rebuild { target } => commands::rebuild::run(&config, &target).await,
        Command::Schema { .. } => unreachable!(),
    }
}
