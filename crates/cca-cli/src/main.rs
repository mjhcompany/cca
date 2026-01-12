//! CCA CLI - Command line interface for CCA
//!
//! Primary usage is through the Command Center (Claude Code with CCA plugin).
//! This CLI is for debugging and testing purposes.

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::unused_async)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::format_collect)]
#![allow(clippy::no_effect_underscore_binding)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod commands;

use commands::{agent, config, daemon, memory, task};

#[derive(Parser)]
#[command(name = "cca")]
#[command(author, version, about = "CCA - Claude Code Agentic CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the CCA daemon
    #[command(subcommand)]
    Daemon(daemon::DaemonCommands),

    /// Manage agents
    #[command(subcommand)]
    Agent(agent::AgentCommands),

    /// Task operations
    #[command(subcommand)]
    Task(task::TaskCommands),

    /// Memory operations
    #[command(subcommand)]
    Memory(memory::MemoryCommands),

    /// Configuration management
    #[command(subcommand)]
    Config(config::ConfigCommands),

    /// Show system status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    let log_level = if cli.verbose { "debug" } else { "info" };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("cca_cli={log_level}").into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    match cli.command {
        Commands::Daemon(cmd) => daemon::run(cmd).await,
        Commands::Agent(cmd) => agent::run(cmd).await,
        Commands::Task(cmd) => task::run(cmd).await,
        Commands::Memory(cmd) => memory::run(cmd).await,
        Commands::Config(cmd) => config::run(cmd).await,
        Commands::Status => show_status().await,
    }
}

async fn show_status() -> Result<()> {
    println!("CCA Status");
    println!("==========");

    // TODO: Connect to daemon and get status
    println!("Daemon: checking...");

    // Try to connect to daemon
    match reqwest::get("http://localhost:9200/health").await {
        Ok(resp) if resp.status().is_success() => {
            println!("Daemon: running");
        }
        _ => {
            println!("Daemon: not running");
        }
    }

    Ok(())
}
