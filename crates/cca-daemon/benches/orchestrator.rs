//! Orchestrator Routing Benchmarks
//!
//! Benchmarks for task routing, workload distribution, and coordination.
//! These are critical paths for agent orchestration performance.
//!
//! ## Hot Paths
//! - Task routing decisions
//! - Workload balancing across agents
//! - Result aggregation
//! - Priority-based scheduling

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;

// ============================================================================
// Simulated Orchestrator Components
// ============================================================================

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Task {
    id: String,
    priority: u8,
    task_type: TaskType,
    payload_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskType {
    CodeAnalysis,
    Search,
    Generation,
    Validation,
}

#[derive(Debug, Clone)]
struct AgentState {
    #[allow(dead_code)]
    id: String,
    current_load: usize,
    max_capacity: usize,
    specializations: Vec<TaskType>,
    success_rate: f64,
}

impl AgentState {
    fn can_handle(&self, task_type: TaskType) -> bool {
        self.specializations.is_empty() || self.specializations.contains(&task_type)
    }

    fn available_capacity(&self) -> usize {
        self.max_capacity.saturating_sub(self.current_load)
    }
}

struct Orchestrator {
    agents: HashMap<String, AgentState>,
    #[allow(dead_code)]
    task_queue: Vec<Task>,
    completed_count: usize,
}

impl Orchestrator {
    fn new(agent_count: usize) -> Self {
        let mut agents = HashMap::new();
        let task_types = [
            TaskType::CodeAnalysis,
            TaskType::Search,
            TaskType::Generation,
            TaskType::Validation,
        ];

        for i in 0..agent_count {
            let agent_id = format!("agent_{}", i);
            let specializations = if i % 3 == 0 {
                vec![] // Generalist
            } else {
                vec![task_types[i % task_types.len()]]
            };

            agents.insert(
                agent_id.clone(),
                AgentState {
                    id: agent_id,
                    current_load: 0,
                    max_capacity: 10,
                    specializations,
                    success_rate: 0.9 + (i as f64 * 0.001),
                },
            );
        }

        Self {
            agents,
            task_queue: Vec::new(),
            completed_count: 0,
        }
    }

    fn route_task(&mut self, task: &Task) -> Option<String> {
        let mut best_agent: Option<&str> = None;
        let mut best_score = f64::MIN;

        for (agent_id, state) in &self.agents {
            if !state.can_handle(task.task_type) {
                continue;
            }

            let capacity = state.available_capacity();
            if capacity == 0 {
                continue;
            }

            // Scoring: prioritize available capacity, success rate, and specialization
            let capacity_score = capacity as f64 / state.max_capacity as f64;
            let specialization_bonus = if state.specializations.contains(&task.task_type) {
                0.2
            } else {
                0.0
            };
            let priority_factor = task.priority as f64 / 10.0;

            let score =
                capacity_score * 0.4 + state.success_rate * 0.4 + specialization_bonus + priority_factor * 0.1;

            if score > best_score {
                best_score = score;
                best_agent = Some(agent_id);
            }
        }

        if let Some(agent_id) = best_agent {
            let agent_id = agent_id.to_string();
            if let Some(state) = self.agents.get_mut(&agent_id) {
                state.current_load += 1;
            }
            Some(agent_id)
        } else {
            None
        }
    }

    fn complete_task(&mut self, agent_id: &str) {
        if let Some(state) = self.agents.get_mut(agent_id) {
            state.current_load = state.current_load.saturating_sub(1);
            self.completed_count += 1;
        }
    }

    fn get_workload_distribution(&self) -> HashMap<String, f64> {
        self.agents
            .iter()
            .map(|(id, state)| {
                let utilization = state.current_load as f64 / state.max_capacity as f64;
                (id.clone(), utilization)
            })
            .collect()
    }

    fn find_available_agents(&self, task_type: TaskType) -> Vec<&str> {
        self.agents
            .iter()
            .filter(|(_, state)| state.can_handle(task_type) && state.available_capacity() > 0)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    fn aggregate_results(&self, results: &[TaskResult]) -> AggregatedResult {
        let total = results.len();
        let successful = results.iter().filter(|r| r.success).count();
        let total_duration: u64 = results.iter().map(|r| r.duration_ms).sum();

        AggregatedResult {
            total_tasks: total,
            successful_tasks: successful,
            failed_tasks: total - successful,
            average_duration_ms: if total > 0 {
                total_duration / total as u64
            } else {
                0
            },
            success_rate: if total > 0 {
                successful as f64 / total as f64
            } else {
                0.0
            },
        }
    }
}

#[allow(dead_code)]
struct TaskResult {
    task_id: String,
    agent_id: String,
    success: bool,
    duration_ms: u64,
}

#[allow(dead_code)]
struct AggregatedResult {
    total_tasks: usize,
    successful_tasks: usize,
    failed_tasks: usize,
    average_duration_ms: u64,
    success_rate: f64,
}

// ============================================================================
// Test Data Generators
// ============================================================================

fn generate_tasks(count: usize) -> Vec<Task> {
    let task_types = [
        TaskType::CodeAnalysis,
        TaskType::Search,
        TaskType::Generation,
        TaskType::Validation,
    ];

    (0..count)
        .map(|i| Task {
            id: format!("task_{}", i),
            priority: ((i % 10) + 1) as u8,
            task_type: task_types[i % task_types.len()],
            payload_size: 100 + (i % 1000),
        })
        .collect()
}

fn generate_task_results(count: usize) -> Vec<TaskResult> {
    (0..count)
        .map(|i| TaskResult {
            task_id: format!("task_{}", i),
            agent_id: format!("agent_{}", i % 10),
            success: i % 10 != 0, // 90% success rate
            duration_ms: 50 + (i as u64 % 200),
        })
        .collect()
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_task_routing(c: &mut Criterion) {
    let tasks = generate_tasks(100);

    let mut group = c.benchmark_group("orchestrator/routing");

    // Benchmark with different agent counts
    for agent_count in [5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("agents", agent_count),
            agent_count,
            |b, &count| {
                b.iter(|| {
                    let mut orchestrator = Orchestrator::new(count);
                    for task in &tasks {
                        let _ = orchestrator.route_task(black_box(task));
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_workload_balancing(c: &mut Criterion) {
    let mut group = c.benchmark_group("orchestrator/workload");

    // Benchmark workload distribution calculation
    for agent_count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("distribution", agent_count),
            agent_count,
            |b, &count| {
                let mut orchestrator = Orchestrator::new(count);
                // Pre-load some tasks
                let tasks = generate_tasks(count * 5);
                for task in &tasks {
                    let _ = orchestrator.route_task(task);
                }

                b.iter(|| orchestrator.get_workload_distribution())
            },
        );
    }

    group.finish();
}

fn bench_agent_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("orchestrator/agent_lookup");

    for agent_count in [10, 50, 100].iter() {
        let orchestrator = Orchestrator::new(*agent_count);

        group.bench_with_input(
            BenchmarkId::new("find_available", agent_count),
            &orchestrator,
            |b, orchestrator| {
                b.iter(|| {
                    let _ = orchestrator.find_available_agents(black_box(TaskType::CodeAnalysis));
                    let _ = orchestrator.find_available_agents(black_box(TaskType::Search));
                    let _ = orchestrator.find_available_agents(black_box(TaskType::Generation));
                })
            },
        );
    }

    group.finish();
}

fn bench_result_aggregation(c: &mut Criterion) {
    let orchestrator = Orchestrator::new(10);

    let mut group = c.benchmark_group("orchestrator/aggregation");

    for result_count in [100, 500, 1000, 5000].iter() {
        let results = generate_task_results(*result_count);

        group.throughput(Throughput::Elements(*result_count as u64));
        group.bench_with_input(
            BenchmarkId::new("results", result_count),
            &results,
            |b, results| b.iter(|| orchestrator.aggregate_results(black_box(results))),
        );
    }

    group.finish();
}

fn bench_high_throughput_routing(c: &mut Criterion) {
    let tasks = generate_tasks(1000);

    c.bench_function("orchestrator/high_throughput_1000_tasks", |b| {
        b.iter(|| {
            let mut orchestrator = Orchestrator::new(20);
            let mut routed = 0;

            for task in &tasks {
                if let Some(agent) = orchestrator.route_task(task) {
                    routed += 1;
                    // Simulate some task completions
                    if routed % 5 == 0 {
                        orchestrator.complete_task(&agent);
                    }
                }
            }
            routed
        })
    });
}

fn bench_priority_scheduling(c: &mut Criterion) {
    // Generate tasks with varied priorities
    let mut tasks = generate_tasks(500);

    // Assign priorities in waves
    for (i, task) in tasks.iter_mut().enumerate() {
        task.priority = if i < 100 {
            10 // Critical
        } else if i < 300 {
            5 // Normal
        } else {
            1 // Low
        };
    }

    c.bench_function("orchestrator/priority_scheduling_500_tasks", |b| {
        b.iter(|| {
            let mut orchestrator = Orchestrator::new(15);

            // Sort by priority (simulating priority queue)
            let mut sorted_tasks = tasks.clone();
            sorted_tasks.sort_by(|a, b| b.priority.cmp(&a.priority));

            for task in &sorted_tasks {
                let _ = orchestrator.route_task(black_box(task));
            }
        })
    });
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    benches,
    bench_task_routing,
    bench_workload_balancing,
    bench_agent_lookup,
    bench_result_aggregation,
    bench_high_throughput_routing,
    bench_priority_scheduling,
);

criterion_main!(benches);
