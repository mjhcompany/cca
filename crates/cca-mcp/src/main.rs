//! CCA MCP Server Binary
//!
//! This binary runs the MCP server that integrates CCA with Claude Code.
//! It communicates via stdio (JSON-RPC 2.0) and connects to the CCA daemon.

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]

use std::path::Path;

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use cca_mcp::McpServer;

/// Load environment variables from CCA env file if not already set
fn load_env_file() {
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
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

/// CCA MCP Server - Model Context Protocol integration for Claude Code
#[derive(Parser, Debug)]
#[command(name = "cca-mcp")]
#[command(version, about, long_about = None)]
struct Args {
    /// CCA daemon URL
    #[arg(short, long, env = "CCA_DAEMON_URL", default_value = "http://127.0.0.1:8580")]
    daemon_url: String,

    /// Enable debug logging (writes to stderr)
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment from cca.env file first
    load_env_file();

    let args = Args::parse();

    // Initialize logging to stderr (stdout is for MCP communication)
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("debug"))
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("warn"))
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .init();
    }

    info!("Starting CCA MCP server");
    info!("Daemon URL: {}", args.daemon_url);

    let server = McpServer::new(&args.daemon_url);
    server.run_stdio().await?;

    Ok(())
}
