//! Configuration loading for CCA Daemon

use std::path::PathBuf;

use anyhow::{Context, Result};
use config::{ConfigBuilder, Environment, File};
use serde::Deserialize;

/// Configuration for the daemon
#[derive(Debug, Clone, Default, Deserialize)]
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
    /// API keys for authentication (set via CCA__DAEMON__API_KEYS as comma-separated list)
    #[serde(default, deserialize_with = "deserialize_api_keys")]
    pub api_keys: Vec<String>,
    /// Whether authentication is required for API endpoints
    pub require_auth: bool,
    /// Log file path (empty means stdout only)
    pub log_file: String,
    /// Data directory containing agent .md files (defaults to /usr/local/share/cca or ./agents)
    pub data_dir: String,
}

/// Deserialize API keys from comma-separated string or array
fn deserialize_api_keys<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ApiKeys {
        String(String),
        Array(Vec<String>),
    }

    match ApiKeys::deserialize(deserializer)? {
        ApiKeys::String(s) => {
            if s.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(s.split(',').map(|s| s.trim().to_string()).collect())
            }
        }
        ApiKeys::Array(arr) => Ok(arr),
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9200".to_string(),
            log_level: "info".to_string(),
            max_agents: 10,
            api_keys: Vec::new(),
            require_auth: false, // Disabled by default for development
            log_file: String::new(), // Empty means stdout only
            data_dir: String::new(), // Empty means auto-detect
        }
    }
}

impl DaemonConfig {
    /// Get the data directory path, auto-detecting if not explicitly set
    pub fn get_data_dir(&self) -> PathBuf {
        if !self.data_dir.is_empty() {
            return PathBuf::from(&self.data_dir);
        }

        // Check standard locations in order:
        // 1. CCA_DATA_DIR environment variable
        // 2. ./agents (current directory - for development)
        // 3. /usr/local/share/cca (installed location)
        // 4. ~/.local/share/cca (user local)

        if let Ok(path) = std::env::var("CCA_DATA_DIR") {
            let p = PathBuf::from(&path);
            if p.exists() {
                return p;
            }
        }

        let local = PathBuf::from("agents");
        if local.exists() {
            return PathBuf::from("."); // Return parent so agents/{role}.md works
        }

        let system = PathBuf::from("/usr/local/share/cca");
        if system.exists() {
            return system;
        }

        if let Some(home) = dirs::home_dir() {
            let user_local = home.join(".local").join("share").join("cca");
            if user_local.exists() {
                return user_local;
            }
        }

        // Default fallback - current directory
        PathBuf::from(".")
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
            // Empty by default - must be explicitly configured
            // Set via CCA__REDIS__URL environment variable
            url: String::new(),
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
            // Empty by default - must be explicitly configured
            // Set via CCA__POSTGRES__URL environment variable
            // SECURITY: Never use hardcoded credentials in production
            url: String::new(),
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
    /// Path to the Claude Code binary (defaults to "claude" which must be in PATH)
    /// Set via CCA__AGENTS__CLAUDE_PATH environment variable if claude is not in PATH
    pub claude_path: String,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            default_timeout_seconds: 300,
            context_compression: true,
            token_budget_per_task: 50000,
            claude_path: "claude".to_string(),
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

        let config: Config = config
            .try_deserialize()
            .context("Failed to deserialize configuration")?;

        // Warn about unconfigured services
        if config.redis.url.is_empty() {
            tracing::warn!(
                "Redis URL not configured. Set CCA__REDIS__URL or redis.url in config file. \
                 Redis features will be disabled."
            );
        }

        if config.postgres.url.is_empty() {
            tracing::warn!(
                "PostgreSQL URL not configured. Set CCA__POSTGRES__URL or postgres.url in config file. \
                 PostgreSQL features will be disabled."
            );
        }

        // Warn about auth configuration
        if config.daemon.require_auth && config.daemon.api_keys.is_empty() {
            tracing::warn!(
                "Authentication is required but no API keys configured. \
                 Set CCA__DAEMON__API_KEYS to enable API access."
            );
        }

        Ok(config)
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
