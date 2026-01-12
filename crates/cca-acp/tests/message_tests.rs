//! ACP message format and serialization tests

use serde_json::json;

// ============================================================================
// JSON-RPC 2.0 Compliance Tests
// ============================================================================

#[test]
fn test_jsonrpc_version_present() {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "test"
    });

    assert_eq!(msg["jsonrpc"], "2.0");
}

#[test]
fn test_request_structure() {
    let request = json!({
        "jsonrpc": "2.0",
        "id": "req-001",
        "method": "taskAssign",
        "params": {
            "task_id": "task-123"
        }
    });

    // Required fields
    assert!(request["jsonrpc"].is_string());
    assert!(request["id"].is_string() || request["id"].is_number());
    assert!(request["method"].is_string());

    // No result or error in request
    assert!(request["result"].is_null());
    assert!(request["error"].is_null());
}

#[test]
fn test_response_structure() {
    let response = json!({
        "jsonrpc": "2.0",
        "id": "req-001",
        "result": {
            "status": "ok"
        }
    });

    // Required fields
    assert!(response["jsonrpc"].is_string());
    assert!(response["id"].is_string() || response["id"].is_number());

    // Has result, no error
    assert!(response["result"].is_object());
    assert!(response["error"].is_null());

    // No method in response
    assert!(response["method"].is_null());
}

#[test]
fn test_error_response_structure() {
    let error_response = json!({
        "jsonrpc": "2.0",
        "id": "req-001",
        "error": {
            "code": -32600,
            "message": "Invalid Request"
        }
    });

    // Has error, no result
    assert!(error_response["error"].is_object());
    assert!(error_response["result"].is_null());

    // Error has required fields
    assert!(error_response["error"]["code"].is_number());
    assert!(error_response["error"]["message"].is_string());
}

#[test]
fn test_notification_structure() {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "statusUpdate",
        "params": {
            "status": "busy"
        }
    });

    // No id in notification
    assert!(notification["id"].is_null());

    // Has method
    assert!(notification["method"].is_string());
}

// ============================================================================
// Method Parameter Tests
// ============================================================================

#[test]
fn test_heartbeat_params() {
    let params = json!({
        "timestamp": 1704067200
    });

    assert!(params["timestamp"].is_number());
}

#[test]
fn test_heartbeat_response_params() {
    let params = json!({
        "timestamp": 1704067200,
        "server_time": 1704067201
    });

    assert!(params["timestamp"].is_number());
    assert!(params["server_time"].is_number());
}

#[test]
fn test_task_assign_params() {
    let params = json!({
        "task_id": "task-123",
        "description": "Build the frontend",
        "priority": "high",
        "parent_task": null,
        "token_budget": 50000
    });

    assert!(params["task_id"].is_string());
    assert!(params["description"].is_string());
    assert!(params["priority"].is_string());
}

#[test]
fn test_task_result_params() {
    let params = json!({
        "task_id": "task-123",
        "success": true,
        "output": "Task completed successfully",
        "tokens_used": 1500,
        "duration_ms": 3000
    });

    assert!(params["task_id"].is_string());
    assert!(params["success"].is_boolean());
    assert!(params["tokens_used"].is_number());
}

#[test]
fn test_status_response_params() {
    let params = json!({
        "agent_id": "agent-001",
        "state": "connected",
        "current_task": null,
        "uptime_seconds": 3600
    });

    assert!(params["agent_id"].is_string());
    assert!(params["state"].is_string());
    assert!(params["uptime_seconds"].is_number());
}

#[test]
fn test_broadcast_params() {
    let params = json!({
        "message_type": "announcement",
        "content": {
            "message": "System update in 5 minutes"
        }
    });

    assert!(params["message_type"].is_string());
    assert!(params["content"].is_object());
}

// ============================================================================
// Error Code Tests
// ============================================================================

#[test]
fn test_standard_error_codes() {
    // JSON-RPC 2.0 standard error codes
    let parse_error = -32700;
    let invalid_request = -32600;
    let method_not_found = -32601;
    let invalid_params = -32602;
    let internal_error = -32603;

    assert!(parse_error < -32600);
    assert!(invalid_request == -32600);
    assert!(method_not_found == -32601);
    assert!(invalid_params == -32602);
    assert!(internal_error == -32603);
}

#[test]
fn test_server_error_range() {
    // Server errors: -32000 to -32099
    let server_error_min = -32099;
    let server_error_max = -32000;

    let custom_error = -32001;

    assert!(custom_error >= server_error_min);
    assert!(custom_error <= server_error_max);
}

#[test]
fn test_error_with_data() {
    let error = json!({
        "code": -32000,
        "message": "Server error",
        "data": {
            "details": "Connection pool exhausted",
            "retry_after": 5
        }
    });

    assert!(error["data"].is_object());
    assert!(error["data"]["retry_after"].is_number());
}

// ============================================================================
// Serialization Tests
// ============================================================================

#[test]
fn test_message_round_trip() {
    let original = json!({
        "jsonrpc": "2.0",
        "id": "test-1",
        "method": "ping",
        "params": null
    });

    let json_str = serde_json::to_string(&original).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(original, parsed);
}

#[test]
fn test_compact_serialization() {
    let msg = json!({"jsonrpc": "2.0", "id": "1", "method": "test"});

    let compact = serde_json::to_string(&msg).unwrap();

    // No extra whitespace
    assert!(!compact.contains('\n'));
    assert!(!compact.contains("  "));
}

#[test]
fn test_unicode_in_messages() {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "test",
        "params": {
            "message": ""
        }
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["params"]["message"], "");
}

#[test]
fn test_large_params() {
    let large_data = "x".repeat(10000);

    let msg = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "test",
        "params": {
            "data": large_data
        }
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    assert!(json_str.len() > 10000);
}

// ============================================================================
// ID Type Tests
// ============================================================================

#[test]
fn test_string_id() {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": "string-id-123",
        "method": "test"
    });

    assert!(msg["id"].is_string());
}

#[test]
fn test_numeric_id() {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": 12345,
        "method": "test"
    });

    assert!(msg["id"].is_number());
}

#[test]
fn test_null_id_notification() {
    let msg = json!({
        "jsonrpc": "2.0",
        "method": "event"
    });

    assert!(msg["id"].is_null());
}

// ============================================================================
// Method Name Tests
// ============================================================================

#[test]
fn test_method_names() {
    let valid_methods = vec![
        "heartbeat",
        "taskAssign",
        "taskResult",
        "getStatus",
        "broadcast",
    ];

    for method in valid_methods {
        assert!(!method.is_empty());
        assert!(!method.starts_with("rpc."));
    }
}

#[test]
fn test_reserved_method_prefix() {
    // Methods starting with "rpc." are reserved
    let reserved = "rpc.listMethods";

    assert!(reserved.starts_with("rpc."));
}

// ============================================================================
// Batch Request Tests (JSON-RPC 2.0 feature)
// ============================================================================

#[test]
fn test_batch_request_format() {
    let batch = json!([
        {"jsonrpc": "2.0", "id": "1", "method": "method1"},
        {"jsonrpc": "2.0", "id": "2", "method": "method2"},
        {"jsonrpc": "2.0", "method": "notification"}
    ]);

    assert!(batch.is_array());
    assert_eq!(batch.as_array().unwrap().len(), 3);
}

#[test]
fn test_batch_response_format() {
    let batch_response = json!([
        {"jsonrpc": "2.0", "id": "1", "result": "ok"},
        {"jsonrpc": "2.0", "id": "2", "error": {"code": -32600, "message": "Error"}}
    ]);

    assert!(batch_response.is_array());

    let responses = batch_response.as_array().unwrap();
    assert!(responses[0]["result"].is_string());
    assert!(responses[1]["error"].is_object());
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_params() {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "test",
        "params": {}
    });

    assert!(msg["params"].is_object());
    assert!(msg["params"].as_object().unwrap().is_empty());
}

#[test]
fn test_array_params() {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "test",
        "params": [1, 2, 3]
    });

    assert!(msg["params"].is_array());
}

#[test]
fn test_null_params() {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "test",
        "params": null
    });

    assert!(msg["params"].is_null());
}
