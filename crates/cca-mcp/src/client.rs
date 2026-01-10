//! HTTP client for communicating with CCA daemon

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, error};

/// HTTP client for daemon communication
pub struct DaemonClient {
    client: Client,
    base_url: String,
}

impl DaemonClient {
    /// Create a new daemon client
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();

        Self {
            client: Client::new(),
            base_url,
        }
    }

    /// Check daemon health
    pub async fn health(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        debug!("GET {}", url);

        match self.client.get(&url).send().await {
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
        self.get(&format!("/api/v1/tasks/{}", task_id)).await
    }

    /// Get activity of all agents
    pub async fn get_activity(&self) -> Result<ActivityResponse> {
        self.get("/api/v1/activity").await
    }

    /// Generic GET request
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed ({}): {}", status, body);
        }

        response.json().await.context("Failed to parse response")
    }

    /// Generic POST request
    async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        debug!("POST {}", url);

        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed ({}): {}", status, body);
        }

        response.json().await.context("Failed to parse response")
    }
}

// Response types from daemon

use serde::Deserialize;

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
    pub id: String,
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
