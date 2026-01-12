//! ACP message types and handling

use serde::{Deserialize, Serialize};

use cca_core::{AgentId, TaskId};

/// ACP method names
pub mod methods {
    pub const SEND_MESSAGE: &str = "sendMessage";
    pub const GET_STATUS: &str = "getStatus";
    pub const EXECUTE_TASK: &str = "executeTask";
    pub const CANCEL_TASK: &str = "cancelTask";
    pub const HEARTBEAT: &str = "heartbeat";
    pub const TASK_ASSIGN: &str = "taskAssign";
    pub const TASK_RESULT: &str = "taskResult";
    pub const TASK_PROGRESS: &str = "taskProgress";
    pub const BROADCAST: &str = "broadcast";
    pub const QUERY_AGENT: &str = "queryAgent";
    pub const REGISTER_AGENT: &str = "registerAgent";
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

/// Parameters for task assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignParams {
    pub task_id: TaskId,
    pub description: String,
    pub priority: u8,
    #[serde(default)]
    pub parent_task: Option<TaskId>,
    #[serde(default)]
    pub token_budget: Option<u64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Parameters for task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultParams {
    pub task_id: TaskId,
    pub success: bool,
    pub output: String,
    pub tokens_used: u64,
    pub duration_ms: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Parameters for task progress updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressParams {
    pub task_id: TaskId,
    pub progress_pct: u8,
    pub message: String,
    #[serde(default)]
    pub subtasks_completed: u32,
    #[serde(default)]
    pub subtasks_total: u32,
}

/// Parameters for broadcast messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastParams {
    pub message_type: BroadcastType,
    pub content: serde_json::Value,
}

/// Types of broadcast messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastType {
    /// System announcement
    Announcement,
    /// Configuration update
    ConfigUpdate,
    /// Agent health check
    HealthCheck,
    /// Task assignment notification
    TaskNotification,
    /// Custom broadcast
    Custom(String),
}

/// Parameters for agent registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentParams {
    pub agent_id: AgentId,
    pub role: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Response to agent registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentResponse {
    pub accepted: bool,
    pub message: String,
    #[serde(default)]
    pub assigned_tasks_channel: Option<String>,
}

/// Query parameters for agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAgentParams {
    pub query_type: AgentQueryType,
    #[serde(default)]
    pub agent_id: Option<AgentId>,
}

/// Types of agent queries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentQueryType {
    /// List all agents
    ListAll,
    /// Get agent status
    Status,
    /// Get agent capabilities
    Capabilities,
    /// Get agent workload
    Workload,
}
