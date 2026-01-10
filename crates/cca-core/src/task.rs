//! Task types and management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentId;

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Task status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    /// Task is queued and waiting
    Pending,
    /// Task is currently being processed
    InProgress,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed(String),
    /// Task was cancelled
    Cancelled,
}

/// A task to be executed by an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub description: String,
    pub status: TaskStatus,
    pub assigned_to: Option<AgentId>,
    pub parent_task: Option<TaskId>,
    pub priority: u8,
    pub token_budget: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

impl Task {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            description: description.into(),
            status: TaskStatus::Pending,
            assigned_to: None,
            parent_task: None,
            priority: 5,
            token_budget: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_budget(mut self, budget: u64) -> Self {
        self.token_budget = Some(budget);
        self
    }

    pub fn with_parent(mut self, parent: TaskId) -> Self {
        self.parent_task = Some(parent);
        self
    }

    pub fn assign_to(mut self, agent: AgentId) -> Self {
        self.assigned_to = Some(agent);
        self
    }

    pub fn start(&mut self) {
        self.status = TaskStatus::InProgress;
        self.started_at = Some(Utc::now());
    }

    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.status = TaskStatus::Failed(reason.into());
        self.completed_at = Some(Utc::now());
    }

    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: TaskId,
    pub success: bool,
    pub output: String,
    pub tokens_used: u64,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

impl TaskResult {
    pub fn success(task_id: TaskId, output: impl Into<String>) -> Self {
        Self {
            task_id,
            success: true,
            output: output.into(),
            tokens_used: 0,
            duration_ms: 0,
            error: None,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn failure(task_id: TaskId, error: impl Into<String>) -> Self {
        Self {
            task_id,
            success: false,
            output: String::new(),
            tokens_used: 0,
            duration_ms: 0,
            error: Some(error.into()),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_tokens(mut self, tokens: u64) -> Self {
        self.tokens_used = tokens;
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_uniqueness() {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_task_creation() {
        let task = Task::new("Test task description");
        assert_eq!(task.description, "Test task description");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.priority, 5);
        assert!(task.assigned_to.is_none());
        assert!(task.parent_task.is_none());
    }

    #[test]
    fn test_task_builder_pattern() {
        let parent_id = TaskId::new();
        let agent_id = AgentId::new();

        let task = Task::new("Sub task")
            .with_priority(10)
            .with_budget(1000)
            .with_parent(parent_id)
            .assign_to(agent_id);

        assert_eq!(task.priority, 10);
        assert_eq!(task.token_budget, Some(1000));
        assert_eq!(task.parent_task, Some(parent_id));
        assert_eq!(task.assigned_to, Some(agent_id));
    }

    #[test]
    fn test_task_lifecycle() {
        let mut task = Task::new("Lifecycle test");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.started_at.is_none());
        assert!(task.completed_at.is_none());

        task.start();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert!(task.started_at.is_some());
        assert!(task.completed_at.is_none());

        task.complete();
        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_task_failure() {
        let mut task = Task::new("Failing task");
        task.start();
        task.fail("Something went wrong");

        assert!(matches!(task.status, TaskStatus::Failed(ref msg) if msg == "Something went wrong"));
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_task_cancellation() {
        let mut task = Task::new("Cancel me");
        task.cancel();

        assert_eq!(task.status, TaskStatus::Cancelled);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_task_serialization() {
        let task = Task::new("Serialize me").with_priority(8);
        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.description, "Serialize me");
        assert_eq!(parsed.priority, 8);
    }

    #[test]
    fn test_task_result_success() {
        let task_id = TaskId::new();
        let result = TaskResult::success(task_id, "Task completed successfully")
            .with_tokens(500)
            .with_duration(1234);

        assert!(result.success);
        assert_eq!(result.output, "Task completed successfully");
        assert_eq!(result.tokens_used, 500);
        assert_eq!(result.duration_ms, 1234);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_task_result_failure() {
        let task_id = TaskId::new();
        let result = TaskResult::failure(task_id, "Something failed");

        assert!(!result.success);
        assert!(result.output.is_empty());
        assert_eq!(result.error, Some("Something failed".to_string()));
    }
}
