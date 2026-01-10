//! Orchestrator - Task routing and coordination

use std::collections::HashMap;

use anyhow::Result;
use tracing::info;

use cca_core::{AgentId, Task, TaskId, TaskResult, TaskStatus};

/// Task orchestrator for routing and coordination
pub struct Orchestrator {
    tasks: HashMap<TaskId, Task>,
}

impl Orchestrator {
    /// Create a new Orchestrator
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Route a task to an agent
    pub async fn route_task(&self, agent_id: AgentId, mut task: Task) -> Result<TaskId> {
        let task_id = task.id;

        task.assigned_to = Some(agent_id);
        task.start();

        info!("Routing task {} to agent {}", task_id, agent_id);

        // TODO: Actually send the task via ACP/Redis

        Ok(task_id)
    }

    /// Get task status
    pub fn get_task_status(&self, task_id: TaskId) -> Option<&TaskStatus> {
        self.tasks.get(&task_id).map(|t| &t.status)
    }

    /// Get a task by ID
    pub fn get_task(&self, task_id: TaskId) -> Option<&Task> {
        self.tasks.get(&task_id)
    }

    /// Store task result
    pub fn store_result(&mut self, task_id: TaskId, result: TaskResult) {
        if let Some(task) = self.tasks.get_mut(&task_id) {
            if result.success {
                task.complete();
            } else {
                task.fail(result.error.unwrap_or_default());
            }
        }
    }

    /// List recent tasks
    pub fn list_tasks(&self, limit: usize) -> Vec<&Task> {
        let mut tasks: Vec<_> = self.tasks.values().collect();
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        tasks.into_iter().take(limit).collect()
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}
