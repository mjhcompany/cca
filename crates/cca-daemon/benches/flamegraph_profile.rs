//! Flamegraph Profiling Benchmark
//!
//! This benchmark is designed for generating flamegraphs and detailed CPU profiles.
//! It runs longer iterations to collect enough samples for meaningful profiles.
//!
//! ## Usage
//!
//! ### Generate flamegraph with pprof (recommended):
//! ```bash
//! cargo bench --bench flamegraph_profile -- --profile-time=10
//! ```
//! Output will be in: target/criterion/profile/*/profile/flamegraph.svg
//!
//! ### Generate flamegraph with perf (Linux):
//! ```bash
//! cargo build --profile profiling
//! perf record -g --call-graph=dwarf ./target/profiling/ccad &
//! # ... run some load ...
//! perf script | stackcollapse-perf.pl | flamegraph.pl > flamegraph.svg
//! ```
//!
//! ### Using cargo-flamegraph:
//! ```bash
//! cargo flamegraph --bench flamegraph_profile -- --bench
//! ```

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pprof::criterion::{Output, PProfProfiler};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Simulated Hot Path Components
// ============================================================================

/// Token counter (matches daemon implementation)
struct TokenCounter {
    chars_per_token: f64,
}

impl TokenCounter {
    fn new() -> Self {
        Self {
            chars_per_token: 4.0,
        }
    }

    fn count(&self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let words = text.split_whitespace().count();
        let chars = text.len();
        let word_estimate = (words as f64 * 1.3) as u32;
        let char_estimate = (chars as f64 / self.chars_per_token) as u32;
        word_estimate.max(char_estimate).max(1)
    }
}

/// Context analyzer (matches daemon implementation)
struct ContextAnalyzer {
    counter: TokenCounter,
    ngram_size: usize,
}

impl ContextAnalyzer {
    fn new() -> Self {
        Self {
            counter: TokenCounter::new(),
            ngram_size: 3,
        }
    }

    fn analyze(&self, content: &str) -> ContextAnalysis {
        let total_tokens = self.counter.count(content);
        let lines: Vec<&str> = content.lines().collect();

        let mut line_counts: HashMap<&str, usize> = HashMap::new();
        for line in &lines {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                *line_counts.entry(trimmed).or_default() += 1;
            }
        }

        let repeated_lines: Vec<_> = line_counts
            .iter()
            .filter(|(_, &count)| count > 1)
            .map(|(&line, &count)| (line.to_string(), count))
            .collect();

        let repeated_tokens: u32 = repeated_lines
            .iter()
            .map(|(line, count)| self.counter.count(line) * (*count as u32 - 1))
            .sum();

        let code_block_count = content.matches("```").count() / 2;
        let long_lines = lines.iter().filter(|l| l.len() > 200).count();

        ContextAnalysis {
            total_tokens,
            repeated_tokens,
            code_block_count,
            long_line_count: long_lines,
            compression_potential: self.estimate_compression_potential(content),
        }
    }

    fn compare(&self, context_a: &str, context_b: &str) -> f64 {
        let ngrams_a = self.extract_ngrams(context_a);
        let ngrams_b = self.extract_ngrams(context_b);

        let intersection: usize = ngrams_a.iter().filter(|n| ngrams_b.contains(*n)).count();
        let union = ngrams_a.len() + ngrams_b.len() - intersection;

        if union > 0 {
            intersection as f64 / union as f64
        } else {
            0.0
        }
    }

    fn extract_ngrams(&self, text: &str) -> HashSet<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut ngrams = HashSet::new();

        if words.len() >= self.ngram_size {
            for window in words.windows(self.ngram_size) {
                ngrams.insert(window.join(" "));
            }
        }

        ngrams
    }

    fn estimate_compression_potential(&self, content: &str) -> f64 {
        let total = self.counter.count(content) as f64;
        if total == 0.0 {
            return 0.0;
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len().max(1);

        let mut seen = HashSet::new();
        let unique_lines = lines.iter().filter(|l| seen.insert(l.trim())).count();
        let repetition_ratio = 1.0 - (unique_lines as f64 / total_lines as f64);

        let filler_words = ["basically", "essentially", "actually", "just", "simply"];
        let filler_count: usize = filler_words
            .iter()
            .map(|w| content.to_lowercase().matches(w).count())
            .sum();
        let verbosity_score = (filler_count as f64 / (content.len() as f64 / 100.0)).min(1.0);

        let structural_chars = content
            .chars()
            .filter(|c| matches!(c, '#' | '-' | '=' | '*' | '|'))
            .count();
        let structural_overhead = (structural_chars as f64 / content.len() as f64).min(0.3);

        (repetition_ratio * 0.4 + verbosity_score * 0.3 + structural_overhead * 0.3).min(0.5)
    }
}

#[allow(dead_code)]
struct ContextAnalysis {
    total_tokens: u32,
    repeated_tokens: u32,
    code_block_count: usize,
    long_line_count: usize,
    compression_potential: f64,
}

/// Simulated task routing (matches orchestrator hot path)
struct TaskRouter {
    agent_loads: HashMap<String, usize>,
}

impl TaskRouter {
    fn new(agent_count: usize) -> Self {
        let mut agent_loads = HashMap::new();
        for i in 0..agent_count {
            agent_loads.insert(format!("agent_{}", i), 0);
        }
        Self { agent_loads }
    }

    fn route_task(&mut self, task_priority: u8) -> String {
        // Find agent with lowest load, preferring higher capacity for high priority
        let mut best_agent = String::new();
        let mut best_score = usize::MAX;

        for (agent_id, load) in &self.agent_loads {
            let score = if task_priority > 5 {
                *load // High priority: just pick lowest load
            } else {
                load + (task_priority as usize * 10) // Lower priority: factor in priority
            };

            if score < best_score {
                best_score = score;
                best_agent = agent_id.clone();
            }
        }

        // Increment load
        if let Some(load) = self.agent_loads.get_mut(&best_agent) {
            *load += 1;
        }

        best_agent
    }

    fn complete_task(&mut self, agent_id: &str) {
        if let Some(load) = self.agent_loads.get_mut(agent_id) {
            *load = load.saturating_sub(1);
        }
    }
}

// ============================================================================
// Test Data Generators
// ============================================================================

fn generate_realistic_context(size: usize) -> String {
    let parts = vec![
        "## Task Context\n\nThe user is working on a complex software project.\n",
        "They need help with implementing a new feature that involves:\n",
        "- Database queries\n- API endpoint design\n- Frontend components\n",
        "\n### Code Analysis\n\nThe existing codebase uses Rust for the backend:\n",
        "```rust\nfn process_request(req: Request) -> Response {\n",
        "    let data = fetch_data(&req.id)?;\n",
        "    transform_and_respond(data)\n}\n```\n",
        "\n### Previous Conversation\n\nUser asked about performance optimization.\n",
        "Assistant suggested using connection pooling and caching.\n",
        "User implemented the suggestions and saw 40% improvement.\n",
    ];

    let base = parts.join("");
    let repeat_count = size / base.len() + 1;
    base.repeat(repeat_count)[..size].to_string()
}

fn generate_agent_messages(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            let agent_id = format!("agent_{}", i % 10);
            let message = format!(
                "Processing task {} with intermediate results: {:?}",
                i,
                vec![i * 2; 5]
            );
            (agent_id, message)
        })
        .collect()
}

// ============================================================================
// Profiling Workloads
// ============================================================================

/// Combined workload simulating real daemon operations
fn daemon_workload(iterations: usize) {
    let analyzer = ContextAnalyzer::new();
    let mut router = TaskRouter::new(10);

    // Simulate realistic workload
    let context_a = generate_realistic_context(5000);
    let context_b = generate_realistic_context(5000);
    let messages = generate_agent_messages(100);

    for i in 0..iterations {
        // Token analysis (hot path)
        let _ = analyzer.analyze(&context_a);

        // Redundancy detection (hot path)
        let _ = analyzer.compare(&context_a, &context_b);

        // Task routing (hot path)
        let priority = ((i % 10) + 1) as u8;
        let agent = router.route_task(priority);

        // Simulate task completion
        if i % 3 == 0 {
            router.complete_task(&agent);
        }

        // Message processing
        for (agent_id, message) in messages.iter().take(10) {
            let _ = analyzer.analyze(message);
            let _ = black_box(agent_id);
        }
    }
}

/// Token-heavy workload for profiling token counting paths
fn token_workload(iterations: usize) {
    let counter = TokenCounter::new();
    let analyzer = ContextAnalyzer::new();

    let contexts: Vec<String> = (0..10)
        .map(|i| generate_realistic_context(1000 * (i + 1)))
        .collect();

    for i in 0..iterations {
        let context = &contexts[i % contexts.len()];

        // Direct token counting
        let _ = counter.count(context);

        // Full analysis
        let _ = analyzer.analyze(context);

        // Compression estimation (involves multiple passes)
        let _ = analyzer.estimate_compression_potential(context);
    }
}

/// N-gram heavy workload for profiling similarity detection
fn ngram_workload(iterations: usize) {
    let analyzer = ContextAnalyzer::new();

    let contexts: Vec<String> = (0..5)
        .map(|i| generate_realistic_context(2000 * (i + 1)))
        .collect();

    for i in 0..iterations {
        let a = &contexts[i % contexts.len()];
        let b = &contexts[(i + 1) % contexts.len()];

        // This exercises the ngram extraction heavily
        let _ = analyzer.compare(a, b);
    }
}

/// Routing workload for profiling task distribution
fn routing_workload(iterations: usize) {
    let mut router = TaskRouter::new(20);

    for i in 0..iterations {
        // Route with varying priorities
        let priority = ((i % 10) + 1) as u8;
        let agent = router.route_task(priority);

        // Complete some tasks to vary load distribution
        if i % 4 == 0 {
            router.complete_task(&agent);
        }

        // Occasionally route burst of high-priority tasks
        if i % 100 == 0 {
            for _ in 0..10 {
                let _ = router.route_task(10);
            }
        }
    }
}

// ============================================================================
// Criterion Benchmarks with pprof
// ============================================================================

fn bench_daemon_workload_profiled(c: &mut Criterion) {
    c.bench_function("profile/daemon_workload", |b| {
        b.iter(|| daemon_workload(black_box(100)))
    });
}

fn bench_token_workload_profiled(c: &mut Criterion) {
    c.bench_function("profile/token_workload", |b| {
        b.iter(|| token_workload(black_box(500)))
    });
}

fn bench_ngram_workload_profiled(c: &mut Criterion) {
    c.bench_function("profile/ngram_workload", |b| {
        b.iter(|| ngram_workload(black_box(200)))
    });
}

fn bench_routing_workload_profiled(c: &mut Criterion) {
    c.bench_function("profile/routing_workload", |b| {
        b.iter(|| routing_workload(black_box(1000)))
    });
}

// ============================================================================
// Criterion Configuration with PProfProfiler
// ============================================================================

fn profiling_config() -> Criterion {
    Criterion::default()
        .with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)))
        .sample_size(50) // Fewer samples but longer iterations
        .measurement_time(std::time::Duration::from_secs(10))
}

criterion_group! {
    name = profiling;
    config = profiling_config();
    targets =
        bench_daemon_workload_profiled,
        bench_token_workload_profiled,
        bench_ngram_workload_profiled,
        bench_routing_workload_profiled,
}

criterion_main!(profiling);
