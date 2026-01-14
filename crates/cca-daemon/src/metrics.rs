//! Prometheus metrics for CCA Daemon
//!
//! Exposes key performance and operational metrics in Prometheus format
//! for monitoring and alerting via Grafana.

// Allow unused code - these metrics functions are infrastructure for future use
#![allow(dead_code)]

use prometheus::{
    HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
    TextEncoder,
};
use std::sync::LazyLock;

/// Global Prometheus registry for CCA metrics
pub static REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    let registry = Registry::new();

    // Register all metrics
    registry.register(Box::new(HTTP_REQUESTS_TOTAL.clone())).unwrap();
    registry.register(Box::new(HTTP_REQUEST_DURATION.clone())).unwrap();
    registry.register(Box::new(ACTIVE_AGENTS.clone())).unwrap();
    registry.register(Box::new(AGENTS_SPAWNED_TOTAL.clone())).unwrap();
    registry.register(Box::new(AGENTS_BY_ROLE.clone())).unwrap();
    registry.register(Box::new(TASKS_TOTAL.clone())).unwrap();
    registry.register(Box::new(TASKS_IN_PROGRESS.clone())).unwrap();
    registry.register(Box::new(TASK_DURATION.clone())).unwrap();
    registry.register(Box::new(WEBSOCKET_CONNECTIONS.clone())).unwrap();
    registry.register(Box::new(WEBSOCKET_MESSAGES_TOTAL.clone())).unwrap();
    registry.register(Box::new(REDIS_OPERATIONS_TOTAL.clone())).unwrap();
    registry.register(Box::new(REDIS_OPERATION_DURATION.clone())).unwrap();
    registry.register(Box::new(REDIS_CONNECTED.clone())).unwrap();
    registry.register(Box::new(POSTGRES_QUERIES_TOTAL.clone())).unwrap();
    registry.register(Box::new(POSTGRES_QUERY_DURATION.clone())).unwrap();
    registry.register(Box::new(POSTGRES_CONNECTED.clone())).unwrap();
    registry.register(Box::new(TOKENS_INPUT_TOTAL.clone())).unwrap();
    registry.register(Box::new(TOKENS_OUTPUT_TOTAL.clone())).unwrap();
    registry.register(Box::new(TOKENS_COMPRESSED_TOTAL.clone())).unwrap();
    registry.register(Box::new(RL_EXPERIENCES_TOTAL.clone())).unwrap();
    registry.register(Box::new(RL_TRAINING_EPISODES.clone())).unwrap();
    registry.register(Box::new(MEMORY_PATTERNS_STORED.clone())).unwrap();
    registry.register(Box::new(EMBEDDINGS_GENERATED_TOTAL.clone())).unwrap();
    registry.register(Box::new(CODE_CHUNKS_INDEXED.clone())).unwrap();

    registry
});

// =============================================================================
// HTTP Metrics
// =============================================================================

/// Total HTTP requests by endpoint and status
pub static HTTP_REQUESTS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new("cca_http_requests_total", "Total number of HTTP requests")
            .namespace("cca")
            .subsystem("http"),
        &["endpoint", "method", "status"],
    )
    .unwrap()
});

/// HTTP request duration histogram
pub static HTTP_REQUEST_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        HistogramOpts::new("cca_http_request_duration_seconds", "HTTP request duration in seconds")
            .namespace("cca")
            .subsystem("http")
            .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
        &["endpoint", "method"],
    )
    .unwrap()
});

// =============================================================================
// Agent Metrics
// =============================================================================

/// Number of currently active agents
pub static ACTIVE_AGENTS: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cca_active_agents", "Number of currently active agents")
        .unwrap()
});

/// Total agents spawned
pub static AGENTS_SPAWNED_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new("cca_agents_spawned_total", "Total number of agents spawned")
            .namespace("cca")
            .subsystem("agent"),
        &["role"],
    )
    .unwrap()
});

/// Active agents by role
pub static AGENTS_BY_ROLE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    IntGaugeVec::new(
        Opts::new("cca_agents_by_role", "Number of active agents by role")
            .namespace("cca")
            .subsystem("agent"),
        &["role"],
    )
    .unwrap()
});

// =============================================================================
// Task Metrics
// =============================================================================

/// Total tasks by status and priority
pub static TASKS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new("cca_tasks_total", "Total number of tasks")
            .namespace("cca")
            .subsystem("task"),
        &["status", "priority"],
    )
    .unwrap()
});

/// Currently in-progress tasks
pub static TASKS_IN_PROGRESS: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cca_tasks_in_progress", "Number of tasks currently in progress")
        .unwrap()
});

/// Task duration histogram
pub static TASK_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        HistogramOpts::new("cca_task_duration_seconds", "Task execution duration in seconds")
            .namespace("cca")
            .subsystem("task")
            .buckets(vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0]),
        &["priority", "role"],
    )
    .unwrap()
});

// =============================================================================
// WebSocket / ACP Metrics
// =============================================================================

/// Active WebSocket connections
pub static WEBSOCKET_CONNECTIONS: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cca_websocket_connections", "Number of active WebSocket connections")
        .unwrap()
});

/// Total WebSocket messages by direction
pub static WEBSOCKET_MESSAGES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new("cca_websocket_messages_total", "Total WebSocket messages")
            .namespace("cca")
            .subsystem("websocket"),
        &["direction"], // "sent" or "received"
    )
    .unwrap()
});

// =============================================================================
// Redis Metrics
// =============================================================================

/// Total Redis operations by type
pub static REDIS_OPERATIONS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new("cca_redis_operations_total", "Total Redis operations")
            .namespace("cca")
            .subsystem("redis"),
        &["operation", "status"], // get, set, publish, subscribe; success, error
    )
    .unwrap()
});

/// Redis operation duration
pub static REDIS_OPERATION_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        HistogramOpts::new("cca_redis_operation_duration_seconds", "Redis operation duration")
            .namespace("cca")
            .subsystem("redis")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
        &["operation"],
    )
    .unwrap()
});

/// Redis connection status (1 = connected, 0 = disconnected)
pub static REDIS_CONNECTED: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cca_redis_connected", "Redis connection status (1=connected, 0=disconnected)")
        .unwrap()
});

// =============================================================================
// PostgreSQL Metrics
// =============================================================================

/// Total PostgreSQL queries by type
pub static POSTGRES_QUERIES_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    IntCounterVec::new(
        Opts::new("cca_postgres_queries_total", "Total PostgreSQL queries")
            .namespace("cca")
            .subsystem("postgres"),
        &["query_type", "status"], // select, insert, update; success, error
    )
    .unwrap()
});

/// PostgreSQL query duration
pub static POSTGRES_QUERY_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    HistogramVec::new(
        HistogramOpts::new("cca_postgres_query_duration_seconds", "PostgreSQL query duration")
            .namespace("cca")
            .subsystem("postgres")
            .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
        &["query_type"],
    )
    .unwrap()
});

/// PostgreSQL connection status
pub static POSTGRES_CONNECTED: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cca_postgres_connected", "PostgreSQL connection status (1=connected, 0=disconnected)")
        .unwrap()
});

// =============================================================================
// Token Metrics
// =============================================================================

/// Total input tokens processed
pub static TOKENS_INPUT_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new("cca_tokens_input_total", "Total input tokens processed")
        .unwrap()
});

/// Total output tokens generated
pub static TOKENS_OUTPUT_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new("cca_tokens_output_total", "Total output tokens generated")
        .unwrap()
});

/// Total tokens saved through compression
pub static TOKENS_COMPRESSED_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new("cca_tokens_compressed_total", "Total tokens saved through compression")
        .unwrap()
});

// =============================================================================
// RL Engine Metrics
// =============================================================================

/// Total RL experiences collected
pub static RL_EXPERIENCES_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new("cca_rl_experiences_total", "Total RL experiences collected")
        .unwrap()
});

/// Total RL training episodes
pub static RL_TRAINING_EPISODES: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new("cca_rl_training_episodes_total", "Total RL training episodes")
        .unwrap()
});

// =============================================================================
// Memory / ReasoningBank Metrics
// =============================================================================

/// Total patterns stored in ReasoningBank
pub static MEMORY_PATTERNS_STORED: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cca_memory_patterns_stored", "Total patterns stored in ReasoningBank")
        .unwrap()
});

// =============================================================================
// Indexing Metrics
// =============================================================================

/// Total embeddings generated
pub static EMBEDDINGS_GENERATED_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new("cca_embeddings_generated_total", "Total embeddings generated")
        .unwrap()
});

/// Total code chunks indexed
pub static CODE_CHUNKS_INDEXED: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new("cca_code_chunks_indexed", "Total code chunks in index")
        .unwrap()
});

// =============================================================================
// Utility Functions
// =============================================================================

/// Encode all metrics in Prometheus text format
pub fn encode_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    encoder.encode_to_string(&metric_families).unwrap_or_default()
}

/// Record HTTP request metrics
pub fn record_http_request(endpoint: &str, method: &str, status: u16, duration_secs: f64) {
    HTTP_REQUESTS_TOTAL
        .with_label_values(&[endpoint, method, &status.to_string()])
        .inc();
    HTTP_REQUEST_DURATION
        .with_label_values(&[endpoint, method])
        .observe(duration_secs);
}

/// Record agent spawn
pub fn record_agent_spawn(role: &str) {
    AGENTS_SPAWNED_TOTAL.with_label_values(&[role]).inc();
    AGENTS_BY_ROLE.with_label_values(&[role]).inc();
    ACTIVE_AGENTS.inc();
}

/// Record agent termination
pub fn record_agent_terminate(role: &str) {
    AGENTS_BY_ROLE.with_label_values(&[role]).dec();
    ACTIVE_AGENTS.dec();
}

/// Record task creation
pub fn record_task_created(priority: &str) {
    TASKS_TOTAL
        .with_label_values(&["created", priority])
        .inc();
    TASKS_IN_PROGRESS.inc();
}

/// Record task completion
pub fn record_task_completed(priority: &str, role: &str, duration_secs: f64) {
    TASKS_TOTAL
        .with_label_values(&["completed", priority])
        .inc();
    TASKS_IN_PROGRESS.dec();
    TASK_DURATION
        .with_label_values(&[priority, role])
        .observe(duration_secs);
}

/// Record task failure
pub fn record_task_failed(priority: &str) {
    TASKS_TOTAL.with_label_values(&["failed", priority]).inc();
    TASKS_IN_PROGRESS.dec();
}

/// Update connection status for Redis
pub fn set_redis_connected(connected: bool) {
    REDIS_CONNECTED.set(if connected { 1 } else { 0 });
}

/// Update connection status for PostgreSQL
pub fn set_postgres_connected(connected: bool) {
    POSTGRES_CONNECTED.set(if connected { 1 } else { 0 });
}

/// Record Redis operation
pub fn record_redis_operation(operation: &str, success: bool, duration_secs: f64) {
    let status = if success { "success" } else { "error" };
    REDIS_OPERATIONS_TOTAL
        .with_label_values(&[operation, status])
        .inc();
    REDIS_OPERATION_DURATION
        .with_label_values(&[operation])
        .observe(duration_secs);
}

/// Record PostgreSQL query
pub fn record_postgres_query(query_type: &str, success: bool, duration_secs: f64) {
    let status = if success { "success" } else { "error" };
    POSTGRES_QUERIES_TOTAL
        .with_label_values(&[query_type, status])
        .inc();
    POSTGRES_QUERY_DURATION
        .with_label_values(&[query_type])
        .observe(duration_secs);
}

/// Record token usage
pub fn record_token_usage(input: u64, output: u64) {
    TOKENS_INPUT_TOTAL.inc_by(input);
    TOKENS_OUTPUT_TOTAL.inc_by(output);
}

/// Record token compression savings
pub fn record_token_compression(saved: u64) {
    TOKENS_COMPRESSED_TOTAL.inc_by(saved);
}

/// Record WebSocket connection change
pub fn record_websocket_connection(connected: bool) {
    if connected {
        WEBSOCKET_CONNECTIONS.inc();
    } else {
        WEBSOCKET_CONNECTIONS.dec();
    }
}

/// Record WebSocket message
pub fn record_websocket_message(direction: &str) {
    WEBSOCKET_MESSAGES_TOTAL
        .with_label_values(&[direction])
        .inc();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_encode() {
        // Record some test metrics
        record_agent_spawn("backend");
        record_task_created("normal");
        set_redis_connected(true);
        set_postgres_connected(true);

        // Encode and verify output
        let output = encode_metrics();
        assert!(output.contains("cca_active_agents"));
        assert!(output.contains("cca_redis_connected"));
        assert!(output.contains("cca_postgres_connected"));
    }

    #[test]
    fn test_http_request_metrics() {
        record_http_request("/api/v1/health", "GET", 200, 0.015);

        let output = encode_metrics();
        assert!(output.contains("cca_http_requests_total"));
        assert!(output.contains("cca_http_request_duration_seconds"));
    }
}
