//! RL Integration - Connects RL engine with daemon and PostgreSQL
//!
//! This module provides:
//! - Async wrapper for RLEngine
//! - Experience persistence to PostgreSQL
//! - Task routing optimization
//! - Training loop management
//!
//! Note: Many methods are infrastructure for future features and not yet called.
#![allow(dead_code)]

use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use cca_core::AgentRole;
use cca_rl::{Action, Experience, RLEngine, State};

use crate::postgres::PostgresServices;

/// RL service configuration
#[derive(Debug, Clone, Deserialize)]
pub struct RLConfig {
    /// Training batch size
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Training interval (in experiences)
    #[serde(default = "default_train_interval")]
    pub train_interval: usize,

    /// Experience buffer capacity
    #[serde(default = "default_buffer_capacity")]
    pub buffer_capacity: usize,

    /// Whether to persist experiences to PostgreSQL
    #[serde(default = "default_persist_experiences")]
    pub persist_experiences: bool,

    /// Default algorithm
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
}

fn default_batch_size() -> usize {
    32
}
fn default_train_interval() -> usize {
    100
}
fn default_buffer_capacity() -> usize {
    10000
}
fn default_persist_experiences() -> bool {
    true
}
fn default_algorithm() -> String {
    "q_learning".to_string()
}

impl Default for RLConfig {
    fn default() -> Self {
        Self {
            batch_size: default_batch_size(),
            train_interval: default_train_interval(),
            buffer_capacity: default_buffer_capacity(),
            persist_experiences: default_persist_experiences(),
            algorithm: default_algorithm(),
        }
    }
}

/// RL service wrapping the engine with async support
pub struct RLService {
    engine: RwLock<RLEngine>,
    config: RLConfig,
    postgres: Option<Arc<PostgresServices>>,
    experience_count: RwLock<usize>,
    last_training_loss: RwLock<f64>,
}

impl RLService {
    /// Create a new RL service
    pub fn new(config: RLConfig) -> Self {
        let mut engine = RLEngine::new();

        // Set configured algorithm
        if let Err(e) = engine.set_algorithm(&config.algorithm) {
            warn!("Failed to set algorithm {}: {}", config.algorithm, e);
        }

        info!(
            "RL service initialized with algorithm: {}, batch_size: {}",
            config.algorithm, config.batch_size
        );

        Self {
            engine: RwLock::new(engine),
            config,
            postgres: None,
            experience_count: RwLock::new(0),
            last_training_loss: RwLock::new(0.0),
        }
    }

    /// Set PostgreSQL services for experience persistence
    pub fn with_postgres(mut self, postgres: Arc<PostgresServices>) -> Self {
        self.postgres = Some(postgres);
        self
    }

    /// Record an experience and optionally persist to PostgreSQL
    pub async fn record_experience(&self, experience: Experience) -> Result<()> {
        // Record in engine's buffer and update reward tracking
        {
            let mut engine = self.engine.write().await;
            engine.record_experience(experience.clone());
            // Update total_rewards counter for stats tracking
            if let Err(e) = engine.update_reward(experience.reward) {
                warn!("Failed to update reward: {}", e);
            }
        }

        // Update count
        let count = {
            let mut count = self.experience_count.write().await;
            *count += 1;
            *count
        };

        // Persist to PostgreSQL if configured
        if self.config.persist_experiences {
            if let Some(ref postgres) = self.postgres {
                let state_json = serde_json::to_value(&experience.state)?;
                let action_json = serde_json::to_value(&experience.action)?;
                let next_state_json = experience
                    .next_state
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()?;

                if let Err(e) = postgres
                    .experiences
                    .store(
                        state_json,
                        action_json,
                        experience.reward,
                        next_state_json,
                        experience.done,
                        Some(&self.config.algorithm),
                    )
                    .await
                {
                    warn!("Failed to persist experience: {}", e);
                }
            }
        }

        // Train periodically
        if count % self.config.train_interval == 0 {
            self.train().await?;
        }

        Ok(())
    }

    /// Train on collected experiences
    pub async fn train(&self) -> Result<f64> {
        let mut engine = self.engine.write().await;
        let loss = engine.train()?;

        if loss > 0.0 {
            let mut last_loss = self.last_training_loss.write().await;
            *last_loss = loss;
            debug!("Training complete, loss: {:.4}", loss);
        }

        Ok(loss)
    }

    /// Predict the best action for a given state
    pub async fn predict(&self, state: &State) -> Action {
        let engine = self.engine.read().await;
        engine.predict(state)
    }

    /// Update after receiving reward
    pub async fn update_reward(&self, reward: f64) -> Result<()> {
        let mut engine = self.engine.write().await;
        engine.update_reward(reward)
    }

    /// Get current statistics
    pub async fn stats(&self) -> RLStats {
        let engine = self.engine.read().await;
        let engine_stats = engine.stats();
        let last_loss = *self.last_training_loss.read().await;
        let experience_count = *self.experience_count.read().await;

        RLStats {
            algorithm: engine_stats.active_algorithm,
            total_steps: engine_stats.total_steps,
            total_rewards: engine_stats.total_rewards,
            average_reward: engine_stats.average_reward,
            buffer_size: engine_stats.buffer_size,
            last_training_loss: last_loss,
            experience_count,
            algorithms_available: engine.list_algorithms().iter().map(std::string::ToString::to_string).collect(),
        }
    }

    /// Get algorithm parameters
    pub async fn get_params(&self) -> serde_json::Value {
        let engine = self.engine.read().await;
        engine.get_algorithm_params()
    }

    /// Set algorithm parameters
    pub async fn set_params(&self, params: serde_json::Value) -> Result<()> {
        let mut engine = self.engine.write().await;
        engine.set_algorithm_params(params)
    }

    /// Switch to a different algorithm
    pub async fn set_algorithm(&self, name: &str) -> Result<()> {
        let mut engine = self.engine.write().await;
        engine.set_algorithm(name)
    }

    /// List available algorithms
    pub async fn list_algorithms(&self) -> Vec<String> {
        let engine = self.engine.read().await;
        engine.list_algorithms().iter().map(std::string::ToString::to_string).collect()
    }

    /// Clear the experience buffer
    pub async fn clear_buffer(&self) {
        let mut engine = self.engine.write().await;
        engine.clear_buffer();

        let mut count = self.experience_count.write().await;
        *count = 0;
    }

    /// Load experiences from PostgreSQL
    pub async fn load_experiences(&self, count: i32) -> Result<usize> {
        let Some(ref postgres) = self.postgres else {
            return Ok(0);
        };

        let experiences = postgres
            .experiences
            .get_recent(count)
            .await
            .context("Failed to load experiences")?;

        let mut loaded = 0;
        let mut engine = self.engine.write().await;

        for record in experiences {
            // Deserialize state and action
            let state: State = serde_json::from_value(record.state)
                .context("Failed to deserialize state")?;
            let action: Action = serde_json::from_value(record.action)
                .context("Failed to deserialize action")?;
            let next_state: Option<State> = record
                .next_state
                .map(serde_json::from_value)
                .transpose()
                .context("Failed to deserialize next_state")?;

            let exp = Experience::new(state, action, record.reward, next_state, record.done);
            engine.record_experience(exp);
            loaded += 1;
        }

        info!("Loaded {} experiences from PostgreSQL", loaded);
        Ok(loaded)
    }
}

/// RL statistics
#[derive(Debug, Clone, Serialize)]
pub struct RLStats {
    pub algorithm: String,
    pub total_steps: u64,
    pub total_rewards: f64,
    pub average_reward: f64,
    pub buffer_size: usize,
    pub last_training_loss: f64,
    pub experience_count: usize,
    pub algorithms_available: Vec<String>,
}

/// Helper to create State from task/agent information
pub struct StateBuilder {
    task_type: String,
    agents: Vec<AgentInfo>,
    token_usage: f64,
    success_history: Vec<f64>,
    complexity: f64,
}

/// Agent info for state building
pub struct AgentInfo {
    pub role: AgentRole,
    pub is_busy: bool,
    pub success_rate: f64,
    pub avg_completion_time: f64,
}

impl StateBuilder {
    pub fn new(task_type: impl Into<String>) -> Self {
        Self {
            task_type: task_type.into(),
            agents: Vec::new(),
            token_usage: 0.0,
            success_history: Vec::new(),
            complexity: 0.5,
        }
    }

    pub fn add_agent(mut self, info: AgentInfo) -> Self {
        self.agents.push(info);
        self
    }

    pub fn token_usage(mut self, usage: f64) -> Self {
        self.token_usage = usage;
        self
    }

    pub fn success_history(mut self, history: Vec<f64>) -> Self {
        self.success_history = history;
        self
    }

    pub fn complexity(mut self, complexity: f64) -> Self {
        self.complexity = complexity;
        self
    }

    pub fn build(self) -> State {
        use cca_rl::state::AgentState as RlAgentState;

        State {
            task_type: self.task_type,
            available_agents: self
                .agents
                .into_iter()
                .map(|a| RlAgentState {
                    role: a.role,
                    is_busy: a.is_busy,
                    success_rate: a.success_rate,
                    avg_completion_time: a.avg_completion_time,
                })
                .collect(),
            token_usage: self.token_usage,
            success_history: self.success_history,
            complexity: self.complexity,
            features: Vec::new(),
        }
    }
}

/// Compute reward from task outcome
pub fn compute_reward(
    success: bool,
    tokens_used: u32,
    duration_ms: u32,
    max_tokens: u32,
    max_duration_ms: u32,
) -> f64 {
    let base_reward = if success { 1.0 } else { -0.5 };

    // Bonus for token efficiency
    let token_efficiency = 1.0 - (tokens_used as f64 / max_tokens as f64).min(1.0);
    let token_bonus = token_efficiency * 0.2;

    // Bonus for speed
    let speed_efficiency = 1.0 - (duration_ms as f64 / max_duration_ms as f64).min(1.0);
    let speed_bonus = speed_efficiency * 0.1;

    base_reward + token_bonus + speed_bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_reward_success() {
        let reward = compute_reward(true, 500, 1000, 1000, 5000);
        assert!(reward > 0.0);
        assert!(reward <= 1.3); // Base + max bonuses
    }

    #[test]
    fn test_compute_reward_failure() {
        let reward = compute_reward(false, 500, 1000, 1000, 5000);
        assert!(reward < 0.0);
    }

    #[test]
    fn test_state_builder() {
        let state = StateBuilder::new("backend_task")
            .add_agent(AgentInfo {
                role: AgentRole::Backend,
                is_busy: false,
                success_rate: 0.9,
                avg_completion_time: 100.0,
            })
            .complexity(0.7)
            .token_usage(0.3)
            .build();

        assert_eq!(state.task_type, "backend_task");
        assert_eq!(state.complexity, 0.7);
        assert_eq!(state.available_agents.len(), 1);
    }

    #[tokio::test]
    async fn test_rl_service_basic() {
        let config = RLConfig::default();
        let service = RLService::new(config);

        let stats = service.stats().await;
        assert_eq!(stats.algorithm, "q_learning");
        assert_eq!(stats.total_steps, 0);
    }

    #[tokio::test]
    async fn test_rl_service_predict() {
        let config = RLConfig::default();
        let service = RLService::new(config);

        let state = StateBuilder::new("test").complexity(0.5).build();
        let action = service.predict(&state).await;

        // Should return some action
        assert!(matches!(action, Action::RouteToAgent(_)));
    }
}
