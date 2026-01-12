//! Integration tests for Agent types
//! Complements the inline unit tests in src/agent.rs

use cca_core::{Agent, AgentId, AgentRole, AgentState};
use std::collections::{HashMap, HashSet};

#[test]
fn test_agent_id_uniqueness_stress() {
    // Generate many IDs and verify uniqueness
    let mut ids = HashSet::new();
    for _ in 0..10000 {
        let id = AgentId::new();
        assert!(ids.insert(id), "Generated duplicate AgentId");
    }
}

#[test]
fn test_agent_id_hashmap_operations() {
    let mut map: HashMap<AgentId, String> = HashMap::new();

    let id1 = AgentId::new();
    let id2 = AgentId::new();
    let id3 = id1; // Copy

    map.insert(id1, "agent1".to_string());
    map.insert(id2, "agent2".to_string());
    map.insert(id3, "agent3".to_string()); // Should overwrite id1

    assert_eq!(map.len(), 2);
    assert_eq!(map.get(&id1), Some(&"agent3".to_string()));
    assert_eq!(map.get(&id2), Some(&"agent2".to_string()));
}

#[test]
fn test_agent_role_case_insensitive_conversion() {
    let test_cases = vec![
        ("COORDINATOR", AgentRole::Coordinator),
        ("coordinator", AgentRole::Coordinator),
        ("CoOrDiNaToR", AgentRole::Coordinator),
        ("FRONTEND", AgentRole::Frontend),
        ("frontend", AgentRole::Frontend),
        ("BACKEND", AgentRole::Backend),
        ("DBA", AgentRole::DBA),
        ("dba", AgentRole::DBA),
        ("DEVOPS", AgentRole::DevOps),
        ("DevOps", AgentRole::DevOps),
        ("SECURITY", AgentRole::Security),
        ("QA", AgentRole::QA),
        ("qa", AgentRole::QA),
    ];

    for (input, expected) in test_cases {
        assert_eq!(AgentRole::from(input), expected, "Failed for input: {}", input);
    }
}

#[test]
fn test_agent_custom_role_preserves_case() {
    // Custom roles should preserve the original case
    let role = AgentRole::from("MyCustomAgent");
    match role {
        AgentRole::Custom(name) => assert_eq!(name, "mycustomagent"),
        _ => panic!("Expected Custom role"),
    }
}

#[test]
fn test_agent_state_error_message_preserved() {
    let error_msg = "Connection failed: timeout after 30s";
    let state = AgentState::Error(error_msg.to_string());

    match state {
        AgentState::Error(msg) => assert_eq!(msg, error_msg),
        _ => panic!("Expected Error state"),
    }
}

#[test]
fn test_agent_state_serialization_roundtrip() {
    let states = vec![
        AgentState::Starting,
        AgentState::Ready,
        AgentState::Busy,
        AgentState::Error("test error with special chars: <>&\"'".to_string()),
        AgentState::Stopping,
        AgentState::Stopped,
    ];

    for state in states {
        let json = serde_json::to_string(&state).unwrap();
        let parsed: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{:?}", state), format!("{:?}", parsed));
    }
}

#[test]
fn test_agent_with_all_fields() {
    let agent = Agent::new(AgentRole::Backend)
        .with_name("backend-worker-1");

    assert_eq!(agent.role, AgentRole::Backend);
    assert_eq!(agent.state, AgentState::Starting);
    assert_eq!(agent.name, Some("backend-worker-1".to_string()));
    assert!(agent.context_hash.is_none());
    assert!(agent.pid.is_none());
}

#[test]
fn test_agent_json_roundtrip() {
    let agent = Agent::new(AgentRole::Coordinator)
        .with_name("main-coordinator");

    let json = serde_json::to_string_pretty(&agent).unwrap();
    let parsed: Agent = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, agent.id);
    assert_eq!(parsed.role, agent.role);
    assert_eq!(parsed.state, agent.state);
    assert_eq!(parsed.name, agent.name);
}

#[test]
fn test_agent_role_all_variants() {
    let roles = vec![
        AgentRole::Coordinator,
        AgentRole::Frontend,
        AgentRole::Backend,
        AgentRole::DBA,
        AgentRole::DevOps,
        AgentRole::Security,
        AgentRole::QA,
        AgentRole::Custom("specialist".to_string()),
    ];

    for role in roles {
        let agent = Agent::new(role.clone());
        assert_eq!(agent.role, role);
    }
}

#[test]
fn test_agent_id_display_uuid_format() {
    let id = AgentId::new();
    let display = id.to_string();

    // UUID format: 8-4-4-4-12
    assert_eq!(display.len(), 36);
    let parts: Vec<&str> = display.split('-').collect();
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 4);
    assert_eq!(parts[2].len(), 4);
    assert_eq!(parts[3].len(), 4);
    assert_eq!(parts[4].len(), 12);
}

#[test]
fn test_agent_clone() {
    let agent1 = Agent::new(AgentRole::Frontend).with_name("original");
    let agent2 = agent1.clone();

    assert_eq!(agent1.id, agent2.id);
    assert_eq!(agent1.role, agent2.role);
    assert_eq!(agent1.name, agent2.name);
}
