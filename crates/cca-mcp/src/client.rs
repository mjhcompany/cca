//! HTTP client for communicating with CCA daemon

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, error};

/// Minimal config structure to extract API key from cca.toml
#[derive(Debug, Deserialize, Default)]
struct MinimalConfig {
    #[serde(default)]
    daemon: MinimalDaemonConfig,
}

#[derive(Debug, Deserialize, Default)]
struct MinimalDaemonConfig {
    #[serde(default)]
    api_keys: Vec<String>,
}

/// HTTP client for daemon communication
pub struct DaemonClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl DaemonClient {
    /// Create a new daemon client
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();

        // Load API key from config file (same locations as daemon)
        let api_key = Self::load_api_key_from_config();

        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }

    /// Load API key from cca.toml config file
    fn load_api_key_from_config() -> Option<String> {
        let config_path = Self::find_config_file()?;

        let content = std::fs::read_to_string(&config_path).ok()?;
        let config: MinimalConfig = toml::from_str(&content).ok()?;

        config.daemon.api_keys.into_iter().next()
    }

    /// Find the configuration file (same logic as daemon)
    fn find_config_file() -> Option<PathBuf> {
        // Check in order: CCA_CONFIG env, system config, user config, current dir
        if let Ok(path) = std::env::var("CCA_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Some(path);
            }
        }

        // System-wide config (installed location)
        let system_config = PathBuf::from("/usr/local/etc/cca/cca.toml");
        if system_config.exists() {
            return Some(system_config);
        }

        // User config
        if let Some(home) = dirs::home_dir() {
            let user_config = home.join(".config").join("cca").join("cca.toml");
            if user_config.exists() {
                return Some(user_config);
            }
        }

        // Current directory (development)
        let local = PathBuf::from("cca.toml");
        if local.exists() {
            return Some(local);
        }

        None
    }

    /// Create a new daemon client with a specific API key
    pub fn with_api_key(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();

        Self {
            client: Client::new(),
            base_url,
            api_key: Some(api_key.into()),
        }
    }

    /// Check daemon health
    pub async fn health(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        debug!("GET {}", url);

        let mut request = self.client.get(&url);
        if let Some(ref api_key) = self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        match request.send().await {
            Ok(resp) if resp.status().is_success() => Ok(true),
            Ok(resp) => {
                error!("Health check failed: {}", resp.status());
                Ok(false)
            }
            Err(e) => {
                error!("Health check error: {}", e);
                Ok(false)
            }
        }
    }

    /// Get daemon status
    pub async fn status(&self) -> Result<DaemonStatus> {
        self.get("/api/v1/status").await
    }

    /// List agents
    pub async fn list_agents(&self) -> Result<AgentsResponse> {
        self.get("/api/v1/agents").await
    }

    /// Create a new task
    pub async fn create_task(&self, request: &CreateTaskRequest) -> Result<TaskResponse> {
        self.post("/api/v1/tasks", request).await
    }

    /// Get task status
    pub async fn get_task(&self, task_id: &str) -> Result<TaskResponse> {
        self.get(&format!("/api/v1/tasks/{task_id}")).await
    }

    /// Get activity of all agents
    pub async fn get_activity(&self) -> Result<ActivityResponse> {
        self.get("/api/v1/activity").await
    }

    /// Get ACP WebSocket server status
    pub async fn get_acp_status(&self) -> Result<AcpStatusResponse> {
        self.get("/api/v1/acp/status").await
    }

    /// Broadcast a message to all agents
    pub async fn broadcast(&self, message: &str) -> Result<BroadcastResponse> {
        self.post(
            "/api/v1/broadcast",
            &serde_json::json!({ "message": message }),
        )
        .await
    }

    /// Get workload distribution across agents
    pub async fn get_workloads(&self) -> Result<WorkloadsResponse> {
        self.get("/api/v1/workloads").await
    }

    /// Get PostgreSQL status
    pub async fn get_postgres_status(&self) -> Result<PostgresStatusResponse> {
        self.get("/api/v1/postgres/status").await
    }

    /// Search memory (ReasoningBank patterns)
    pub async fn search_memory(&self, query: &str, limit: i32) -> Result<MemorySearchResponse> {
        self.post(
            "/api/v1/memory/search",
            &serde_json::json!({ "query": query, "limit": limit }),
        )
        .await
    }

    /// Get RL stats
    pub async fn get_rl_stats(&self) -> Result<RLStatsResponse> {
        self.get("/api/v1/rl/stats").await
    }

    /// Trigger RL training
    pub async fn rl_train(&self) -> Result<RLTrainResponse> {
        self.post("/api/v1/rl/train", &serde_json::json!({})).await
    }

    /// Set RL algorithm
    pub async fn set_rl_algorithm(&self, algorithm: &str) -> Result<RLAlgorithmResponse> {
        self.post(
            "/api/v1/rl/algorithm",
            &serde_json::json!({ "algorithm": algorithm }),
        )
        .await
    }

    /// Get RL algorithm parameters
    pub async fn get_rl_params(&self) -> Result<RLParamsResponse> {
        self.get("/api/v1/rl/params").await
    }

    /// Set RL algorithm parameters
    pub async fn set_rl_params(&self, params: serde_json::Value) -> Result<RLParamsResponse> {
        self.post("/api/v1/rl/params", &params).await
    }

    /// Analyze context for token usage
    pub async fn tokens_analyze(&self, content: &str, agent_id: Option<&str>) -> Result<TokenAnalysisResponse> {
        self.post(
            "/api/v1/tokens/analyze",
            &serde_json::json!({ "content": content, "agent_id": agent_id }),
        )
        .await
    }

    /// Compress context with strategies
    pub async fn tokens_compress(
        &self,
        content: &str,
        strategies: Option<Vec<String>>,
        target_reduction: Option<f64>,
        agent_id: Option<&str>,
    ) -> Result<TokenCompressResponse> {
        self.post(
            "/api/v1/tokens/compress",
            &serde_json::json!({
                "content": content,
                "strategies": strategies,
                "target_reduction": target_reduction,
                "agent_id": agent_id
            }),
        )
        .await
    }

    /// Get token efficiency metrics
    pub async fn tokens_metrics(&self) -> Result<TokenMetricsResponse> {
        self.get("/api/v1/tokens/metrics").await
    }

    /// Get token optimization recommendations
    pub async fn tokens_recommendations(&self) -> Result<TokenRecommendationsResponse> {
        self.get("/api/v1/tokens/recommendations").await
    }

    /// Generic GET request
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET {}", url);

        let mut request = self.client.get(&url);

        // Add API key header if configured
        if let Some(ref api_key) = self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request.send().await.context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed ({status}): {body}");
        }

        response.json().await.context("Failed to parse response")
    }

    /// Generic POST request
    async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        debug!("POST {}", url);

        let mut request = self.client.post(&url).json(body);

        // Add API key header if configured
        if let Some(ref api_key) = self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request.send().await.context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed ({status}): {body}");
        }

        response.json().await.context("Failed to parse response")
    }
}

// Response types from daemon

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub status: String,
    pub version: String,
    #[serde(default)]
    pub agents_count: usize,
    #[serde(default)]
    pub tasks_pending: usize,
    #[serde(default)]
    pub tasks_completed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsResponse {
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    #[serde(default)]
    pub current_task: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub description: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub task_id: String,
    pub status: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub assigned_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityResponse {
    pub agents: Vec<AgentActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActivity {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    #[serde(default)]
    pub current_task: Option<String>,
    #[serde(default)]
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpWorker {
    pub agent_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpStatusResponse {
    pub running: bool,
    pub port: u16,
    pub connected_agents: usize,
    #[serde(default)]
    pub workers: Vec<AcpWorker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastResponse {
    pub success: bool,
    pub agents_notified: usize,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadsResponse {
    pub agents: Vec<AgentWorkload>,
    pub total_tasks: usize,
    pub pending_tasks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWorkload {
    pub agent_id: String,
    pub role: String,
    pub current_tasks: u32,
    pub max_tasks: u32,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresStatusResponse {
    pub connected: bool,
    #[serde(default)]
    pub pool_size: Option<u32>,
    #[serde(default)]
    pub patterns_count: Option<i64>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResponse {
    pub success: bool,
    #[serde(default)]
    pub patterns: Vec<PatternResult>,
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternResult {
    pub id: String,
    pub pattern_type: String,
    pub content: String,
    #[serde(default)]
    pub success_rate: Option<f64>,
    #[serde(default)]
    pub success_count: i32,
    #[serde(default)]
    pub failure_count: i32,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLStatsResponse {
    pub algorithm: String,
    #[serde(default)]
    pub total_steps: u64,
    #[serde(default)]
    pub total_rewards: f64,
    #[serde(default)]
    pub average_reward: f64,
    #[serde(default)]
    pub buffer_size: usize,
    #[serde(default)]
    pub last_training_loss: f64,
    #[serde(default)]
    pub experience_count: usize,
    #[serde(default)]
    pub algorithms_available: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLTrainResponse {
    pub success: bool,
    #[serde(default)]
    pub loss: f64,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLAlgorithmResponse {
    pub success: bool,
    #[serde(default)]
    pub algorithm: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLParamsResponse {
    pub success: bool,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

// Token efficiency response types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAnalysisResponse {
    pub success: bool,
    #[serde(default)]
    pub token_count: u32,
    #[serde(default)]
    pub repeated_lines: usize,
    #[serde(default)]
    pub compression_potential: f64,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCompressResponse {
    pub success: bool,
    #[serde(default)]
    pub original_tokens: u32,
    #[serde(default)]
    pub final_tokens: u32,
    #[serde(default)]
    pub tokens_saved: u32,
    #[serde(default)]
    pub reduction: String,
    #[serde(default)]
    pub compressed_content: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetricsResponse {
    pub success: bool,
    #[serde(default)]
    pub total_tokens_used: u64,
    #[serde(default)]
    pub total_tokens_saved: u64,
    #[serde(default)]
    pub efficiency_percent: f64,
    #[serde(default)]
    pub agent_count: usize,
    #[serde(default)]
    pub agents: Vec<AgentTokenMetrics>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTokenMetrics {
    pub agent_id: String,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub tokens_saved: u64,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub efficiency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecommendationsResponse {
    pub success: bool,
    #[serde(default)]
    pub recommendations: Vec<TokenRecommendation>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecommendation {
    pub category: String,
    pub message: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub potential_savings: Option<String>,
}
