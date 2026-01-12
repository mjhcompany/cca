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

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
    // Load environment from cca.env file first
    load_env_file();

    // Load configuration to get log settings
    let config = Config::load()?;

    // Initialize tracing with optional file logging
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("ccad={},tower_http=debug", config.daemon.log_level).into());

    let file_logging_enabled = if !config.daemon.log_file.is_empty() {
        // Try to set up file logging
        let log_path = std::path::Path::new(&config.daemon.log_file);
        let log_dir = log_path.parent().unwrap_or(std::path::Path::new("."));
        let log_filename = log_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("ccad.log");

        // Try to create log directory and test write permissions
        let can_write = (|| -> std::io::Result<()> {
            if !log_dir.exists() {
                std::fs::create_dir_all(log_dir)?;
            }
            // Test write permissions
            let test_path = log_dir.join(".write_test");
            std::fs::write(&test_path, "test")?;
            std::fs::remove_file(&test_path)?;
            Ok(())
        })();

        match can_write {
            Ok(()) => {
                let file_appender = tracing_appender::rolling::never(log_dir, log_filename);
                let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

                // Log to both file and stdout
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
                    .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
                    .init();

                // Keep guard alive for entire program - leak it intentionally
                Box::leak(Box::new(_guard));
                true
            }
            Err(e) => {
                // Fall back to stdout-only logging
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(tracing_subscriber::fmt::layer())
                    .init();
                eprintln!(
                    "Warning: Could not set up file logging to '{}': {}. Using stdout only.",
                    config.daemon.log_file, e
                );
                false
            }
        }
    } else {
        // Stdout only
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
        false
    };

    info!("Starting CCA Daemon v{}", env!("CARGO_PKG_VERSION"));
    if file_logging_enabled {
        info!("Logging to file: {}", config.daemon.log_file);
    } else if !config.daemon.log_file.is_empty() {
        warn!("File logging was configured but could not be enabled");
    }
    info!("Data directory: {:?}", config.daemon.get_data_dir());
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
