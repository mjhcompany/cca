//! State, Action, and Reward types for RL

use serde::{Deserialize, Serialize};

use cca_core::AgentRole;

/// Reward value from environment
pub type Reward = f64;

/// State representation for RL algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// Current task type/category
    pub task_type: String,

    /// Available agents and their states
    pub available_agents: Vec<AgentState>,

    /// Current token usage across agents
    pub token_usage: f64,

    /// Success rate history (last N tasks)
    pub success_history: Vec<f64>,

    /// Task complexity estimate (0-1)
    pub complexity: f64,

    /// Additional features
    pub features: Vec<f64>,
}

/// Agent state within the RL state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub role: AgentRole,
    pub is_busy: bool,
    pub success_rate: f64,
    pub avg_completion_time: f64,
}

impl State {
    /// Convert state to feature vector for neural network
    pub fn to_features(&self) -> Vec<f64> {
        let mut features = self.features.clone();

        // Add agent-related features
        for agent in &self.available_agents {
            features.push(if agent.is_busy { 1.0 } else { 0.0 });
            features.push(agent.success_rate);
            features.push(agent.avg_completion_time / 300.0); // Normalize
        }

        // Add other features
        features.push(self.token_usage);
        features.push(self.complexity);

        // Add success history
        features.extend(&self.success_history);

        features
    }

    /// State dimension for neural networks
    pub fn dimension(&self) -> usize {
        self.to_features().len()
    }
}

/// Action in the RL environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Route task to specific agent role
    RouteToAgent(AgentRole),

    /// Allocate token budget (as fraction of max)
    AllocateTokens(f64),

    /// Use pattern from ReasoningBank
    UsePattern(String),

    /// Compress context
    CompressContext(f64),

    /// Multi-action
    Composite(Vec<Action>),
}

impl Action {
    /// Convert action to index for discrete action spaces
    pub fn to_index(&self) -> usize {
        match self {
            Action::RouteToAgent(role) => match role {
                AgentRole::Coordinator => 0,
                AgentRole::Frontend => 1,
                AgentRole::Backend => 2,
                AgentRole::DBA => 3,
                AgentRole::DevOps => 4,
                AgentRole::Security => 5,
                AgentRole::QA => 6,
                AgentRole::Custom(_) => 7,
            },
            Action::AllocateTokens(_) => 8,
            Action::UsePattern(_) => 9,
            Action::CompressContext(_) => 10,
            Action::Composite(_) => 11,
        }
    }

    /// Create action from index (for discrete action spaces)
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Action::RouteToAgent(AgentRole::Coordinator)),
            1 => Some(Action::RouteToAgent(AgentRole::Frontend)),
            2 => Some(Action::RouteToAgent(AgentRole::Backend)),
            3 => Some(Action::RouteToAgent(AgentRole::DBA)),
            4 => Some(Action::RouteToAgent(AgentRole::DevOps)),
            5 => Some(Action::RouteToAgent(AgentRole::Security)),
            6 => Some(Action::RouteToAgent(AgentRole::QA)),
            _ => None,
        }
    }

    /// Number of discrete actions
    pub fn action_space_size() -> usize {
        12
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> State {
        State {
            task_type: "backend".to_string(),
            available_agents: vec![
                AgentState {
                    role: AgentRole::Backend,
                    is_busy: false,
                    success_rate: 0.95,
                    avg_completion_time: 120.0,
                },
                AgentState {
                    role: AgentRole::Frontend,
                    is_busy: true,
                    success_rate: 0.88,
                    avg_completion_time: 90.0,
                },
            ],
            token_usage: 0.5,
            success_history: vec![1.0, 0.8, 1.0],
            complexity: 0.6,
            features: vec![0.1, 0.2, 0.3],
        }
    }

    #[test]
    fn test_state_to_features() {
        let state = create_test_state();
        let features = state.to_features();

        // Should include: base features (3) + agent features (2 agents * 3 each) + token_usage + complexity + success_history (3)
        assert!(!features.is_empty());
        assert!(features.len() >= 10);
    }

    #[test]
    fn test_state_dimension() {
        let state = create_test_state();
        let dim = state.dimension();

        assert_eq!(dim, state.to_features().len());
    }

    #[test]
    fn test_action_to_index() {
        assert_eq!(Action::RouteToAgent(AgentRole::Coordinator).to_index(), 0);
        assert_eq!(Action::RouteToAgent(AgentRole::Frontend).to_index(), 1);
        assert_eq!(Action::RouteToAgent(AgentRole::Backend).to_index(), 2);
        assert_eq!(Action::RouteToAgent(AgentRole::DBA).to_index(), 3);
        assert_eq!(Action::RouteToAgent(AgentRole::DevOps).to_index(), 4);
        assert_eq!(Action::RouteToAgent(AgentRole::Security).to_index(), 5);
        assert_eq!(Action::RouteToAgent(AgentRole::QA).to_index(), 6);
        assert_eq!(Action::AllocateTokens(0.5).to_index(), 8);
        assert_eq!(Action::CompressContext(0.3).to_index(), 10);
    }

    #[test]
    fn test_action_from_index() {
        assert!(matches!(
            Action::from_index(0),
            Some(Action::RouteToAgent(AgentRole::Coordinator))
        ));
        assert!(matches!(
            Action::from_index(2),
            Some(Action::RouteToAgent(AgentRole::Backend))
        ));
        assert!(matches!(
            Action::from_index(6),
            Some(Action::RouteToAgent(AgentRole::QA))
        ));
        assert!(Action::from_index(100).is_none());
    }

    #[test]
    fn test_action_space_size() {
        assert_eq!(Action::action_space_size(), 12);
    }

    #[test]
    fn test_action_serialization() {
        let action = Action::RouteToAgent(AgentRole::Backend);
        let json = serde_json::to_string(&action).unwrap();
        let parsed: Action = serde_json::from_str(&json).unwrap();

        assert!(matches!(
            parsed,
            Action::RouteToAgent(AgentRole::Backend)
        ));
    }

    #[test]
    fn test_composite_action() {
        let composite = Action::Composite(vec![
            Action::RouteToAgent(AgentRole::Backend),
            Action::AllocateTokens(0.8),
        ]);

        assert_eq!(composite.to_index(), 11);
    }

    #[test]
    fn test_state_serialization() {
        let state = create_test_state();
        let json = serde_json::to_string(&state).unwrap();
        let parsed: State = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.task_type, "backend");
        assert_eq!(parsed.complexity, 0.6);
        assert_eq!(parsed.available_agents.len(), 2);
    }
}
