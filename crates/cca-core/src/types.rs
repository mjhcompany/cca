//! Common types used throughout CCA

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Pattern identifier for ReasoningBank
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatternId(pub Uuid);

impl PatternId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PatternId {
    fn default() -> Self {
        Self::new()
    }
}

/// Timestamped wrapper for any value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timestamped<T> {
    pub value: T,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl<T> Timestamped<T> {
    pub fn new(value: T) -> Self {
        let now = Utc::now();
        Self {
            value,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn update(&mut self, value: T) {
        self.value = value;
        self.updated_at = Utc::now();
    }
}

/// Configuration for CCA daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CCAConfig {
    pub daemon: DaemonConfig,
    pub redis: RedisConfig,
    pub postgres: PostgresConfig,
    pub agents: AgentsConfig,
    pub acp: AcpConfig,
    pub mcp: McpConfig,
    pub learning: LearningConfig,
    pub token_efficiency: TokenEfficiencyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub bind_address: String,
    pub log_level: String,
    pub max_agents: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: usize,
    pub context_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
    pub pool_size: u32,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    pub default_timeout_seconds: u64,
    pub context_compression: bool,
    pub token_budget_per_task: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConfig {
    pub websocket_port: u16,
    pub reconnect_interval_ms: u64,
    pub max_reconnect_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub enabled: bool,
    pub bind_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConfig {
    pub enabled: bool,
    pub default_algorithm: String,
    pub training_batch_size: usize,
    pub update_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEfficiencyConfig {
    pub enabled: bool,
    pub target_reduction: f64,
    pub compression_algorithm: String,
}

impl Default for CCAConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig {
                bind_address: "127.0.0.1:9200".to_string(),
                log_level: "info".to_string(),
                max_agents: 10,
            },
            redis: RedisConfig {
                url: "redis://localhost:6380".to_string(),
                pool_size: 10,
                context_ttl_seconds: 3600,
            },
            postgres: PostgresConfig {
                url: "postgres://cca:cca@localhost:5433/cca".to_string(),
                pool_size: 10,
                max_connections: 20,
            },
            agents: AgentsConfig {
                default_timeout_seconds: 300,
                context_compression: true,
                token_budget_per_task: 50000,
            },
            acp: AcpConfig {
                websocket_port: 9100,
                reconnect_interval_ms: 1000,
                max_reconnect_attempts: 5,
            },
            mcp: McpConfig {
                enabled: true,
                bind_address: "127.0.0.1:9201".to_string(),
            },
            learning: LearningConfig {
                enabled: true,
                default_algorithm: "ppo".to_string(),
                training_batch_size: 32,
                update_interval_seconds: 300,
            },
            token_efficiency: TokenEfficiencyConfig {
                enabled: true,
                target_reduction: 0.30,
                compression_algorithm: "context_distillation".to_string(),
            },
        }
    }
}
