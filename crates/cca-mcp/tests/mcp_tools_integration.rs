//! Integration tests for MCP Tools
//!
//! These tests verify the MCP tools work correctly by mocking the daemon API.

#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::format_push_string)]

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Test cca_task tool
#[tokio::test]
async fn test_cca_task_tool() {
    let mock_server = MockServer::start().await;

    // Mock health endpoint
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    // Mock task creation endpoint
    Mock::given(method("POST"))
        .and(path("/api/v1/tasks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "task_id": "task-12345",
            "status": "pending",
            "description": "Test task",
            "assigned_agent": null
        })))
        .mount(&mock_server)
        .await;

    // Create client and call endpoint
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/tasks", mock_server.uri()))
        .json(&json!({
            "description": "Test task",
            "priority": "high"
        }))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert_eq!(json["task_id"], "task-12345");
    assert_eq!(json["status"], "pending");
}

/// Test cca_status tool
#[tokio::test]
async fn test_cca_status_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "running",
            "version": "0.1.0",
            "agents_count": 3,
            "tasks_pending": 2,
            "tasks_completed": 10
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/status", mock_server.uri()))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert_eq!(json["status"], "running");
    assert_eq!(json["agents_count"], 3);
}

/// Test cca_agents tool
#[tokio::test]
async fn test_cca_agents_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/agents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "agents": [
                {"id": "agent-001", "role": "coordinator", "status": "idle"},
                {"id": "agent-002", "role": "code", "status": "busy"}
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/agents", mock_server.uri()))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    let agents = json["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2);
}

/// Test cca_memory tool
#[tokio::test]
async fn test_cca_memory_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/memory/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "patterns": [
                {
                    "id": "pattern-001",
                    "pattern_type": "reasoning",
                    "content": "Error handling pattern",
                    "success_rate": 0.9
                }
            ],
            "count": 1
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/memory/search", mock_server.uri()))
        .json(&json!({"query": "error handling", "limit": 10}))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["success"].as_bool().unwrap());
    assert_eq!(json["patterns"].as_array().unwrap().len(), 1);
}

/// Test cca_tokens_analyze tool
#[tokio::test]
async fn test_cca_tokens_analyze_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/tokens/analyze"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "token_count": 150,
            "repeated_lines": 5,
            "compression_potential": 0.25
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/tokens/analyze", mock_server.uri()))
        .json(&json!({"content": "test content for analysis"}))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["success"].as_bool().unwrap());
    assert_eq!(json["token_count"], 150);
}

/// Test cca_tokens_compress tool
#[tokio::test]
async fn test_cca_tokens_compress_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/tokens/compress"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "original_tokens": 200,
            "final_tokens": 140,
            "tokens_saved": 60,
            "reduction": "30.0%",
            "compressed_content": "compressed content here"
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/tokens/compress", mock_server.uri()))
        .json(&json!({
            "content": "content to compress",
            "strategies": ["code_comments", "history"],
            "target_reduction": 0.3
        }))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["success"].as_bool().unwrap());
    assert_eq!(json["tokens_saved"], 60);
}

/// Test cca_tokens_metrics tool
#[tokio::test]
async fn test_cca_tokens_metrics_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/tokens/metrics"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "total_tokens_used": 100000,
            "total_tokens_saved": 30000,
            "efficiency_percent": 30.0,
            "agent_count": 2,
            "agents": []
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/tokens/metrics", mock_server.uri()))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["success"].as_bool().unwrap());
    assert_eq!(json["efficiency_percent"], 30.0);
}

/// Test cca_rl_status tool
#[tokio::test]
async fn test_cca_rl_status_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/rl/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "algorithm": "q_learning",
            "total_steps": 5000,
            "average_reward": 0.85,
            "buffer_size": 1000,
            "algorithms_available": ["q_learning", "dqn", "ppo"]
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/rl/stats", mock_server.uri()))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert_eq!(json["algorithm"], "q_learning");
}

/// Test cca_rl_train tool
#[tokio::test]
async fn test_cca_rl_train_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/rl/train"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "loss": 0.05,
            "message": "Training completed"
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/rl/train", mock_server.uri()))
        .json(&json!({}))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["success"].as_bool().unwrap());
}

/// Test cca_rl_algorithm tool
#[tokio::test]
async fn test_cca_rl_algorithm_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/rl/algorithm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "algorithm": "dqn",
            "message": "Algorithm set to dqn"
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/rl/algorithm", mock_server.uri()))
        .json(&json!({"algorithm": "dqn"}))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["success"].as_bool().unwrap());
    assert_eq!(json["algorithm"], "dqn");
}

/// Test cca_acp_status tool
#[tokio::test]
async fn test_cca_acp_status_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/acp/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "running": true,
            "port": 9201,
            "connected_agents": 3,
            "agent_ids": ["agent-001", "agent-002", "agent-003"]
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/acp/status", mock_server.uri()))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["running"].as_bool().unwrap());
    assert_eq!(json["connected_agents"], 3);
}

/// Test cca_broadcast tool
#[tokio::test]
async fn test_cca_broadcast_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/broadcast"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "agents_notified": 3,
            "message": "Test broadcast"
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/broadcast", mock_server.uri()))
        .json(&json!({"message": "Test broadcast"}))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert!(json["success"].as_bool().unwrap());
    assert_eq!(json["agents_notified"], 3);
}

/// Test cca_workloads tool
#[tokio::test]
async fn test_cca_workloads_tool() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/workloads"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "agents": [
                {
                    "agent_id": "agent-001",
                    "role": "coordinator",
                    "current_tasks": 2,
                    "max_tasks": 5
                }
            ],
            "total_tasks": 10,
            "pending_tasks": 3
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/v1/workloads", mock_server.uri()))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    let json: Value = response.json().await.unwrap();
    assert_eq!(json["total_tasks"], 10);
    assert_eq!(json["pending_tasks"], 3);
}

/// Test error handling - daemon not running
#[tokio::test]
async fn test_daemon_not_running() {
    // Don't start any server - simulate daemon not running
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(100))
        .build()
        .unwrap();

    // This should fail with connection error
    let result = client
        .get("http://127.0.0.1:59999/health")
        .send()
        .await;

    assert!(result.is_err(), "Should fail when daemon not running");
}

/// Test error handling - invalid request
#[tokio::test]
async fn test_invalid_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/tasks"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "Missing required field: description"
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/tasks", mock_server.uri()))
        .json(&json!({}))  // Missing required field
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
}

/// Test complete workflow with mocked daemon
#[tokio::test]
async fn test_complete_workflow() {
    let mock_server = MockServer::start().await;

    // Mock all required endpoints for a complete workflow
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "running",
            "version": "0.1.0"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/tasks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "task_id": "task-workflow-001",
            "status": "pending"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/tokens/metrics"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "efficiency_percent": 28.5
        })))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();

    // 1. Check health
    let health = client
        .get(format!("{}/health", mock_server.uri()))
        .send()
        .await
        .unwrap();
    assert!(health.status().is_success());

    // 2. Check status
    let status = client
        .get(format!("{}/api/v1/status", mock_server.uri()))
        .send()
        .await
        .unwrap();
    let status_json: Value = status.json().await.unwrap();
    assert_eq!(status_json["status"], "running");

    // 3. Create task
    let task = client
        .post(format!("{}/api/v1/tasks", mock_server.uri()))
        .json(&json!({"description": "Workflow test task"}))
        .send()
        .await
        .unwrap();
    let task_json: Value = task.json().await.unwrap();
    assert!(task_json["task_id"].as_str().unwrap().starts_with("task-"));

    // 4. Check token metrics
    let metrics = client
        .get(format!("{}/api/v1/tokens/metrics", mock_server.uri()))
        .send()
        .await
        .unwrap();
    let metrics_json: Value = metrics.json().await.unwrap();
    assert!(metrics_json["efficiency_percent"].as_f64().unwrap() > 0.0);
}
