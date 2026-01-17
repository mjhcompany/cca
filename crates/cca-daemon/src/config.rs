//! Configuration loading for CCA Daemon

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use config::{ConfigBuilder, Environment, File};
use serde::Deserialize;
use tokio::sync::RwLock;

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
    pub embeddings: EmbeddingsConfig,
    pub indexing: IndexingConfig,
}

/// Configuration for an API key with role permissions
#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeyConfig {
    /// The API key value
    pub key: String,
    /// Roles this key is authorized to register as (empty = all roles allowed)
    #[serde(default)]
    pub allowed_roles: Vec<String>,
    /// Optional identifier for this key (for logging, never expose key itself)
    #[serde(default)]
    pub key_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub bind_address: String,
    pub log_level: String,
    pub max_agents: usize,
    /// API keys for authentication (set via `CCA__DAEMON__API_KEYS` as comma-separated list)
    /// These are legacy keys with no role restrictions
    #[serde(default, deserialize_with = "deserialize_api_keys")]
    pub api_keys: Vec<String>,
    /// API keys with role-based permissions (preferred over `api_keys`)
    /// Configure via config file for role restrictions
    #[serde(default)]
    pub api_key_configs: Vec<ApiKeyConfig>,
    /// Whether authentication is required for API endpoints
    pub require_auth: bool,
    /// Log file path (empty means stdout only)
    pub log_file: String,
    /// Data directory containing agent .md files (defaults to /usr/local/share/cca or ./agents)
    pub data_dir: String,
    /// Rate limit: requests per second per IP (0 = disabled)
    /// SEC-004: Per-IP rate limiting for DoS protection
    pub rate_limit_rps: u32,
    /// Rate limit burst size: max requests allowed in a burst before limiting kicks in
    /// `SEC-004`: Allows short bursts while maintaining average rate
    pub rate_limit_burst: u32,
    /// Global rate limit: total requests per second across all IPs (`0` = disabled)
    /// `SEC-004`: Absolute limit to prevent distributed DoS
    pub rate_limit_global_rps: u32,
    /// Whether to trust `X-Forwarded-For` header for client IP (only enable behind trusted proxy)
    /// `SEC-004`: SECURITY WARNING - enabling this behind untrusted proxies allows IP spoofing
    pub rate_limit_trust_proxy: bool,
    /// Rate limit: requests per second per API key (`0` = disabled)
    /// `SEC-004`: Per-API-key rate limiting for authenticated clients
    pub rate_limit_api_key_rps: u32,
    /// Rate limit burst size for API key rate limiting
    /// `SEC-004`: Allows short bursts for authenticated clients
    pub rate_limit_api_key_burst: u32,
    /// `SEC-010`: CORS allowed origins (comma-separated list or array)
    /// Empty list means CORS is disabled (no cross-origin requests allowed)
    /// Use `"*"` for development only - NEVER in production
    /// Example: `"https://app.example.com,https://admin.example.com"`
    /// Set via `CCA__DAEMON__CORS_ORIGINS` environment variable
    #[serde(default, deserialize_with = "deserialize_cors_origins")]
    pub cors_origins: Vec<String>,
    /// `SEC-010`: Whether to allow credentials in CORS requests
    /// Only set to true if `cors_origins` contains explicit origins (not `"*"`)
    pub cors_allow_credentials: bool,
    /// `SEC-010`: Max age in seconds for CORS preflight cache (default: 3600 = 1 hour)
    pub cors_max_age_secs: u64,
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

/// SEC-010: Deserialize CORS origins from comma-separated string or array
fn deserialize_cors_origins<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum CorsOrigins {
        String(String),
        Array(Vec<String>),
    }

    match CorsOrigins::deserialize(deserializer)? {
        CorsOrigins::String(s) => {
            if s.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(s.split(',').map(|s| s.trim().to_string()).collect())
            }
        }
        CorsOrigins::Array(arr) => Ok(arr),
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        // SECURITY: Authentication is ALWAYS ENABLED by default.
        // In production builds (without "dev" feature), auth cannot be disabled.
        // Only in dev builds can you set CCA__DAEMON__REQUIRE_AUTH=false.
        Self {
            bind_address: "127.0.0.1:8580".to_string(),
            log_level: "info".to_string(),
            max_agents: 10,
            api_keys: Vec::new(),
            api_key_configs: Vec::new(),
            require_auth: true, // SECURITY: Enabled by default, enforced in production
            log_file: String::new(), // Empty means stdout only
            data_dir: String::new(), // Empty means auto-detect
            // SEC-004: Rate limiting defaults
            rate_limit_rps: 100,           // 100 requests/second per IP
            rate_limit_burst: 50,          // Allow bursts of 50 requests
            rate_limit_global_rps: 1000,   // 1000 total requests/second globally
            rate_limit_trust_proxy: false, // Don't trust proxy headers by default
            rate_limit_api_key_rps: 200,   // 200 requests/second per API key (higher for authenticated)
            rate_limit_api_key_burst: 100, // Allow bursts of 100 requests for authenticated clients
            // SEC-010: CORS defaults - disabled by default (empty origins)
            cors_origins: Vec::new(),         // No origins allowed by default (CORS disabled)
            cors_allow_credentials: false,    // Don't allow credentials by default
            cors_max_age_secs: 3600,          // Cache preflight for 1 hour
        }
    }
}

impl DaemonConfig {
    /// Returns whether authentication is required.
    /// SECURITY: In production builds (without "dev" feature), this ALWAYS returns true.
    /// The `require_auth` config option is only respected in dev builds.
    #[inline]
    #[allow(clippy::unused_self)]  // self is used in dev builds
    pub fn is_auth_required(&self) -> bool {
        #[cfg(feature = "dev")]
        {
            self.require_auth
        }
        #[cfg(not(feature = "dev"))]
        {
            // SECURITY: Production builds ALWAYS require authentication
            // This cannot be disabled via configuration
            true
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
    /// Statement timeout in milliseconds for PostgreSQL queries (`STAB-004`)
    /// This is set via the `statement_timeout` connection parameter
    /// Set via `CCA__POSTGRES__STATEMENT_TIMEOUT_MS` environment variable
    pub statement_timeout_ms: u64,
    /// Query timeout in seconds for application-level timeout (`STAB-004`)
    /// This wraps queries with `tokio::time::timeout` as a safety net
    /// Set via `CCA__POSTGRES__QUERY_TIMEOUT_SECS` environment variable
    pub query_timeout_secs: u64,
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
            // STAB-004: Default statement timeout of 30 seconds
            statement_timeout_ms: 30_000,
            // STAB-004: Default query timeout of 60 seconds (gives buffer above statement_timeout)
            query_timeout_secs: 60,
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
    /// Set via `CCA__AGENTS__CLAUDE_PATH` environment variable if claude is not in PATH
    pub claude_path: String,
    /// `SEC-007`: Permission configuration for Claude Code invocations
    /// Controls how agent permissions are handled instead of blanket `--dangerously-skip-permissions`
    pub permissions: PermissionsConfig,
}

/// Deserialize tool list from comma-separated string or array
fn deserialize_tool_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ToolList {
        String(String),
        Array(Vec<String>),
    }

    match ToolList::deserialize(deserializer)? {
        ToolList::String(s) => {
            if s.is_empty() {
                Ok(Vec::new())
            } else {
                // Handle both comma and semicolon separators (semicolon useful for tools with patterns)
                Ok(s.split([',', ';'])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect())
            }
        }
        ToolList::Array(arr) => Ok(arr),
    }
}

/// Role-specific permission overrides for `SEC-007`
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RolePermissions {
    /// Override `allowed_tools` for this role (if set, replaces default)
    #[serde(default, deserialize_with = "deserialize_tool_list")]
    pub allowed_tools: Vec<String>,

    /// Additional denied tools for this role (merged with global `denied_tools`)
    #[serde(default, deserialize_with = "deserialize_tool_list")]
    pub denied_tools: Vec<String>,

    /// Override permission mode for this role
    pub mode: Option<String>,
}

/// `SEC-007`: Permission configuration for Claude Code invocations
/// Replaces blanket `--dangerously-skip-permissions` with granular control
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PermissionsConfig {
    /// Permission mode: "allowlist" (default, secure), "sandbox", or "dangerous" (legacy)
    /// - allowlist: Uses `--allowedTools` with configured tool list (recommended)
    /// - sandbox: Expects external sandboxing (container/VM), uses minimal permissions
    /// - dangerous: Uses `--dangerously-skip-permissions` (NOT RECOMMENDED, legacy only)
    pub mode: String,

    /// Tools allowed without prompting when mode is "allowlist"
    /// Examples: "Read", "Write(src/\*\*)", "Bash(git \*)", "Bash(npm \*)"
    /// Set via `CCA__AGENTS__PERMISSIONS__ALLOWED_TOOLS` as comma-separated list
    #[serde(default, deserialize_with = "deserialize_tool_list")]
    pub allowed_tools: Vec<String>,

    /// Tools explicitly denied (blocklist) - applied in all modes except "dangerous"
    /// Examples: "Bash(rm -rf \*)", "Bash(sudo \*)", "Write(.env\*)"
    /// Set via `CCA__AGENTS__PERMISSIONS__DENIED_TOOLS` as comma-separated list
    #[serde(default, deserialize_with = "deserialize_tool_list")]
    pub denied_tools: Vec<String>,

    /// Working directory restriction - agents can only access files under this path
    /// Empty means current working directory (most restrictive)
    /// Set via `CCA__AGENTS__PERMISSIONS__WORKING_DIR`
    pub working_dir: String,

    /// Whether to allow network access in Bash commands
    /// When false, adds `Bash(curl*)`, `Bash(wget*)`, etc. to `denied_tools` automatically
    pub allow_network: bool,

    /// Role-specific permission overrides
    /// Allows different roles to have different permission levels
    #[serde(default)]
    pub role_overrides: std::collections::HashMap<String, RolePermissions>,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            // SEC-007: Default to allowlist mode (secure by default)
            mode: "allowlist".to_string(),

            // Default allowed tools - safe read/write operations with restrictions
            // These allow Claude to do its job without dangerous operations
            allowed_tools: vec![
                // File reading is generally safe
                "Read".to_string(),
                // Glob/Grep for code exploration
                "Glob".to_string(),
                "Grep".to_string(),
                // Restricted write operations (no system files, no secrets)
                "Write(src/**)".to_string(),
                "Write(tests/**)".to_string(),
                "Write(docs/**)".to_string(),
                // Common safe git operations
                "Bash(git status)".to_string(),
                "Bash(git diff*)".to_string(),
                "Bash(git log*)".to_string(),
                "Bash(git show*)".to_string(),
                "Bash(git branch*)".to_string(),
            ],

            // Default denied tools - dangerous operations
            denied_tools: vec![
                // Destructive file operations
                "Bash(rm -rf *)".to_string(),
                "Bash(rm -r *)".to_string(),
                // Privilege escalation
                "Bash(sudo *)".to_string(),
                "Bash(su *)".to_string(),
                // Sensitive file access
                "Read(.env*)".to_string(),
                "Write(.env*)".to_string(),
                "Read(*credentials*)".to_string(),
                "Write(*credentials*)".to_string(),
                "Read(*secret*)".to_string(),
                "Write(*secret*)".to_string(),
                // System modifications
                "Bash(chmod 777 *)".to_string(),
                "Bash(chown *)".to_string(),
            ],

            working_dir: String::new(),
            allow_network: false,
            role_overrides: std::collections::HashMap::new(),
        }
    }
}

impl PermissionsConfig {
    /// Get effective allowed tools for a role, considering overrides
    pub fn get_allowed_tools(&self, role: &str) -> Vec<String> {
        if let Some(override_config) = self.role_overrides.get(role) {
            if !override_config.allowed_tools.is_empty() {
                return override_config.allowed_tools.clone();
            }
        }
        self.allowed_tools.clone()
    }

    /// Get effective denied tools for a role (merges global + role-specific)
    pub fn get_denied_tools(&self, role: &str) -> Vec<String> {
        let mut denied = self.denied_tools.clone();

        // Add network restrictions if not allowed
        if !self.allow_network {
            denied.extend(vec![
                "Bash(curl *)".to_string(),
                "Bash(wget *)".to_string(),
                "Bash(nc *)".to_string(),
                "Bash(netcat *)".to_string(),
            ]);
        }

        // Add role-specific denials
        if let Some(override_config) = self.role_overrides.get(role) {
            denied.extend(override_config.denied_tools.clone());
        }

        denied
    }

    /// Get effective permission mode for a role
    pub fn get_mode(&self, role: &str) -> &str {
        if let Some(override_config) = self.role_overrides.get(role) {
            if let Some(ref mode) = override_config.mode {
                return mode;
            }
        }
        &self.mode
    }
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            default_timeout_seconds: 600, // 10 minutes for complex analysis tasks
            context_compression: true,
            token_budget_per_task: 50000,
            claude_path: "claude".to_string(),
            permissions: PermissionsConfig::default(),
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
            websocket_port: 8581,
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

/// Configuration for embedding service (semantic search via Ollama)
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmbeddingsConfig {
    /// Whether embeddings are enabled
    pub enabled: bool,
    /// Ollama API base URL (e.g., `"http://192.168.33.218:11434"`)
    pub ollama_url: String,
    /// Model name for embeddings (e.g., "nomic-embed-text:latest")
    pub model: String,
    /// Expected embedding dimension (768 for nomic-embed-text)
    pub dimension: usize,
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default until configured
            ollama_url: "http://localhost:11434".to_string(),
            model: "nomic-embed-text:latest".to_string(),
            dimension: 768,
        }
    }
}

/// Configuration for codebase indexing
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IndexingConfig {
    /// Enable code indexing features
    pub enabled: bool,
    /// Default batch size for embedding generation
    pub batch_size: usize,
    /// Maximum chunk size in characters
    pub max_chunk_size: usize,
    /// Default file extensions to index
    #[serde(default, deserialize_with = "deserialize_tool_list")]
    pub default_extensions: Vec<String>,
    /// Default exclude patterns (glob format)
    #[serde(default, deserialize_with = "deserialize_tool_list")]
    pub default_excludes: Vec<String>,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            batch_size: 10,
            max_chunk_size: 4000,
            default_extensions: vec![
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "jsx".to_string(),
                "tsx".to_string(),
                "go".to_string(),
                "java".to_string(),
                "c".to_string(),
                "cpp".to_string(),
                "h".to_string(),
                "hpp".to_string(),
            ],
            default_excludes: vec![
                "**/node_modules/**".to_string(),
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
                "**/vendor/**".to_string(),
                "**/__pycache__/**".to_string(),
                "**/dist/**".to_string(),
                "**/build/**".to_string(),
            ],
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
        // SECURITY: Use is_auth_required() which enforces auth in production builds
        if config.daemon.is_auth_required() && config.daemon.api_keys.is_empty() {
            tracing::warn!(
                "Authentication is required but no API keys configured. \
                 Set CCA__DAEMON__API_KEYS to enable API access."
            );
        }

        Ok(config)
    }

    /// Find the configuration file
    fn find_config_file() -> Option<PathBuf> {
        Self::find_config_file_path()
    }

    /// Find the configuration file path (public for reload)
    pub fn find_config_file_path() -> Option<PathBuf> {
        // Check in order: CCA_CONFIG env, ./cca.toml, /usr/local/etc/cca/cca.toml, ~/.config/cca/cca.toml
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

        // System-wide config (installed location)
        let system = PathBuf::from("/usr/local/etc/cca/cca.toml");
        if system.exists() {
            return Some(system);
        }

        if let Some(home) = dirs::home_dir() {
            let user_config = home.join(".config").join("cca").join("cca.toml");
            if user_config.exists() {
                return Some(user_config);
            }
        }

        None
    }

    /// Extract reloadable configuration for hot-reload
    pub fn to_reloadable(&self) -> ReloadableConfig {
        ReloadableConfig {
            // Auth settings
            api_keys: self.daemon.api_keys.clone(),
            api_key_configs: self.daemon.api_key_configs.clone(),
            // Rate limiting settings
            rate_limit_rps: self.daemon.rate_limit_rps,
            rate_limit_burst: self.daemon.rate_limit_burst,
            rate_limit_global_rps: self.daemon.rate_limit_global_rps,
            rate_limit_api_key_rps: self.daemon.rate_limit_api_key_rps,
            rate_limit_api_key_burst: self.daemon.rate_limit_api_key_burst,
            // Agent settings
            default_timeout_seconds: self.agents.default_timeout_seconds,
            permissions: self.agents.permissions.clone(),
            token_budget_per_task: self.agents.token_budget_per_task,
            // Learning settings
            learning_enabled: self.learning.enabled,
            training_batch_size: self.learning.training_batch_size,
        }
    }
}

/// Hot-reloadable configuration fields
///
/// These configuration values can be updated at runtime without restarting the daemon.
/// Changes to these fields take effect immediately after a reload.
///
/// Note: Database URLs, bind addresses, and port numbers are NOT reloadable
/// as they require service recreation which would break existing connections.
#[derive(Debug, Clone)]
pub struct ReloadableConfig {
    // Auth settings - can be reloaded without breaking connections
    /// API keys for authentication
    pub api_keys: Vec<String>,
    /// API keys with role-based permissions
    pub api_key_configs: Vec<ApiKeyConfig>,

    // Rate limiting settings - can be reloaded (new limits apply to new requests)
    /// Requests per second per IP
    pub rate_limit_rps: u32,
    /// Burst size for IP rate limiting
    pub rate_limit_burst: u32,
    /// Global rate limit
    pub rate_limit_global_rps: u32,
    /// Requests per second per API key
    pub rate_limit_api_key_rps: u32,
    /// Burst size for API key rate limiting
    pub rate_limit_api_key_burst: u32,

    // Agent settings - can be reloaded for new tasks
    /// Default timeout for agent operations
    pub default_timeout_seconds: u64,
    /// Permission configuration
    pub permissions: PermissionsConfig,
    /// Token budget per task
    pub token_budget_per_task: u64,

    // Learning settings - can be reloaded
    /// Whether learning is enabled
    pub learning_enabled: bool,
    /// Training batch size
    pub training_batch_size: usize,
}

impl Default for ReloadableConfig {
    fn default() -> Self {
        let config = Config::default();
        config.to_reloadable()
    }
}

/// Wrapper for thread-safe reloadable configuration
pub type SharedReloadableConfig = Arc<RwLock<ReloadableConfig>>;

/// Result of a configuration reload operation
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReloadResult {
    /// Whether the reload was successful
    pub success: bool,
    /// Config file that was reloaded
    pub config_file: Option<String>,
    /// Fields that were changed
    pub changed_fields: Vec<String>,
    /// Error message if reload failed
    pub error: Option<String>,
}

impl ReloadableConfig {
    /// Compare with another config and return list of changed fields
    pub fn diff(&self, other: &ReloadableConfig) -> Vec<String> {
        let mut changes = Vec::new();

        if self.api_keys != other.api_keys {
            changes.push("api_keys".to_string());
        }
        if self.api_key_configs.len() != other.api_key_configs.len() {
            changes.push("api_key_configs".to_string());
        }
        if self.rate_limit_rps != other.rate_limit_rps {
            changes.push("rate_limit_rps".to_string());
        }
        if self.rate_limit_burst != other.rate_limit_burst {
            changes.push("rate_limit_burst".to_string());
        }
        if self.rate_limit_global_rps != other.rate_limit_global_rps {
            changes.push("rate_limit_global_rps".to_string());
        }
        if self.rate_limit_api_key_rps != other.rate_limit_api_key_rps {
            changes.push("rate_limit_api_key_rps".to_string());
        }
        if self.rate_limit_api_key_burst != other.rate_limit_api_key_burst {
            changes.push("rate_limit_api_key_burst".to_string());
        }
        if self.default_timeout_seconds != other.default_timeout_seconds {
            changes.push("default_timeout_seconds".to_string());
        }
        if self.permissions.mode != other.permissions.mode {
            changes.push("permissions.mode".to_string());
        }
        if self.permissions.allowed_tools != other.permissions.allowed_tools {
            changes.push("permissions.allowed_tools".to_string());
        }
        if self.permissions.denied_tools != other.permissions.denied_tools {
            changes.push("permissions.denied_tools".to_string());
        }
        if self.token_budget_per_task != other.token_budget_per_task {
            changes.push("token_budget_per_task".to_string());
        }
        if self.learning_enabled != other.learning_enabled {
            changes.push("learning_enabled".to_string());
        }
        if self.training_batch_size != other.training_batch_size {
            changes.push("training_batch_size".to_string());
        }

        changes
    }
}
