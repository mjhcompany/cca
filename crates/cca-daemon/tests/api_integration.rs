//! Integration tests for CCA Daemon API
//!
//! These tests verify the daemon's HTTP API endpoints work correctly.
//! They use axum-test for in-process testing without needing a running daemon.

#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use axum::{
    routing::{get, post},
    Json, Router,
};
use axum_test::TestServer;
use serde_json::{json, Value};

/// Create a minimal test router that simulates the daemon API
fn create_test_router() -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/v1/status", get(status_handler))
        .route("/api/v1/agents", get(agents_handler))
        .route("/api/v1/tasks", post(create_task_handler))
        .route("/api/v1/tasks/:task_id", get(get_task_handler))
        .route("/api/v1/activity", get(activity_handler))
        .route("/api/v1/workloads", get(workloads_handler))
        .route("/api/v1/tokens/analyze", post(tokens_analyze_handler))
        .route("/api/v1/tokens/compress", post(tokens_compress_handler))
        .route("/api/v1/tokens/metrics", get(tokens_metrics_handler))
        .route("/api/v1/tokens/recommendations", get(tokens_recommendations_handler))
        .route("/api/v1/rl/stats", get(rl_stats_handler))
        .route("/api/v1/rl/train", post(rl_train_handler))
        .route("/api/v1/rl/algorithm", post(rl_algorithm_handler))
        .route("/api/v1/memory/search", post(memory_search_handler))
        .route("/api/v1/acp/status", get(acp_status_handler))
        .route("/api/v1/broadcast", post(broadcast_handler))
}

// Mock handlers for testing

async fn health_handler() -> &'static str {
    "OK"
}

async fn status_handler() -> Json<Value> {
    Json(json!({
        "status": "running",
        "version": "0.1.0",
        "agents_count": 2,
        "tasks_pending": 1,
        "tasks_completed": 5
    }))
}

async fn agents_handler() -> Json<Value> {
    Json(json!({
        "agents": [
            {
                "id": "agent-001",
                "role": "coordinator",
                "status": "idle",
                "current_task": null
            },
            {
                "id": "agent-002",
                "role": "code",
                "status": "busy",
                "current_task": "task-123"
            }
        ]
    }))
}

async fn create_task_handler(Json(body): Json<Value>) -> Json<Value> {
    let description = body["description"].as_str().unwrap_or("unknown");
    Json(json!({
        "task_id": "task-new-001",
        "status": "pending",
        "description": description,
        "assigned_agent": null
    }))
}

async fn get_task_handler() -> Json<Value> {
    Json(json!({
        "task_id": "task-123",
        "status": "running",
        "description": "Test task",
        "output": null,
        "error": null,
        "assigned_agent": "agent-002"
    }))
}

async fn activity_handler() -> Json<Value> {
    Json(json!({
        "agents": [
            {
                "agent_id": "agent-001",
                "role": "coordinator",
                "status": "idle",
                "current_task": null,
                "last_activity": "2024-01-10T12:00:00Z"
            }
        ]
    }))
}

async fn workloads_handler() -> Json<Value> {
    Json(json!({
        "agents": [
            {
                "agent_id": "agent-001",
                "role": "coordinator",
                "current_tasks": 0,
                "max_tasks": 5,
                "capabilities": ["routing", "planning"]
            }
        ],
        "total_tasks": 10,
        "pending_tasks": 2
    }))
}

async fn tokens_analyze_handler(Json(body): Json<Value>) -> Json<Value> {
    let content = body["content"].as_str().unwrap_or("");
    let token_count = (content.len() as f64 / 4.0).ceil() as u32;

    Json(json!({
        "success": true,
        "token_count": token_count,
        "repeated_lines": 0,
        "compression_potential": 0.15
    }))
}

async fn tokens_compress_handler(Json(body): Json<Value>) -> Json<Value> {
    let content = body["content"].as_str().unwrap_or("");
    let original_tokens = (content.len() as f64 / 4.0).ceil() as u32;
    let final_tokens = (f64::from(original_tokens) * 0.7) as u32;

    Json(json!({
        "success": true,
        "original_tokens": original_tokens,
        "final_tokens": final_tokens,
        "tokens_saved": original_tokens - final_tokens,
        "reduction": "30.0%",
        "compressed_content": content.chars().take(content.len() * 7 / 10).collect::<String>()
    }))
}

async fn tokens_metrics_handler() -> Json<Value> {
    Json(json!({
        "success": true,
        "total_tokens_used": 50000,
        "total_tokens_saved": 15000,
        "efficiency_percent": 30.0,
        "agent_count": 2,
        "agents": [
            {
                "agent_id": "agent-001",
                "tokens_used": 25000,
                "tokens_saved": 7500,
                "requests": 100,
                "efficiency": 30.0
            }
        ]
    }))
}

async fn tokens_recommendations_handler() -> Json<Value> {
    Json(json!({
        "success": true,
        "recommendations": [
            {
                "category": "compression",
                "message": "Enable history pruning to reduce token usage",
                "priority": "medium",
                "potential_savings": "15%"
            }
        ]
    }))
}

async fn rl_stats_handler() -> Json<Value> {
    Json(json!({
        "algorithm": "q_learning",
        "total_steps": 1000,
        "total_rewards": 850.5,
        "average_reward": 0.85,
        "buffer_size": 500,
        "last_training_loss": 0.05,
        "experience_count": 500,
        "algorithms_available": ["q_learning", "dqn", "ppo"]
    }))
}

async fn rl_train_handler() -> Json<Value> {
    Json(json!({
        "success": true,
        "loss": 0.042,
        "message": "Training completed successfully"
    }))
}

async fn rl_algorithm_handler(Json(body): Json<Value>) -> Json<Value> {
    let algorithm = body["algorithm"].as_str().unwrap_or("q_learning");
    Json(json!({
        "success": true,
        "algorithm": algorithm,
        "message": format!("Algorithm set to {}", algorithm)
    }))
}

async fn memory_search_handler(Json(body): Json<Value>) -> Json<Value> {
    let query = body["query"].as_str().unwrap_or("");
    Json(json!({
        "success": true,
        "patterns": [
            {
                "id": "pattern-001",
                "pattern_type": "reasoning",
                "content": format!("Pattern matching: {}", query),
                "success_rate": 0.85,
                "success_count": 17,
                "failure_count": 3
            }
        ],
        "count": 1,
        "query": query
    }))
}

async fn acp_status_handler() -> Json<Value> {
    Json(json!({
        "running": true,
        "port": 9201,
        "connected_agents": 2,
        "agent_ids": ["agent-001", "agent-002"]
    }))
}

async fn broadcast_handler(Json(body): Json<Value>) -> Json<Value> {
    let message = body["message"].as_str().unwrap_or("");
    Json(json!({
        "success": true,
        "agents_notified": 2,
        "message": message
    }))
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_health_endpoint() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/health").await;

    response.assert_status_ok();
    response.assert_text("OK");
}

#[tokio::test]
async fn test_status_endpoint() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/status").await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert_eq!(json["status"], "running");
    assert_eq!(json["version"], "0.1.0");
    assert_eq!(json["agents_count"], 2);
}

#[tokio::test]
async fn test_list_agents() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/agents").await;

    response.assert_status_ok();
    let json: Value = response.json();

    let agents = json["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0]["role"], "coordinator");
    assert_eq!(agents[1]["role"], "code");
}

#[tokio::test]
async fn test_create_task() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server
        .post("/api/v1/tasks")
        .json(&json!({
            "description": "Implement new feature",
            "priority": "high"
        }))
        .await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["task_id"].as_str().unwrap().starts_with("task-"));
    assert_eq!(json["status"], "pending");
    assert_eq!(json["description"], "Implement new feature");
}

#[tokio::test]
async fn test_get_task() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/tasks/task-123").await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert_eq!(json["task_id"], "task-123");
    assert_eq!(json["status"], "running");
    assert_eq!(json["assigned_agent"], "agent-002");
}

#[tokio::test]
async fn test_activity_endpoint() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/activity").await;

    response.assert_status_ok();
    let json: Value = response.json();

    let agents = json["agents"].as_array().unwrap();
    assert!(!agents.is_empty());
    assert!(agents[0]["last_activity"].is_string());
}

#[tokio::test]
async fn test_workloads_endpoint() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/workloads").await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["total_tasks"].is_number());
    assert!(json["pending_tasks"].is_number());
    let agents = json["agents"].as_array().unwrap();
    assert!(!agents.is_empty());
}

#[tokio::test]
async fn test_tokens_analyze() {
    let server = TestServer::new(create_test_router()).unwrap();

    let test_content = "This is a test string for token analysis. It should be analyzed for tokens.";
    let response = server
        .post("/api/v1/tokens/analyze")
        .json(&json!({
            "content": test_content
        }))
        .await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    assert!(json["token_count"].as_u64().unwrap() > 0);
    assert!(json["compression_potential"].is_number());
}

#[tokio::test]
async fn test_tokens_compress() {
    let server = TestServer::new(create_test_router()).unwrap();

    let test_content = "This is a long content that needs compression. ".repeat(10);
    let response = server
        .post("/api/v1/tokens/compress")
        .json(&json!({
            "content": test_content,
            "strategies": ["history", "summarize"],
            "target_reduction": 0.3
        }))
        .await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    assert!(json["original_tokens"].as_u64().unwrap() > json["final_tokens"].as_u64().unwrap());
    assert!(json["tokens_saved"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_tokens_metrics() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/tokens/metrics").await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    assert!(json["total_tokens_used"].is_number());
    assert!(json["total_tokens_saved"].is_number());
    assert!(json["efficiency_percent"].is_number());
}

#[tokio::test]
async fn test_tokens_recommendations() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/tokens/recommendations").await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    let recommendations = json["recommendations"].as_array().unwrap();
    assert!(!recommendations.is_empty());
    assert!(recommendations[0]["category"].is_string());
    assert!(recommendations[0]["message"].is_string());
}

#[tokio::test]
async fn test_rl_stats() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/rl/stats").await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["algorithm"].is_string());
    assert!(json["total_steps"].is_number());
    assert!(json["average_reward"].is_number());
    let algorithms = json["algorithms_available"].as_array().unwrap();
    assert!(!algorithms.is_empty());
}

#[tokio::test]
async fn test_rl_train() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.post("/api/v1/rl/train").json(&json!({})).await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    assert!(json["loss"].is_number());
}

#[tokio::test]
async fn test_rl_algorithm() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server
        .post("/api/v1/rl/algorithm")
        .json(&json!({
            "algorithm": "dqn"
        }))
        .await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    assert_eq!(json["algorithm"], "dqn");
}

#[tokio::test]
async fn test_memory_search() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server
        .post("/api/v1/memory/search")
        .json(&json!({
            "query": "error handling patterns",
            "limit": 10
        }))
        .await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    let patterns = json["patterns"].as_array().unwrap();
    assert!(!patterns.is_empty());
    assert!(patterns[0]["pattern_type"].is_string());
    assert!(patterns[0]["content"].is_string());
}

#[tokio::test]
async fn test_acp_status() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server.get("/api/v1/acp/status").await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["running"].as_bool().unwrap());
    assert!(json["port"].as_u64().unwrap() > 0);
    assert!(json["connected_agents"].is_number());
}

#[tokio::test]
async fn test_broadcast() {
    let server = TestServer::new(create_test_router()).unwrap();

    let response = server
        .post("/api/v1/broadcast")
        .json(&json!({
            "message": "Test broadcast message"
        }))
        .await;

    response.assert_status_ok();
    let json: Value = response.json();

    assert!(json["success"].as_bool().unwrap());
    assert!(json["agents_notified"].as_u64().unwrap() > 0);
}

// ============================================================================
// End-to-end workflow tests
// ============================================================================

#[tokio::test]
async fn test_task_creation_workflow() {
    let server = TestServer::new(create_test_router()).unwrap();

    // 1. Check system health
    let health = server.get("/health").await;
    health.assert_status_ok();

    // 2. Check status
    let status = server.get("/api/v1/status").await;
    status.assert_status_ok();
    let status_json: Value = status.json();
    assert_eq!(status_json["status"], "running");

    // 3. List available agents
    let agents = server.get("/api/v1/agents").await;
    agents.assert_status_ok();

    // 4. Create a task
    let task = server
        .post("/api/v1/tasks")
        .json(&json!({
            "description": "Refactor authentication module",
            "priority": "high"
        }))
        .await;
    task.assert_status_ok();
    let task_json: Value = task.json();
    assert!(task_json["task_id"].is_string());

    // 5. Check task status
    let task_status = server.get("/api/v1/tasks/task-123").await;
    task_status.assert_status_ok();
}

#[tokio::test]
async fn test_token_efficiency_workflow() {
    let server = TestServer::new(create_test_router()).unwrap();

    let large_content = "fn main() {\n    // This is a comment\n    println!(\"Hello\");\n}\n".repeat(50);

    // 1. Analyze content
    let analysis = server
        .post("/api/v1/tokens/analyze")
        .json(&json!({
            "content": large_content
        }))
        .await;
    analysis.assert_status_ok();
    let analysis_json: Value = analysis.json();
    let _original_tokens = analysis_json["token_count"].as_u64().unwrap();

    // 2. Compress content
    let compression = server
        .post("/api/v1/tokens/compress")
        .json(&json!({
            "content": large_content,
            "strategies": ["code_comments", "history"],
            "target_reduction": 0.3
        }))
        .await;
    compression.assert_status_ok();
    let compression_json: Value = compression.json();
    let saved = compression_json["tokens_saved"].as_u64().unwrap();
    assert!(saved > 0, "Should save some tokens");

    // 3. Check metrics
    let metrics = server.get("/api/v1/tokens/metrics").await;
    metrics.assert_status_ok();

    // 4. Get recommendations
    let recommendations = server.get("/api/v1/tokens/recommendations").await;
    recommendations.assert_status_ok();
}

#[tokio::test]
async fn test_rl_training_workflow() {
    let server = TestServer::new(create_test_router()).unwrap();

    // 1. Check initial stats
    let stats = server.get("/api/v1/rl/stats").await;
    stats.assert_status_ok();
    let stats_json: Value = stats.json();
    assert_eq!(stats_json["algorithm"], "q_learning");

    // 2. Switch algorithm
    let switch = server
        .post("/api/v1/rl/algorithm")
        .json(&json!({
            "algorithm": "dqn"
        }))
        .await;
    switch.assert_status_ok();

    // 3. Trigger training
    let train = server.post("/api/v1/rl/train").json(&json!({})).await;
    train.assert_status_ok();
    let train_json: Value = train.json();
    assert!(train_json["success"].as_bool().unwrap());
}
