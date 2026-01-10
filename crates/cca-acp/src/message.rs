//! ACP message types and handling

use serde::{Deserialize, Serialize};


/// ACP method names
pub mod methods {
    pub const SEND_MESSAGE: &str = "sendMessage";
    pub const GET_STATUS: &str = "getStatus";
    pub const EXECUTE_TASK: &str = "executeTask";
    pub const CANCEL_TASK: &str = "cancelTask";
    pub const HEARTBEAT: &str = "heartbeat";
}

/// Parameters for sendMessage method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageParams {
    pub to: String,
    pub content: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Parameters for executeTask method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteTaskParams {
    pub description: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Parameters for cancelTask method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskParams {
    pub task_id: String,
}

/// Status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub agent_id: String,
    pub state: String,
    pub current_task: Option<String>,
    pub uptime_seconds: u64,
}

/// Heartbeat parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatParams {
    pub timestamp: i64,
}

/// Heartbeat response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub timestamp: i64,
    pub server_time: i64,
}
