//! Integration tests for Task types
//! Complements the inline unit tests in src/task.rs

use cca_core::{Task, TaskId, TaskStatus, TaskResult, AgentId};
use std::collections::HashSet;

#[test]
fn test_task_id_uniqueness_stress() {
    let mut ids = HashSet::new();
    for _ in 0..10000 {
        let id = TaskId::new();
        assert!(ids.insert(id), "Generated duplicate TaskId");
    }
}

#[test]
fn test_task_priority_range() {
    // Priority is u8, so 0-255
    let task_min = Task::new("min priority").with_priority(0);
    let task_max = Task::new("max priority").with_priority(255);
    let task_default = Task::new("default priority");

    assert_eq!(task_min.priority, 0);
    assert_eq!(task_max.priority, 255);
    assert_eq!(task_default.priority, 5); // Default is 5
}

#[test]
fn test_task_full_builder_chain() {
    let parent_id = TaskId::new();
    let agent_id = AgentId::new();

    let task = Task::new("Complex task")
        .with_priority(10)
        .with_budget(50000)
        .with_parent(parent_id)
        .assign_to(agent_id);

    assert_eq!(task.description, "Complex task");
    assert_eq!(task.priority, 10);
    assert_eq!(task.token_budget, Some(50000));
    assert_eq!(task.parent_task, Some(parent_id));
    assert_eq!(task.assigned_to, Some(agent_id));
    assert_eq!(task.status, TaskStatus::Pending);
}

#[test]
fn test_task_status_failed_with_reason() {
    let mut task = Task::new("Will fail");
    task.start();
    task.fail("Network timeout");

    match task.status {
        TaskStatus::Failed(reason) => assert_eq!(reason, "Network timeout"),
        _ => panic!("Expected Failed status"),
    }
}

#[test]
fn test_task_lifecycle_timing() {
    let mut task = Task::new("Timing test");

    // Initial state
    assert!(task.started_at.is_none());
    assert!(task.completed_at.is_none());

    // After start
    task.start();
    let started = task.started_at.expect("started_at should be set");
    assert!(task.completed_at.is_none());
    assert!(started >= task.created_at);

    // After complete
    task.complete();
    let completed = task.completed_at.expect("completed_at should be set");
    assert!(completed >= started);
}

#[test]
fn test_task_json_roundtrip() {
    let task = Task::new("Serialize me")
        .with_priority(8)
        .with_budget(10000);

    let json = serde_json::to_string_pretty(&task).unwrap();
    let parsed: Task = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, task.id);
    assert_eq!(parsed.description, task.description);
    assert_eq!(parsed.priority, task.priority);
    assert_eq!(parsed.token_budget, task.token_budget);
    assert_eq!(parsed.status, task.status);
}

#[test]
fn test_task_result_success_builder() {
    let task_id = TaskId::new();
    let result = TaskResult::success(task_id, "Task completed!")
        .with_tokens(1500)
        .with_duration(3000);

    assert_eq!(result.task_id, task_id);
    assert!(result.success);
    assert_eq!(result.output, "Task completed!");
    assert_eq!(result.tokens_used, 1500);
    assert_eq!(result.duration_ms, 3000);
    assert!(result.error.is_none());
}

#[test]
fn test_task_result_failure_builder() {
    let task_id = TaskId::new();
    let result = TaskResult::failure(task_id, "Database connection failed")
        .with_tokens(100)
        .with_duration(500);

    assert_eq!(result.task_id, task_id);
    assert!(!result.success);
    assert!(result.output.is_empty());
    assert_eq!(result.error, Some("Database connection failed".to_string()));
    assert_eq!(result.tokens_used, 100);
}

#[test]
fn test_task_status_equality() {
    assert_eq!(TaskStatus::Pending, TaskStatus::Pending);
    assert_eq!(TaskStatus::InProgress, TaskStatus::InProgress);
    assert_eq!(TaskStatus::Completed, TaskStatus::Completed);
    assert_eq!(TaskStatus::Cancelled, TaskStatus::Cancelled);

    // Failed with same message
    assert_eq!(
        TaskStatus::Failed("error".to_string()),
        TaskStatus::Failed("error".to_string())
    );

    // Failed with different messages
    assert_ne!(
        TaskStatus::Failed("error1".to_string()),
        TaskStatus::Failed("error2".to_string())
    );
}

#[test]
fn test_task_empty_description() {
    let task = Task::new("");
    assert_eq!(task.description, "");
}

#[test]
fn test_task_long_description() {
    let long_desc = "x".repeat(100000);
    let task = Task::new(&long_desc);
    assert_eq!(task.description.len(), 100000);
}

#[test]
fn test_task_cancel_from_pending() {
    let mut task = Task::new("Will be cancelled");
    assert_eq!(task.status, TaskStatus::Pending);

    task.cancel();

    assert_eq!(task.status, TaskStatus::Cancelled);
    assert!(task.completed_at.is_some());
}

#[test]
fn test_task_id_display_format() {
    let id = TaskId::new();
    let display = id.to_string();

    // UUID format: 8-4-4-4-12
    assert_eq!(display.len(), 36);
    assert!(display.chars().filter(|c| *c == '-').count() == 4);
}
