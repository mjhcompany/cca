//! CCA Daemon - Main orchestration service
//!
//! The daemon manages Claude Code instances, routes tasks, and provides
//! the core orchestration functionality for CCA.

// Pedantic clippy allows - intentional design decisions for this crate:
// - doc_markdown: SEC-XXX security tags are pervasive and don't need backticks
// - too_many_lines: Complex initialization functions are kept cohesive
// - similar_names: stats/state naming is clear in context
// - struct_excessive_bools: Config structs intentionally have many boolean flags
// - cast_precision_loss: Duration conversions are safe within expected ranges
// - cast_possible_truncation: u128->u64 duration conversions are safe (< 584 years)
// - cast_sign_loss: Signed/unsigned casts in metrics are controlled
// - cast_possible_wrap: i32/usize conversions are bounded by data
// - cast_lossless: u32 to f64 is intentional for metrics
// - if_not_else: "if !x.is_empty()" is often more readable than inverted branches
// - manual_let_else: match with Ok/Some patterns is often clearer than let-else
// - single_match_else: single-arm match with else is a valid pattern
// - ref_option: &Option<T> in function signatures is valid for optional params
// - bool_to_int_with_if: explicit if conversion is often clearer
// - unused_self: Methods preserve API consistency even when self unused
// - unnecessary_wraps: Consistent Result returns for API uniformity
// - map_unwrap_or: map().unwrap_or() pattern is idiomatic
// - format_push_string: format! append is clearer than write! for simple cases
// - unused_async: Async handlers maintain consistency in axum
// - used_underscore_binding: _ prefix is intentional for unused-but-required params
// - no_effect_underscore_binding: Side-effect detection for _ bindings
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::if_not_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::ref_option)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::unused_self)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::format_push_string)]
#![allow(clippy::unused_async)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::no_effect_underscore_binding)]

use std::sync::Arc;

use anyhow::Result;
use cca_core::util::load_env_file;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent_manager;
mod auth;
mod code_parser;
mod config;
mod daemon;
mod embeddings;
mod indexing;
mod metrics;
mod orchestrator;
mod postgres;
mod redis;
mod rl;
mod tmux;
mod tokens;
mod validation;

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

    // Security warnings for authentication configuration
    // SECURITY: Use is_auth_required() which enforces auth in production builds
    if !config.daemon.is_auth_required() {
        warn!("============================================================");
        warn!("WARNING: API authentication is DISABLED!");
        warn!("Anyone can access the CCA API without credentials.");
        warn!("This is only possible in dev builds (--features dev).");
        warn!("Production builds ALWAYS require authentication.");
        warn!("============================================================");
    } else if config.daemon.api_keys.is_empty() {
        error!("============================================================");
        error!("ERROR: Authentication is enabled but no API keys configured!");
        error!("All API requests will be rejected.");
        error!("Set CCA__DAEMON__API_KEYS=your-secret-key");
        error!("Or disable auth with CCA__DAEMON__REQUIRE_AUTH=false (dev only)");
        error!("============================================================");
    } else {
        info!(
            "API authentication: enabled ({} API key(s) configured)",
            config.daemon.api_keys.len()
        );
    }

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
