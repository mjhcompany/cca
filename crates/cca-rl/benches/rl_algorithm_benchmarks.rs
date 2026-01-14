//! RL Algorithm Benchmarks
//!
//! Comprehensive benchmarks for Reinforcement Learning algorithms:
//! - Q-Learning training and prediction
//! - Experience buffer operations
//! - State feature extraction
//! - Action selection (epsilon-greedy)
//!
//! ## Hot Paths Identified
//! 1. RLEngine::predict() - Called for every task routing decision
//! 2. QLearning::train() - Called during training batches
//! 3. ExperienceBuffer::sample() - Random sampling from replay buffer
//! 4. State::to_features() - Feature vector generation
//!
//! ## Performance Targets
//! - Prediction: < 10µs per decision
//! - Training step: < 1ms per batch of 32 experiences
//! - Experience sampling: < 100µs for batch of 32
//! - Feature extraction: < 1µs per state

#![allow(dead_code)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// ============================================================================
// Local copies of RL types for benchmarking
// ============================================================================

mod rl_bench {
    use std::collections::{HashMap, VecDeque};

    pub type Reward = f64;

    #[derive(Debug, Clone)]
    pub struct State {
        pub task_type: String,
        pub available_agents: Vec<AgentState>,
        pub token_usage: f64,
        pub success_history: Vec<f64>,
        pub complexity: f64,
        pub features: Vec<f64>,
    }

    impl State {
        pub fn to_features(&self) -> Vec<f64> {
            let mut features = self.features.clone();

            for agent in &self.available_agents {
                features.push(if agent.is_busy { 1.0 } else { 0.0 });
                features.push(agent.success_rate);
                features.push(agent.avg_completion_time / 300.0);
            }

            features.push(self.token_usage);
            features.push(self.complexity);
            features.extend(&self.success_history);

            features
        }

        pub fn dimension(&self) -> usize {
            self.to_features().len()
        }
    }

    #[derive(Debug, Clone)]
    pub struct AgentState {
        pub role: String,
        pub is_busy: bool,
        pub success_rate: f64,
        pub avg_completion_time: f64,
    }

    #[derive(Debug, Clone)]
    pub enum Action {
        RouteToAgent(String),
        AllocateTokens(f64),
        CompressContext(f64),
    }

    impl Action {
        pub fn to_index(&self) -> usize {
            match self {
                Action::RouteToAgent(role) => match role.as_str() {
                    "coordinator" => 0,
                    "frontend" => 1,
                    "backend" => 2,
                    "dba" => 3,
                    "devops" => 4,
                    "security" => 5,
                    "qa" => 6,
                    _ => 7,
                },
                Action::AllocateTokens(_) => 8,
                Action::CompressContext(_) => 10,
            }
        }

        pub fn from_index(index: usize) -> Option<Self> {
            match index {
                0 => Some(Action::RouteToAgent("coordinator".to_string())),
                1 => Some(Action::RouteToAgent("frontend".to_string())),
                2 => Some(Action::RouteToAgent("backend".to_string())),
                3 => Some(Action::RouteToAgent("dba".to_string())),
                4 => Some(Action::RouteToAgent("devops".to_string())),
                5 => Some(Action::RouteToAgent("security".to_string())),
                6 => Some(Action::RouteToAgent("qa".to_string())),
                _ => None,
            }
        }

        pub fn action_space_size() -> usize {
            12
        }
    }

    #[derive(Debug, Clone)]
    pub struct Experience {
        pub state: State,
        pub action: Action,
        pub reward: Reward,
        pub next_state: Option<State>,
        pub done: bool,
    }

    impl Experience {
        pub fn new(
            state: State,
            action: Action,
            reward: Reward,
            next_state: Option<State>,
            done: bool,
        ) -> Self {
            Self {
                state,
                action,
                reward,
                next_state,
                done,
            }
        }
    }

    pub struct ExperienceBuffer {
        buffer: VecDeque<Experience>,
        capacity: usize,
    }

    impl ExperienceBuffer {
        pub fn new(capacity: usize) -> Self {
            Self {
                buffer: VecDeque::with_capacity(capacity),
                capacity,
            }
        }

        pub fn push(&mut self, experience: Experience) {
            if self.buffer.len() >= self.capacity {
                self.buffer.pop_front();
            }
            self.buffer.push_back(experience);
        }

        pub fn sample(&self, batch_size: usize) -> Vec<Experience> {
            use rand::seq::SliceRandom;
            let mut rng = rand::thread_rng();
            let experiences: Vec<_> = self.buffer.iter().cloned().collect();
            experiences
                .choose_multiple(&mut rng, batch_size.min(experiences.len()))
                .cloned()
                .collect()
        }

        pub fn len(&self) -> usize {
            self.buffer.len()
        }

        pub fn is_empty(&self) -> bool {
            self.buffer.is_empty()
        }

        pub fn clear(&mut self) {
            self.buffer.clear();
        }

        pub fn all(&self) -> Vec<Experience> {
            self.buffer.iter().cloned().collect()
        }
    }

    pub struct QLearning {
        q_table: HashMap<String, Vec<f64>>,
        learning_rate: f64,
        discount_factor: f64,
        epsilon: f64,
        action_space_size: usize,
    }

    impl QLearning {
        pub fn new(learning_rate: f64, discount_factor: f64, epsilon: f64) -> Self {
            Self {
                q_table: HashMap::new(),
                learning_rate,
                discount_factor,
                epsilon,
                action_space_size: Action::action_space_size(),
            }
        }

        fn state_key(state: &State) -> String {
            format!("{:.2}_{:.2}", state.complexity, state.token_usage)
        }

        fn get_q_values(&self, state: &State) -> Vec<f64> {
            let key = Self::state_key(state);
            self.q_table
                .get(&key)
                .cloned()
                .unwrap_or_else(|| vec![0.0; self.action_space_size])
        }

        pub fn train(&mut self, experiences: &[Experience]) -> f64 {
            let mut total_loss = 0.0;

            for exp in experiences {
                let state_key = Self::state_key(&exp.state);
                let action_idx = exp.action.to_index();

                let q_values = self.get_q_values(&exp.state);
                let current_q = q_values[action_idx];

                let target = if exp.done {
                    exp.reward
                } else if let Some(ref next_state) = exp.next_state {
                    let next_q = self.get_q_values(next_state);
                    let max_next_q = next_q.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    exp.reward + self.discount_factor * max_next_q
                } else {
                    exp.reward
                };

                let new_q = current_q + self.learning_rate * (target - current_q);

                let q_values = self
                    .q_table
                    .entry(state_key)
                    .or_insert_with(|| vec![0.0; self.action_space_size]);
                q_values[action_idx] = new_q;

                total_loss += (target - current_q).powi(2);
            }

            total_loss / experiences.len() as f64
        }

        pub fn predict(&self, state: &State) -> Action {
            let q_values = self.get_q_values(state);

            if rand::random::<f64>() < self.epsilon {
                let idx = rand::random::<usize>() % self.action_space_size;
                Action::from_index(idx).unwrap_or(Action::RouteToAgent("coordinator".to_string()))
            } else {
                let best_idx = q_values
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                    .map_or(0, |(i, _)| i);
                Action::from_index(best_idx)
                    .unwrap_or(Action::RouteToAgent("coordinator".to_string()))
            }
        }

        pub fn update(&mut self, _reward: Reward) {
            self.epsilon *= 0.999;
            if self.epsilon < 0.01 {
                self.epsilon = 0.01;
            }
        }

        pub fn q_table_size(&self) -> usize {
            self.q_table.len()
        }
    }

    pub struct RLEngine {
        algorithm: QLearning,
        experience_buffer: ExperienceBuffer,
        training_batch_size: usize,
        total_steps: u64,
        total_rewards: f64,
    }

    impl RLEngine {
        pub fn new() -> Self {
            Self {
                algorithm: QLearning::new(0.1, 0.99, 0.1),
                experience_buffer: ExperienceBuffer::new(10000),
                training_batch_size: 32,
                total_steps: 0,
                total_rewards: 0.0,
            }
        }

        pub fn record_experience(&mut self, experience: Experience) {
            self.experience_buffer.push(experience);
            self.total_steps += 1;
        }

        pub fn train(&mut self) -> f64 {
            if self.experience_buffer.len() < self.training_batch_size {
                return 0.0;
            }

            let batch = self.experience_buffer.sample(self.training_batch_size);
            self.algorithm.train(&batch)
        }

        pub fn predict(&self, state: &State) -> Action {
            self.algorithm.predict(state)
        }

        pub fn update_reward(&mut self, reward: Reward) {
            self.total_rewards += reward;
            self.algorithm.update(reward);
        }

        pub fn buffer_size(&self) -> usize {
            self.experience_buffer.len()
        }

        pub fn total_steps(&self) -> u64 {
            self.total_steps
        }
    }
}

use rl_bench::*;

// ============================================================================
// Test Data Generators
// ============================================================================

fn create_test_state() -> State {
    State {
        task_type: "backend".to_string(),
        available_agents: vec![
            AgentState {
                role: "backend".to_string(),
                is_busy: false,
                success_rate: 0.95,
                avg_completion_time: 120.0,
            },
            AgentState {
                role: "frontend".to_string(),
                is_busy: true,
                success_rate: 0.88,
                avg_completion_time: 90.0,
            },
            AgentState {
                role: "dba".to_string(),
                is_busy: false,
                success_rate: 0.92,
                avg_completion_time: 150.0,
            },
        ],
        token_usage: 0.5,
        success_history: vec![1.0, 0.8, 1.0, 0.9, 1.0],
        complexity: 0.6,
        features: vec![0.1, 0.2, 0.3, 0.4, 0.5],
    }
}

fn create_test_experience(state: State) -> Experience {
    Experience::new(
        state.clone(),
        Action::RouteToAgent("backend".to_string()),
        1.0,
        Some(state),
        false,
    )
}

fn generate_experiences(count: usize) -> Vec<Experience> {
    (0..count)
        .map(|i| {
            let mut state = create_test_state();
            state.complexity = (i as f64 * 0.1) % 1.0;
            state.token_usage = (i as f64 * 0.05) % 1.0;
            Experience::new(
                state.clone(),
                Action::RouteToAgent(["backend", "frontend", "dba", "devops"][i % 4].to_string()),
                if i % 3 == 0 { 1.0 } else { 0.5 },
                Some(state),
                i % 10 == 0,
            )
        })
        .collect()
}

fn generate_varied_states(count: usize) -> Vec<State> {
    (0..count)
        .map(|i| {
            let mut state = create_test_state();
            state.complexity = (i as f64 * 0.1) % 1.0;
            state.token_usage = (i as f64 * 0.05) % 1.0;
            state.task_type = ["backend", "frontend", "database", "api"][i % 4].to_string();
            state
        })
        .collect()
}

// ============================================================================
// State Feature Extraction Benchmarks
// ============================================================================

fn bench_state_to_features(c: &mut Criterion) {
    let state = create_test_state();

    c.bench_function("state/to_features", |b| {
        b.iter(|| state.to_features())
    });
}

fn bench_state_to_features_varying_agents(c: &mut Criterion) {
    let agent_counts = [1, 3, 5, 10, 20];

    let mut group = c.benchmark_group("state/to_features_by_agent_count");
    for count in agent_counts {
        let mut state = create_test_state();
        state.available_agents = (0..count)
            .map(|i| AgentState {
                role: format!("agent_{i}"),
                is_busy: i % 2 == 0,
                success_rate: 0.9,
                avg_completion_time: 100.0,
            })
            .collect();

        group.bench_with_input(BenchmarkId::from_parameter(count), &state, |b, state| {
            b.iter(|| state.to_features())
        });
    }
    group.finish();
}

fn bench_state_dimension(c: &mut Criterion) {
    let state = create_test_state();

    c.bench_function("state/dimension", |b| {
        b.iter(|| state.dimension())
    });
}

// ============================================================================
// Experience Buffer Benchmarks
// ============================================================================

fn bench_experience_buffer_push(c: &mut Criterion) {
    let sizes = [100, 1000, 5000, 10000];

    let mut group = c.benchmark_group("experience_buffer/push");
    for size in sizes {
        let mut buffer = ExperienceBuffer::new(size);
        let experience = create_test_experience(create_test_state());

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &experience,
            |b, experience| {
                b.iter(|| {
                    buffer.push(experience.clone());
                })
            },
        );
    }
    group.finish();
}

fn bench_experience_buffer_sample(c: &mut Criterion) {
    let batch_sizes = [8, 16, 32, 64, 128];

    let mut group = c.benchmark_group("experience_buffer/sample");
    for batch_size in batch_sizes {
        // Pre-fill buffer with 1000 experiences
        let mut buffer = ExperienceBuffer::new(1000);
        for exp in generate_experiences(1000) {
            buffer.push(exp);
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter(|| buffer.sample(black_box(batch_size)))
            },
        );
    }
    group.finish();
}

fn bench_experience_buffer_sample_from_sizes(c: &mut Criterion) {
    let buffer_sizes = [100, 500, 1000, 5000, 10000];

    let mut group = c.benchmark_group("experience_buffer/sample_from_buffer_size");
    for buffer_size in buffer_sizes {
        let mut buffer = ExperienceBuffer::new(buffer_size);
        for exp in generate_experiences(buffer_size) {
            buffer.push(exp);
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(buffer_size),
            &buffer,
            |b, buffer| {
                b.iter(|| buffer.sample(32))
            },
        );
    }
    group.finish();
}

// ============================================================================
// Q-Learning Algorithm Benchmarks
// ============================================================================

fn bench_qlearning_train(c: &mut Criterion) {
    let batch_sizes = [8, 16, 32, 64, 128];

    let mut group = c.benchmark_group("qlearning/train");
    for batch_size in batch_sizes {
        let experiences = generate_experiences(batch_size);

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &experiences,
            |b, experiences| {
                let mut qlearning = QLearning::new(0.1, 0.99, 0.1);
                b.iter(|| qlearning.train(black_box(experiences)))
            },
        );
    }
    group.finish();
}

fn bench_qlearning_predict(c: &mut Criterion) {
    let state = create_test_state();
    let qlearning = QLearning::new(0.1, 0.99, 0.1);

    c.bench_function("qlearning/predict", |b| {
        b.iter(|| qlearning.predict(black_box(&state)))
    });
}

fn bench_qlearning_predict_trained(c: &mut Criterion) {
    // Pre-train Q-learning with some experiences
    let mut qlearning = QLearning::new(0.1, 0.99, 0.1);
    let experiences = generate_experiences(1000);
    for chunk in experiences.chunks(32) {
        qlearning.train(chunk);
    }

    let states = generate_varied_states(100);

    let mut group = c.benchmark_group("qlearning/predict_trained");
    group.bench_function("single_prediction", |b| {
        let state = &states[0];
        b.iter(|| qlearning.predict(black_box(state)))
    });

    group.bench_function("100_predictions", |b| {
        b.iter(|| {
            for state in &states {
                let _ = qlearning.predict(black_box(state));
            }
        })
    });
    group.finish();
}

fn bench_qlearning_q_table_growth(c: &mut Criterion) {
    let training_iterations = [10, 50, 100, 500, 1000];

    let mut group = c.benchmark_group("qlearning/q_table_growth");
    for iterations in training_iterations {
        let experiences = generate_experiences(iterations);

        group.bench_with_input(
            BenchmarkId::from_parameter(iterations),
            &experiences,
            |b, experiences| {
                b.iter(|| {
                    let mut qlearning = QLearning::new(0.1, 0.99, 0.1);
                    for chunk in experiences.chunks(32) {
                        qlearning.train(chunk);
                    }
                    qlearning.q_table_size()
                })
            },
        );
    }
    group.finish();
}

// ============================================================================
// RL Engine Benchmarks
// ============================================================================

fn bench_rl_engine_full_cycle(c: &mut Criterion) {
    c.bench_function("rl_engine/full_cycle", |b| {
        let mut engine = RLEngine::new();
        let experiences = generate_experiences(100);

        // Pre-fill buffer
        for exp in &experiences {
            engine.record_experience(exp.clone());
        }

        let state = create_test_state();

        b.iter(|| {
            // Predict
            let action = engine.predict(black_box(&state));
            // Record experience
            let exp = Experience::new(
                state.clone(),
                action,
                1.0,
                Some(state.clone()),
                false,
            );
            engine.record_experience(exp);
            // Train
            engine.train();
            // Update
            engine.update_reward(1.0);
        })
    });
}

fn bench_rl_engine_predict_only(c: &mut Criterion) {
    let mut engine = RLEngine::new();

    // Pre-train
    for exp in generate_experiences(500) {
        engine.record_experience(exp);
    }
    for _ in 0..10 {
        engine.train();
    }

    let state = create_test_state();

    c.bench_function("rl_engine/predict_only", |b| {
        b.iter(|| engine.predict(black_box(&state)))
    });
}

fn bench_rl_engine_train_only(c: &mut Criterion) {
    let mut engine = RLEngine::new();

    // Pre-fill buffer
    for exp in generate_experiences(1000) {
        engine.record_experience(exp);
    }

    c.bench_function("rl_engine/train_only", |b| {
        b.iter(|| engine.train())
    });
}

fn bench_rl_engine_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("rl_engine/throughput");
    group.throughput(Throughput::Elements(100));

    group.bench_function("100_experiences", |b| {
        let experiences = generate_experiences(100);

        b.iter(|| {
            let mut engine = RLEngine::new();
            for exp in &experiences {
                engine.record_experience(exp.clone());
                if engine.buffer_size() >= 32 {
                    engine.train();
                }
            }
            engine.total_steps()
        })
    });

    group.finish();
}

// ============================================================================
// Action Selection Benchmarks
// ============================================================================

fn bench_action_to_index(c: &mut Criterion) {
    let actions = vec![
        Action::RouteToAgent("backend".to_string()),
        Action::RouteToAgent("frontend".to_string()),
        Action::AllocateTokens(0.5),
        Action::CompressContext(0.3),
    ];

    c.bench_function("action/to_index", |b| {
        b.iter(|| {
            for action in &actions {
                let _ = action.to_index();
            }
        })
    });
}

fn bench_action_from_index(c: &mut Criterion) {
    let indices = vec![0, 1, 2, 3, 4, 5, 6, 7];

    c.bench_function("action/from_index", |b| {
        b.iter(|| {
            for &idx in &indices {
                let _ = Action::from_index(idx);
            }
        })
    });
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    name = state_benchmarks;
    config = Criterion::default();
    targets =
        bench_state_to_features,
        bench_state_to_features_varying_agents,
        bench_state_dimension,
);

criterion_group!(
    name = experience_buffer_benchmarks;
    config = Criterion::default();
    targets =
        bench_experience_buffer_push,
        bench_experience_buffer_sample,
        bench_experience_buffer_sample_from_sizes,
);

criterion_group!(
    name = qlearning_benchmarks;
    config = Criterion::default();
    targets =
        bench_qlearning_train,
        bench_qlearning_predict,
        bench_qlearning_predict_trained,
        bench_qlearning_q_table_growth,
);

criterion_group!(
    name = rl_engine_benchmarks;
    config = Criterion::default();
    targets =
        bench_rl_engine_full_cycle,
        bench_rl_engine_predict_only,
        bench_rl_engine_train_only,
        bench_rl_engine_throughput,
);

criterion_group!(
    name = action_benchmarks;
    config = Criterion::default();
    targets =
        bench_action_to_index,
        bench_action_from_index,
);

criterion_main!(
    state_benchmarks,
    experience_buffer_benchmarks,
    qlearning_benchmarks,
    rl_engine_benchmarks,
    action_benchmarks
);
