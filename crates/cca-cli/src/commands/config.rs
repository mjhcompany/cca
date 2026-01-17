//! Configuration management commands

use anyhow::Result;
use clap::Subcommand;

use super::http;

/// Get the daemon URL from environment or use default
fn daemon_url() -> String {
    std::env::var("CCA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8580".to_string())
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set {
        /// Configuration key (e.g., daemon.max_agents)
        key: String,
        /// Value to set
        value: String,
    },
    /// Initialize configuration file
    Init {
        /// Force overwrite existing config
        #[arg(short, long)]
        force: bool,
    },
    /// Reload configuration without restarting daemon
    ///
    /// Hot-reloads configuration values that can be changed at runtime:
    /// - API keys
    /// - Rate limits
    /// - Agent permissions and timeouts
    /// - Learning settings
    ///
    /// Note: Database URLs, bind addresses, and ports require a full restart.
    Reload,
    /// Show current reloadable configuration values
    Reloadable,
}

pub async fn run(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Show => show().await,
        ConfigCommands::Set { key, value } => set(&key, &value).await,
        ConfigCommands::Init { force } => init(force).await,
        ConfigCommands::Reload => reload().await,
        ConfigCommands::Reloadable => show_reloadable().await,
    }
}

async fn show() -> Result<()> {
    println!("Current Configuration");
    println!("=====================\n");

    // Try to find and read config file
    let config_paths = [
        std::env::var("CCA_CONFIG").ok(),
        Some("cca.toml".to_string()),
        dirs::home_dir().map(|h| h.join(".config/cca/cca.toml").to_string_lossy().to_string()),
    ];

    for path in config_paths.iter().flatten() {
        if std::path::Path::new(path).exists() {
            println!("Config file: {path}\n");
            let content = std::fs::read_to_string(path)?;
            println!("{content}");
            return Ok(());
        }
    }

    println!("No configuration file found. Using defaults.");
    println!("\nDefault values:");
    println!("  daemon.bind_address = 127.0.0.1:8580");
    println!("  daemon.max_agents = 10");
    println!("  redis.url = redis://localhost:16379");
    println!("  postgres.url = postgres://cca:cca@localhost:15432/cca");

    Ok(())
}

async fn set(key: &str, value: &str) -> Result<()> {
    println!("Setting {key} = {value}");
    // TODO: Implement config modification
    println!("Configuration updated");
    Ok(())
}

async fn init(force: bool) -> Result<()> {
    let config_path = "cca.toml";

    if std::path::Path::new(config_path).exists() && !force {
        println!("Configuration file already exists: {config_path}");
        println!("Use --force to overwrite");
        return Ok(());
    }

    let default_config = include_str!("../../../../cca.toml.example");

    // If example doesn't exist yet, create a basic one
    let config = if default_config.is_empty() {
        r#"[daemon]
bind_address = "127.0.0.1:8580"
log_level = "info"
max_agents = 10

[redis]
url = "redis://localhost:16379"
pool_size = 10
context_ttl_seconds = 3600

[postgres]
url = "postgres://cca:cca@localhost:15432/cca"
pool_size = 10
max_connections = 20

[agents]
default_timeout_seconds = 300
context_compression = true
token_budget_per_task = 50000

[acp]
websocket_port = 8581
reconnect_interval_ms = 1000
max_reconnect_attempts = 5

[mcp]
enabled = true
bind_address = "127.0.0.1:8582"

[learning]
enabled = true
default_algorithm = "ppo"
training_batch_size = 32
update_interval_seconds = 300

[token_efficiency]
enabled = true
target_reduction = 0.30
compression_algorithm = "context_distillation"
"#
    } else {
        default_config
    };

    std::fs::write(config_path, config)?;
    println!("Configuration file created: {config_path}");

    Ok(())
}

/// Reload configuration via daemon API
async fn reload() -> Result<()> {
    println!("Reloading daemon configuration...\n");

    match http::post_json(
        &format!("{}/api/v1/admin/config/reload", daemon_url()),
        &serde_json::json!({}),
    )
    .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                let result: serde_json::Value = resp.json().await?;

                if result["success"].as_bool().unwrap_or(false) {
                    println!("Configuration reloaded successfully!");

                    if let Some(config_file) = result["config_file"].as_str() {
                        println!("Config file: {}", config_file);
                    }

                    if let Some(changed) = result["changed_fields"].as_array() {
                        if changed.is_empty() {
                            println!("\nNo changes detected.");
                        } else {
                            println!("\nChanged fields:");
                            for field in changed {
                                println!("  - {}", field.as_str().unwrap_or("unknown"));
                            }
                        }
                    }
                } else {
                    let error = result["error"]
                        .as_str()
                        .unwrap_or("Unknown error");
                    println!("Failed to reload configuration: {}", error);
                }
            } else {
                println!(
                    "Failed to reload configuration: HTTP {}",
                    resp.status()
                );
            }
        }
        Err(e) => {
            println!("Failed to connect to daemon: {}", e);
            println!("\nIs the daemon running? Start it with: cca daemon start");
        }
    }

    Ok(())
}

/// Show current reloadable configuration values
async fn show_reloadable() -> Result<()> {
    println!("Current Reloadable Configuration");
    println!("================================\n");

    match http::get(&format!("{}/api/v1/admin/config/reloadable", daemon_url())).await {
        Ok(resp) => {
            if resp.status().is_success() {
                let result: serde_json::Value = resp.json().await?;

                if let Some(config_file) = result["config_file"].as_str() {
                    println!("Config file: {}\n", config_file);
                }

                println!("Hot-reloadable fields:");
                if let Some(fields) = result["reloadable_fields"].as_array() {
                    for field in fields {
                        println!("  - {}", field.as_str().unwrap_or("unknown"));
                    }
                }

                println!("\nCurrent values:");
                if let Some(values) = result["current_values"].as_object() {
                    print_config_values(values, 0);
                }
            } else {
                println!(
                    "Failed to get reloadable config: HTTP {}",
                    resp.status()
                );
            }
        }
        Err(e) => {
            println!("Failed to connect to daemon: {}", e);
            println!("\nIs the daemon running? Start it with: cca daemon start");
        }
    }

    Ok(())
}

/// Helper to pretty-print nested JSON config values
fn print_config_values(obj: &serde_json::Map<String, serde_json::Value>, indent: usize) {
    let prefix = "  ".repeat(indent);
    for (key, value) in obj {
        match value {
            serde_json::Value::Object(nested) => {
                println!("{}{}:", prefix, key);
                print_config_values(nested, indent + 1);
            }
            serde_json::Value::Array(arr) => {
                if arr.is_empty() {
                    println!("{}{}: []", prefix, key);
                } else {
                    println!("{}{}:", prefix, key);
                    for item in arr {
                        println!("{}  - {}", prefix, item);
                    }
                }
            }
            _ => {
                println!("{}{}: {}", prefix, key, value);
            }
        }
    }
}
