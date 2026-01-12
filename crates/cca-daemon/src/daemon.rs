//! Main CCA Daemon implementation
//!
//! Note: Some fields in structs are infrastructure for future features.
#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::{
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use cca_acp::AcpServer;
use cca_core::{AgentRole, AgentId, TaskId};

use crate::agent_manager::AgentManager;
use crate::auth::{auth_middleware, AuthConfig};
use crate::config::Config;
use crate::orchestrator::Orchestrator;
use crate::postgres::PostgresServices;
use crate::redis::{PubSubMessage, RedisAgentState, RedisServices};
use crate::rl::{RLConfig, RLService};
use crate::tokens::TokenService;

/// Shared daemon state for API handlers
#[derive(Clone)]
pub struct DaemonState {
    pub config: Config,
    pub agent_manager: Arc<RwLock<AgentManager>>,
    pub orchestrator: Arc<RwLock<Orchestrator>>,
    pub tasks: Arc<RwLock<HashMap<String, TaskState>>>,
    pub redis: Option<Arc<RedisServices>>,
    pub postgres: Option<Arc<PostgresServices>>,
    pub acp_server: Arc<AcpServer>,
    pub rl_service: Arc<RLService>,
    pub token_service: Arc<TokenService>,
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

        // Initialize Redis services
        let redis = match RedisServices::new(&config.redis).await {
            Ok(services) => {
                info!("Redis services initialized");
                Some(Arc::new(services))
            }
            Err(e) => {
                warn!("Redis unavailable, running without caching: {}", e);
                None
            }
        };

        // Initialize PostgreSQL services
        let postgres = match PostgresServices::new(&config.postgres).await {
            Ok(services) => {
                info!("PostgreSQL services initialized");
                Some(Arc::new(services))
            }
            Err(e) => {
                warn!("PostgreSQL unavailable, running without persistence: {}", e);
                None
            }
        };

        // Initialize ACP WebSocket server
        let acp_addr: SocketAddr = format!("127.0.0.1:{}", config.acp.websocket_port)
            .parse()
            .map_err(|e| anyhow::anyhow!(
                "Invalid ACP address '127.0.0.1:{}': {}",
                config.acp.websocket_port,
                e
            ))?;
        let acp_server = Arc::new(AcpServer::new(acp_addr));
        info!("ACP server configured on port {}", config.acp.websocket_port);

        // Initialize RL service
        let rl_config = RLConfig::default();
        let rl_service = RLService::new(rl_config);
        let rl_service = if let Some(ref pg) = postgres {
            rl_service.with_postgres(pg.clone())
        } else {
            rl_service
        };
        let rl_service = Arc::new(rl_service);
        info!("RL service initialized with algorithm: q_learning");

        // Initialize Orchestrator with all dependencies
        let mut orchestrator = Orchestrator::new();
        if let Some(ref r) = redis {
            orchestrator = orchestrator.with_redis(r.clone());
        }
        orchestrator = orchestrator.with_acp(acp_server.clone());
        orchestrator = orchestrator.with_rl(rl_service.clone());
        let orchestrator = Arc::new(RwLock::new(orchestrator));
        info!("Orchestrator initialized with RL-based task routing");

        // Initialize Token efficiency service
        let token_service = Arc::new(TokenService::new());
        info!("Token efficiency service initialized");

        let state = DaemonState {
            config: config.clone(),
            agent_manager: agent_manager.clone(),
            orchestrator: orchestrator.clone(),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            redis,
            postgres,
            acp_server,
            rl_service,
            token_service,
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

        // Start ACP WebSocket server in background
        let acp_server = self.state.acp_server.clone();
        let acp_task = tokio::spawn(async move {
            if let Err(e) = acp_server.run().await {
                tracing::error!("ACP server error: {}", e);
            }
        });

        info!(
            "ACP WebSocket server started on port {}",
            self.config.acp.websocket_port
        );

        // Serve HTTP API with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.recv().await;
            })
            .await?;

        // Shutdown ACP server
        self.state.acp_server.shutdown();
        acp_task.abort();

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
    // Create auth config from daemon config
    let auth_config = AuthConfig {
        api_keys: state.config.daemon.api_keys.clone(),
        required: state.config.daemon.require_auth,
    };

    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/status", get(get_status))
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/agents", post(spawn_agent))
        .route("/api/v1/agents/:agent_id/send", post(send_to_agent))
        .route("/api/v1/agents/:agent_id/attach", post(start_agent_session))
        .route("/api/v1/agents/:agent_id/logs", get(get_agent_logs))
        .route("/api/v1/delegate", post(delegate_task))
        .route("/api/v1/tasks", get(list_tasks))
        .route("/api/v1/tasks", post(create_task))
        .route("/api/v1/tasks/:task_id", get(get_task))
        .route("/api/v1/activity", get(get_activity))
        .route("/api/v1/redis/status", get(redis_status))
        .route("/api/v1/postgres/status", get(postgres_status))
        .route("/api/v1/memory/search", post(memory_search))
        .route("/api/v1/pubsub/broadcast", post(pubsub_broadcast))
        .route("/api/v1/acp/status", get(acp_status))
        .route("/api/v1/broadcast", post(broadcast_all))
        .route("/api/v1/workloads", get(get_workloads))
        .route("/api/v1/rl/stats", get(rl_stats))
        .route("/api/v1/rl/train", post(rl_train))
        .route("/api/v1/rl/algorithm", post(rl_set_algorithm))
        .route("/api/v1/rl/params", get(rl_get_params))
        .route("/api/v1/rl/params", post(rl_set_params))
        // Token efficiency endpoints
        .route("/api/v1/tokens/analyze", post(tokens_analyze))
        .route("/api/v1/tokens/compress", post(tokens_compress))
        .route("/api/v1/tokens/metrics", get(tokens_metrics))
        .route("/api/v1/tokens/recommendations", get(tokens_recommendations))
        // Apply auth middleware (bypasses /health automatically)
        .layer(axum::middleware::from_fn_with_state(auth_config, auth_middleware))
        .with_state(state)
}

// API Request/Response types

/// Maximum size limits for API inputs (security: prevent DoS via memory exhaustion)
const MAX_TASK_DESCRIPTION_LEN: usize = 100_000;   // 100KB
const MAX_BROADCAST_MESSAGE_LEN: usize = 10_000;   // 10KB
const MAX_CONTENT_LEN: usize = 1_000_000;          // 1MB
const MAX_QUERY_LEN: usize = 1_000;                // 1KB

/// Coordinator system prompt - enforces JSON delegation output
const COORDINATOR_SYSTEM_PROMPT: &str = r#"You are a COORDINATOR agent. You do NOT execute tasks yourself.

Your ONLY job is to decide which specialist agent should handle a task and output a JSON delegation.

Available specialists: backend, frontend, dba, devops, security, qa

You MUST respond with ONLY a JSON object in this exact format:
{"action":"delegate","delegations":[{"role":"AGENT_ROLE","task":"Task description for the specialist","context":"Optional context"}],"summary":"Brief summary"}

Example for "Analyze code structure":
{"action":"delegate","delegations":[{"role":"backend","task":"Analyze the code structure and document components","context":"Code analysis request"}],"summary":"Delegating to backend specialist"}

RULES:
- Output ONLY valid JSON, nothing else
- Always delegate - never answer directly
- Use "backend" for code analysis, API work
- Use "frontend" for UI/UX work
- Use "dba" for database work
- Use "devops" for infrastructure
- Use "security" for security reviews
- Use "qa" for testing"#;

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

/// Request for delegating a task to a specialist agent
#[derive(Debug, Clone, Deserialize)]
pub struct DelegateTaskRequest {
    /// The role of the agent to delegate to (frontend, backend, dba, devops, security, qa)
    pub role: String,
    /// The task description to send to the agent
    pub task: String,
    /// Optional context to include
    #[serde(default)]
    pub context: Option<String>,
    /// Timeout in seconds (default: 120)
    #[serde(default = "default_delegate_timeout")]
    pub timeout_seconds: u64,
}

fn default_delegate_timeout() -> u64 {
    120
}

/// Response from task delegation
#[derive(Debug, Clone, Serialize)]
pub struct DelegateTaskResponse {
    pub success: bool,
    pub agent_id: String,
    pub role: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Coordinator response format for delegation decisions
#[derive(Debug, Clone, Deserialize)]
pub struct CoordinatorResponse {
    pub action: String, // "delegate", "direct", or "error"
    #[serde(default)]
    pub delegations: Vec<CoordinatorDelegation>,
    #[serde(default)]
    pub response: Option<String>, // For direct responses
    #[serde(default)]
    pub error: Option<String>, // For error responses
    #[serde(default)]
    pub summary: Option<String>,
}

/// A single delegation from coordinator
#[derive(Debug, Clone, Deserialize)]
pub struct CoordinatorDelegation {
    pub role: String,
    pub task: String,
    #[serde(default)]
    pub context: Option<String>,
}

/// Request for sending a message to an agent (task mode)
#[derive(Debug, Clone, Deserialize)]
pub struct SendToAgentRequest {
    pub message: String,
    #[serde(default = "default_delegate_timeout")]
    pub timeout_seconds: u64,
}

/// Response from sending a message to an agent
#[derive(Debug, Clone, Serialize)]
pub struct SendToAgentResponse {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub current_task: Option<String>,
}

// API handlers

/// Health check response with service status
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub services: ServiceHealth,
}

/// Individual service health status
#[derive(Debug, Serialize)]
pub struct ServiceHealth {
    pub redis: bool,
    pub postgres: bool,
    pub acp: bool,
}

async fn health_check(State(state): State<DaemonState>) -> Json<HealthResponse> {
    let redis_ok = state.redis.is_some();
    let postgres_ok = state.postgres.is_some();

    let status = if redis_ok && postgres_ok {
        "healthy"
    } else {
        "degraded"
    };

    Json(HealthResponse {
        status,
        version: env!("CARGO_PKG_VERSION"),
        services: ServiceHealth {
            redis: redis_ok,
            postgres: postgres_ok,
            acp: true, // Always true if daemon is running
        },
    })
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
            current_task: manager.get_current_task(a.id),
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
        Ok(agent_id) => {
            // Update agent state in Redis
            update_agent_redis_state(
                &state.redis,
                agent_id,
                &role.to_string(),
                "running",
                None,
            )
            .await;

            // Publish agent status change event
            if let Some(ref redis) = state.redis {
                let msg = PubSubMessage::AgentStatusChange {
                    agent_id,
                    old_state: "none".to_string(),
                    new_state: "running".to_string(),
                };
                let _ = redis.pubsub.publish_agent(&msg).await;
            }

            Json(serde_json::json!({
                "agent_id": agent_id.to_string(),
                "role": role.to_string(),
                "status": "running"
            }))
        }
        Err(e) => Json(serde_json::json!({
            "error": format!("Failed to spawn agent: {}", e)
        })),
    }
}

/// Send a message to an agent (uses task/print mode for reliable execution)
/// Uses non-blocking pattern to avoid holding lock during Claude Code execution
async fn send_to_agent(
    State(state): State<DaemonState>,
    Path(agent_id): Path<String>,
    Json(request): Json<SendToAgentRequest>,
) -> Json<SendToAgentResponse> {
    let start = std::time::Instant::now();

    // Validate input size
    if request.message.len() > MAX_TASK_DESCRIPTION_LEN {
        return Json(SendToAgentResponse {
            success: false,
            output: None,
            error: Some(format!(
                "Message too large: {} bytes (max: {} bytes)",
                request.message.len(),
                MAX_TASK_DESCRIPTION_LEN
            )),
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // Parse agent ID
    let agent_id = match Uuid::parse_str(&agent_id) {
        Ok(uuid) => AgentId(uuid),
        Err(_) => {
            return Json(SendToAgentResponse {
                success: false,
                output: None,
                error: Some(format!("Invalid agent ID: {}", agent_id)),
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }
    };

    // Step 1: Briefly acquire lock to prepare task (get config, set current task)
    let config = {
        let mut manager = state.agent_manager.write().await;
        match manager.prepare_task(agent_id, &request.message) {
            Ok(cfg) => cfg,
            Err(e) => {
                return Json(SendToAgentResponse {
                    success: false,
                    output: None,
                    error: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        }
    }; // Lock released here

    info!(
        "Sending task to {} agent {}: {}",
        config.role,
        agent_id,
        if request.message.len() > 100 {
            &request.message[..100]
        } else {
            &request.message
        }
    );

    warn!(
        "Agent {} running with --dangerously-skip-permissions. \
         Ensure environment is properly sandboxed.",
        agent_id
    );

    // Step 2: Execute Claude Code WITHOUT holding the lock
    let timeout = std::time::Duration::from_secs(request.timeout_seconds);
    let result = tokio::time::timeout(timeout, async {
        tokio::process::Command::new(&config.claude_path)
            .arg("--dangerously-skip-permissions")
            .arg("--print")
            .arg("--output-format")
            .arg("text")
            .arg(&request.message)
            .env("CLAUDE_MD", &config.claude_md_path)
            .env("NO_COLOR", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?
            .wait_with_output()
            .await
            .map_err(|e| e.to_string())
    })
    .await;

    // Step 3: Briefly acquire lock to record result
    match result {
        Ok(Ok(output)) if output.status.success() => {
            let response = String::from_utf8_lossy(&output.stdout).to_string();
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(agent_id, true, &response, None);
            }
            info!("Message sent to agent {} successfully", agent_id);
            Json(SendToAgentResponse {
                success: true,
                output: Some(response),
                error: None,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(agent_id, false, "", Some(&stderr));
            }
            error!("Failed to send message to agent {}: {}", agent_id, stderr);
            Json(SendToAgentResponse {
                success: false,
                output: None,
                error: Some(format!("Claude Code failed: {}", stderr)),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
        Ok(Err(e)) => {
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(agent_id, false, "", Some(&e));
            }
            error!("Failed to send message to agent {}: {}", agent_id, e);
            Json(SendToAgentResponse {
                success: false,
                output: None,
                error: Some(e),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
        Err(_) => {
            error!("Timeout sending message to agent {}", agent_id);
            {
                let mut manager = state.agent_manager.write().await;
                manager.add_log(agent_id, "ERROR", &format!("Task timed out after {} seconds", request.timeout_seconds));
                manager.clear_current_task(agent_id);
            }
            Json(SendToAgentResponse {
                success: false,
                output: None,
                error: Some(format!(
                    "Timeout after {} seconds",
                    request.timeout_seconds
                )),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }
}

/// Start an interactive PTY session for an agent (for attach functionality)
async fn start_agent_session(
    State(state): State<DaemonState>,
    Path(agent_id): Path<String>,
) -> Json<serde_json::Value> {
    // Parse agent ID
    let agent_id = match Uuid::parse_str(&agent_id) {
        Ok(uuid) => AgentId(uuid),
        Err(_) => {
            return Json(serde_json::json!({
                "success": false,
                "error": format!("Invalid agent ID: {}", agent_id)
            }));
        }
    };

    // Start interactive session
    let mut manager = state.agent_manager.write().await;
    match manager.start_interactive_session(agent_id).await {
        Ok(()) => {
            info!("Started interactive session for agent {}", agent_id);
            Json(serde_json::json!({
                "success": true,
                "agent_id": agent_id.to_string(),
                "message": "Interactive session started"
            }))
        }
        Err(e) => {
            error!("Failed to start interactive session for agent {}: {}", agent_id, e);
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

/// Query parameters for logs endpoint
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    #[serde(default = "default_log_lines")]
    lines: usize,
}

fn default_log_lines() -> usize {
    50
}

/// Get logs for a specific agent
async fn get_agent_logs(
    State(state): State<DaemonState>,
    Path(agent_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<LogsQuery>,
) -> Json<serde_json::Value> {
    // Parse agent ID
    let agent_id = match Uuid::parse_str(&agent_id) {
        Ok(uuid) => AgentId(uuid),
        Err(_) => {
            return Json(serde_json::json!({
                "error": format!("Invalid agent ID: {}", agent_id)
            }));
        }
    };

    let manager = state.agent_manager.read().await;
    let logs = manager.get_logs(agent_id, query.lines);

    let log_entries: Vec<serde_json::Value> = logs
        .iter()
        .map(|entry| {
            serde_json::json!({
                "timestamp": entry.timestamp.to_rfc3339(),
                "level": entry.level,
                "message": entry.message
            })
        })
        .collect();

    Json(serde_json::json!({
        "agent_id": agent_id.to_string(),
        "logs": log_entries
    }))
}

/// Delegate a task to a specialist agent
/// This endpoint is used by the coordinator to delegate tasks to sub-agents
async fn delegate_task(
    State(state): State<DaemonState>,
    Json(request): Json<DelegateTaskRequest>,
) -> Json<DelegateTaskResponse> {
    let start = std::time::Instant::now();

    // Parse role
    let role = match request.role.to_lowercase().as_str() {
        "frontend" => AgentRole::Frontend,
        "backend" => AgentRole::Backend,
        "dba" => AgentRole::DBA,
        "devops" => AgentRole::DevOps,
        "security" => AgentRole::Security,
        "qa" => AgentRole::QA,
        _ => {
            return Json(DelegateTaskResponse {
                success: false,
                agent_id: String::new(),
                role: request.role.clone(),
                output: None,
                error: Some(format!("Unknown agent role: {}. Valid roles: frontend, backend, dba, devops, security, qa", request.role)),
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }
    };

    info!("Delegating task to {} agent: {}", request.role, request.task);

    // Find existing agent with this role or spawn a new one
    let agent_id = {
        let manager = state.agent_manager.read().await;
        manager
            .list()
            .iter()
            .find(|a| a.role == role)
            .map(|a| a.id)
    };

    let agent_id = match agent_id {
        Some(id) => {
            debug!("Found existing {} agent: {}", request.role, id);
            id
        }
        None => {
            // Spawn new agent
            info!("No {} agent found, spawning new one", request.role);
            let mut manager = state.agent_manager.write().await;
            match manager.spawn(role.clone()).await {
                Ok(id) => {
                    info!("Spawned new {} agent: {}", request.role, id);
                    id
                }
                Err(e) => {
                    return Json(DelegateTaskResponse {
                        success: false,
                        agent_id: String::new(),
                        role: request.role.clone(),
                        output: None,
                        error: Some(format!("Failed to spawn {} agent: {}", request.role, e)),
                        duration_ms: start.elapsed().as_millis() as u64,
                    });
                }
            }
        }
    };

    // Prepare the full message (task + context if provided)
    let message = if let Some(ref ctx) = request.context {
        format!("{}\n\nContext:\n{}", request.task, ctx)
    } else {
        request.task.clone()
    };

    // Step 1: Briefly acquire lock to prepare task
    let config = {
        let mut manager = state.agent_manager.write().await;
        match manager.prepare_task(agent_id, &message) {
            Ok(cfg) => cfg,
            Err(e) => {
                return Json(DelegateTaskResponse {
                    success: false,
                    agent_id: agent_id.to_string(),
                    role: request.role.clone(),
                    output: None,
                    error: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        }
    }; // Lock released here

    info!(
        "Sending task to {} agent {}: {}",
        config.role,
        agent_id,
        if message.len() > 100 { &message[..100] } else { &message }
    );

    warn!(
        "Agent {} running with --dangerously-skip-permissions. \
         Ensure environment is properly sandboxed.",
        agent_id
    );

    // Step 2: Execute Claude Code WITHOUT holding the lock
    let timeout = std::time::Duration::from_secs(request.timeout_seconds);
    let result = tokio::time::timeout(timeout, async {
        tokio::process::Command::new(&config.claude_path)
            .arg("--dangerously-skip-permissions")
            .arg("--print")
            .arg("--output-format")
            .arg("text")
            .arg(&message)
            .env("CLAUDE_MD", &config.claude_md_path)
            .env("NO_COLOR", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?
            .wait_with_output()
            .await
            .map_err(|e| e.to_string())
    })
    .await;

    // Step 3: Briefly acquire lock to record result
    match result {
        Ok(Ok(output)) if output.status.success() => {
            let response = String::from_utf8_lossy(&output.stdout).to_string();
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(agent_id, true, &response, None);
            }
            info!("Task completed by {} agent in {}ms", request.role, start.elapsed().as_millis());
            Json(DelegateTaskResponse {
                success: true,
                agent_id: agent_id.to_string(),
                role: request.role.clone(),
                output: Some(response),
                error: None,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(agent_id, false, "", Some(&stderr));
            }
            warn!("Task failed for {} agent: {}", request.role, stderr);
            Json(DelegateTaskResponse {
                success: false,
                agent_id: agent_id.to_string(),
                role: request.role.clone(),
                output: None,
                error: Some(format!("Agent error: {}", stderr)),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
        Ok(Err(e)) => {
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(agent_id, false, "", Some(&e));
            }
            warn!("Task failed for {} agent: {}", request.role, e);
            Json(DelegateTaskResponse {
                success: false,
                agent_id: agent_id.to_string(),
                role: request.role.clone(),
                output: None,
                error: Some(format!("Agent error: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
        Err(_) => {
            warn!("Task timeout for {} agent after {}s", request.role, request.timeout_seconds);
            {
                let mut manager = state.agent_manager.write().await;
                manager.add_log(agent_id, "ERROR", &format!("Task timed out after {} seconds", request.timeout_seconds));
                manager.clear_current_task(agent_id);
            }
            Json(DelegateTaskResponse {
                success: false,
                agent_id: agent_id.to_string(),
                role: request.role.clone(),
                output: None,
                error: Some(format!("Timeout after {} seconds", request.timeout_seconds)),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
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
    // Input validation: check description length
    if request.description.len() > MAX_TASK_DESCRIPTION_LEN {
        return Json(TaskResponse {
            task_id: String::new(),
            status: "error".to_string(),
            output: None,
            error: Some(format!(
                "Description too long: {} bytes (max: {} bytes)",
                request.description.len(),
                MAX_TASK_DESCRIPTION_LEN
            )),
            assigned_agent: None,
        });
    }

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

    // Step 1: Find or spawn a Coordinator (brief locks only)
    let (coordinator_id, config) = {
        // Check for existing coordinator
        let existing_id = {
            let manager = state.agent_manager.read().await;
            manager
                .list()
                .iter()
                .find(|a| matches!(a.role, AgentRole::Coordinator))
                .map(|a| a.id)
        };

        let coordinator_id = match existing_id {
            Some(id) => id,
            None => {
                // Spawn new coordinator
                let mut manager = state.agent_manager.write().await;
                match manager.spawn(AgentRole::Coordinator).await {
                    Ok(id) => id,
                    Err(e) => {
                        let error_msg = format!("Failed to spawn Coordinator: {e}");
                        {
                            let mut tasks = state.tasks.write().await;
                            if let Some(task) = tasks.get_mut(&task_id) {
                                task.status = "failed".to_string();
                                task.error = Some(error_msg.clone());
                                task.updated_at = Utc::now();
                            }
                        }
                        return Json(TaskResponse {
                            task_id,
                            status: "failed".to_string(),
                            output: None,
                            error: Some(error_msg),
                            assigned_agent: None,
                        });
                    }
                }
            }
        };

        // Prepare task (brief write lock)
        let mut config = {
            let mut manager = state.agent_manager.write().await;
            match manager.prepare_task(coordinator_id, &request.description) {
                Ok(cfg) => cfg,
                Err(e) => {
                    let error_msg = e.to_string();
                    {
                        let mut tasks = state.tasks.write().await;
                        if let Some(task) = tasks.get_mut(&task_id) {
                            task.status = "failed".to_string();
                            task.error = Some(error_msg.clone());
                            task.updated_at = Utc::now();
                        }
                    }
                    return Json(TaskResponse {
                        task_id,
                        status: "failed".to_string(),
                        output: None,
                        error: Some(error_msg),
                        assigned_agent: None,
                    });
                }
            }
        };

        // Set coordinator system prompt to enforce JSON delegation output
        config.system_prompt = Some(COORDINATOR_SYSTEM_PROMPT.to_string());

        (coordinator_id, config)
    }; // All locks released here

    // Update task to running status
    {
        let mut tasks = state.tasks.write().await;
        if let Some(task) = tasks.get_mut(&task_id) {
            task.status = "running".to_string();
            task.assigned_agent = Some(coordinator_id.to_string());
            task.updated_at = Utc::now();
        }
    }

    info!(
        "Sending task to {} agent {}: {}",
        config.role,
        coordinator_id,
        if request.description.len() > 100 { &request.description[..100] } else { &request.description }
    );

    warn!(
        "Agent {} running with --dangerously-skip-permissions. \
         Ensure environment is properly sandboxed.",
        coordinator_id
    );

    // Step 2: Execute Claude Code (Coordinator) WITHOUT holding any locks
    let mut cmd = tokio::process::Command::new(&config.claude_path);
    cmd.arg("--dangerously-skip-permissions")
        .arg("--print")
        .arg("--output-format")
        .arg("text");

    // Add system prompt if provided (critical for coordinator)
    if let Some(ref system_prompt) = config.system_prompt {
        cmd.arg("--system-prompt").arg(system_prompt);
    }

    cmd.arg(&request.description)
        .env("CLAUDE_MD", &config.claude_md_path)
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let result = cmd.spawn()
        .map_err(|e| e.to_string())
        .and_then(|child| Ok(child));

    let result = match result {
        Ok(child) => child.wait_with_output().await.map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };

    // Step 3: Process coordinator's response
    match result {
        Ok(output) if output.status.success() => {
            let coordinator_output = String::from_utf8_lossy(&output.stdout).to_string();

            info!(
                "Coordinator {} returned output ({} bytes) for task {}",
                coordinator_id,
                coordinator_output.len(),
                task_id
            );

            // Record coordinator result
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(coordinator_id, true, &coordinator_output, None);
            }

            // Try to parse coordinator's JSON response
            // Extract JSON from output (coordinator might include markdown or other text)
            let json_str = extract_json_from_output(&coordinator_output);

            match json_str.and_then(|s| serde_json::from_str::<CoordinatorResponse>(&s).ok()) {
                Some(coord_response) => {
                    info!("Coordinator decision: action={}, summary={:?}",
                          coord_response.action, coord_response.summary);

                    match coord_response.action.as_str() {
                        "delegate" => {
                            // Execute delegations to specialist agents
                            let delegation_results = execute_delegations(
                                &state,
                                &coord_response.delegations,
                            ).await;

                            // Aggregate results
                            let mut combined_output = String::new();
                            let mut all_success = true;
                            let mut errors = Vec::new();

                            if let Some(summary) = &coord_response.summary {
                                combined_output.push_str(&format!("## Coordinator Summary\n{}\n\n", summary));
                            }

                            for (delegation, result) in coord_response.delegations.iter().zip(delegation_results.iter()) {
                                combined_output.push_str(&format!("## {} Agent\n", delegation.role));
                                if result.success {
                                    if let Some(ref out) = result.output {
                                        combined_output.push_str(out);
                                    }
                                } else {
                                    all_success = false;
                                    if let Some(ref err) = result.error {
                                        errors.push(format!("{}: {}", delegation.role, err));
                                        combined_output.push_str(&format!("Error: {}\n", err));
                                    }
                                }
                                combined_output.push_str("\n\n");
                            }

                            // Update task state
                            {
                                let mut tasks = state.tasks.write().await;
                                if let Some(task) = tasks.get_mut(&task_id) {
                                    task.status = if all_success { "completed" } else { "partial" }.to_string();
                                    task.output = Some(combined_output.clone());
                                    if !errors.is_empty() {
                                        task.error = Some(errors.join("; "));
                                    }
                                    task.updated_at = Utc::now();
                                }
                            }

                            info!(
                                "Task {} {}: {} delegation(s), {} succeeded, {} failed",
                                task_id,
                                if all_success { "completed" } else { "partially completed" },
                                coord_response.delegations.len(),
                                delegation_results.iter().filter(|r| r.success).count(),
                                delegation_results.iter().filter(|r| !r.success).count()
                            );

                            // Publish event
                            publish_task_event(
                                &state.redis,
                                PubSubMessage::TaskCompleted {
                                    task_id: TaskId::new(),
                                    agent_id: coordinator_id,
                                    success: all_success,
                                },
                            )
                            .await;

                            Json(TaskResponse {
                                task_id,
                                status: if all_success { "completed" } else { "partial" }.to_string(),
                                output: Some(combined_output),
                                error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
                                assigned_agent: Some(coordinator_id.to_string()),
                            })
                        }
                        "direct" => {
                            // Coordinator should NOT handle tasks directly - warn and treat as error
                            warn!("Coordinator attempted direct response instead of delegating - this violates coordination rules");

                            let error_msg = "Coordinator error: attempted to handle task directly instead of delegating to specialists. Tasks must be delegated.";

                            // Update task state
                            {
                                let mut tasks = state.tasks.write().await;
                                if let Some(task) = tasks.get_mut(&task_id) {
                                    task.status = "failed".to_string();
                                    task.error = Some(error_msg.to_string());
                                    task.updated_at = Utc::now();
                                }
                            }

                            Json(TaskResponse {
                                task_id,
                                status: "failed".to_string(),
                                output: None,
                                error: Some(error_msg.to_string()),
                                assigned_agent: Some(coordinator_id.to_string()),
                            })
                        }
                        "error" => {
                            let error_msg = coord_response.error.unwrap_or_else(|| "Unknown coordinator error".to_string());

                            // Update task state
                            {
                                let mut tasks = state.tasks.write().await;
                                if let Some(task) = tasks.get_mut(&task_id) {
                                    task.status = "failed".to_string();
                                    task.error = Some(error_msg.clone());
                                    task.updated_at = Utc::now();
                                }
                            }

                            Json(TaskResponse {
                                task_id,
                                status: "failed".to_string(),
                                output: None,
                                error: Some(error_msg),
                                assigned_agent: Some(coordinator_id.to_string()),
                            })
                        }
                        _ => {
                            // Unknown action, treat as direct response
                            warn!("Unknown coordinator action: {}, treating as direct", coord_response.action);

                            // Update task state
                            {
                                let mut tasks = state.tasks.write().await;
                                if let Some(task) = tasks.get_mut(&task_id) {
                                    task.status = "completed".to_string();
                                    task.output = Some(coordinator_output.clone());
                                    task.updated_at = Utc::now();
                                }
                            }

                            Json(TaskResponse {
                                task_id,
                                status: "completed".to_string(),
                                output: Some(coordinator_output),
                                error: None,
                                assigned_agent: Some(coordinator_id.to_string()),
                            })
                        }
                    }
                }
                None => {
                    // Coordinator didn't return valid JSON, treat as direct response
                    info!(
                        "Task {} completed (non-JSON coordinator response, {} bytes output)",
                        task_id,
                        coordinator_output.len()
                    );

                    // Update task state
                    {
                        let mut tasks = state.tasks.write().await;
                        if let Some(task) = tasks.get_mut(&task_id) {
                            task.status = "completed".to_string();
                            task.output = Some(coordinator_output.clone());
                            task.updated_at = Utc::now();
                        }
                    }

                    publish_task_event(
                        &state.redis,
                        PubSubMessage::TaskCompleted {
                            task_id: TaskId::new(),
                            agent_id: coordinator_id,
                            success: true,
                        },
                    )
                    .await;

                    Json(TaskResponse {
                        task_id,
                        status: "completed".to_string(),
                        output: Some(coordinator_output),
                        error: None,
                        assigned_agent: Some(coordinator_id.to_string()),
                    })
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let error_msg = format!("Claude Code failed: {}", stderr);

            error!(
                "Task {} failed: coordinator {} returned non-zero exit code: {}",
                task_id,
                coordinator_id,
                stderr.lines().next().unwrap_or("(no error message)")
            );

            // Record in agent manager
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(coordinator_id, false, "", Some(&stderr));
            }

            // Update task state
            {
                let mut tasks = state.tasks.write().await;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.status = "failed".to_string();
                    task.error = Some(error_msg.clone());
                    task.updated_at = Utc::now();
                }
            }

            Json(TaskResponse {
                task_id,
                status: "failed".to_string(),
                output: None,
                error: Some(error_msg),
                assigned_agent: Some(coordinator_id.to_string()),
            })
        }
        Err(e) => {
            error!(
                "Task {} failed: coordinator execution error: {}",
                task_id, e
            );

            // Record in agent manager
            {
                let mut manager = state.agent_manager.write().await;
                manager.record_task_result(coordinator_id, false, "", Some(&e));
            }

            // Update task state
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

/// Extract JSON object from coordinator output (may contain markdown or other text)
fn extract_json_from_output(output: &str) -> Option<String> {
    // Try to find JSON object in the output
    // Look for { ... } pattern, handling nested braces
    let trimmed = output.trim();

    // If the output starts with {, try to parse directly
    if trimmed.starts_with('{') {
        // Find matching closing brace
        let mut depth = 0;
        let mut end_pos = 0;
        for (i, c) in trimmed.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if end_pos > 0 {
            return Some(trimmed[..end_pos].to_string());
        }
    }

    // Look for ```json code blocks
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return Some(trimmed[json_start..json_start + end].trim().to_string());
        }
    }

    // Look for ``` code blocks (might not be marked as json)
    if let Some(start) = trimmed.find("```\n{") {
        let json_start = start + 4;
        if let Some(end) = trimmed[json_start..].find("```") {
            return Some(trimmed[json_start..json_start + end].trim().to_string());
        }
    }

    // Look for first { in the output
    if let Some(start) = trimmed.find('{') {
        let json_part = &trimmed[start..];
        let mut depth = 0;
        let mut end_pos = 0;
        for (i, c) in json_part.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if end_pos > 0 {
            return Some(json_part[..end_pos].to_string());
        }
    }

    None
}

/// Execute delegations to specialist agents
async fn execute_delegations(
    state: &DaemonState,
    delegations: &[CoordinatorDelegation],
) -> Vec<DelegateTaskResponse> {
    let mut results = Vec::new();

    for delegation in delegations {
        info!("Executing delegation to {}: {}", delegation.role,
              if delegation.task.len() > 50 { &delegation.task[..50] } else { &delegation.task });

        // Validate role (must be a known role)
        let _role = match delegation.role.to_lowercase().as_str() {
            "frontend" => AgentRole::Frontend,
            "backend" => AgentRole::Backend,
            "dba" => AgentRole::DBA,
            "devops" => AgentRole::DevOps,
            "security" => AgentRole::Security,
            "qa" => AgentRole::QA,
            _ => {
                results.push(DelegateTaskResponse {
                    success: false,
                    agent_id: String::new(),
                    role: delegation.role.clone(),
                    output: None,
                    error: Some(format!("Unknown role: {}", delegation.role)),
                    duration_ms: 0,
                });
                continue;
            }
        };

        let start = std::time::Instant::now();

        // Find a connected agent with this role via WebSocket
        let agent_id = state.acp_server.find_agent_by_role(&delegation.role).await;

        let agent_id = match agent_id {
            Some(id) => {
                info!("Found connected {} agent: {}", delegation.role, id);
                id
            }
            None => {
                warn!("No {} agent connected via WebSocket. Start one with: cca agent worker {}",
                      delegation.role, delegation.role);
                results.push(DelegateTaskResponse {
                    success: false,
                    agent_id: String::new(),
                    role: delegation.role.clone(),
                    output: None,
                    error: Some(format!(
                        "No {} agent connected. Start one with: cca agent worker {}",
                        delegation.role, delegation.role
                    )),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                continue;
            }
        };

        info!("Sending task to {} agent {} via WebSocket", delegation.role, agent_id);

        // Send task via WebSocket
        let timeout = std::time::Duration::from_secs(state.config.agents.default_timeout_seconds);
        let result = state.acp_server.send_task(
            agent_id,
            &delegation.task,
            delegation.context.as_deref(),
            timeout,
        ).await;

        // Record result
        match result {
            Ok(output) => {
                info!("{} agent completed task in {}ms", delegation.role, start.elapsed().as_millis());
                results.push(DelegateTaskResponse {
                    success: true,
                    agent_id: agent_id.to_string(),
                    role: delegation.role.clone(),
                    output: Some(output),
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
            Err(e) => {
                let error_msg = e.to_string();
                warn!("{} agent error: {}", delegation.role, error_msg);
                results.push(DelegateTaskResponse {
                    success: false,
                    agent_id: agent_id.to_string(),
                    role: delegation.role.clone(),
                    output: None,
                    error: Some(error_msg),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        }
    }

    results
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

    // Get activity from Redis if available, otherwise from memory
    let activity: Vec<serde_json::Value> = if let Some(ref redis) = state.redis {
        match redis.agent_states.get_all().await {
            Ok(states) => states
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "agent_id": s.agent_id.to_string(),
                        "role": s.role,
                        "status": s.state,
                        "current_task": s.current_task.map(|t| t.to_string()),
                        "last_activity": s.last_heartbeat.to_rfc3339(),
                        "tokens_used": s.tokens_used,
                        "tasks_completed": s.tasks_completed
                    })
                })
                .collect(),
            Err(_) => {
                // Fallback to in-memory
                manager
                    .list()
                    .iter()
                    .map(|a| {
                        serde_json::json!({
                            "agent_id": a.id.to_string(),
                            "role": a.role.to_string(),
                            "status": format!("{:?}", a.state),
                            "current_task": serde_json::Value::Null,
                            "last_activity": serde_json::Value::Null
                        })
                    })
                    .collect()
            }
        }
    } else {
        manager
            .list()
            .iter()
            .map(|a| {
                serde_json::json!({
                    "agent_id": a.id.to_string(),
                    "role": a.role.to_string(),
                    "status": format!("{:?}", a.state),
                    "current_task": serde_json::Value::Null,
                    "last_activity": serde_json::Value::Null
                })
            })
            .collect()
    };

    Json(serde_json::json!({
        "agents": activity
    }))
}

/// Redis status endpoint
async fn redis_status(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    match &state.redis {
        Some(redis) => {
            // Try to get stats
            let agent_count = redis.agent_states.get_all().await.map(|v| v.len()).unwrap_or(0);

            Json(serde_json::json!({
                "connected": true,
                "pool_size": state.config.redis.pool_size,
                "context_ttl_seconds": state.config.redis.context_ttl_seconds,
                "agents_tracked": agent_count
            }))
        }
        None => Json(serde_json::json!({
            "connected": false,
            "error": "Redis not available"
        })),
    }
}

/// PostgreSQL status endpoint
async fn postgres_status(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    match &state.postgres {
        Some(postgres) => {
            // Get pattern count
            let pattern_count = postgres.patterns.count().await.unwrap_or(0);

            Json(serde_json::json!({
                "connected": true,
                "pool_size": state.config.postgres.max_connections,
                "patterns_count": pattern_count
                // SECURITY: Database URL intentionally omitted - never expose connection strings
            }))
        }
        None => Json(serde_json::json!({
            "connected": false,
            "error": "PostgreSQL not available"
        })),
    }
}

/// Memory search request
#[derive(Debug, Clone, Deserialize)]
pub struct MemorySearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

fn default_limit() -> i32 {
    10
}

/// Memory search endpoint - query ReasoningBank patterns
async fn memory_search(
    State(state): State<DaemonState>,
    Json(request): Json<MemorySearchRequest>,
) -> Json<serde_json::Value> {
    // Input validation: check query length
    if request.query.len() > MAX_QUERY_LEN {
        return Json(serde_json::json!({
            "success": false,
            "error": format!(
                "Query too long: {} bytes (max: {} bytes)",
                request.query.len(),
                MAX_QUERY_LEN
            )
        }));
    }

    match &state.postgres {
        Some(postgres) => {
            // First try text search (embedding search would require an embedding model)
            match postgres.patterns.search_text(&request.query, request.limit).await {
                Ok(patterns) => {
                    let results: Vec<serde_json::Value> = patterns
                        .iter()
                        .map(|p| {
                            serde_json::json!({
                                "id": p.id.to_string(),
                                "pattern_type": p.pattern_type,
                                "content": p.content,
                                "success_rate": p.success_rate,
                                "success_count": p.success_count,
                                "failure_count": p.failure_count,
                                "created_at": p.created_at.to_rfc3339()
                            })
                        })
                        .collect();

                    Json(serde_json::json!({
                        "success": true,
                        "patterns": results,
                        "count": results.len(),
                        "query": request.query
                    }))
                }
                Err(e) => Json(serde_json::json!({
                    "success": false,
                    "error": format!("Failed to search patterns: {}", e)
                })),
            }
        }
        None => Json(serde_json::json!({
            "success": false,
            "error": "PostgreSQL not available"
        })),
    }
}

/// ACP WebSocket status endpoint
async fn acp_status(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let agents_with_roles = state.acp_server.agents_with_roles().await;
    let connection_count = state.acp_server.connection_count().await;

    let workers: Vec<serde_json::Value> = agents_with_roles
        .iter()
        .map(|(id, role)| {
            serde_json::json!({
                "agent_id": id.to_string(),
                "role": role.clone().unwrap_or_else(|| "unregistered".to_string())
            })
        })
        .collect();

    Json(serde_json::json!({
        "running": true,
        "port": state.config.acp.websocket_port,
        "connected_agents": connection_count,
        "workers": workers
    }))
}

/// Broadcast request
#[derive(Debug, Clone, Deserialize)]
pub struct BroadcastRequest {
    pub message: String,
}

/// Pub/Sub broadcast endpoint
async fn pubsub_broadcast(
    State(state): State<DaemonState>,
    Json(request): Json<BroadcastRequest>,
) -> Json<serde_json::Value> {
    // Input validation: check message length
    if request.message.len() > MAX_BROADCAST_MESSAGE_LEN {
        return Json(serde_json::json!({
            "success": false,
            "error": format!(
                "Message too long: {} bytes (max: {} bytes)",
                request.message.len(),
                MAX_BROADCAST_MESSAGE_LEN
            )
        }));
    }

    match &state.redis {
        Some(redis) => {
            let msg = PubSubMessage::Broadcast {
                from: AgentId::new(), // System broadcast
                message: request.message.clone(),
            };

            match redis.pubsub.broadcast(&msg).await {
                Ok(()) => Json(serde_json::json!({
                    "success": true,
                    "message": "Broadcast sent"
                })),
                Err(e) => Json(serde_json::json!({
                    "success": false,
                    "error": format!("Failed to broadcast: {}", e)
                })),
            }
        }
        None => Json(serde_json::json!({
            "success": false,
            "error": "Redis not available"
        })),
    }
}

/// Broadcast to all agents via ACP and Redis
async fn broadcast_all(
    State(state): State<DaemonState>,
    Json(request): Json<BroadcastRequest>,
) -> Json<serde_json::Value> {
    // Input validation: check message length
    if request.message.len() > MAX_BROADCAST_MESSAGE_LEN {
        return Json(serde_json::json!({
            "success": false,
            "error": format!(
                "Message too long: {} bytes (max: {} bytes)",
                request.message.len(),
                MAX_BROADCAST_MESSAGE_LEN
            )
        }));
    }

    let mut acp_count = 0;
    let mut redis_success = false;

    // Broadcast via ACP WebSocket
    let acp_message = cca_acp::AcpMessage::notification(
        cca_acp::methods::BROADCAST,
        serde_json::json!({
            "message_type": "announcement",
            "content": { "message": request.message }
        }),
    );

    match state.acp_server.broadcast(acp_message).await {
        Ok(count) => {
            acp_count = count;
        }
        Err(e) => {
            warn!("Failed to broadcast via ACP: {}", e);
        }
    }

    // Also broadcast via Redis pub/sub
    if let Some(redis) = &state.redis {
        let msg = PubSubMessage::Broadcast {
            from: AgentId::new(),
            message: request.message.clone(),
        };

        if redis.pubsub.broadcast(&msg).await.is_ok() {
            redis_success = true;
        }
    }

    Json(serde_json::json!({
        "success": true,
        "agents_notified": acp_count,
        "message": format!("Broadcast sent to {} agents via ACP, Redis: {}", acp_count, redis_success)
    }))
}

/// Get workload distribution across agents
async fn get_workloads(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let manager = state.agent_manager.read().await;
    let tasks = state.tasks.read().await;

    let agents: Vec<serde_json::Value> = manager
        .list()
        .iter()
        .map(|a| {
            // Count tasks assigned to this agent (running or in_progress)
            let current_tasks = tasks
                .values()
                .filter(|t| {
                    t.assigned_agent
                        .as_ref()
                        .is_some_and(|id| id == &a.id.to_string())
                        && (t.status == "running" || t.status == "in_progress")
                })
                .count();

            serde_json::json!({
                "agent_id": a.id.to_string(),
                "role": a.role.to_string(),
                "current_tasks": current_tasks,
                "max_tasks": 10, // Default max
                "capabilities": []
            })
        })
        .collect();

    let total_tasks = tasks.len();
    let pending_tasks = tasks.values().filter(|t| t.status == "pending").count();

    Json(serde_json::json!({
        "agents": agents,
        "total_tasks": total_tasks,
        "pending_tasks": pending_tasks
    }))
}

/// Helper to publish task events to Redis
async fn publish_task_event(redis: &Option<Arc<RedisServices>>, msg: PubSubMessage) {
    if let Some(redis) = redis {
        if let Err(e) = redis.pubsub.publish_task(&msg).await {
            warn!("Failed to publish task event: {}", e);
        }
    }
}

/// Helper to update agent state in Redis
async fn update_agent_redis_state(
    redis: &Option<Arc<RedisServices>>,
    agent_id: AgentId,
    role: &str,
    state: &str,
    current_task: Option<TaskId>,
) {
    if let Some(redis) = redis {
        let agent_state = RedisAgentState {
            agent_id,
            role: role.to_string(),
            state: state.to_string(),
            current_task,
            tokens_used: 0,
            tasks_completed: 0,
            last_heartbeat: Utc::now(),
        };
        if let Err(e) = redis.agent_states.update(&agent_state).await {
            warn!("Failed to update agent state in Redis: {}", e);
        }
    }
}

// RL API handlers

/// Get RL statistics
async fn rl_stats(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let stats = state.rl_service.stats().await;
    Json(serde_json::json!({
        "algorithm": stats.algorithm,
        "total_steps": stats.total_steps,
        "total_rewards": stats.total_rewards,
        "average_reward": stats.average_reward,
        "buffer_size": stats.buffer_size,
        "last_training_loss": stats.last_training_loss,
        "experience_count": stats.experience_count,
        "algorithms_available": stats.algorithms_available
    }))
}

/// Trigger training on collected experiences
async fn rl_train(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    match state.rl_service.train().await {
        Ok(loss) => Json(serde_json::json!({
            "success": true,
            "loss": loss,
            "message": "Training complete"
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Training failed: {}", e)
        })),
    }
}

/// Set RL algorithm request
#[derive(Debug, Clone, Deserialize)]
pub struct SetAlgorithmRequest {
    pub algorithm: String,
}

/// Set the RL algorithm
async fn rl_set_algorithm(
    State(state): State<DaemonState>,
    Json(request): Json<SetAlgorithmRequest>,
) -> Json<serde_json::Value> {
    match state.rl_service.set_algorithm(&request.algorithm).await {
        Ok(()) => Json(serde_json::json!({
            "success": true,
            "algorithm": request.algorithm,
            "message": format!("Switched to algorithm: {}", request.algorithm)
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to set algorithm: {}", e)
        })),
    }
}

/// Get RL algorithm parameters
async fn rl_get_params(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let params = state.rl_service.get_params().await;
    Json(serde_json::json!({
        "success": true,
        "params": params
    }))
}

/// Set RL algorithm parameters
async fn rl_set_params(
    State(state): State<DaemonState>,
    Json(params): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    match state.rl_service.set_params(params).await {
        Ok(()) => Json(serde_json::json!({
            "success": true,
            "message": "Parameters updated"
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to set parameters: {}", e)
        })),
    }
}

// Token Efficiency API handlers

/// Analyze context request
#[derive(Debug, Clone, Deserialize)]
pub struct AnalyzeContextRequest {
    pub content: String,
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// Analyze context for token usage and compression potential
async fn tokens_analyze(
    State(state): State<DaemonState>,
    Json(request): Json<AnalyzeContextRequest>,
) -> Json<serde_json::Value> {
    // Input validation: check content length
    if request.content.len() > MAX_CONTENT_LEN {
        return Json(serde_json::json!({
            "success": false,
            "error": format!(
                "Content too long: {} bytes (max: {} bytes)",
                request.content.len(),
                MAX_CONTENT_LEN
            )
        }));
    }

    let analysis = state.token_service.analyzer.analyze(&request.content);

    Json(serde_json::json!({
        "success": true,
        "total_tokens": analysis.total_tokens,
        "repeated_tokens": analysis.repeated_tokens,
        "code_blocks": analysis.code_block_count,
        "long_lines": analysis.long_line_count,
        "compression_potential": format!("{:.1}%", analysis.compression_potential * 100.0),
        "repeated_lines": analysis.repeated_lines.len()
    }))
}

/// Compress context request
#[derive(Debug, Clone, Deserialize)]
pub struct CompressContextRequest {
    pub content: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default = "default_compress")]
    pub compress_code: bool,
    #[serde(default)]
    pub target_reduction: Option<f64>,
}

fn default_compress() -> bool {
    true
}

/// Compress context to reduce token usage
async fn tokens_compress(
    State(state): State<DaemonState>,
    Json(request): Json<CompressContextRequest>,
) -> Json<serde_json::Value> {
    // Input validation: check content length
    if request.content.len() > MAX_CONTENT_LEN {
        return Json(serde_json::json!({
            "success": false,
            "error": format!(
                "Content too long: {} bytes (max: {} bytes)",
                request.content.len(),
                MAX_CONTENT_LEN
            )
        }));
    }

    let original_tokens = state.token_service.counter.count(&request.content);

    // Apply compression strategies
    let mut compressed = request.content.clone();

    // Compress code blocks if requested
    if request.compress_code {
        compressed = state.token_service.compressor.compress_code(&compressed);
    }

    // Apply summarization if target reduction specified
    if let Some(target) = request.target_reduction {
        if target > 0.0 && target < 1.0 {
            compressed = state.token_service.compressor.summarize(&compressed, target);
        }
    }

    let final_tokens = state.token_service.counter.count(&compressed);
    let tokens_saved = original_tokens.saturating_sub(final_tokens);
    let reduction = if original_tokens > 0 {
        (tokens_saved as f64 / original_tokens as f64) * 100.0
    } else {
        0.0
    };

    // Record savings if agent_id provided
    if let Some(agent_id_str) = &request.agent_id {
        if let Ok(agent_id) = agent_id_str.parse::<Uuid>() {
            state.token_service.metrics.record_savings(AgentId(agent_id), tokens_saved).await;
        }
    }

    Json(serde_json::json!({
        "success": true,
        "original_tokens": original_tokens,
        "final_tokens": final_tokens,
        "tokens_saved": tokens_saved,
        "reduction": format!("{:.1}%", reduction),
        "compressed_content": compressed
    }))
}

/// Get token metrics for all agents
async fn tokens_metrics(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let summary = state.token_service.get_efficiency_summary().await;
    let agent_metrics = state.token_service.metrics.get_all_metrics().await;

    let agents: Vec<serde_json::Value> = agent_metrics
        .iter()
        .map(|(id, m)| {
            serde_json::json!({
                "agent_id": id.to_string(),
                "total_input": m.total_input,
                "total_output": m.total_output,
                "total_context": m.total_context,
                "message_count": m.message_count,
                "avg_input_per_message": m.avg_input_per_message,
                "avg_output_per_message": m.avg_output_per_message,
                "peak_context_size": m.peak_context_size,
                "compression_savings": m.compression_savings
            })
        })
        .collect();

    Json(serde_json::json!({
        "success": true,
        "summary": {
            "total_tokens_used": summary.total_tokens_used,
            "total_tokens_saved": summary.total_tokens_saved,
            "compression_ratio": format!("{:.1}%", summary.compression_ratio * 100.0),
            "agents_tracked": summary.agents_tracked,
            "target_reduction": format!("{:.0}%", summary.target_reduction * 100.0),
            "current_reduction": format!("{:.1}%", summary.current_reduction * 100.0),
            "on_track": summary.on_track
        },
        "agents": agents
    }))
}

/// Get token efficiency recommendations
async fn tokens_recommendations(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let recommendations = state.token_service.metrics.get_recommendations().await;

    let recs: Vec<serde_json::Value> = recommendations
        .iter()
        .map(|r| {
            serde_json::json!({
                "agent_id": r.agent_id.to_string(),
                "category": r.category,
                "severity": r.severity,
                "message": r.message,
                "potential_savings": r.potential_savings
            })
        })
        .collect();

    let total_potential: u32 = recommendations.iter().map(|r| r.potential_savings).sum();

    Json(serde_json::json!({
        "success": true,
        "recommendations": recs,
        "count": recommendations.len(),
        "total_potential_savings": total_potential
    }))
}
