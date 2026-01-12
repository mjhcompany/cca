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

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use cca_mcp::McpServer;

/// CCA MCP Server - Model Context Protocol integration for Claude Code
#[derive(Parser, Debug)]
#[command(name = "cca-mcp")]
#[command(version, about, long_about = None)]
struct Args {
    /// CCA daemon URL
    #[arg(short, long, default_value = "http://127.0.0.1:8580")]
    daemon_url: String,

    /// Enable debug logging (writes to stderr)
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
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
