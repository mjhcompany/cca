//! Inter-Agent Communication Benchmarks
//!
//! Benchmarks for agent-to-agent messaging performance:
//! - Message creation and serialization
//! - ACP (Agent Client Protocol) message handling
//! - JSON-RPC message creation
//! - Message routing overhead
//!
//! ## Hot Paths Identified
//! 1. AcpMessage::request/response/notification - High-frequency message creation
//! 2. InterAgentMessage serialization/deserialization - Every message exchange
//! 3. Message target matching - Routing decision for each message
//!
//! ## Performance Targets
//! - Message creation: < 1µs
//! - JSON serialization: < 10µs for typical messages
//! - JSON deserialization: < 10µs for typical messages
//! - Message routing decision: < 100ns

#![allow(dead_code)]
#![allow(unused_imports)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;

// ============================================================================
// Local copies of communication types for benchmarking
// ============================================================================

mod comm_bench {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    pub type AgentId = uuid::Uuid;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum MessageType {
        TaskAssign,
        TaskResult,
        StatusUpdate,
        Broadcast,
        Query,
        QueryResponse,
        Heartbeat,
        Error,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct InterAgentMessage {
        pub id: uuid::Uuid,
        pub from: AgentId,
        pub to: MessageTarget,
        pub msg_type: MessageType,
        pub payload: serde_json::Value,
        pub timestamp: i64,
        pub correlation_id: Option<uuid::Uuid>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum MessageTarget {
        Agent(AgentId),
        Broadcast,
        Coordinator,
    }

    impl InterAgentMessage {
        pub fn new(
            from: AgentId,
            to: MessageTarget,
            msg_type: MessageType,
            payload: serde_json::Value,
        ) -> Self {
            Self {
                id: uuid::Uuid::new_v4(),
                from,
                to,
                msg_type,
                payload,
                timestamp: chrono::Utc::now().timestamp(),
                correlation_id: None,
            }
        }

        pub fn with_correlation(mut self, correlation_id: uuid::Uuid) -> Self {
            self.correlation_id = Some(correlation_id);
            self
        }

        pub fn reply(&self, payload: serde_json::Value, msg_type: MessageType) -> Self {
            Self {
                id: uuid::Uuid::new_v4(),
                from: match &self.to {
                    MessageTarget::Agent(id) => *id,
                    _ => self.from,
                },
                to: MessageTarget::Agent(self.from),
                msg_type,
                payload,
                timestamp: chrono::Utc::now().timestamp(),
                correlation_id: Some(self.id),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AcpMessage {
        pub jsonrpc: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub method: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub params: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub result: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub error: Option<AcpError>,
    }

    impl AcpMessage {
        pub fn request(
            id: impl Into<String>,
            method: impl Into<String>,
            params: serde_json::Value,
        ) -> Self {
            Self {
                jsonrpc: "2.0".to_string(),
                id: Some(id.into()),
                method: Some(method.into()),
                params: Some(params),
                result: None,
                error: None,
            }
        }

        pub fn notification(method: impl Into<String>, params: serde_json::Value) -> Self {
            Self {
                jsonrpc: "2.0".to_string(),
                id: None,
                method: Some(method.into()),
                params: Some(params),
                result: None,
                error: None,
            }
        }

        pub fn response(id: impl Into<String>, result: serde_json::Value) -> Self {
            Self {
                jsonrpc: "2.0".to_string(),
                id: Some(id.into()),
                method: None,
                params: None,
                result: Some(result),
                error: None,
            }
        }

        pub fn error_response(id: impl Into<String>, error: AcpError) -> Self {
            Self {
                jsonrpc: "2.0".to_string(),
                id: Some(id.into()),
                method: None,
                params: None,
                result: None,
                error: Some(error),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AcpError {
        pub code: i32,
        pub message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub data: Option<serde_json::Value>,
    }

    impl AcpError {
        pub fn parse_error() -> Self {
            Self {
                code: -32700,
                message: "Parse error".to_string(),
                data: None,
            }
        }

        pub fn invalid_request() -> Self {
            Self {
                code: -32600,
                message: "Invalid Request".to_string(),
                data: None,
            }
        }

        pub fn method_not_found() -> Self {
            Self {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }
        }

        pub fn invalid_params(message: impl Into<String>) -> Self {
            Self {
                code: -32602,
                message: message.into(),
                data: None,
            }
        }

        pub fn internal_error(message: impl Into<String>) -> Self {
            Self {
                code: -32603,
                message: message.into(),
                data: None,
            }
        }
    }

    // Redis channel helpers
    pub mod channels {
        pub const BROADCAST: &str = "cca:broadcast";
        pub const COORDINATION: &str = "cca:coord";
        pub const STATUS: &str = "cca:status";
        pub const LEARNING: &str = "cca:learning";

        pub fn agent_tasks(agent_id: &str) -> String {
            format!("cca:tasks:{agent_id}")
        }

        pub fn agent_status(agent_id: &str) -> String {
            format!("cca:status:{agent_id}")
        }
    }
}

use comm_bench::*;

// ============================================================================
// Test Data Generators
// ============================================================================

fn create_task_payload() -> serde_json::Value {
    serde_json::json!({
        "task_id": uuid::Uuid::new_v4().to_string(),
        "description": "Implement feature X with comprehensive testing",
        "priority": "high",
        "deadline": "2024-12-31",
        "requirements": ["rust", "async", "testing"],
        "context": {
            "project": "cca",
            "branch": "feature/benchmark",
            "files": ["src/main.rs", "src/lib.rs", "tests/integration.rs"]
        }
    })
}

fn create_status_payload() -> serde_json::Value {
    serde_json::json!({
        "agent_id": uuid::Uuid::new_v4().to_string(),
        "status": "active",
        "current_task": "Processing task XYZ",
        "progress": 0.75,
        "cpu_usage": 45.2,
        "memory_usage": 1024,
        "uptime_seconds": 3600
    })
}

fn create_result_payload() -> serde_json::Value {
    serde_json::json!({
        "task_id": uuid::Uuid::new_v4().to_string(),
        "success": true,
        "duration_ms": 1250,
        "output": {
            "files_modified": ["src/feature.rs", "tests/feature_test.rs"],
            "lines_added": 150,
            "lines_removed": 25,
            "tests_passed": 12,
            "tests_failed": 0
        },
        "metrics": {
            "tokens_used": 5000,
            "api_calls": 3
        }
    })
}

fn create_large_payload() -> serde_json::Value {
    let items: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("Item {}", i),
                "description": "This is a description that adds some bulk to the payload",
                "value": i * 10,
                "tags": ["tag1", "tag2", "tag3"]
            })
        })
        .collect();

    serde_json::json!({
        "items": items,
        "total": 100,
        "page": 1
    })
}

// ============================================================================
// InterAgentMessage Benchmarks
// ============================================================================

fn bench_inter_agent_message_creation(c: &mut Criterion) {
    let from = uuid::Uuid::new_v4();
    let to = MessageTarget::Agent(uuid::Uuid::new_v4());
    let payload = create_task_payload();

    c.bench_function("inter_agent_message/create", |b| {
        b.iter(|| {
            InterAgentMessage::new(
                black_box(from),
                black_box(to.clone()),
                MessageType::TaskAssign,
                black_box(payload.clone()),
            )
        })
    });
}

fn bench_inter_agent_message_with_correlation(c: &mut Criterion) {
    let from = uuid::Uuid::new_v4();
    let to = MessageTarget::Agent(uuid::Uuid::new_v4());
    let payload = create_task_payload();
    let correlation_id = uuid::Uuid::new_v4();

    c.bench_function("inter_agent_message/create_with_correlation", |b| {
        b.iter(|| {
            InterAgentMessage::new(
                black_box(from),
                black_box(to.clone()),
                MessageType::TaskAssign,
                black_box(payload.clone()),
            )
            .with_correlation(correlation_id)
        })
    });
}

fn bench_inter_agent_message_reply(c: &mut Criterion) {
    let from = uuid::Uuid::new_v4();
    let to = MessageTarget::Agent(uuid::Uuid::new_v4());
    let original = InterAgentMessage::new(from, to, MessageType::Query, create_task_payload());
    let reply_payload = create_result_payload();

    c.bench_function("inter_agent_message/reply", |b| {
        b.iter(|| original.reply(black_box(reply_payload.clone()), MessageType::QueryResponse))
    });
}

fn bench_inter_agent_message_serialization(c: &mut Criterion) {
    let payloads = vec![
        ("small", create_status_payload()),
        ("medium", create_task_payload()),
        ("large", create_large_payload()),
    ];

    let mut group = c.benchmark_group("inter_agent_message/serialize");
    for (name, payload) in payloads {
        let msg = InterAgentMessage::new(
            uuid::Uuid::new_v4(),
            MessageTarget::Broadcast,
            MessageType::StatusUpdate,
            payload,
        );

        group.bench_with_input(BenchmarkId::from_parameter(name), &msg, |b, msg| {
            b.iter(|| serde_json::to_string(black_box(msg)))
        });
    }
    group.finish();
}

fn bench_inter_agent_message_deserialization(c: &mut Criterion) {
    let payloads = vec![
        ("small", create_status_payload()),
        ("medium", create_task_payload()),
        ("large", create_large_payload()),
    ];

    let mut group = c.benchmark_group("inter_agent_message/deserialize");
    for (name, payload) in payloads {
        let msg = InterAgentMessage::new(
            uuid::Uuid::new_v4(),
            MessageTarget::Broadcast,
            MessageType::StatusUpdate,
            payload,
        );
        let json = serde_json::to_string(&msg).unwrap();

        group.throughput(Throughput::Bytes(json.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &json, |b, json| {
            b.iter(|| serde_json::from_str::<InterAgentMessage>(black_box(json)))
        });
    }
    group.finish();
}

// ============================================================================
// ACP Message Benchmarks
// ============================================================================

fn bench_acp_request_creation(c: &mut Criterion) {
    let params = create_task_payload();

    c.bench_function("acp_message/request", |b| {
        b.iter(|| {
            AcpMessage::request(
                black_box("req-123"),
                black_box("task.assign"),
                black_box(params.clone()),
            )
        })
    });
}

fn bench_acp_notification_creation(c: &mut Criterion) {
    let params = create_status_payload();

    c.bench_function("acp_message/notification", |b| {
        b.iter(|| {
            AcpMessage::notification(black_box("status.update"), black_box(params.clone()))
        })
    });
}

fn bench_acp_response_creation(c: &mut Criterion) {
    let result = create_result_payload();

    c.bench_function("acp_message/response", |b| {
        b.iter(|| AcpMessage::response(black_box("req-123"), black_box(result.clone())))
    });
}

fn bench_acp_error_response_creation(c: &mut Criterion) {
    let errors = vec![
        ("parse_error", AcpError::parse_error()),
        ("invalid_request", AcpError::invalid_request()),
        ("method_not_found", AcpError::method_not_found()),
        (
            "invalid_params",
            AcpError::invalid_params("Missing required field"),
        ),
        (
            "internal_error",
            AcpError::internal_error("Database connection failed"),
        ),
    ];

    let mut group = c.benchmark_group("acp_message/error_response");
    for (name, error) in errors {
        group.bench_with_input(BenchmarkId::from_parameter(name), &error, |b, error| {
            b.iter(|| AcpMessage::error_response(black_box("req-123"), black_box(error.clone())))
        });
    }
    group.finish();
}

fn bench_acp_message_serialization(c: &mut Criterion) {
    let messages = vec![
        (
            "request",
            AcpMessage::request("1", "method.call", create_task_payload()),
        ),
        (
            "notification",
            AcpMessage::notification("event.notify", create_status_payload()),
        ),
        (
            "response",
            AcpMessage::response("1", create_result_payload()),
        ),
        (
            "error",
            AcpMessage::error_response("1", AcpError::internal_error("Error")),
        ),
    ];

    let mut group = c.benchmark_group("acp_message/serialize");
    for (name, msg) in messages {
        group.bench_with_input(BenchmarkId::from_parameter(name), &msg, |b, msg| {
            b.iter(|| serde_json::to_string(black_box(msg)))
        });
    }
    group.finish();
}

fn bench_acp_message_deserialization(c: &mut Criterion) {
    let messages = vec![
        (
            "request",
            AcpMessage::request("1", "method.call", create_task_payload()),
        ),
        (
            "notification",
            AcpMessage::notification("event.notify", create_status_payload()),
        ),
        (
            "response",
            AcpMessage::response("1", create_result_payload()),
        ),
        (
            "error",
            AcpMessage::error_response("1", AcpError::internal_error("Error")),
        ),
    ];

    let mut group = c.benchmark_group("acp_message/deserialize");
    for (name, msg) in messages {
        let json = serde_json::to_string(&msg).unwrap();
        group.throughput(Throughput::Bytes(json.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &json, |b, json| {
            b.iter(|| serde_json::from_str::<AcpMessage>(black_box(json)))
        });
    }
    group.finish();
}

// ============================================================================
// Channel Name Generation Benchmarks
// ============================================================================

fn bench_channel_generation(c: &mut Criterion) {
    let agent_ids: Vec<String> = (0..10)
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect();

    let mut group = c.benchmark_group("channels/generation");

    group.bench_function("agent_tasks", |b| {
        let id = &agent_ids[0];
        b.iter(|| channels::agent_tasks(black_box(id)))
    });

    group.bench_function("agent_status", |b| {
        let id = &agent_ids[0];
        b.iter(|| channels::agent_status(black_box(id)))
    });

    group.bench_function("batch_10_channels", |b| {
        b.iter(|| {
            for id in &agent_ids {
                let _ = channels::agent_tasks(black_box(id));
                let _ = channels::agent_status(black_box(id));
            }
        })
    });

    group.finish();
}

// ============================================================================
// Message Routing Decision Benchmarks
// ============================================================================

fn bench_message_target_matching(c: &mut Criterion) {
    let targets = vec![
        MessageTarget::Agent(uuid::Uuid::new_v4()),
        MessageTarget::Broadcast,
        MessageTarget::Coordinator,
    ];

    c.bench_function("message_target/matching", |b| {
        b.iter(|| {
            for target in &targets {
                match target {
                    MessageTarget::Agent(id) => {
                        let _ = black_box(id);
                    }
                    MessageTarget::Broadcast => {
                        let _ = black_box("broadcast");
                    }
                    MessageTarget::Coordinator => {
                        let _ = black_box("coordinator");
                    }
                }
            }
        })
    });
}

// ============================================================================
// Throughput Benchmarks
// ============================================================================

fn bench_message_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_throughput");
    group.throughput(Throughput::Elements(1000));

    group.bench_function("1000_inter_agent_messages", |b| {
        let from = uuid::Uuid::new_v4();
        let to = MessageTarget::Broadcast;
        let payload = create_status_payload();

        b.iter(|| {
            for _ in 0..1000 {
                let msg = InterAgentMessage::new(
                    from,
                    to.clone(),
                    MessageType::StatusUpdate,
                    payload.clone(),
                );
                let _ = serde_json::to_string(&msg);
            }
        })
    });

    group.bench_function("1000_acp_notifications", |b| {
        let params = create_status_payload();

        b.iter(|| {
            for i in 0..1000 {
                let msg = AcpMessage::notification(format!("event.{i}"), params.clone());
                let _ = serde_json::to_string(&msg);
            }
        })
    });

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    name = inter_agent_message_benchmarks;
    config = Criterion::default();
    targets =
        bench_inter_agent_message_creation,
        bench_inter_agent_message_with_correlation,
        bench_inter_agent_message_reply,
        bench_inter_agent_message_serialization,
        bench_inter_agent_message_deserialization,
);

criterion_group!(
    name = acp_message_benchmarks;
    config = Criterion::default();
    targets =
        bench_acp_request_creation,
        bench_acp_notification_creation,
        bench_acp_response_creation,
        bench_acp_error_response_creation,
        bench_acp_message_serialization,
        bench_acp_message_deserialization,
);

criterion_group!(
    name = routing_benchmarks;
    config = Criterion::default();
    targets =
        bench_channel_generation,
        bench_message_target_matching,
);

criterion_group!(
    name = throughput_benchmarks;
    config = Criterion::default();
    targets =
        bench_message_throughput,
);

criterion_main!(
    inter_agent_message_benchmarks,
    acp_message_benchmarks,
    routing_benchmarks,
    throughput_benchmarks
);
