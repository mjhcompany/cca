//! MCP types for Claude Code integration

use serde::{Deserialize, Serialize};

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Task request sent via cca_task tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub description: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Task response from cca_task tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub task_id: String,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// Status request for cca_status tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusRequest {
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Activity entry for cca_activity tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub agent_id: String,
    pub role: String,
    pub current_task: Option<String>,
    pub status: String,
}

/// Memory query for cca_memory tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// Memory search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub patterns: Vec<PatternMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternMatch {
    pub id: String,
    pub pattern_type: String,
    pub content: String,
    pub score: f64,
    pub success_rate: f64,
}
