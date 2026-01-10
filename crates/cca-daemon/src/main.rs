//! CCA Daemon - Main orchestration service
//!
//! The daemon manages Claude Code instances, routes tasks, and provides
//! the core orchestration functionality for CCA.

use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent_manager;
mod config;
mod daemon;
mod orchestrator;

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
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT (Ctrl+C)");
        }
        _ = terminate => {
            info!("Received SIGTERM");
        }
    }
}
