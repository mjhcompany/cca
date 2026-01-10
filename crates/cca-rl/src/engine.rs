//! RL Engine - Coordinates training and inference

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use tracing::{debug, info};

use crate::algorithm::{QLearning, RLAlgorithm, DQN, PPO};
use crate::experience::{Experience, ExperienceBuffer};
use crate::state::{Action, Reward, State};

/// RL Engine for managing algorithms and training
pub struct RLEngine {
    algorithms: HashMap<String, Box<dyn RLAlgorithm>>,
    active_algorithm: String,
    experience_buffer: ExperienceBuffer,
    training_batch_size: usize,
    total_steps: u64,
    total_rewards: f64,
}

impl RLEngine {
    /// Create a new RL engine with default algorithms
    pub fn new() -> Self {
        let mut algorithms: HashMap<String, Box<dyn RLAlgorithm>> = HashMap::new();

        // Register default algorithms
        algorithms.insert("q_learning".to_string(), Box::new(QLearning::default()));
        algorithms.insert("ppo".to_string(), Box::new(PPO::default()));
        algorithms.insert("dqn".to_string(), Box::new(DQN::default()));

        Self {
            algorithms,
            active_algorithm: "q_learning".to_string(),
            experience_buffer: ExperienceBuffer::new(10000),
            training_batch_size: 32,
            total_steps: 0,
            total_rewards: 0.0,
        }
    }

    /// Set the active algorithm
    pub fn set_algorithm(&mut self, name: &str) -> Result<()> {
        if self.algorithms.contains_key(name) {
            self.active_algorithm = name.to_string();
            info!("Active algorithm set to: {}", name);
            Ok(())
        } else {
            Err(anyhow!("Unknown algorithm: {}", name))
        }
    }

    /// Get the active algorithm name
    pub fn active_algorithm(&self) -> &str {
        &self.active_algorithm
    }

    /// List available algorithms
    pub fn list_algorithms(&self) -> Vec<&str> {
        self.algorithms.keys().map(|s| s.as_str()).collect()
    }

    /// Record an experience
    pub fn record_experience(&mut self, experience: Experience) {
        self.experience_buffer.push(experience);
        self.total_steps += 1;
    }

    /// Train on collected experiences
    pub fn train(&mut self) -> Result<f64> {
        if self.experience_buffer.len() < self.training_batch_size {
            return Ok(0.0);
        }

        let batch = self.experience_buffer.sample(self.training_batch_size);

        let algorithm = self
            .algorithms
            .get_mut(&self.active_algorithm)
            .ok_or_else(|| anyhow!("Active algorithm not found"))?;

        let loss = algorithm.train(&batch)?;

        debug!("Training step complete, loss: {:.4}", loss);

        Ok(loss)
    }

    /// Predict the best action for a state
    pub fn predict(&self, state: &State) -> Action {
        self.algorithms
            .get(&self.active_algorithm)
            .map(|alg| alg.predict(state))
            .unwrap_or(Action::RouteToAgent(cca_core::AgentRole::Coordinator))
    }

    /// Update after receiving reward
    pub fn update_reward(&mut self, reward: Reward) -> Result<()> {
        self.total_rewards += reward;

        if let Some(algorithm) = self.algorithms.get_mut(&self.active_algorithm) {
            algorithm.update(reward)?;
        }

        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> EngineStats {
        EngineStats {
            total_steps: self.total_steps,
            total_rewards: self.total_rewards,
            average_reward: if self.total_steps > 0 {
                self.total_rewards / self.total_steps as f64
            } else {
                0.0
            },
            buffer_size: self.experience_buffer.len(),
            active_algorithm: self.active_algorithm.clone(),
        }
    }

    /// Get algorithm parameters
    pub fn get_algorithm_params(&self) -> serde_json::Value {
        self.algorithms
            .get(&self.active_algorithm)
            .map(|alg| alg.get_params())
            .unwrap_or(serde_json::Value::Null)
    }

    /// Set algorithm parameters
    pub fn set_algorithm_params(&mut self, params: serde_json::Value) -> Result<()> {
        if let Some(algorithm) = self.algorithms.get_mut(&self.active_algorithm) {
            algorithm.set_params(params)?;
        }
        Ok(())
    }

    /// Clear experience buffer
    pub fn clear_buffer(&mut self) {
        self.experience_buffer.clear();
    }
}

impl Default for RLEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Engine statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineStats {
    pub total_steps: u64,
    pub total_rewards: f64,
    pub average_reward: f64,
    pub buffer_size: usize,
    pub active_algorithm: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AgentState as RlAgentState, State};

    fn create_test_state() -> State {
        State {
            task_type: "test".to_string(),
            available_agents: vec![RlAgentState {
                role: cca_core::AgentRole::Backend,
                is_busy: false,
                success_rate: 0.9,
                avg_completion_time: 100.0,
            }],
            token_usage: 0.5,
            success_history: vec![1.0],
            complexity: 0.3,
            features: vec![0.1, 0.2],
        }
    }

    #[test]
    fn test_engine_creation() {
        let engine = RLEngine::new();
        assert_eq!(engine.active_algorithm(), "q_learning");
    }

    #[test]
    fn test_list_algorithms() {
        let engine = RLEngine::new();
        let algorithms = engine.list_algorithms();

        assert!(algorithms.contains(&"q_learning"));
        assert!(algorithms.contains(&"ppo"));
        assert!(algorithms.contains(&"dqn"));
    }

    #[test]
    fn test_set_algorithm() {
        let mut engine = RLEngine::new();

        assert!(engine.set_algorithm("ppo").is_ok());
        assert_eq!(engine.active_algorithm(), "ppo");

        assert!(engine.set_algorithm("dqn").is_ok());
        assert_eq!(engine.active_algorithm(), "dqn");

        assert!(engine.set_algorithm("nonexistent").is_err());
    }

    #[test]
    fn test_record_experience() {
        let mut engine = RLEngine::new();
        let state = create_test_state();

        let exp = Experience {
            state: state.clone(),
            action: Action::RouteToAgent(cca_core::AgentRole::Backend),
            reward: 1.0,
            next_state: Some(state),
            done: false,
        };

        engine.record_experience(exp);
        assert_eq!(engine.stats().total_steps, 1);
        assert_eq!(engine.stats().buffer_size, 1);
    }

    #[test]
    fn test_update_reward() {
        let mut engine = RLEngine::new();
        engine.update_reward(0.5).unwrap();
        engine.update_reward(1.0).unwrap();

        assert_eq!(engine.stats().total_rewards, 1.5);
    }

    #[test]
    fn test_predict() {
        let engine = RLEngine::new();
        let state = create_test_state();

        let action = engine.predict(&state);
        // Should return a valid action
        assert!(matches!(action, Action::RouteToAgent(_)));
    }

    #[test]
    fn test_stats() {
        let mut engine = RLEngine::new();
        let state = create_test_state();

        for i in 0..5 {
            let exp = Experience {
                state: state.clone(),
                action: Action::RouteToAgent(cca_core::AgentRole::Frontend),
                reward: i as f64,
                next_state: Some(state.clone()),
                done: false,
            };
            engine.record_experience(exp);
            engine.update_reward(i as f64).unwrap();
        }

        let stats = engine.stats();
        assert_eq!(stats.total_steps, 5);
        assert_eq!(stats.buffer_size, 5);
        assert_eq!(stats.total_rewards, 10.0); // 0+1+2+3+4
        assert_eq!(stats.average_reward, 2.0);
        assert_eq!(stats.active_algorithm, "q_learning");
    }

    #[test]
    fn test_train_insufficient_batch() {
        let mut engine = RLEngine::new();
        // With fewer experiences than batch size, should return 0.0
        let loss = engine.train().unwrap();
        assert_eq!(loss, 0.0);
    }

    #[test]
    fn test_clear_buffer() {
        let mut engine = RLEngine::new();
        let state = create_test_state();

        let exp = Experience {
            state: state.clone(),
            action: Action::RouteToAgent(cca_core::AgentRole::QA),
            reward: 1.0,
            next_state: Some(state),
            done: true,
        };
        engine.record_experience(exp);
        assert_eq!(engine.stats().buffer_size, 1);

        engine.clear_buffer();
        assert_eq!(engine.stats().buffer_size, 0);
    }

    #[test]
    fn test_get_and_set_params() {
        let mut engine = RLEngine::new();
        let params = engine.get_algorithm_params();
        assert!(!params.is_null());

        let new_params = serde_json::json!({"learning_rate": 0.01});
        assert!(engine.set_algorithm_params(new_params).is_ok());
    }
}
