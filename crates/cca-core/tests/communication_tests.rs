//! Integration tests for Communication types
//! Tests ACP messages, inter-agent messaging, and channel utilities

use cca_core::communication::{
    AcpError, AcpMessage, InterAgentMessage, MessageTarget, MessageType, channels,
};
use cca_core::AgentId;
use serde_json::json;
use uuid::Uuid;

#[test]
fn test_acp_message_request_structure() {
    let msg = AcpMessage::request("req-123", "test.method", json!({"param": "value"}));

    assert_eq!(msg.jsonrpc, "2.0");
    assert_eq!(msg.id, Some("req-123".to_string()));
    assert_eq!(msg.method, Some("test.method".to_string()));
    assert!(msg.params.is_some());
    assert!(msg.result.is_none());
    assert!(msg.error.is_none());
}

#[test]
fn test_acp_message_response_structure() {
    let msg = AcpMessage::response("req-123", json!({"status": "ok"}));

    assert_eq!(msg.jsonrpc, "2.0");
    assert_eq!(msg.id, Some("req-123".to_string()));
    assert!(msg.method.is_none());
    assert!(msg.result.is_some());
    assert!(msg.error.is_none());
}

#[test]
fn test_acp_message_notification_no_id() {
    let msg = AcpMessage::notification("notify.event", json!({"data": "test"}));

    assert_eq!(msg.jsonrpc, "2.0");
    assert!(msg.id.is_none());
    assert_eq!(msg.method, Some("notify.event".to_string()));
    assert!(msg.params.is_some());
}

#[test]
fn test_acp_message_error_response() {
    let error = AcpError::method_not_found();
    let msg = AcpMessage::error_response("req-456", error);

    assert_eq!(msg.id, Some("req-456".to_string()));
    assert!(msg.error.is_some());
    assert!(msg.result.is_none());
}

#[test]
fn test_acp_error_codes() {
    assert_eq!(AcpError::parse_error().code, -32700);
    assert_eq!(AcpError::invalid_request().code, -32600);
    assert_eq!(AcpError::method_not_found().code, -32601);
    assert_eq!(AcpError::invalid_params("test").code, -32602);
    assert_eq!(AcpError::internal_error("test").code, -32603);
}

#[test]
fn test_acp_error_with_custom_message() {
    let error = AcpError::invalid_params("Missing required field: task_id");
    assert_eq!(error.code, -32602);
    assert_eq!(error.message, "Missing required field: task_id");
}

#[test]
fn test_acp_message_json_roundtrip() {
    let msg = AcpMessage::request("1", "method", json!({"key": "value"}));

    let json = serde_json::to_string(&msg).unwrap();
    let parsed: AcpMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.jsonrpc, "2.0");
    assert_eq!(parsed.method, Some("method".to_string()));
}

#[test]
fn test_inter_agent_message_creation() {
    let from = AgentId::new();
    let to = AgentId::new();

    let msg = InterAgentMessage::new(
        from,
        MessageTarget::Agent(to),
        MessageType::TaskAssign,
        json!({"task": "test"}),
    );

    assert_eq!(msg.from, from);
    assert!(matches!(msg.to, MessageTarget::Agent(_)));
    assert!(matches!(msg.msg_type, MessageType::TaskAssign));
}

#[test]
fn test_inter_agent_message_reply() {
    let from = AgentId::new();
    let to = AgentId::new();

    let original = InterAgentMessage::new(
        from,
        MessageTarget::Agent(to),
        MessageType::Query,
        json!({"question": "test"}),
    );

    let reply = original.reply(json!({"answer": "response"}), MessageType::QueryResponse);

    // Reply should be addressed to the original sender
    assert!(matches!(reply.to, MessageTarget::Agent(id) if id == from));
    // Reply should have correlation_id set
    assert_eq!(reply.correlation_id, Some(original.id));
}

#[test]
fn test_message_target_variants() {
    let agent_id = AgentId::new();

    let targets = vec![
        MessageTarget::Agent(agent_id),
        MessageTarget::Broadcast,
        MessageTarget::Coordinator,
    ];

    for target in targets {
        let json = serde_json::to_string(&target).unwrap();
        let _parsed: MessageTarget = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn test_message_type_all_variants() {
    let types = vec![
        MessageType::TaskAssign,
        MessageType::TaskResult,
        MessageType::StatusUpdate,
        MessageType::Broadcast,
        MessageType::Query,
        MessageType::QueryResponse,
        MessageType::Heartbeat,
        MessageType::Error,
    ];

    for msg_type in types {
        let json = serde_json::to_string(&msg_type).unwrap();
        let parsed: MessageType = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{:?}", msg_type), format!("{:?}", parsed));
    }
}

#[test]
fn test_channel_names() {
    assert_eq!(channels::BROADCAST, "cca:broadcast");
    assert_eq!(channels::COORDINATION, "cca:coord");
    assert_eq!(channels::STATUS, "cca:status");
    assert_eq!(channels::LEARNING, "cca:learning");
}

#[test]
fn test_agent_channel_name_generation() {
    let agent_id = "test-agent-123";

    let task_channel = channels::agent_tasks(agent_id);
    let status_channel = channels::agent_status(agent_id);

    assert_eq!(task_channel, "cca:tasks:test-agent-123");
    assert_eq!(status_channel, "cca:status:test-agent-123");
}

#[test]
fn test_acp_error_serialization() {
    let error = AcpError {
        code: -32000,
        message: "Custom error".to_string(),
        data: Some(json!({"details": "more info"})),
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: AcpError = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.code, error.code);
    assert_eq!(parsed.message, error.message);
    assert!(parsed.data.is_some());
}

#[test]
fn test_acp_message_with_array_params() {
    let msg = AcpMessage::request("1", "method", json!([1, 2, 3]));
    assert!(msg.params.unwrap().is_array());
}

#[test]
fn test_inter_agent_message_with_correlation() {
    let from = AgentId::new();
    let correlation_id = Uuid::new_v4();

    let msg = InterAgentMessage::new(
        from,
        MessageTarget::Broadcast,
        MessageType::StatusUpdate,
        json!({"status": "ready"}),
    ).with_correlation(correlation_id);

    assert_eq!(msg.correlation_id, Some(correlation_id));
}

#[test]
fn test_message_type_serialization_snake_case() {
    let msg_type = MessageType::TaskAssign;
    let json = serde_json::to_string(&msg_type).unwrap();

    // Should be snake_case
    assert!(json.contains("task_assign"));
}
