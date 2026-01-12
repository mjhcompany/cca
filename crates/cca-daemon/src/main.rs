//! CCA Daemon - Main orchestration service
//!
//! The daemon manages Claude Code instances, routes tasks, and provides
//! the core orchestration functionality for CCA.

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::unused_async)]
#![allow(clippy::unused_self)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
#![allow(clippy::ref_option)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::float_cmp)]

use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent_manager;
mod auth;
mod config;
mod daemon;
mod orchestrator;
mod postgres;
mod redis;
mod rl;
mod tokens;

use crate::config::Config;
use crate::daemon::CCADaemon;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cca_daemon=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting CCA Daemon v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = Config::load()?;
    info!(
        "Configuration loaded: bind_address={}",
        config.daemon.bind_address
    );

    // Create and start daemon
    let daemon = Arc::new(CCADaemon::new(config).await?);

    // Clone for signal handler
    let daemon_handle = daemon.clone();

    // Spawn the main daemon loop
    let daemon_task = tokio::spawn(async move {
        if let Err(e) = daemon.run().await {
            error!("Daemon error: {}", e);
        }
    });

    // Wait for shutdown signal (SIGINT or SIGTERM)
    shutdown_signal().await;

    // Graceful shutdown
    info!("Initiating graceful shutdown...");
    daemon_handle.shutdown().await?;

    // Wait for daemon task to complete
    let _ = daemon_task.await;

    info!("CCA Daemon stopped");
    Ok(())
}

/// Wait for shutdown signal (SIGINT, SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(e) => {
                error!("Failed to install Ctrl+C handler: {}. Using fallback.", e);
                // Fallback: wait indefinitely (will be woken by terminate signal or process kill)
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                error!("Failed to install SIGTERM handler: {}. Using Ctrl+C only.", e);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("Received SIGINT (Ctrl+C)");
        }
        () = terminate => {
            info!("Received SIGTERM");
        }
    }
}
