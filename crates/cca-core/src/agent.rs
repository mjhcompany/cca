//! Agent types and management

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Predefined agent roles
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    /// Coordinator agent - routes tasks to specialists
    Coordinator,
    /// Frontend specialist
    Frontend,
    /// Backend specialist
    Backend,
    /// Database administrator
    DBA,
    /// DevOps/Infrastructure
    DevOps,
    /// Security specialist
    Security,
    /// Quality assurance / Testing
    QA,
    /// Custom role with specified name
    Custom(String),
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Coordinator => write!(f, "coordinator"),
            AgentRole::Frontend => write!(f, "frontend"),
            AgentRole::Backend => write!(f, "backend"),
            AgentRole::DBA => write!(f, "dba"),
            AgentRole::DevOps => write!(f, "devops"),
            AgentRole::Security => write!(f, "security"),
            AgentRole::QA => write!(f, "qa"),
            AgentRole::Custom(name) => write!(f, "{}", name),
        }
    }
}

impl From<&str> for AgentRole {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "coordinator" => AgentRole::Coordinator,
            "frontend" => AgentRole::Frontend,
            "backend" => AgentRole::Backend,
            "dba" => AgentRole::DBA,
            "devops" => AgentRole::DevOps,
            "security" => AgentRole::Security,
            "qa" => AgentRole::QA,
            other => AgentRole::Custom(other.to_string()),
        }
    }
}

/// Current state of an agent
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    /// Agent is starting up
    Starting,
    /// Agent is ready to receive tasks
    Ready,
    /// Agent is currently processing a task
    Busy,
    /// Agent has encountered an error
    Error(String),
    /// Agent is shutting down
    Stopping,
    /// Agent has stopped
    Stopped,
}

/// Agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub role: AgentRole,
    pub state: AgentState,
    pub name: Option<String>,
    pub context_hash: Option<String>,
    pub pid: Option<u32>,
}

impl Agent {
    pub fn new(role: AgentRole) -> Self {
        Self {
            id: AgentId::new(),
            role,
            state: AgentState::Starting,
            name: None,
            context_hash: None,
            pid: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Agent activity information for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActivity {
    pub agent_id: AgentId,
    pub role: AgentRole,
    pub current_task: Option<String>,
    pub tokens_used: u64,
    pub tasks_completed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_uniqueness() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_agent_id_display() {
        let id = AgentId::new();
        let display = format!("{}", id);
        assert!(!display.is_empty());
        assert!(display.contains('-')); // UUID format
    }

    #[test]
    fn test_agent_role_from_str() {
        assert_eq!(AgentRole::from("coordinator"), AgentRole::Coordinator);
        assert_eq!(AgentRole::from("FRONTEND"), AgentRole::Frontend);
        assert_eq!(AgentRole::from("Backend"), AgentRole::Backend);
        assert_eq!(AgentRole::from("dba"), AgentRole::DBA);
        assert_eq!(AgentRole::from("devops"), AgentRole::DevOps);
        assert_eq!(AgentRole::from("security"), AgentRole::Security);
        assert_eq!(AgentRole::from("qa"), AgentRole::QA);
        assert_eq!(
            AgentRole::from("custom-role"),
            AgentRole::Custom("custom-role".to_string())
        );
    }

    #[test]
    fn test_agent_role_display() {
        assert_eq!(format!("{}", AgentRole::Coordinator), "coordinator");
        assert_eq!(format!("{}", AgentRole::Frontend), "frontend");
        assert_eq!(
            format!("{}", AgentRole::Custom("my-agent".to_string())),
            "my-agent"
        );
    }

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new(AgentRole::Backend);
        assert_eq!(agent.role, AgentRole::Backend);
        assert_eq!(agent.state, AgentState::Starting);
        assert!(agent.name.is_none());
    }

    #[test]
    fn test_agent_with_name() {
        let agent = Agent::new(AgentRole::Frontend).with_name("frontend-1");
        assert_eq!(agent.name, Some("frontend-1".to_string()));
    }

    #[test]
    fn test_agent_serialization() {
        let agent = Agent::new(AgentRole::Coordinator).with_name("main");
        let json = serde_json::to_string(&agent).unwrap();
        let parsed: Agent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, AgentRole::Coordinator);
        assert_eq!(parsed.name, Some("main".to_string()));
    }

    #[test]
    fn test_agent_state_variants() {
        let states = vec![
            AgentState::Starting,
            AgentState::Ready,
            AgentState::Busy,
            AgentState::Error("test error".to_string()),
            AgentState::Stopping,
            AgentState::Stopped,
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let _parsed: AgentState = serde_json::from_str(&json).unwrap();
        }
    }
}
