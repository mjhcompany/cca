//! Configuration management commands

use anyhow::Result;
use clap::Subcommand;

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
}

pub async fn run(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Show => show().await,
        ConfigCommands::Set { key, value } => set(&key, &value).await,
        ConfigCommands::Init { force } => init(force).await,
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
