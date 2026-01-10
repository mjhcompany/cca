//! Configuration loading for CCA Daemon

use std::path::PathBuf;

use anyhow::{Context, Result};
use config::{ConfigBuilder, Environment, File};
use serde::Deserialize;

/// Configuration for the daemon
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub redis: RedisConfig,
    pub postgres: PostgresConfig,
    pub agents: AgentsConfig,
    pub acp: AcpConfig,
    pub mcp: McpConfig,
    pub learning: LearningConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub bind_address: String,
    pub log_level: String,
    pub max_agents: usize,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9200".to_string(),
            log_level: "info".to_string(),
            max_agents: 10,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: usize,
    pub context_ttl_seconds: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6380".to_string(),
            pool_size: 10,
            context_ttl_seconds: 3600,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PostgresConfig {
    pub url: String,
    pub pool_size: u32,
    pub max_connections: u32,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            url: "postgres://cca:cca@localhost:5433/cca".to_string(),
            pool_size: 10,
            max_connections: 20,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AgentsConfig {
    pub default_timeout_seconds: u64,
    pub context_compression: bool,
    pub token_budget_per_task: u64,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            default_timeout_seconds: 300,
            context_compression: true,
            token_budget_per_task: 50000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AcpConfig {
    pub websocket_port: u16,
    pub reconnect_interval_ms: u64,
    pub max_reconnect_attempts: u32,
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            websocket_port: 9100,
            reconnect_interval_ms: 1000,
            max_reconnect_attempts: 5,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    pub bind_address: String,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "127.0.0.1:9201".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LearningConfig {
    pub enabled: bool,
    pub default_algorithm: String,
    pub training_batch_size: usize,
    pub update_interval_seconds: u64,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_algorithm: "ppo".to_string(),
            training_batch_size: 32,
            update_interval_seconds: 300,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            redis: RedisConfig::default(),
            postgres: PostgresConfig::default(),
            agents: AgentsConfig::default(),
            acp: AcpConfig::default(),
            mcp: McpConfig::default(),
            learning: LearningConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file and environment
    pub fn load() -> Result<Self> {
        let config_path = Self::find_config_file();

        let mut builder = ConfigBuilder::<config::builder::DefaultState>::default();

        // Add config file if it exists
        if let Some(path) = &config_path {
            tracing::info!("Loading config from: {:?}", path);
            builder = builder.add_source(File::from(path.clone()).required(false));
        } else {
            tracing::info!("No config file found, using defaults");
        }

        // Add environment variables with CCA_ prefix
        builder = builder.add_source(
            Environment::with_prefix("CCA")
                .separator("__")
                .try_parsing(true),
        );

        let config = builder.build()?;

        config
            .try_deserialize()
            .context("Failed to deserialize configuration")
    }

    /// Find the configuration file
    fn find_config_file() -> Option<PathBuf> {
        // Check in order: CCA_CONFIG env, ./cca.toml, ~/.config/cca/cca.toml
        if let Ok(path) = std::env::var("CCA_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Some(path);
            }
        }

        let local = PathBuf::from("cca.toml");
        if local.exists() {
            return Some(local);
        }

        if let Some(home) = dirs::home_dir() {
            let user_config = home.join(".config").join("cca").join("cca.toml");
            if user_config.exists() {
                return Some(user_config);
            }
        }

        None
    }
}
