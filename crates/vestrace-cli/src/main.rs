#![forbid(unsafe_code)]

mod commands;

use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
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
    Rebuild,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load_from_with_overrides(
        cli.config.as_deref(),
        ConfigOverrides {
            http_bind: cli.http_bind,
        },
    )?;

    match cli.command {
        Command::Server => commands::server::run(&config),
        Command::Worker => commands::worker::run(&config),
        Command::Mcp => commands::mcp::run(&config),
        Command::Migrate => commands::migrate::run(&config),
        Command::Doctor => commands::doctor::run(&config),
        Command::Rebuild => commands::rebuild::run(&config),
    }
}
