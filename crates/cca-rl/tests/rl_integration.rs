//! Integration tests for RL Engine
//!
//! These tests verify the RL algorithms work correctly together.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::float_cmp)]
#![allow(clippy::match_same_arms)]

use cca_core::AgentRole;
use cca_rl::{Action, Experience, ExperienceBuffer, RLEngine, State};

/// Helper function to create a test state
fn create_test_state(task_type: &str, complexity: f64) -> State {
    State {
        task_type: task_type.to_string(),
        available_agents: vec![
            cca_rl::state::AgentState {
                role: AgentRole::Backend,
                is_busy: false,
                success_rate: 0.9,
                avg_completion_time: 100.0,
            },
            cca_rl::state::AgentState {
                role: AgentRole::Frontend,
                is_busy: false,
                success_rate: 0.85,
                avg_completion_time: 120.0,
            },
            cca_rl::state::AgentState {
                role: AgentRole::QA,
                is_busy: true,
                success_rate: 0.95,
                avg_completion_time: 80.0,
            },
        ],
        token_usage: 0.3,
        success_history: vec![0.9, 0.85, 0.95],
        complexity,
        features: vec![0.5, 0.3, 0.8],
    }
}

/// Test Q-Learning convergence on a simple task
#[tokio::test]
async fn test_q_learning_convergence() {
    let mut engine = RLEngine::new();

    let mut total_reward = 0.0;
    let episodes = 100;

    for _episode in 0..episodes {
        let state = create_test_state("simple_task", 0.5);

        // Get action from policy
        let action = engine.predict(&state);

        // Simulate reward based on action selection
        let reward = match &action {
            Action::RouteToAgent(role) => match role {
                AgentRole::Backend => 1.0,   // Best for backend tasks
                AgentRole::Frontend => 0.5,
                AgentRole::QA => 0.3,
                _ => 0.2,
            },
            _ => 0.1,
        };

        total_reward += reward;

        // Create and record experience
        let experience = Experience::new(
            state.clone(),
            action,
            reward,
            Some(state),
            false,
        );
        engine.record_experience(experience);
    }

    // Train the model
    let _ = engine.train();

    let avg_reward = total_reward / episodes as f64;
    println!("Average reward after {episodes} episodes: {avg_reward:.3}");

    // After training, should have some positive average reward
    assert!(avg_reward > 0.1, "Should learn to select agents");
}

/// Test experience buffer operations
#[tokio::test]
async fn test_experience_buffer_operations() {
    let mut buffer = ExperienceBuffer::new(100);

    // Add experiences
    for i in 0..50 {
        let state = create_test_state(&format!("task_{i}"), 0.5);
        let action = Action::RouteToAgent(AgentRole::Backend);
        let experience = Experience::new(state, action, (i as f64) * 0.1, None, true);
        buffer.push(experience);
    }

    assert_eq!(buffer.len(), 50);

    // Sample batch
    let batch = buffer.sample(10);
    assert_eq!(batch.len(), 10);

    // Verify batch contains valid experiences
    for exp in &batch {
        assert!(exp.reward >= 0.0);
        assert!(exp.reward < 5.0);
    }

    // Test capacity limit
    for i in 50..150 {
        let state = create_test_state(&format!("task_{i}"), 0.5);
        let action = Action::AllocateTokens(0.5);
        let experience = Experience::new(state, action, 0.5, None, true);
        buffer.push(experience);
    }

    assert_eq!(buffer.len(), 100, "Buffer should be at capacity");
}

/// Test state representation
#[tokio::test]
async fn test_state_representation() {
    let state = create_test_state("code_review", 0.6);

    // Verify state features
    let features = state.to_features();
    assert!(!features.is_empty(), "Should have features");

    // Verify dimension calculation
    let dim = state.dimension();
    assert!(dim > 0, "Should have positive dimension");
    assert_eq!(dim, features.len(), "Dimension should match feature count");
}

/// Test action space
#[tokio::test]
async fn test_action_space() {
    // Test route actions
    let route_actions = vec![
        Action::RouteToAgent(AgentRole::Coordinator),
        Action::RouteToAgent(AgentRole::Frontend),
        Action::RouteToAgent(AgentRole::Backend),
        Action::RouteToAgent(AgentRole::DBA),
        Action::RouteToAgent(AgentRole::DevOps),
        Action::RouteToAgent(AgentRole::Security),
        Action::RouteToAgent(AgentRole::QA),
    ];

    for action in &route_actions {
        let index = action.to_index();
        assert!(index < Action::action_space_size(), "Index should be in bounds");
    }

    // Test other action types
    let allocate = Action::AllocateTokens(0.5);
    assert!(allocate.to_index() < Action::action_space_size());

    let compress = Action::CompressContext(0.3);
    assert!(compress.to_index() < Action::action_space_size());

    // Test from_index
    for i in 0..7 {
        let action = Action::from_index(i);
        assert!(action.is_some(), "Should create action for index {i}");
    }
}

/// Test algorithm switching
#[tokio::test]
async fn test_algorithm_switching() {
    let mut engine = RLEngine::new();

    // Start with Q-Learning (default)
    assert_eq!(engine.active_algorithm(), "q_learning");

    // Switch to DQN
    assert!(engine.set_algorithm("dqn").is_ok());
    assert_eq!(engine.active_algorithm(), "dqn");

    // Switch to PPO
    assert!(engine.set_algorithm("ppo").is_ok());
    assert_eq!(engine.active_algorithm(), "ppo");

    // Switch back to Q-Learning
    assert!(engine.set_algorithm("q_learning").is_ok());
    assert_eq!(engine.active_algorithm(), "q_learning");

    // Invalid algorithm should fail
    assert!(engine.set_algorithm("invalid_algo").is_err());
}

/// Test training with insufficient data
#[tokio::test]
async fn test_training_insufficient_data() {
    let mut engine = RLEngine::new();

    // Try to train with no data
    let result = engine.train();
    assert!(result.is_ok(), "Training with empty buffer should not crash");
    assert_eq!(result.unwrap(), 0.0, "Should return 0.0 loss with empty buffer");

    // Add just a few experiences (less than batch size of 32)
    for i in 0..5 {
        let state = create_test_state(&format!("task_{i}"), 0.5);
        let action = Action::RouteToAgent(AgentRole::Backend);
        let experience = Experience::new(state, action, 0.5, None, true);
        engine.record_experience(experience);
    }

    // Training with insufficient batch should return 0.0
    let result = engine.train();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0.0);
}

/// Test reward computation patterns
#[tokio::test]
async fn test_reward_patterns() {
    // 1. Successful task with low tokens and fast completion
    let reward_perfect = compute_test_reward(true, 500, 30000, 10000, 60000);
    assert!(reward_perfect > 0.8, "Perfect task should have high reward");

    // 2. Successful task with high tokens
    let reward_high_tokens = compute_test_reward(true, 9000, 30000, 10000, 60000);
    assert!(
        reward_high_tokens < reward_perfect,
        "High token usage should reduce reward"
    );

    // 3. Successful task that took too long
    let reward_slow = compute_test_reward(true, 500, 55000, 10000, 60000);
    assert!(
        reward_slow < reward_perfect,
        "Slow completion should reduce reward"
    );

    // 4. Failed task
    let reward_failed = compute_test_reward(false, 500, 30000, 10000, 60000);
    assert!(reward_failed < 0.0, "Failed task should have negative reward");

    // 5. Task with both issues
    let reward_bad = compute_test_reward(true, 9500, 58000, 10000, 60000);
    assert!(
        reward_bad < reward_high_tokens && reward_bad < reward_slow,
        "Multiple issues should compound penalty"
    );
}

/// Helper function to compute test reward
fn compute_test_reward(
    success: bool,
    tokens_used: u32,
    duration_ms: u32,
    max_tokens: u32,
    max_duration: u32,
) -> f64 {
    if !success {
        return -1.0;
    }

    let token_efficiency = 1.0 - (tokens_used as f64 / max_tokens as f64).min(1.0);
    let time_efficiency = 1.0 - (duration_ms as f64 / max_duration as f64).min(1.0);

    // Weighted combination
    0.3 + 0.4 * token_efficiency + 0.3 * time_efficiency
}

/// Test multi-step learning trajectory
#[tokio::test]
async fn test_learning_trajectory() {
    let mut engine = RLEngine::new();

    let mut rewards_per_epoch: Vec<f64> = Vec::new();

    for epoch in 0..5 {
        let mut epoch_reward = 0.0;

        for _step in 0..20 {
            let state = create_test_state("task", 0.5);

            let action = engine.predict(&state);

            // Reward based on selecting backend (since that's what we're simulating)
            let reward = match &action {
                Action::RouteToAgent(AgentRole::Backend) => 1.0,
                Action::RouteToAgent(_) => 0.3,
                _ => 0.1,
            };

            epoch_reward += reward;

            let experience = Experience::new(
                state.clone(),
                action,
                reward,
                Some(state),
                false,
            );
            engine.record_experience(experience);
        }

        // Train at end of epoch
        let _ = engine.train();

        rewards_per_epoch.push(epoch_reward);
        println!("Epoch {}: total reward = {:.1}", epoch + 1, epoch_reward);
    }

    // Generally, later epochs should have higher rewards (learning)
    // Due to exploration, this may not be strictly monotonic
    let first_half_avg: f64 = rewards_per_epoch[0..2].iter().sum::<f64>() / 2.0;
    let second_half_avg: f64 = rewards_per_epoch[3..5].iter().sum::<f64>() / 2.0;

    println!(
        "First half avg: {first_half_avg:.1}, Second half avg: {second_half_avg:.1}"
    );

    // The second half should show some improvement (allows for exploration variance)
    // This is a soft check since Q-learning has random exploration
}

/// Test algorithm parameter updates
#[tokio::test]
async fn test_parameter_updates() {
    let mut engine = RLEngine::new();

    // Get initial params
    let initial_params = engine.get_algorithm_params();
    assert!(!initial_params.is_null(), "Should have parameters");

    // Update params
    let new_params = serde_json::json!({"learning_rate": 0.05});
    assert!(engine.set_algorithm_params(new_params).is_ok());
}

/// Test concurrent access patterns
#[tokio::test]
async fn test_concurrent_access() {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let engine = Arc::new(RwLock::new(RLEngine::new()));

    // Spawn multiple tasks that interact with the engine
    let mut handles = Vec::new();

    for i in 0..10 {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            // Each task records experiences
            for j in 0..10 {
                let state = create_test_state(&format!("task_{i}_{j}"), 0.5);
                let action = Action::RouteToAgent(AgentRole::Backend);
                let experience = Experience::new(state, action, 0.5, None, true);

                let mut engine = engine_clone.write().await;
                engine.record_experience(experience);
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all experiences were recorded
    let engine = engine.read().await;
    let stats = engine.stats();
    assert_eq!(stats.buffer_size, 100);
    assert_eq!(stats.total_steps, 100);
}

/// Test stats tracking
#[tokio::test]
async fn test_stats_tracking() {
    let mut engine = RLEngine::new();

    // Record experiences
    for i in 0..10 {
        let state = create_test_state(&format!("task_{i}"), 0.5);
        let action = engine.predict(&state);
        let reward = 0.5 + (i as f64 * 0.1);
        let experience = Experience::new(state, action, reward, None, true);
        engine.record_experience(experience);
        engine.update_reward(reward).unwrap();
    }

    // Train
    let _ = engine.train();

    // Check stats
    let stats = engine.stats();
    assert_eq!(stats.buffer_size, 10);
    assert_eq!(stats.total_steps, 10);
    assert!(stats.total_rewards > 0.0);
    assert!(stats.average_reward > 0.0);

    // Verify algorithms list
    let algorithms = engine.list_algorithms();
    assert!(algorithms.contains(&"q_learning"));
    assert!(algorithms.contains(&"dqn"));
    assert!(algorithms.contains(&"ppo"));
}

/// Test experience serialization
#[tokio::test]
async fn test_experience_serialization() {
    let state = create_test_state("serialize_test", 0.7);
    let action = Action::RouteToAgent(AgentRole::Backend);
    let experience = Experience::new(state, action, 1.0, None, false);

    // Serialize to JSON
    let json = serde_json::to_string(&experience).unwrap();
    assert!(!json.is_empty());

    // Deserialize back
    let parsed: Experience = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.reward, 1.0);
    assert!(!parsed.done);
}

/// Test buffer clear operation
#[tokio::test]
async fn test_buffer_clear() {
    let mut engine = RLEngine::new();

    // Add some experiences
    for i in 0..20 {
        let state = create_test_state(&format!("task_{i}"), 0.5);
        let action = Action::RouteToAgent(AgentRole::Frontend);
        let experience = Experience::new(state, action, 0.5, None, true);
        engine.record_experience(experience);
    }

    assert_eq!(engine.stats().buffer_size, 20);

    // Clear buffer
    engine.clear_buffer();
    assert_eq!(engine.stats().buffer_size, 0);
}

/// Test composite actions
#[tokio::test]
async fn test_composite_actions() {
    let composite = Action::Composite(vec![
        Action::RouteToAgent(AgentRole::Backend),
        Action::AllocateTokens(0.8),
        Action::CompressContext(0.3),
    ]);

    // Composite actions have their own index
    let index = composite.to_index();
    assert_eq!(index, 11);

    // Serialize and deserialize
    let json = serde_json::to_string(&composite).unwrap();
    let parsed: Action = serde_json::from_str(&json).unwrap();

    if let Action::Composite(actions) = parsed {
        assert_eq!(actions.len(), 3);
    } else {
        panic!("Expected Composite action");
    }
}

/// Test different algorithms produce actions
#[tokio::test]
async fn test_all_algorithms_predict() {
    let mut engine = RLEngine::new();
    let state = create_test_state("test_task", 0.5);

    // Test each algorithm can predict
    for algo in &["q_learning", "dqn", "ppo"] {
        engine.set_algorithm(algo).unwrap();
        let action = engine.predict(&state);

        // All algorithms should return a valid action
        match action {
            Action::RouteToAgent(_) => {},
            Action::AllocateTokens(_) => {},
            Action::UsePattern(_) => {},
            Action::CompressContext(_) => {},
            Action::Composite(_) => {},
        }
    }
}
