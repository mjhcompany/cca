//! Main CCA Daemon implementation

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::State;
use axum::{
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use cca_core::AgentRole;

use crate::agent_manager::AgentManager;
use crate::config::Config;
use crate::orchestrator::Orchestrator;

/// Shared daemon state for API handlers
#[derive(Clone)]
pub struct DaemonState {
    pub config: Config,
    pub agent_manager: Arc<RwLock<AgentManager>>,
    pub orchestrator: Arc<RwLock<Orchestrator>>,
    pub tasks: Arc<RwLock<HashMap<String, TaskState>>>,
}

/// Task tracking state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task_id: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub assigned_agent: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Main CCA Daemon
pub struct CCADaemon {
    config: Config,
    state: DaemonState,
    shutdown: tokio::sync::broadcast::Sender<()>,
}

impl CCADaemon {
    /// Create a new CCA Daemon instance
    pub async fn new(config: Config) -> Result<Self> {
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        let agent_manager = Arc::new(RwLock::new(AgentManager::new(&config)));
        let orchestrator = Arc::new(RwLock::new(Orchestrator::new()));

        let state = DaemonState {
            config: config.clone(),
            agent_manager: agent_manager.clone(),
            orchestrator: orchestrator.clone(),
            tasks: Arc::new(RwLock::new(HashMap::new())),
        };

        Ok(Self {
            config,
            state,
            shutdown: shutdown_tx,
        })
    }

    /// Run the daemon main loop
    pub async fn run(&self) -> Result<()> {
        info!("CCA Daemon running on {}", self.config.daemon.bind_address);

        // Start API server
        let addr: std::net::SocketAddr = self.config.daemon.bind_address.parse()?;

        // Create the API router with state
        let app = create_router(self.state.clone());

        // Create listener
        let listener = tokio::net::TcpListener::bind(addr).await?;

        // Subscribe to shutdown signal
        let mut shutdown_rx = self.shutdown.subscribe();

        // Serve with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.recv().await;
            })
            .await?;

        Ok(())
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down daemon...");

        // Signal all tasks to stop
        let _ = self.shutdown.send(());

        // Stop all agents
        let mut manager = self.state.agent_manager.write().await;
        manager.stop_all().await?;

        info!("Daemon shutdown complete");
        Ok(())
    }
}

/// Create the API router with state
fn create_router(state: DaemonState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/status", get(get_status))
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/agents", post(spawn_agent))
        .route("/api/v1/tasks", get(list_tasks))
        .route("/api/v1/tasks", post(create_task))
        .route("/api/v1/tasks/{task_id}", get(get_task))
        .route("/api/v1/activity", get(get_activity))
        .with_state(state)
}

// API Request/Response types

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTaskRequest {
    pub description: String,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskResponse {
    pub task_id: String,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub assigned_agent: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpawnAgentRequest {
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub current_task: Option<String>,
}

// API handlers

async fn health_check() -> &'static str {
    "OK"
}

async fn get_status(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let tasks = state.tasks.read().await;
    let agents = state.agent_manager.read().await;

    let pending = tasks.values().filter(|t| t.status == "pending").count();
    let completed = tasks.values().filter(|t| t.status == "completed").count();

    Json(serde_json::json!({
        "status": "running",
        "version": env!("CARGO_PKG_VERSION"),
        "agents_count": agents.list().len(),
        "tasks_pending": pending,
        "tasks_completed": completed
    }))
}

async fn list_agents(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let manager = state.agent_manager.read().await;
    let agents: Vec<AgentInfo> = manager
        .list()
        .iter()
        .map(|a| AgentInfo {
            agent_id: a.id.to_string(),
            role: a.role.to_string(),
            status: format!("{:?}", a.state),
            current_task: None,
        })
        .collect();

    Json(serde_json::json!({
        "agents": agents
    }))
}

async fn spawn_agent(
    State(state): State<DaemonState>,
    Json(request): Json<SpawnAgentRequest>,
) -> Json<serde_json::Value> {
    let role = match request.role.to_lowercase().as_str() {
        "coordinator" => AgentRole::Coordinator,
        "frontend" => AgentRole::Frontend,
        "backend" => AgentRole::Backend,
        "dba" => AgentRole::DBA,
        "devops" => AgentRole::DevOps,
        "security" => AgentRole::Security,
        "qa" => AgentRole::QA,
        _ => {
            return Json(serde_json::json!({
                "error": format!("Unknown agent role: {}", request.role)
            }));
        }
    };

    let mut manager = state.agent_manager.write().await;

    match manager.spawn(role.clone()).await {
        Ok(agent_id) => Json(serde_json::json!({
            "agent_id": agent_id.to_string(),
            "role": role.to_string(),
            "status": "running"
        })),
        Err(e) => Json(serde_json::json!({
            "error": format!("Failed to spawn agent: {}", e)
        })),
    }
}

async fn list_tasks(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let tasks = state.tasks.read().await;
    let task_list: Vec<TaskResponse> = tasks
        .values()
        .map(|t| TaskResponse {
            task_id: t.task_id.clone(),
            status: t.status.clone(),
            output: t.output.clone(),
            error: t.error.clone(),
            assigned_agent: t.assigned_agent.clone(),
        })
        .collect();

    Json(serde_json::json!({
        "tasks": task_list
    }))
}

async fn create_task(
    State(state): State<DaemonState>,
    Json(request): Json<CreateTaskRequest>,
) -> Json<TaskResponse> {
    let task_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let priority = request.priority.unwrap_or_else(|| "normal".to_string());

    // Create task state
    let task = TaskState {
        task_id: task_id.clone(),
        description: request.description.clone(),
        status: "pending".to_string(),
        priority,
        output: None,
        error: None,
        assigned_agent: None,
        created_at: now,
        updated_at: now,
    };

    // Store task
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id.clone(), task);
    }

    info!("Task created: {} - {}", task_id, request.description);

    // Try to find or spawn a Coordinator to handle the task
    let coordinator_id = {
        let manager = state.agent_manager.read().await;
        manager
            .list()
            .iter()
            .find(|a| matches!(a.role, AgentRole::Coordinator))
            .map(|a| a.id)
    };

    let result = if let Some(agent_id) = coordinator_id {
        // Send task to existing Coordinator
        let mut manager = state.agent_manager.write().await;
        match manager.send(agent_id, &request.description).await {
            Ok(response) => {
                // Update task with response
                let mut tasks = state.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.status = "completed".to_string();
                    task.output = Some(response.clone());
                    task.assigned_agent = Some(agent_id.to_string());
                    task.updated_at = Utc::now();
                }
                Ok((response, agent_id.to_string()))
            }
            Err(e) => Err(e.to_string()),
        }
    } else {
        // No Coordinator running - try to spawn one
        let spawn_result = {
            let mut manager = state.agent_manager.write().await;
            manager.spawn(AgentRole::Coordinator).await
        };

        match spawn_result {
            Ok(agent_id) => {
                // Send task to new Coordinator
                let mut manager = state.agent_manager.write().await;
                match manager.send(agent_id, &request.description).await {
                    Ok(response) => {
                        let mut tasks = state.tasks.write().await;
                        if let Some(task) = tasks.get_mut(&task_id) {
                            task.status = "completed".to_string();
                            task.output = Some(response.clone());
                            task.assigned_agent = Some(agent_id.to_string());
                            task.updated_at = Utc::now();
                        }
                        Ok((response, agent_id.to_string()))
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            Err(e) => Err(format!("Failed to spawn Coordinator: {}", e)),
        }
    };

    match result {
        Ok((response, agent_id)) => Json(TaskResponse {
            task_id,
            status: "completed".to_string(),
            output: Some(response),
            error: None,
            assigned_agent: Some(agent_id),
        }),
        Err(e) => {
            // Update task with error
            {
                let mut tasks = state.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.status = "failed".to_string();
                    task.error = Some(e.clone());
                    task.updated_at = Utc::now();
                }
            }
            Json(TaskResponse {
                task_id,
                status: "failed".to_string(),
                output: None,
                error: Some(e),
                assigned_agent: None,
            })
        }
    }
}

async fn get_task(
    State(state): State<DaemonState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Result<Json<TaskResponse>, axum::http::StatusCode> {
    let tasks = state.tasks.read().await;

    match tasks.get(&task_id) {
        Some(task) => Ok(Json(TaskResponse {
            task_id: task.task_id.clone(),
            status: task.status.clone(),
            output: task.output.clone(),
            error: task.error.clone(),
            assigned_agent: task.assigned_agent.clone(),
        })),
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn get_activity(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let manager = state.agent_manager.read().await;

    let activity: Vec<_> = manager
        .list()
        .iter()
        .map(|a| {
            serde_json::json!({
                "agent_id": a.id.to_string(),
                "role": a.role.to_string(),
                "status": format!("{:?}", a.state),
                "current_task": null,
                "last_activity": null
            })
        })
        .collect();

    Json(serde_json::json!({
        "agents": activity
    }))
}
