//! Communication types for inter-agent messaging

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentId;

/// Message types for inter-agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Task assignment from coordinator
    TaskAssign,
    /// Task result from execution agent
    TaskResult,
    /// Status update
    StatusUpdate,
    /// Broadcast to all agents
    Broadcast,
    /// Request for information
    Query,
    /// Response to a query
    QueryResponse,
    /// Heartbeat/ping
    Heartbeat,
    /// Error notification
    Error,
}

/// Inter-agent message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterAgentMessage {
    pub id: Uuid,
    pub from: AgentId,
    pub to: MessageTarget,
    pub msg_type: MessageType,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Option<Uuid>,
}

/// Target for a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageTarget {
    /// Single agent
    Agent(AgentId),
    /// All agents
    Broadcast,
    /// Coordinator only
    Coordinator,
}

impl InterAgentMessage {
    pub fn new(
        from: AgentId,
        to: MessageTarget,
        msg_type: MessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            msg_type,
            payload,
            timestamp: Utc::now(),
            correlation_id: None,
        }
    }

    pub fn with_correlation(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn reply(&self, payload: serde_json::Value, msg_type: MessageType) -> Self {
        Self {
            id: Uuid::new_v4(),
            from: match &self.to {
                MessageTarget::Agent(id) => *id,
                _ => self.from,
            },
            to: MessageTarget::Agent(self.from),
            msg_type,
            payload,
            timestamp: Utc::now(),
            correlation_id: Some(self.id),
        }
    }
}

/// Redis pub/sub channels
pub mod channels {
    pub const BROADCAST: &str = "cca:broadcast";
    pub const COORDINATION: &str = "cca:coord";
    pub const STATUS: &str = "cca:status";
    pub const LEARNING: &str = "cca:learning";

    pub fn agent_tasks(agent_id: &str) -> String {
        format!("cca:tasks:{agent_id}")
    }

    pub fn agent_status(agent_id: &str) -> String {
        format!("cca:status:{agent_id}")
    }
}

/// ACP (Agent Client Protocol) message following JSON-RPC 2.0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AcpError>,
}

impl AcpMessage {
    pub fn request(
        id: impl Into<String>,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id.into()),
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn notification(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn response(id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id.into()),
            method: None,
            params: None,
            result: Some(result),
            error: None,
        }
    }

    pub fn error_response(id: impl Into<String>, error: AcpError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id.into()),
            method: None,
            params: None,
            result: None,
            error: Some(error),
        }
    }
}

/// ACP error structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl AcpError {
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }
}
