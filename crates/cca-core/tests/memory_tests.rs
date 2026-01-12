//! Integration tests for Memory types
//! Tests Pattern, ContextSnapshot, and related types

use cca_core::AgentId;
use cca_core::memory::{
    Pattern, PatternType, ContextSnapshot, SearchMatch, AgentContext, ContextMessage, MessageRole,
};
use chrono::Utc;
use uuid::Uuid;

#[test]
fn test_pattern_creation() {
    let pattern = Pattern::new(PatternType::Code, "Use async/await for concurrency");

    assert_eq!(pattern.pattern_type, PatternType::Code);
    assert_eq!(pattern.content, "Use async/await for concurrency");
    assert_eq!(pattern.success_count, 0);
    assert_eq!(pattern.failure_count, 0);
    assert!(pattern.agent_id.is_none());
    assert!(pattern.embedding.is_none());
}

#[test]
fn test_pattern_types() {
    let types = vec![
        PatternType::Code,
        PatternType::Routing,
        PatternType::ErrorHandling,
        PatternType::Communication,
        PatternType::Optimization,
        PatternType::Custom("special".to_string()),
    ];

    for pattern_type in types {
        let pattern = Pattern::new(pattern_type.clone(), "test");
        assert_eq!(pattern.pattern_type, pattern_type);
    }
}

#[test]
fn test_pattern_success_rate() {
    let mut pattern = Pattern::new(PatternType::Code, "test pattern");

    // 0/0 = 0.0
    assert_eq!(pattern.success_rate(), 0.0);

    // 1 success, 0 failures = 100%
    pattern.record_success();
    assert_eq!(pattern.success_rate(), 1.0);

    // 1 success, 1 failure = 50%
    pattern.record_failure();
    assert!((pattern.success_rate() - 0.5).abs() < 0.001);

    // 2 successes, 1 failure = 66.67%
    pattern.record_success();
    assert!((pattern.success_rate() - 0.6666).abs() < 0.01);
}

#[test]
fn test_pattern_updated_at_changes() {
    let mut pattern = Pattern::new(PatternType::Optimization, "cache results");

    let original_updated = pattern.updated_at;

    // Small delay to ensure timestamp changes
    std::thread::sleep(std::time::Duration::from_millis(10));

    pattern.record_success();
    assert!(pattern.updated_at > original_updated);
}

#[test]
fn test_pattern_serialization() {
    let mut pattern = Pattern::new(PatternType::ErrorHandling, "retry with backoff");
    pattern.record_success();
    pattern.record_success();
    pattern.record_failure();

    let json = serde_json::to_string(&pattern).unwrap();
    let parsed: Pattern = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.content, pattern.content);
    assert_eq!(parsed.success_count, pattern.success_count);
    assert_eq!(parsed.failure_count, pattern.failure_count);
}

#[test]
fn test_pattern_type_serialization_snake_case() {
    let pattern_type = PatternType::ErrorHandling;
    let json = serde_json::to_string(&pattern_type).unwrap();

    // Should be snake_case
    assert!(json.contains("error_handling"));
}

#[test]
fn test_context_snapshot_creation() {
    let agent_id = AgentId::new();
    let snapshot = ContextSnapshot {
        id: Uuid::new_v4(),
        agent_id,
        context_hash: "abc123def456".to_string(),
        compressed_context: vec![1, 2, 3, 4, 5],
        token_count: 1500,
        created_at: Utc::now(),
    };

    assert_eq!(snapshot.agent_id, agent_id);
    assert_eq!(snapshot.context_hash, "abc123def456");
    assert_eq!(snapshot.token_count, 1500);
    assert_eq!(snapshot.compressed_context.len(), 5);
}

#[test]
fn test_context_snapshot_serialization() {
    let snapshot = ContextSnapshot {
        id: Uuid::new_v4(),
        agent_id: AgentId::new(),
        context_hash: "hash123".to_string(),
        compressed_context: vec![10, 20, 30],
        token_count: 2000,
        created_at: Utc::now(),
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    let parsed: ContextSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.context_hash, snapshot.context_hash);
    assert_eq!(parsed.token_count, snapshot.token_count);
}

#[test]
fn test_search_match_creation() {
    let pattern = Pattern::new(PatternType::Code, "test pattern");
    let search_match = SearchMatch {
        pattern: pattern.clone(),
        score: 0.95,
    };

    assert!((search_match.score - 0.95).abs() < 0.001);
    assert_eq!(search_match.pattern.content, "test pattern");
}

#[test]
fn test_agent_context_creation() {
    let agent_id = AgentId::new();
    let ctx = AgentContext {
        agent_id,
        conversation_history: vec![
            ContextMessage {
                role: MessageRole::User,
                content: "Hello".to_string(),
                timestamp: Utc::now(),
            },
            ContextMessage {
                role: MessageRole::Assistant,
                content: "Hi there!".to_string(),
                timestamp: Utc::now(),
            },
        ],
        working_directory: "/home/user/project".to_string(),
        active_files: vec!["main.rs".to_string(), "lib.rs".to_string()],
        token_count: 5000,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    assert_eq!(ctx.agent_id, agent_id);
    assert_eq!(ctx.conversation_history.len(), 2);
    assert_eq!(ctx.active_files.len(), 2);
    assert_eq!(ctx.token_count, 5000);
}

#[test]
fn test_message_role_variants() {
    let roles = vec![
        MessageRole::System,
        MessageRole::User,
        MessageRole::Assistant,
    ];

    for role in roles {
        let json = serde_json::to_string(&role).unwrap();
        let parsed: MessageRole = serde_json::from_str(&json).unwrap();
        assert_eq!(role, parsed);
    }
}

#[test]
fn test_context_message_serialization() {
    let msg = ContextMessage {
        role: MessageRole::User,
        content: "Test message content".to_string(),
        timestamp: Utc::now(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ContextMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.role, MessageRole::User);
    assert_eq!(parsed.content, "Test message content");
}

#[test]
fn test_pattern_with_long_content() {
    let long_content = "x".repeat(100000);
    let pattern = Pattern::new(PatternType::Code, &long_content);
    assert_eq!(pattern.content.len(), 100000);
}

#[test]
fn test_pattern_clone() {
    let mut pattern = Pattern::new(PatternType::Routing, "original");
    pattern.record_success();

    let cloned = pattern.clone();

    assert_eq!(cloned.content, pattern.content);
    assert_eq!(cloned.success_count, pattern.success_count);
    assert_eq!(cloned.id, pattern.id);
}

#[test]
fn test_pattern_type_equality() {
    assert_eq!(PatternType::Code, PatternType::Code);
    assert_ne!(PatternType::Code, PatternType::Routing);

    assert_eq!(
        PatternType::Custom("test".to_string()),
        PatternType::Custom("test".to_string())
    );
    assert_ne!(
        PatternType::Custom("test1".to_string()),
        PatternType::Custom("test2".to_string())
    );
}

#[test]
fn test_context_snapshot_with_large_data() {
    let large_data = vec![0u8; 1000000]; // 1MB

    let snapshot = ContextSnapshot {
        id: Uuid::new_v4(),
        agent_id: AgentId::new(),
        context_hash: "large-hash".to_string(),
        compressed_context: large_data.clone(),
        token_count: 50000,
        created_at: Utc::now(),
    };

    assert_eq!(snapshot.compressed_context.len(), 1000000);
}
