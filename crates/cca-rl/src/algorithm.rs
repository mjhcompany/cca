//! RL Algorithm trait and implementations

use anyhow::Result;

use crate::experience::Experience;
use crate::state::{Action, Reward, State};

/// Trait for RL algorithms
pub trait RLAlgorithm: Send + Sync {
    /// Algorithm name
    fn name(&self) -> &str;

    /// Train on a batch of experiences
    fn train(&mut self, experiences: &[Experience]) -> Result<f64>;

    /// Predict best action for state
    fn predict(&self, state: &State) -> Action;

    /// Update after receiving reward
    fn update(&mut self, reward: Reward) -> Result<()>;

    /// Get algorithm parameters as JSON
    fn get_params(&self) -> serde_json::Value;

    /// Set algorithm parameters from JSON
    fn set_params(&mut self, params: serde_json::Value) -> Result<()>;
}

/// Q-Learning implementation (tabular)
pub struct QLearning {
    q_table: std::collections::HashMap<String, Vec<f64>>,
    learning_rate: f64,
    discount_factor: f64,
    epsilon: f64,
    action_space_size: usize,
}

impl QLearning {
    pub fn new(learning_rate: f64, discount_factor: f64, epsilon: f64) -> Self {
        Self {
            q_table: std::collections::HashMap::new(),
            learning_rate,
            discount_factor,
            epsilon,
            action_space_size: Action::action_space_size(),
        }
    }

    fn state_key(state: &State) -> String {
        // Simple state hashing - in production, use better discretization
        format!("{:.2}_{:.2}", state.complexity, state.token_usage)
    }

    fn get_q_values(&self, state: &State) -> Vec<f64> {
        let key = Self::state_key(state);
        self.q_table
            .get(&key)
            .cloned()
            .unwrap_or_else(|| vec![0.0; self.action_space_size])
    }
}

impl RLAlgorithm for QLearning {
    fn name(&self) -> &str {
        "q_learning"
    }

    fn train(&mut self, experiences: &[Experience]) -> Result<f64> {
        let mut total_loss = 0.0;

        for exp in experiences {
            let state_key = Self::state_key(&exp.state);
            let action_idx = exp.action.to_index();

            // Get current Q-value
            let q_values = self.get_q_values(&exp.state);
            let current_q = q_values[action_idx];

            // Calculate target Q-value
            let target = if exp.done {
                exp.reward
            } else if let Some(ref next_state) = exp.next_state {
                let next_q = self.get_q_values(next_state);
                let max_next_q = next_q.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                exp.reward + self.discount_factor * max_next_q
            } else {
                exp.reward
            };

            // Update Q-value
            let new_q = current_q + self.learning_rate * (target - current_q);

            let q_values = self
                .q_table
                .entry(state_key)
                .or_insert_with(|| vec![0.0; self.action_space_size]);
            q_values[action_idx] = new_q;

            total_loss += (target - current_q).powi(2);
        }

        Ok(total_loss / experiences.len() as f64)
    }

    fn predict(&self, state: &State) -> Action {
        let q_values = self.get_q_values(state);

        // Epsilon-greedy action selection
        if rand::random::<f64>() < self.epsilon {
            // Random action
            let idx = rand::random::<usize>() % self.action_space_size;
            Action::from_index(idx)
                .unwrap_or(Action::RouteToAgent(cca_core::AgentRole::Coordinator))
        } else {
            // Greedy action
            let best_idx = q_values
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0);
            Action::from_index(best_idx)
                .unwrap_or(Action::RouteToAgent(cca_core::AgentRole::Coordinator))
        }
    }

    fn update(&mut self, _reward: Reward) -> Result<()> {
        // Decay epsilon
        self.epsilon *= 0.999;
        if self.epsilon < 0.01 {
            self.epsilon = 0.01;
        }
        Ok(())
    }

    fn get_params(&self) -> serde_json::Value {
        serde_json::json!({
            "learning_rate": self.learning_rate,
            "discount_factor": self.discount_factor,
            "epsilon": self.epsilon,
            "q_table_size": self.q_table.len()
        })
    }

    fn set_params(&mut self, params: serde_json::Value) -> Result<()> {
        if let Some(lr) = params["learning_rate"].as_f64() {
            self.learning_rate = lr;
        }
        if let Some(df) = params["discount_factor"].as_f64() {
            self.discount_factor = df;
        }
        if let Some(eps) = params["epsilon"].as_f64() {
            self.epsilon = eps;
        }
        Ok(())
    }
}

impl Default for QLearning {
    fn default() -> Self {
        Self::new(0.1, 0.99, 0.1)
    }
}

// Placeholder for other algorithms - to be implemented in Phase 4

/// PPO placeholder
pub struct PPO {
    // TODO: Implement PPO
}

impl PPO {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for PPO {
    fn default() -> Self {
        Self::new()
    }
}

impl RLAlgorithm for PPO {
    fn name(&self) -> &str {
        "ppo"
    }
    fn train(&mut self, _: &[Experience]) -> Result<f64> {
        Ok(0.0)
    }
    fn predict(&self, _: &State) -> Action {
        Action::RouteToAgent(cca_core::AgentRole::Coordinator)
    }
    fn update(&mut self, _: Reward) -> Result<()> {
        Ok(())
    }
    fn get_params(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    fn set_params(&mut self, _: serde_json::Value) -> Result<()> {
        Ok(())
    }
}

/// DQN placeholder
pub struct DQN {
    // TODO: Implement DQN with neural network
}

impl DQN {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for DQN {
    fn default() -> Self {
        Self::new()
    }
}

impl RLAlgorithm for DQN {
    fn name(&self) -> &str {
        "dqn"
    }
    fn train(&mut self, _: &[Experience]) -> Result<f64> {
        Ok(0.0)
    }
    fn predict(&self, _: &State) -> Action {
        Action::RouteToAgent(cca_core::AgentRole::Coordinator)
    }
    fn update(&mut self, _: Reward) -> Result<()> {
        Ok(())
    }
    fn get_params(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    fn set_params(&mut self, _: serde_json::Value) -> Result<()> {
        Ok(())
    }
}
