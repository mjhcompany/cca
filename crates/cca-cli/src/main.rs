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
use std::path::Path;

mod commands;

use commands::{agent, config, daemon, memory, task};

/// Load environment variables from CCA env file if not already set
fn load_env_file() {
    // Check standard locations for env file
    let env_paths = [
        "/usr/local/etc/cca/cca.env".to_string(),
        dirs::config_dir()
            .map(|p| p.join("cca/cca.env").to_string_lossy().to_string())
            .unwrap_or_default(),
        dirs::home_dir()
            .map(|p| p.join(".config/cca/cca.env").to_string_lossy().to_string())
            .unwrap_or_default(),
    ];

    for path in &env_paths {
        if path.is_empty() {
            continue;
        }
        if Path::new(path).exists() {
            if let Ok(contents) = std::fs::read_to_string(path) {
                parse_env_file(&contents);
            }
            break;
        }
    }
}

/// Parse env file contents and set environment variables (only if not already set)
fn parse_env_file(contents: &str) {
    for line in contents.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle export VAR=value or VAR=value
        let line = line.strip_prefix("export ").unwrap_or(line);

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Remove quotes if present
            let value = value.trim_matches('"').trim_matches('\'');

            // Only set if not already defined
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

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
    // Load environment from cca.env file (before parsing args)
    load_env_file();

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

/// Get the daemon URL from environment or use default
fn daemon_url() -> String {
    std::env::var("CCA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8580".to_string())
}

async fn show_status() -> Result<()> {
    println!("CCA Status");
    println!("==========");

    // TODO: Connect to daemon and get status
    println!("Daemon: checking...");

    // Try to connect to daemon
    match commands::http::get(&format!("{}/api/v1/health", daemon_url())).await {
        Ok(resp) if resp.status().is_success() => {
            println!("Daemon: running");
        }
        _ => {
            println!("Daemon: not running");
        }
    }

    Ok(())
}
