//! Orchestrator - Task routing and coordination
//!
//! Handles task delegation to specialist agents, result aggregation,
//! and broadcast messaging via ACP WebSocket and Redis pub/sub.
//!
//! Integrates with RL engine for intelligent task routing decisions.
//!
//! Note: Many methods are infrastructure for future features and not yet called.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use cca_acp::{
    methods, AcpMessage, AcpServer, BroadcastParams, BroadcastType, TaskAssignParams,
    TaskResultParams,
};
use cca_core::communication::channels;
use cca_core::{AgentId, AgentRole, Task, TaskId, TaskResult, TaskStatus};
use cca_rl::{Action, Experience};

use crate::redis::RedisServices;
use crate::rl::{compute_reward, AgentInfo, RLService, StateBuilder};

/// Agent workload information
#[derive(Debug, Clone)]
pub struct AgentWorkload {
    pub agent_id: AgentId,
    pub role: String,
    pub current_tasks: u32,
    pub max_tasks: u32,
    pub capabilities: Vec<String>,
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    /// Average task completion time in ms
    pub avg_completion_time: f64,
    /// Total tasks completed
    pub tasks_completed: u32,
    /// Total tasks failed
    pub tasks_failed: u32,
}

/// Pending result aggregation
#[derive(Debug)]
pub struct PendingAggregation {
    pub parent_task_id: TaskId,
    pub subtask_ids: Vec<TaskId>,
    pub results: HashMap<TaskId, TaskResult>,
    pub required_count: usize,
}

/// Task orchestrator for routing and coordination
pub struct Orchestrator {
    /// Active tasks
    tasks: Arc<RwLock<HashMap<TaskId, Task>>>,
    /// Agent workloads
    agent_workloads: Arc<RwLock<HashMap<AgentId, AgentWorkload>>>,
    /// Pending result aggregations (parent_task_id -> aggregation)
    pending_aggregations: Arc<RwLock<HashMap<TaskId, PendingAggregation>>>,
    /// ACP server for WebSocket communication
    acp_server: Option<Arc<AcpServer>>,
    /// Redis services for pub/sub
    redis: Option<Arc<RedisServices>>,
    /// RL service for intelligent routing
    rl_service: Option<Arc<RLService>>,
    /// Request timeout
    request_timeout: Duration,
    /// Whether to use RL for routing decisions
    use_rl_routing: bool,
    /// Task start times for duration tracking
    task_start_times: Arc<RwLock<HashMap<TaskId, std::time::Instant>>>,
}

impl Orchestrator {
    /// Create a new Orchestrator
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            agent_workloads: Arc::new(RwLock::new(HashMap::new())),
            pending_aggregations: Arc::new(RwLock::new(HashMap::new())),
            acp_server: None,
            redis: None,
            rl_service: None,
            request_timeout: Duration::from_secs(30),
            use_rl_routing: false,
            task_start_times: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Configure with ACP server
    pub fn with_acp(mut self, acp_server: Arc<AcpServer>) -> Self {
        self.acp_server = Some(acp_server);
        self
    }

    /// Configure with Redis services
    pub fn with_redis(mut self, redis: Arc<RedisServices>) -> Self {
        self.redis = Some(redis);
        self
    }

    /// Configure with RL service for intelligent routing
    pub fn with_rl(mut self, rl_service: Arc<RLService>) -> Self {
        self.rl_service = Some(rl_service);
        self.use_rl_routing = true;
        info!("Orchestrator RL routing enabled");
        self
    }

    /// Enable or disable RL-based routing
    pub fn set_rl_routing(&mut self, enabled: bool) {
        self.use_rl_routing = enabled;
        info!("RL routing {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Configure request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Register an agent with the orchestrator
    pub async fn register_agent(
        &self,
        agent_id: AgentId,
        role: String,
        capabilities: Vec<String>,
        max_tasks: u32,
    ) {
        let workload = AgentWorkload {
            agent_id,
            role: role.clone(),
            current_tasks: 0,
            max_tasks,
            capabilities,
            success_rate: 1.0, // Start optimistic
            avg_completion_time: 0.0,
            tasks_completed: 0,
            tasks_failed: 0,
        };

        let mut workloads = self.agent_workloads.write().await;
        workloads.insert(agent_id, workload);

        info!(
            "Registered agent {} with role {} (max {} tasks)",
            agent_id, role, max_tasks
        );
    }

    /// Unregister an agent
    pub async fn unregister_agent(&self, agent_id: AgentId) {
        let mut workloads = self.agent_workloads.write().await;
        workloads.remove(&agent_id);
        info!("Unregistered agent {}", agent_id);
    }

    /// Route a task to a specific agent
    pub async fn route_task(&self, agent_id: AgentId, mut task: Task) -> Result<TaskId> {
        let task_id = task.id;

        task.assigned_to = Some(agent_id);
        task.start();

        info!("Routing task {} to agent {}", task_id, agent_id);

        // Store the task
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task.clone());
        }

        // Track task start time for duration calculation
        {
            let mut start_times = self.task_start_times.write().await;
            start_times.insert(task_id, std::time::Instant::now());
        }

        // Update agent workload
        {
            let mut workloads = self.agent_workloads.write().await;
            if let Some(workload) = workloads.get_mut(&agent_id) {
                workload.current_tasks += 1;
            }
        }

        // Send via ACP WebSocket
        if let Some(ref acp) = self.acp_server {
            let params = TaskAssignParams {
                task_id,
                description: task.description.clone(),
                priority: task.priority,
                parent_task: task.parent_task,
                token_budget: task.token_budget,
                metadata: task.metadata.clone(),
            };

            let message =
                AcpMessage::request(task_id.to_string(), methods::TASK_ASSIGN, serde_json::to_value(params)?);

            match acp.send_to(agent_id, message).await {
                Ok(()) => debug!("Task {} sent to agent {} via ACP", task_id, agent_id),
                Err(e) => {
                    warn!("Failed to send task via ACP, falling back to Redis: {}", e);
                    // Fallback to Redis
                    self.send_task_via_redis(agent_id, &task).await?;
                }
            }
        } else if self.redis.is_some() {
            // Use Redis if no ACP server
            self.send_task_via_redis(agent_id, &task).await?;
        }

        Ok(task_id)
    }

    /// Send task via Redis pub/sub
    async fn send_task_via_redis(&self, agent_id: AgentId, task: &Task) -> Result<()> {
        if let Some(ref redis) = self.redis {
            let channel = channels::agent_tasks(&agent_id.to_string());
            let message = serde_json::to_string(task)?;
            redis.pubsub.publish(&channel, &message).await?;
            debug!("Task {} sent to agent {} via Redis", task.id, agent_id);
        }
        Ok(())
    }

    /// Route a task to the best available agent based on role/capabilities
    /// Uses RL predictions when enabled
    pub async fn route_task_auto(&self, task: Task, required_role: &str) -> Result<TaskId> {
        let agent_id = if self.use_rl_routing && self.rl_service.is_some() {
            self.find_best_agent_rl(required_role, &task).await?
        } else {
            self.find_best_agent_heuristic(required_role).await?
        };
        self.route_task(agent_id, task).await
    }

    /// Find the best available agent using RL prediction
    async fn find_best_agent_rl(&self, required_role: &str, task: &Task) -> Result<AgentId> {
        let workloads = self.agent_workloads.read().await;

        // Find agents with matching role and available capacity
        let candidates: Vec<_> = workloads
            .values()
            .filter(|w| w.role == required_role && w.current_tasks < w.max_tasks)
            .collect();

        if candidates.is_empty() {
            return Err(anyhow::anyhow!("No available agent for role: {required_role}"));
        }

        // Build RL state from current context
        let mut state_builder = StateBuilder::new(&task.description)
            .complexity(0.5); // Default complexity, could be extracted from task metadata

        // Add agent info to state
        for agent in &candidates {
            let role = AgentRole::from(&agent.role as &str);
            state_builder = state_builder.add_agent(AgentInfo {
                role,
                is_busy: agent.current_tasks > 0,
                success_rate: agent.success_rate,
                avg_completion_time: agent.avg_completion_time,
            });
        }

        let state = state_builder.build();

        // Get RL prediction
        let rl_service = self.rl_service.as_ref().unwrap();
        let action = rl_service.predict(&state).await;

        // Map action to agent selection
        let selected_agent = match &action {
            Action::RouteToAgent(role) => {
                // Find the best agent with the suggested role
                candidates
                    .iter()
                    .filter(|a| AgentRole::from(&a.role as &str) == *role)
                    .min_by_key(|a| a.current_tasks)
                    .map(|a| a.agent_id)
            }
            Action::Composite(actions) => {
                // Find the first RouteToAgent action in the composite
                let primary_role = actions.iter().find_map(|a| {
                    if let Action::RouteToAgent(role) = a {
                        Some(role.clone())
                    } else {
                        None
                    }
                });
                if let Some(role) = primary_role {
                    candidates
                        .iter()
                        .filter(|a| AgentRole::from(&a.role as &str) == role)
                        .min_by_key(|a| a.current_tasks)
                        .map(|a| a.agent_id)
                } else {
                    None
                }
            }
            // Other action types (AllocateTokens, UsePattern, CompressContext)
            // don't directly map to agent selection, fall back to heuristic
            _ => {
                debug!("RL action {:?} doesn't select agent, using heuristic", action);
                None
            }
        };

        // Fall back to heuristic if RL didn't find a match
        if let Some(agent_id) = selected_agent {
            debug!("RL selected agent {} for role {}", agent_id, required_role);
            Ok(agent_id)
        } else {
            debug!("RL found no match, using heuristic fallback");
            self.find_best_agent_heuristic(required_role).await
        }
    }

    /// Find the best available agent using simple heuristic (least busy)
    async fn find_best_agent_heuristic(&self, required_role: &str) -> Result<AgentId> {
        let workloads = self.agent_workloads.read().await;

        // Find agents with matching role and available capacity
        let mut candidates: Vec<_> = workloads
            .values()
            .filter(|w| w.role == required_role && w.current_tasks < w.max_tasks)
            .collect();

        // Sort by current workload (prefer least busy)
        candidates.sort_by_key(|w| w.current_tasks);

        candidates
            .first()
            .map(|w| w.agent_id)
            .ok_or_else(|| anyhow::anyhow!("No available agent for role: {required_role}"))
    }

    /// Delegate task to multiple specialists and aggregate results
    pub async fn delegate_to_specialists(
        &self,
        parent_task: Task,
        subtasks: Vec<(String, Task)>, // (role, task)
    ) -> Result<TaskId> {
        let parent_id = parent_task.id;

        // Store parent task
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(parent_id, parent_task);
        }

        let mut subtask_ids = Vec::new();

        // Route each subtask to a specialist
        for (role, mut subtask) in subtasks {
            subtask.parent_task = Some(parent_id);

            match self.route_task_auto(subtask, &role).await {
                Ok(task_id) => {
                    subtask_ids.push(task_id);
                }
                Err(e) => {
                    error!("Failed to route subtask to {}: {}", role, e);
                    // Continue with other subtasks
                }
            }
        }

        // Create pending aggregation
        let aggregation = PendingAggregation {
            parent_task_id: parent_id,
            subtask_ids: subtask_ids.clone(),
            results: HashMap::new(),
            required_count: subtask_ids.len(),
        };

        {
            let mut pending = self.pending_aggregations.write().await;
            pending.insert(parent_id, aggregation);
        }

        info!(
            "Delegated {} subtasks for parent task {}",
            subtask_ids.len(),
            parent_id
        );

        Ok(parent_id)
    }

    /// Process a task result from an agent
    pub async fn process_result(&self, result: TaskResult) -> Result<Option<TaskResult>> {
        let task_id = result.task_id;

        // Get task duration from start times
        let duration_ms = {
            let mut start_times = self.task_start_times.write().await;
            if let Some(start_time) = start_times.remove(&task_id) {
                start_time.elapsed().as_millis() as u64
            } else {
                result.duration_ms // Use reported duration if we don't have start time
            }
        };

        // Get task info for RL experience recording
        let (assigned_agent, task_description) = {
            let tasks = self.tasks.read().await;
            tasks.get(&task_id).map_or((None, String::new()), |t| (t.assigned_to, t.description.clone()))
        };

        // Update agent workload and stats
        if let Some(agent_id) = assigned_agent {
            let mut workloads = self.agent_workloads.write().await;
            if let Some(workload) = workloads.get_mut(&agent_id) {
                // Decrement current tasks
                if workload.current_tasks > 0 {
                    workload.current_tasks -= 1;
                }

                // Update success/failure counts
                if result.success {
                    workload.tasks_completed += 1;
                } else {
                    workload.tasks_failed += 1;
                }

                // Update success rate
                let total = workload.tasks_completed + workload.tasks_failed;
                if total > 0 {
                    workload.success_rate = workload.tasks_completed as f64 / total as f64;
                }

                // Update average completion time (exponential moving average)
                let alpha = 0.2; // Smoothing factor
                if workload.avg_completion_time == 0.0 {
                    workload.avg_completion_time = duration_ms as f64;
                } else {
                    workload.avg_completion_time =
                        alpha * duration_ms as f64 + (1.0 - alpha) * workload.avg_completion_time;
                }

                debug!(
                    "Agent {} stats: success_rate={:.2}, avg_time={:.0}ms, completed={}, failed={}",
                    agent_id, workload.success_rate, workload.avg_completion_time,
                    workload.tasks_completed, workload.tasks_failed
                );
            }
        }

        // Record RL experience for learning
        if let Some(ref rl_service) = self.rl_service {
            if let Some(agent_id) = assigned_agent {
                // Build state from task context
                let workloads = self.agent_workloads.read().await;
                let mut state_builder = StateBuilder::new(&task_description).complexity(0.5);

                // Add current agent states
                for w in workloads.values() {
                    let role = AgentRole::from(&w.role as &str);
                    state_builder = state_builder.add_agent(AgentInfo {
                        role,
                        is_busy: w.current_tasks > 0,
                        success_rate: w.success_rate,
                        avg_completion_time: w.avg_completion_time,
                    });
                }
                drop(workloads);

                let state = state_builder.build();

                // Determine the action that was taken
                let agent_role = {
                    let workloads = self.agent_workloads.read().await;
                    workloads.get(&agent_id)
                        .map_or(AgentRole::Backend, |w| AgentRole::from(&w.role as &str))
                };
                let action = Action::RouteToAgent(agent_role);

                // Compute reward based on outcome
                let reward = compute_reward(
                    result.success,
                    result.tokens_used as u32,
                    duration_ms as u32,
                    10000,  // max tokens budget
                    60000,  // max duration (1 minute)
                );

                // Create and record experience
                let experience = Experience::new(
                    state,
                    action,
                    reward,
                    None, // next_state (terminal state for now)
                    true, // done
                );

                if let Err(e) = rl_service.record_experience(experience).await {
                    warn!("Failed to record RL experience: {}", e);
                } else {
                    debug!("Recorded RL experience: reward={:.3}", reward);
                }
            }
        }

        // Update task status
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                if result.success {
                    task.complete();
                } else {
                    task.fail(result.error.clone().unwrap_or_default());
                }
            }
        }

        // Publish result via Redis
        if let Some(ref redis) = self.redis {
            let task_result_params = TaskResultParams {
                task_id: result.task_id,
                success: result.success,
                output: result.output.clone(),
                tokens_used: result.tokens_used,
                duration_ms,
                error: result.error.clone(),
                metadata: result.metadata.clone(),
            };

            let message = serde_json::to_string(&task_result_params)?;
            redis
                .pubsub
                .publish(channels::COORDINATION, &message)
                .await?;
        }

        // Check if this is part of an aggregation
        let aggregated_result = self.try_aggregate_result(task_id, result).await?;

        Ok(aggregated_result)
    }

    /// Try to aggregate a result into a parent task
    async fn try_aggregate_result(
        &self,
        task_id: TaskId,
        result: TaskResult,
    ) -> Result<Option<TaskResult>> {
        // Find the parent task for this result
        let parent_id = {
            let tasks = self.tasks.read().await;
            tasks.get(&task_id).and_then(|t| t.parent_task)
        };

        let parent_id = match parent_id {
            Some(id) => id,
            None => return Ok(None), // Not part of an aggregation
        };

        // Add result to aggregation
        let aggregation_complete = {
            let mut pending = self.pending_aggregations.write().await;

            if let Some(agg) = pending.get_mut(&parent_id) {
                agg.results.insert(task_id, result);

                if agg.results.len() >= agg.required_count {
                    // Aggregation complete
                    pending.remove(&parent_id)
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Aggregate results if complete
        if let Some(agg) = aggregation_complete {
            let aggregated = self.aggregate_results(agg).await?;
            return Ok(Some(aggregated));
        }

        Ok(None)
    }

    /// Aggregate multiple results into a single result
    async fn aggregate_results(&self, aggregation: PendingAggregation) -> Result<TaskResult> {
        let mut combined_output = String::new();
        let mut total_tokens = 0u64;
        let mut total_duration = 0u64;
        let mut all_success = true;
        let mut errors = Vec::new();

        for (task_id, result) in &aggregation.results {
            if result.success {
                use std::fmt::Write;
                let _ = write!(combined_output, "\n--- Task {task_id} ---\n{}\n", result.output);
            } else {
                all_success = false;
                if let Some(ref error) = result.error {
                    errors.push(format!("Task {task_id}: {error}"));
                }
            }
            total_tokens += result.tokens_used;
            total_duration += result.duration_ms;
        }

        // Update parent task status
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&aggregation.parent_task_id) {
                if all_success {
                    task.complete();
                } else {
                    task.fail(errors.join("; "));
                }
            }
        }

        let aggregated_result = TaskResult {
            task_id: aggregation.parent_task_id,
            success: all_success,
            output: combined_output,
            tokens_used: total_tokens,
            duration_ms: total_duration,
            error: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
            metadata: serde_json::Value::Null,
        };

        info!(
            "Aggregated {} results for parent task {} (success: {})",
            aggregation.results.len(),
            aggregation.parent_task_id,
            all_success
        );

        Ok(aggregated_result)
    }

    /// Broadcast a message to all connected agents
    pub async fn broadcast(&self, broadcast_type: BroadcastType, content: serde_json::Value) -> Result<usize> {
        let params = BroadcastParams {
            message_type: broadcast_type.clone(),
            content: content.clone(),
        };

        let mut sent_count = 0;

        // Broadcast via ACP WebSocket
        if let Some(ref acp) = self.acp_server {
            let message =
                AcpMessage::notification(methods::BROADCAST, serde_json::to_value(&params)?);

            match acp.broadcast(message).await {
                Ok(result) => {
                    sent_count += result.sent;
                    debug!("Broadcast sent to {} agents via ACP ({})", result.sent, result);
                    if result.had_backpressure() {
                        warn!("Broadcast had backpressure: {}", result);
                    }
                }
                Err(e) => {
                    error!("Failed to broadcast via ACP: {}", e);
                }
            }
        }

        // Also broadcast via Redis pub/sub
        if let Some(ref redis) = self.redis {
            let message = serde_json::to_string(&params)?;
            if redis.pubsub.publish(channels::BROADCAST, &message).await.is_ok() {
                debug!("Broadcast sent via Redis");
            }
        }

        Ok(sent_count)
    }

    /// Broadcast an announcement
    pub async fn announce(&self, message: &str) -> Result<usize> {
        self.broadcast(
            BroadcastType::Announcement,
            serde_json::json!({ "message": message }),
        )
        .await
    }

    /// Send a health check to all agents
    pub async fn health_check(&self) -> Result<usize> {
        self.broadcast(
            BroadcastType::HealthCheck,
            serde_json::json!({ "timestamp": chrono::Utc::now().timestamp() }),
        )
        .await
    }

    /// Get task status
    pub async fn get_task_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let tasks = self.tasks.read().await;
        tasks.get(&task_id).map(|t| t.status.clone())
    }

    /// Get a task by ID
    pub async fn get_task(&self, task_id: TaskId) -> Option<Task> {
        let tasks = self.tasks.read().await;
        tasks.get(&task_id).cloned()
    }

    /// Store task result (for backwards compatibility)
    pub async fn store_result(&self, task_id: TaskId, result: TaskResult) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(&task_id) {
            if result.success {
                task.complete();
            } else {
                task.fail(result.error.unwrap_or_default());
            }
        }
    }

    /// List recent tasks
    pub async fn list_tasks(&self, limit: usize) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        let mut task_list: Vec<_> = tasks.values().cloned().collect();
        task_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        task_list.into_iter().take(limit).collect()
    }

    /// Get agent workloads
    pub async fn get_agent_workloads(&self) -> Vec<AgentWorkload> {
        let workloads = self.agent_workloads.read().await;
        workloads.values().cloned().collect()
    }

    /// Get pending aggregation count
    pub async fn pending_aggregation_count(&self) -> usize {
        let pending = self.pending_aggregations.read().await;
        pending.len()
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_registration() {
        let orchestrator = Orchestrator::new();

        let agent_id = AgentId::new();
        orchestrator
            .register_agent(
                agent_id,
                "specialist".to_string(),
                vec!["code".to_string()],
                5,
            )
            .await;

        let workloads = orchestrator.get_agent_workloads().await;
        assert_eq!(workloads.len(), 1);
        assert_eq!(workloads[0].role, "specialist");
        assert_eq!(workloads[0].max_tasks, 5);
    }

    #[tokio::test]
    async fn test_find_best_agent() {
        let orchestrator = Orchestrator::new();

        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        // Register agent1 with some load
        orchestrator
            .register_agent(agent1, "specialist".to_string(), vec![], 5)
            .await;

        // Register agent2 with no load
        orchestrator
            .register_agent(agent2, "specialist".to_string(), vec![], 5)
            .await;

        // Simulate agent1 having a task
        {
            let mut workloads = orchestrator.agent_workloads.write().await;
            if let Some(w) = workloads.get_mut(&agent1) {
                w.current_tasks = 2;
            }
        }

        // Should pick agent2 (less busy)
        let best = orchestrator.find_best_agent_heuristic("specialist").await.unwrap();
        assert_eq!(best, agent2);
    }

    #[tokio::test]
    async fn test_task_routing() {
        let orchestrator = Orchestrator::new();
        let agent_id = AgentId::new();

        orchestrator
            .register_agent(agent_id, "coordinator".to_string(), vec![], 10)
            .await;

        let task = Task::new("Test task");
        let task_id = orchestrator.route_task(agent_id, task).await.unwrap();

        let stored_task = orchestrator.get_task(task_id).await.unwrap();
        assert_eq!(stored_task.assigned_to, Some(agent_id));
        assert!(matches!(stored_task.status, TaskStatus::InProgress));
    }

    #[tokio::test]
    async fn test_list_tasks() {
        let orchestrator = Orchestrator::new();
        let agent_id = AgentId::new();

        orchestrator
            .register_agent(agent_id, "test".to_string(), vec![], 10)
            .await;

        for i in 0..5 {
            let task = Task::new(format!("Task {i}"));
            orchestrator.route_task(agent_id, task).await.unwrap();
        }

        let tasks = orchestrator.list_tasks(3).await;
        assert_eq!(tasks.len(), 3);
    }
}
