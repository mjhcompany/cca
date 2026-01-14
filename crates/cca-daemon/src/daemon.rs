//! Main CCA Daemon implementation
//!
//! Note: Some fields in structs are infrastructure for future features.
#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Method};
use axum::{
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use cca_acp::AcpServer;
use cca_core::{AgentRole, AgentId, TaskId};
use cca_core::util::safe_truncate;
use cca_rl::{Action, Experience, State as RLState, state::AgentState as RLAgentState};

use crate::rl::compute_reward;

use crate::agent_manager::{AgentManager, apply_permissions_to_command};
use crate::auth::{
    auth_middleware, create_rate_limiter_state, rate_limit_middleware,
    AuthConfig, RateLimitConfig,
};
use crate::config::Config;
use crate::orchestrator::Orchestrator;
use crate::postgres::PostgresServices;
use crate::redis::{PubSubMessage, RedisAgentState, RedisServices};
use crate::rl::{RLConfig, RLService};
use crate::tokens::TokenService;
use crate::embeddings::{EmbeddingConfig, EmbeddingService};
use crate::indexing::{IndexingService, StartIndexingRequest};

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
    pub tmux_manager: Arc<crate::tmux::TmuxManager>,
    /// Track which agents are currently busy with a task
    pub busy_agents: Arc<RwLock<HashMap<AgentId, String>>>,
    /// Cached health check result - PERF-003
    health_cache: Arc<RwLock<Option<CachedHealthCheck>>>,
    /// Embedding service for semantic search (optional, requires Ollama)
    pub embedding_service: Option<Arc<EmbeddingService>>,
    /// Indexing service for codebase indexing (optional, requires embeddings + postgres)
    pub indexing_service: Option<Arc<IndexingService>>,
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

        // Initialize ACP WebSocket server with authentication
        let acp_addr: SocketAddr = format!("127.0.0.1:{}", config.acp.websocket_port)
            .parse()
            .map_err(|e| anyhow::anyhow!(
                "Invalid ACP address '127.0.0.1:{}': {}",
                config.acp.websocket_port,
                e
            ))?;

        // Convert api_key_configs to ApiKeyMetadata for role-based authorization
        let api_key_metadata: Vec<cca_acp::ApiKeyMetadata> = config
            .daemon
            .api_key_configs
            .iter()
            .map(|cfg| cca_acp::ApiKeyMetadata {
                key: cfg.key.clone(),
                allowed_roles: cfg.allowed_roles.clone(),
                key_id: cfg.key_id.clone(),
            })
            .collect();

        let acp_auth_config = cca_acp::AcpAuthConfig {
            api_keys: config.daemon.api_keys.clone(),
            api_key_metadata,
            require_auth: config.daemon.is_auth_required(),
        };
        let acp_server = Arc::new(AcpServer::with_auth(acp_addr, acp_auth_config));
        info!(
            "ACP server configured on port {} (auth: {})",
            config.acp.websocket_port,
            if config.daemon.is_auth_required() { "enabled" } else { "disabled" }
        );

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

        // Initialize Tmux manager for auto-spawning agents
        let tmux_manager = Arc::new(crate::tmux::TmuxManager::new());
        if tmux_manager.is_available() {
            info!("Tmux auto-spawn enabled (max {} agents)", crate::tmux::MAX_AUTO_AGENTS);
        }

        // Initialize Embedding service for semantic search (optional)
        let embedding_service = if config.embeddings.enabled {
            let emb_config = EmbeddingConfig {
                ollama_url: config.embeddings.ollama_url.clone(),
                model: config.embeddings.model.clone(),
                dimension: config.embeddings.dimension,
            };
            let service = EmbeddingService::new(emb_config);
            info!(
                "Embedding service enabled: {} ({}d)",
                config.embeddings.model, config.embeddings.dimension
            );
            Some(Arc::new(service))
        } else {
            info!("Embedding service disabled (enable via embeddings.enabled=true)");
            None
        };

        // Initialize Indexing service for codebase semantic search (requires embeddings + postgres)
        let indexing_service = match (&embedding_service, &postgres) {
            (Some(emb_svc), Some(pg_svc)) if config.indexing.enabled => {
                let service = IndexingService::new(
                    config.indexing.clone(),
                    Arc::clone(emb_svc),
                    Arc::clone(pg_svc),
                );
                info!("Codebase indexing service enabled");
                Some(Arc::new(service))
            }
            _ => {
                if config.indexing.enabled {
                    info!(
                        "Codebase indexing disabled (requires embeddings + postgres)"
                    );
                } else {
                    debug!("Codebase indexing disabled via config");
                }
                None
            }
        };

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
            tmux_manager,
            busy_agents: Arc::new(RwLock::new(HashMap::new())),
            health_cache: Arc::new(RwLock::new(None)),
            embedding_service,
            indexing_service,
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

        // Start task cleanup background job (STABILITY: prevent unbounded task HashMap growth)
        let tasks_ref = self.state.tasks.clone();
        let cleanup_task = tokio::spawn(async move {
            task_cleanup_job(tasks_ref).await;
        });

        // Serve HTTP API with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.recv().await;
            })
            .await?;

        // Shutdown ACP server
        self.state.acp_server.shutdown();
        acp_task.abort();
        cleanup_task.abort();

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

        // Cleanup auto-spawned tmux agents
        self.state.tmux_manager.cleanup().await;

        info!("Daemon shutdown complete");
        Ok(())
    }
}

/// Create the API router with state
/// SEC-004: Includes per-IP rate limiting middleware for DoS protection
fn create_router(state: DaemonState) -> Router {
    // Create auth config from daemon config
    // SECURITY: Use is_auth_required() which enforces auth in production builds
    let auth_config = AuthConfig {
        api_keys: state.config.daemon.api_keys.clone(),
        required: state.config.daemon.is_auth_required(),
    };

    // SEC-004: Create per-IP and per-API-key rate limiter from config
    let rate_limit_config = RateLimitConfig {
        requests_per_second: state.config.daemon.rate_limit_rps,
        burst_size: state.config.daemon.rate_limit_burst,
        global_rps: state.config.daemon.rate_limit_global_rps,
        trust_proxy: state.config.daemon.rate_limit_trust_proxy,
        api_key_rps: state.config.daemon.rate_limit_api_key_rps,
        api_key_burst: state.config.daemon.rate_limit_api_key_burst,
    };
    let rate_limiter = create_rate_limiter_state(&rate_limit_config);

    let mut router = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(prometheus_metrics))
        .route("/api/v1/health", get(health_check))
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
        .route("/api/v1/memory/backfill-embeddings", post(backfill_embeddings))
        // Codebase indexing endpoints
        .route("/api/v1/memory/index", post(start_indexing))
        .route("/api/v1/memory/index/:job_id", get(get_indexing_status))
        .route("/api/v1/memory/index/:job_id/cancel", post(cancel_indexing))
        .route("/api/v1/memory/index/jobs", get(list_indexing_jobs))
        .route("/api/v1/code/search", post(search_code))
        .route("/api/v1/code/stats", get(code_stats))
        .route("/api/v1/pubsub/broadcast", post(pubsub_broadcast))
        .route("/api/v1/acp/status", get(acp_status))
        .route("/api/v1/acp/disconnect", post(acp_disconnect))
        .route("/api/v1/acp/send", post(acp_send_task))
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
        .layer(axum::middleware::from_fn_with_state(auth_config, auth_middleware));

    // SEC-004: Apply per-IP and per-API-key rate limiting if configured (rate_limit_rps > 0)
    if state.config.daemon.rate_limit_rps > 0 {
        info!(
            "Rate limiting enabled: {} req/s per IP (burst: {}), {} req/s per API key (burst: {}), global: {} req/s",
            rate_limit_config.requests_per_second,
            rate_limit_config.burst_size,
            rate_limit_config.api_key_rps,
            rate_limit_config.api_key_burst,
            rate_limit_config.global_rps
        );
        router = router.layer(axum::middleware::from_fn_with_state(rate_limiter, rate_limit_middleware));
    }

    // SEC-010: Apply CORS middleware if origins are configured
    let cors_origins = &state.config.daemon.cors_origins;
    if !cors_origins.is_empty() {
        let cors = build_cors_layer(
            cors_origins,
            state.config.daemon.cors_allow_credentials,
            state.config.daemon.cors_max_age_secs,
        );
        info!(
            "CORS enabled for {} origin(s), credentials: {}, max_age: {}s",
            cors_origins.len(),
            state.config.daemon.cors_allow_credentials,
            state.config.daemon.cors_max_age_secs
        );
        router = router.layer(cors);
    } else {
        debug!("CORS disabled (no origins configured)");
    }

    router.with_state(state)
}

/// SEC-010: Build CORS layer with explicit allowed origins configuration
///
/// SECURITY: This function enforces secure CORS defaults:
/// - Only allows explicitly configured origins (no wildcards in production)
/// - Restricts allowed methods to safe API operations (GET, POST, OPTIONS)
/// - Restricts allowed headers to standard API headers
/// - Warns if credentials are enabled with wildcard origins
fn build_cors_layer(
    origins: &[String],
    allow_credentials: bool,
    max_age_secs: u64,
) -> CorsLayer {
    // SEC-010: Check for wildcard origin - warn if credentials enabled
    let has_wildcard = origins.iter().any(|o| o == "*");
    if has_wildcard && allow_credentials {
        warn!(
            "SEC-010: CORS credentials enabled with wildcard origin '*' is insecure! \
             Credentials will be DISABLED. Use explicit origins instead."
        );
    }

    // Build allowed origins
    let allow_origin = if has_wildcard {
        // Wildcard - for development only
        warn!("SEC-010: Using wildcard CORS origin '*' - this should only be used in development!");
        AllowOrigin::any()
    } else {
        // Explicit origins - parse and validate each one
        let parsed_origins: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|origin| {
                match origin.parse::<HeaderValue>() {
                    Ok(hv) => Some(hv),
                    Err(e) => {
                        warn!("SEC-010: Invalid CORS origin '{}': {}", origin, e);
                        None
                    }
                }
            })
            .collect();

        if parsed_origins.is_empty() {
            warn!("SEC-010: No valid CORS origins after parsing, using restrictive default");
            AllowOrigin::exact("https://localhost".parse().unwrap())
        } else {
            AllowOrigin::list(parsed_origins)
        }
    };

    // Build the CORS layer with secure defaults
    let mut cors = CorsLayer::new()
        .allow_origin(allow_origin)
        // SEC-010: Only allow safe HTTP methods for API
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        // SEC-010: Allow standard API headers
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::header::ACCEPT,
            axum::http::header::ORIGIN,
            // Custom header for API key auth
            axum::http::header::HeaderName::from_static("x-api-key"),
        ])
        // SEC-010: Cache preflight requests
        .max_age(std::time::Duration::from_secs(max_age_secs));

    // SEC-010: Only allow credentials with explicit origins (not wildcard)
    if allow_credentials && !has_wildcard {
        cors = cors.allow_credentials(true);
    }

    cors
}

/// Health check cache TTL (5 seconds) - PERF-003
const HEALTH_CHECK_TTL_SECS: u64 = 5;

/// Task time-to-live for cleanup (1 hour)
const TASK_TTL_SECS: i64 = 3600;
/// Maximum number of tasks to keep in memory
const MAX_TASKS: usize = 10_000;
/// How often to run task cleanup (5 minutes)
const TASK_CLEANUP_INTERVAL_SECS: u64 = 300;

/// Background job to clean up old tasks and prevent unbounded memory growth
async fn task_cleanup_job(tasks: Arc<RwLock<HashMap<String, TaskState>>>) {
    use tokio::time::{interval, Duration};

    let mut cleanup_interval = interval(Duration::from_secs(TASK_CLEANUP_INTERVAL_SECS));

    loop {
        cleanup_interval.tick().await;

        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(TASK_TTL_SECS);

        let mut tasks = tasks.write().await;
        let before_count = tasks.len();

        // Remove completed/failed tasks older than TTL
        tasks.retain(|_id, task| {
            // Keep pending/in_progress tasks
            if task.status == "pending" || task.status == "in_progress" {
                return true;
            }
            // Remove old completed/failed tasks
            task.updated_at > cutoff
        });

        // If still over limit, remove oldest completed tasks
        if tasks.len() > MAX_TASKS {
            let mut completed_tasks: Vec<_> = tasks
                .iter()
                .filter(|(_, t)| t.status == "completed" || t.status == "failed")
                .map(|(id, t)| (id.clone(), t.updated_at))
                .collect();

            // Sort by updated_at (oldest first)
            completed_tasks.sort_by_key(|(_, updated_at)| *updated_at);

            // Remove oldest tasks until under limit
            let to_remove = tasks.len().saturating_sub(MAX_TASKS);
            for (id, _) in completed_tasks.into_iter().take(to_remove) {
                tasks.remove(&id);
            }
        }

        let removed = before_count.saturating_sub(tasks.len());
        if removed > 0 {
            info!("Task cleanup: removed {} old tasks, {} remaining", removed, tasks.len());
        }
    }
}

// API Request/Response types

/// Maximum size limits for API inputs (security: prevent DoS via memory exhaustion)
const MAX_TASK_DESCRIPTION_LEN: usize = 100_000;   // 100KB
const MAX_BROADCAST_MESSAGE_LEN: usize = 10_000;   // 10KB
const MAX_CONTENT_LEN: usize = 1_000_000;          // 1MB
const MAX_QUERY_LEN: usize = 1_000;                // 1KB

/// SEC-009: Sanitize broadcast message content to prevent injection attacks
/// Removes or escapes potentially dangerous content before forwarding to agents
fn sanitize_broadcast_message(message: &str) -> String {
    let mut sanitized = String::with_capacity(message.len());

    for ch in message.chars() {
        match ch {
            // Allow printable ASCII, newlines, and tabs
            '\n' | '\t' | '\r' => sanitized.push(ch),
            // Remove null bytes and other control characters (except newline/tab/cr)
            c if c.is_control() => {
                // Skip control characters - potential injection vectors
            }
            // Allow normal printable characters including unicode
            c => sanitized.push(c),
        }
    }

    // Trim excessive whitespace (prevent whitespace-based DoS)
    let trimmed = sanitized.trim();

    // Collapse multiple consecutive newlines to max 2
    let mut result = String::with_capacity(trimmed.len());
    let mut consecutive_newlines = 0;
    for ch in trimmed.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                result.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            result.push(ch);
        }
    }

    result
}

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
    #[serde(default)]
    pub tokens_used: u64,
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
    #[serde(default)]
    pub tokens_used: u64,
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
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub services: ServiceHealth,
}

/// Individual service health status
#[derive(Debug, Clone, Serialize)]
pub struct ServiceHealth {
    pub redis: bool,
    pub postgres: bool,
    pub acp: bool,
    pub embeddings: bool,
}

/// Cached health check result - PERF-003
#[derive(Debug, Clone)]
struct CachedHealthCheck {
    response: HealthResponse,
    cached_at: std::time::Instant,
}

/// Prometheus metrics endpoint
async fn prometheus_metrics() -> ([(axum::http::header::HeaderName, &'static str); 1], String) {
    let metrics = crate::metrics::encode_metrics();
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        metrics,
    )
}

async fn health_check(State(state): State<DaemonState>) -> Json<HealthResponse> {
    // PERF-003: Check cache first
    {
        let cache = state.health_cache.read().await;
        if let Some(ref cached) = *cache {
            if cached.cached_at.elapsed().as_secs() < HEALTH_CHECK_TTL_SECS {
                debug!("Returning cached health check response");
                return Json(cached.response.clone());
            }
        }
    }

    // Cache miss or expired - perform actual health check
    let redis_ok = state.redis.is_some();
    let postgres_ok = state.postgres.is_some();

    // Actually verify Ollama connectivity for embeddings
    let embeddings_ok = if let Some(ref emb_service) = state.embedding_service {
        emb_service.health_check().await
    } else {
        false
    };

    let status = if redis_ok && postgres_ok {
        "healthy"
    } else {
        "degraded"
    };

    let response = HealthResponse {
        status,
        version: env!("CARGO_PKG_VERSION"),
        services: ServiceHealth {
            redis: redis_ok,
            postgres: postgres_ok,
            acp: true, // Always true if daemon is running
            embeddings: embeddings_ok,
        },
    };

    // Update cache
    {
        let mut cache = state.health_cache.write().await;
        *cache = Some(CachedHealthCheck {
            response: response.clone(),
            cached_at: std::time::Instant::now(),
        });
    }

    Json(response)
}

async fn get_status(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let tasks = state.tasks.read().await;
    let agents = state.agent_manager.read().await;

    let pending = tasks.values().filter(|t| t.status == "pending").count();
    let completed = tasks.values().filter(|t| t.status == "completed").count();

    // Get auto-spawned tmux agents info
    let tmux_agents = state.tmux_manager.list_agents().await;
    let tmux_agents_info: Vec<serde_json::Value> = tmux_agents
        .iter()
        .map(|a| {
            serde_json::json!({
                "role": a.role,
                "window": a.window_name,
                "pane_id": a.pane_id,
                "uptime_secs": a.spawned_at.elapsed().as_secs()
            })
        })
        .collect();

    // Get embedding service info
    let embeddings_info = if let Some(ref emb_service) = state.embedding_service {
        serde_json::json!({
            "enabled": true,
            "model": emb_service.model(),
            "dimension": emb_service.dimension()
        })
    } else {
        serde_json::json!({
            "enabled": false
        })
    };

    Json(serde_json::json!({
        "status": "running",
        "version": env!("CARGO_PKG_VERSION"),
        "agents_count": agents.list().len(),
        "tasks_pending": pending,
        "tasks_completed": completed,
        "tmux": {
            "available": state.tmux_manager.is_available(),
            "auto_spawned_agents": tmux_agents_info
        },
        "embeddings": embeddings_info
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
            tokens_used: 0,
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
                tokens_used: 0,
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
                    tokens_used: 0,
                });
            }
        }
    }; // Lock released here

    info!(
        "Sending task to {} agent {}: {}",
        config.role,
        agent_id,
        safe_truncate(&request.message, 100)
    );

    // SEC-007: Apply permission configuration instead of blanket --dangerously-skip-permissions
    let permissions = state.config.agents.permissions.clone();
    let role_str = config.role.to_string();

    // Step 2: Execute Claude Code WITHOUT holding the lock
    let timeout = std::time::Duration::from_secs(request.timeout_seconds);
    let result = tokio::time::timeout(timeout, async {
        let mut cmd = tokio::process::Command::new(&config.claude_path);

        // Apply permission configuration
        apply_permissions_to_command(&mut cmd, &permissions, &role_str);

        cmd.arg("--print")
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
                tokens_used: 0,
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
                tokens_used: 0,
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
                tokens_used: 0,
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
                tokens_used: 0,
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

    // SEC-008: Input validation - check task length
    if request.task.len() > MAX_TASK_DESCRIPTION_LEN {
        return Json(DelegateTaskResponse {
            success: false,
            agent_id: String::new(),
            role: request.role.clone(),
            output: None,
            error: Some(format!(
                "Task too long: {} bytes (max: {} bytes)",
                request.task.len(),
                MAX_TASK_DESCRIPTION_LEN
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            tokens_used: 0,
        });
    }

    // SEC-008: Input validation - check context length
    if let Some(ref ctx) = request.context {
        if ctx.len() > MAX_TASK_DESCRIPTION_LEN {
            return Json(DelegateTaskResponse {
                success: false,
                agent_id: String::new(),
                role: request.role.clone(),
                output: None,
                error: Some(format!(
                    "Context too long: {} bytes (max: {} bytes)",
                    ctx.len(),
                    MAX_TASK_DESCRIPTION_LEN
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                tokens_used: 0,
            });
        }
    }

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
                tokens_used: 0,
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
                        tokens_used: 0,
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
                    tokens_used: 0,
                });
            }
        }
    }; // Lock released here

    info!(
        "Sending task to {} agent {}: {}",
        config.role,
        agent_id,
        safe_truncate(&message, 100)
    );

    // SEC-007: Apply permission configuration instead of blanket --dangerously-skip-permissions
    let permissions = state.config.agents.permissions.clone();
    let role_str = config.role.to_string();

    // Step 2: Execute Claude Code WITHOUT holding the lock
    let timeout = std::time::Duration::from_secs(request.timeout_seconds);
    let result = tokio::time::timeout(timeout, async {
        let mut cmd = tokio::process::Command::new(&config.claude_path);

        // Apply permission configuration
        apply_permissions_to_command(&mut cmd, &permissions, &role_str);

        cmd.arg("--print")
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
                tokens_used: 0,
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
                tokens_used: 0,
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
                tokens_used: 0,
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
                tokens_used: 0,
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

    // Step 1: Find connected coordinator worker via WebSocket
    let coordinator_id = match state.acp_server.find_agent_by_role("coordinator").await {
        Some(id) => {
            info!("Found connected coordinator worker: {}", id);
            id
        }
        None => {
            let error_msg = "No coordinator worker connected. Start one with: cca agent worker coordinator".to_string();
            warn!("{}", error_msg);
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
    };

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
        "Sending task to coordinator {} via WebSocket: {}",
        coordinator_id,
        safe_truncate(&request.description, 100)
    );

    // Step 2: Get available workers to inform coordinator
    let agents_with_roles = state.acp_server.agents_with_roles().await;
    let available_roles: Vec<String> = agents_with_roles
        .iter()
        .filter_map(|(_, role_opt)| {
            let role = role_opt.as_ref()?;
            if role != "coordinator" && role != "unregistered" {
                Some(role.clone())
            } else {
                None
            }
        })
        .collect();

    // Build context with system prompt and available workers
    let workers_info = if available_roles.is_empty() {
        "IMPORTANT: No specialist workers are currently connected. \
         You MUST return an error response telling the user to start the required worker(s).\n\
         Example: {\"action\":\"error\",\"error\":\"No workers available. Start required workers with: cca agent worker <role>\",\"required_workers\":[\"backend\"]}".to_string()
    } else {
        format!(
            "Available workers: {}. Only delegate to these roles. \
             If a required role is not available, return an error response listing the missing workers.",
            available_roles.join(", ")
        )
    };
    let context = format!("{}\n\n{}", COORDINATOR_SYSTEM_PROMPT, workers_info);

    // Send task to coordinator via WebSocket
    let timeout = std::time::Duration::from_secs(state.config.agents.default_timeout_seconds);
    let result = state.acp_server.send_task(
        coordinator_id,
        &request.description,
        Some(&context),
        timeout,
    ).await;

    // Step 3: Process coordinator's response
    match result {
        Ok(coordinator_response) => {
            let coordinator_output = coordinator_response.output;
            let _coordinator_tokens = coordinator_response.tokens_used; // Available for future use

            info!(
                "Coordinator {} returned output ({} bytes) for task {}",
                coordinator_id,
                coordinator_output.len(),
                task_id
            );

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

                            // FIX 1: Record RL experiences for each delegation result
                            for result in &delegation_results {
                                if !result.agent_id.is_empty() {
                                    // Build RL state from task context
                                    let rl_state = RLState {
                                        task_type: result.role.clone(),
                                        available_agents: vec![RLAgentState {
                                            role: AgentRole::from(result.role.as_str()),
                                            is_busy: false,
                                            success_rate: if result.success { 1.0 } else { 0.0 },
                                            avg_completion_time: result.duration_ms as f64,
                                        }],
                                        token_usage: result.tokens_used as f64 / 100_000.0, // Normalized
                                        success_history: vec![if result.success { 1.0 } else { 0.0 }],
                                        complexity: 0.5,
                                        features: vec![],
                                    };

                                    // Action was routing to this agent's role
                                    let action = Action::RouteToAgent(AgentRole::from(result.role.as_str()));

                                    // Compute reward based on success, tokens, and duration
                                    let reward = compute_reward(
                                        result.success,
                                        result.tokens_used as u32,
                                        result.duration_ms as u32,
                                        100_000, // max_tokens
                                        300_000, // max_duration_ms (5 min)
                                    );

                                    let experience = Experience::new(
                                        rl_state.clone(),
                                        action,
                                        reward,
                                        Some(rl_state), // next_state same as current for terminal
                                        true, // done
                                    );

                                    if let Err(e) = state.rl_service.record_experience(experience).await {
                                        warn!("Failed to record RL experience for {} agent: {}", result.role, e);
                                    } else {
                                        debug!("Recorded RL experience: role={}, success={}, reward={:.3}",
                                            result.role, result.success, reward);
                                    }
                                }
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
        Err(e) => {
            let error_msg = format!("Coordinator error: {}", e);
            error!(
                "Task {} failed: coordinator {} error: {}",
                task_id, coordinator_id, e
            );

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
    // Safety: "```json" is 7 ASCII bytes, so start + 7 is always a valid UTF-8 boundary
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7; // "```json".len()
        if json_start <= trimmed.len() {
            if let Some(end) = trimmed[json_start..].find("```") {
                let json_end = json_start + end;
                return Some(trimmed[json_start..json_end].trim().to_string());
            }
        }
    }

    // Look for ``` code blocks (might not be marked as json)
    // Safety: "```\n" is 4 ASCII bytes, so start + 4 is always a valid UTF-8 boundary
    if let Some(start) = trimmed.find("```\n{") {
        let json_start = start + 4; // "```\n".len()
        if json_start <= trimmed.len() {
            if let Some(end) = trimmed[json_start..].find("```") {
                let json_end = json_start + end;
                return Some(trimmed[json_start..json_end].trim().to_string());
            }
        }
    }

    // Look for first { in the output
    // Safety: '{' is ASCII, so `start` is always a valid UTF-8 boundary
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

/// P2: Store a successful task completion as a pattern in the ReasoningBank
async fn store_task_as_pattern(
    state: &DaemonState,
    agent_id: AgentId,
    role: &str,
    task_description: &str,
    output: &str,
    duration_ms: u64,
) {
    // Only store patterns if PostgreSQL is configured
    let Some(postgres_services) = &state.postgres else {
        return;
    };

    // Build metadata with task context
    let metadata = serde_json::json!({
        "agent_id": agent_id.to_string(),
        "role": role.to_lowercase(),
        "task": safe_truncate(task_description, 200),
        "duration_ms": duration_ms,
        "timestamp": Utc::now().to_rfc3339(),
        "pattern_source": "automatic_task_completion"
    });

    // Generate embedding if embedding service is available
    // Combine task description and output for richer semantic representation
    let embedding = if let Some(ref emb_service) = state.embedding_service {
        let text_for_embedding = format!("Task: {}\n\nSolution: {}",
            safe_truncate(task_description, 500),
            safe_truncate(output, 1500)
        );
        match emb_service.embed(&text_for_embedding).await {
            Ok(emb) => {
                debug!("Generated embedding ({} dims) for pattern", emb.len());
                Some(emb)
            }
            Err(e) => {
                warn!("Failed to generate embedding: {} - storing without embedding", e);
                None
            }
        }
    } else {
        None
    };

    // Store the pattern with Solution type
    match postgres_services
        .patterns
        .create(
            Some(agent_id.0), // Extract Uuid from AgentId
            crate::postgres::PatternType::Solution,
            output,
            embedding.as_deref(),
            metadata,
        )
        .await
    {
        Ok(pattern_id) => {
            let with_emb = if embedding.is_some() { " (with embedding)" } else { "" };
            debug!("Stored pattern {}{} for {} agent {} ({}ms)", pattern_id, with_emb, role, agent_id, duration_ms);
        }
        Err(e) => {
            warn!("Failed to store pattern for {} agent {}: {}", role, agent_id, e);
        }
    }
}

/// Execute delegations to specialist agents IN PARALLEL
///
/// This is the core of CCA's value - multiple agents working simultaneously.
/// All delegations are spawned concurrently and awaited together.
async fn execute_delegations(
    state: &DaemonState,
    delegations: &[CoordinatorDelegation],
) -> Vec<DelegateTaskResponse> {
    use futures_util::future::join_all;

    if delegations.is_empty() {
        return Vec::new();
    }

    info!("Executing {} delegations IN PARALLEL", delegations.len());

    // Phase 1: Prepare all delegations - validate roles and find/spawn agents
    // This phase is sequential to avoid race conditions when spawning agents
    let mut prepared: Vec<(CoordinatorDelegation, AgentId)> = Vec::new();
    let mut errors: Vec<DelegateTaskResponse> = Vec::new();

    for delegation in delegations {
        info!("Preparing delegation to {}: {}", delegation.role,
              safe_truncate(&delegation.task, 50));

        // Validate role
        let valid_role = matches!(
            delegation.role.to_lowercase().as_str(),
            "frontend" | "backend" | "dba" | "devops" | "security" | "qa"
        );

        if !valid_role {
            errors.push(DelegateTaskResponse {
                success: false,
                agent_id: String::new(),
                role: delegation.role.clone(),
                output: None,
                error: Some(format!("Unknown role: {}", delegation.role)),
                duration_ms: 0,
                tokens_used: 0,
            });
            continue;
        }

        // Find an available agent (not already assigned in this batch)
        let already_assigned: Vec<AgentId> = prepared.iter().map(|(_, id)| *id).collect();
        let agent_id = find_available_agent_excluding(state, &delegation.role, &already_assigned).await;

        let agent_id = match agent_id {
            Some(id) => {
                info!("Found available {} agent: {}", delegation.role, id);
                id
            }
            None => {
                // No available agent - try to spawn one via tmux
                if state.tmux_manager.is_available() {
                    let existing_tmux_agents = state.tmux_manager.agents_by_role(&delegation.role).await;
                    // Allow more agents for parallel work (up to 5 per role)
                    if existing_tmux_agents.len() >= 5 {
                        errors.push(DelegateTaskResponse {
                            success: false,
                            agent_id: String::new(),
                            role: delegation.role.clone(),
                            output: None,
                            error: Some(format!(
                                "No available {} agent. {} agents spawned but all busy.",
                                delegation.role, existing_tmux_agents.len()
                            )),
                            duration_ms: 0,
                            tokens_used: 0,
                        });
                        continue;
                    }

                    info!("No available {} agent, spawning via tmux (existing: {})",
                          delegation.role, existing_tmux_agents.len());
                    match state.tmux_manager.spawn_agent(&delegation.role).await {
                        Ok(pane_id) => {
                            info!("Spawned {} agent in tmux pane {}", delegation.role, pane_id);
                            // Wait for the agent to connect with retries
                            let mut new_agent_id = None;
                            for attempt in 1..=5 {
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                // Find an available agent that's NOT already assigned
                                if let Some(id) = find_available_agent_excluding(
                                    state,
                                    &delegation.role,
                                    &already_assigned,
                                ).await {
                                    new_agent_id = Some(id);
                                    break;
                                }
                                info!("Waiting for {} agent to connect (attempt {}/5)", delegation.role, attempt);
                            }
                            match new_agent_id {
                                Some(id) => id,
                                None => {
                                    warn!("Spawned agent hasn't connected after 10 seconds");
                                    errors.push(DelegateTaskResponse {
                                        success: false,
                                        agent_id: String::new(),
                                        role: delegation.role.clone(),
                                        output: None,
                                        error: Some("Agent spawned but not connected. Try again.".to_string()),
                                        duration_ms: 0,
                                        tokens_used: 0,
                                    });
                                    continue;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to spawn {} agent via tmux: {}", delegation.role, e);
                            errors.push(DelegateTaskResponse {
                                success: false,
                                agent_id: String::new(),
                                role: delegation.role.clone(),
                                output: None,
                                error: Some(format!(
                                    "No {} agent available. Start one with: cca agent worker {}",
                                    delegation.role, delegation.role
                                )),
                                duration_ms: 0,
                                tokens_used: 0,
                            });
                            continue;
                        }
                    }
                } else {
                    warn!("No {} agent connected and tmux not available", delegation.role);
                    errors.push(DelegateTaskResponse {
                        success: false,
                        agent_id: String::new(),
                        role: delegation.role.clone(),
                        output: None,
                        error: Some(format!(
                            "No {} agent connected. Start one with: cca agent worker {}",
                            delegation.role, delegation.role
                        )),
                        duration_ms: 0,
                        tokens_used: 0,
                    });
                    continue;
                }
            }
        };

        prepared.push((delegation.clone(), agent_id));
    }

    if prepared.is_empty() {
        return errors;
    }

    // Phase 2: Mark all agents as busy BEFORE spawning tasks
    {
        let mut busy = state.busy_agents.write().await;
        for (delegation, agent_id) in &prepared {
            busy.insert(*agent_id, delegation.task.clone());
        }
    }

    // Update Redis state for all agents
    for (delegation, agent_id) in &prepared {
        update_agent_redis_state(
            &state.redis,
            *agent_id,
            &delegation.role,
            "busy",
            Some(TaskId::new()),
        ).await;
    }

    // Phase 3: Spawn ALL tasks concurrently
    info!("Spawning {} tasks concurrently", prepared.len());
    let timeout = std::time::Duration::from_secs(state.config.agents.default_timeout_seconds);

    let task_futures: Vec<_> = prepared
        .iter()
        .map(|(delegation, agent_id)| {
            let state = state.clone();
            let delegation = delegation.clone();
            let agent_id = *agent_id;

            async move {
                let start = std::time::Instant::now();
                info!("Sending task to {} agent {} via WebSocket", delegation.role, agent_id);

                let result = state.acp_server.send_task(
                    agent_id,
                    &delegation.task,
                    delegation.context.as_deref(),
                    timeout,
                ).await;

                (delegation, agent_id, start, result)
            }
        })
        .collect();

    // Phase 4: Await ALL tasks together (this is where parallelism happens)
    let task_results = join_all(task_futures).await;

    // Phase 5: Process results and cleanup
    let mut results = errors; // Start with any errors from preparation phase

    for (delegation, agent_id, start, result) in task_results {
        // Unmark agent as busy
        {
            let mut busy = state.busy_agents.write().await;
            busy.remove(&agent_id);
        }

        // Update Redis - agent is now idle
        update_agent_redis_state(
            &state.redis,
            agent_id,
            &delegation.role,
            "idle",
            None,
        ).await;

        match result {
            Ok(task_response) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let tokens_used = task_response.tokens_used;
                let output = task_response.output;

                info!("{} agent {} completed task in {}ms (tokens: {})",
                      delegation.role, agent_id, duration_ms, tokens_used);

                // Track token usage
                if tokens_used > 0 {
                    use crate::tokens::TokenUsage;
                    let usage = TokenUsage {
                        agent_id,
                        input_tokens: 0,
                        output_tokens: tokens_used as u32,
                        total_tokens: tokens_used as u32,
                        context_tokens: 0,
                        timestamp: Utc::now().timestamp(),
                    };
                    state.token_service.metrics.record(usage).await;
                }

                // Store as pattern in ReasoningBank
                store_task_as_pattern(
                    state,
                    agent_id,
                    &delegation.role,
                    &delegation.task,
                    &output,
                    duration_ms,
                ).await;

                results.push(DelegateTaskResponse {
                    success: true,
                    agent_id: agent_id.to_string(),
                    role: delegation.role.clone(),
                    output: Some(output),
                    error: None,
                    duration_ms,
                    tokens_used,
                });
            }
            Err(e) => {
                let error_msg = e.to_string();
                warn!("{} agent {} error: {}", delegation.role, agent_id, error_msg);
                results.push(DelegateTaskResponse {
                    success: false,
                    agent_id: agent_id.to_string(),
                    role: delegation.role.clone(),
                    output: None,
                    error: Some(error_msg),
                    duration_ms: start.elapsed().as_millis() as u64,
                    tokens_used: 0,
                });
            }
        }
    }

    info!("All {} delegations completed", results.len());
    results
}

/// Find an available (not busy) agent with the specified role
async fn find_available_agent(state: &DaemonState, role: &str) -> Option<AgentId> {
    find_available_agent_excluding(state, role, &[]).await
}

/// Find an available (not busy) agent with the specified role, excluding specific agents
///
/// This is critical for parallel task assignment - we need to exclude agents
/// that have already been assigned tasks in the current batch, even if they
/// haven't been marked as "busy" yet.
async fn find_available_agent_excluding(
    state: &DaemonState,
    role: &str,
    exclude: &[AgentId],
) -> Option<AgentId> {
    let agents_with_roles = state.acp_server.agents_with_roles().await;

    // Find matching agent that's not busy AND not in the exclusion list
    let found_agent = {
        let busy_agents = state.busy_agents.read().await;
        agents_with_roles.into_iter().find(|(agent_id, agent_role)| {
            if let Some(r) = agent_role {
                r.to_lowercase() == role.to_lowercase()
                    && !busy_agents.contains_key(agent_id)
                    && !exclude.contains(agent_id)
            } else {
                false
            }
        })
    }; // busy_agents lock released here

    if let Some((agent_id, agent_role)) = found_agent {
        let role_name = agent_role.unwrap_or_default();

        // Ensure agent is registered in orchestrator for workload tracking
        let orchestrator = state.orchestrator.read().await;
        let workloads = orchestrator.get_agent_workloads().await;

        if !workloads.iter().any(|w| w.agent_id == agent_id) {
            orchestrator.register_agent(
                agent_id,
                role_name.clone(),
                vec![role_name.clone()],
                5,
            ).await;
            info!("Auto-registered {} agent {} in orchestrator for workload tracking", role_name, agent_id);
        }

        return Some(agent_id);
    }

    None
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
/// Uses semantic search (embeddings) when available, falls back to text search
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

    let postgres = match &state.postgres {
        Some(pg) => pg,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "PostgreSQL not available"
            }));
        }
    };

    // Try semantic search if embedding service is available
    if let Some(ref emb_service) = state.embedding_service {
        match emb_service.embed(&request.query).await {
            Ok(query_embedding) => {
                // Use cosine similarity search with minimum threshold of 0.3
                match postgres.patterns.search_similar(&query_embedding, request.limit, 0.3).await {
                    Ok(patterns) => {
                        let results: Vec<serde_json::Value> = patterns
                            .iter()
                            .map(|pw| {
                                serde_json::json!({
                                    "id": pw.pattern.id.to_string(),
                                    "pattern_type": pw.pattern.pattern_type,
                                    "content": pw.pattern.content,
                                    "success_rate": pw.pattern.success_rate,
                                    "success_count": pw.pattern.success_count,
                                    "failure_count": pw.pattern.failure_count,
                                    "similarity": pw.similarity,
                                    "created_at": pw.pattern.created_at.to_rfc3339()
                                })
                            })
                            .collect();

                        return Json(serde_json::json!({
                            "success": true,
                            "patterns": results,
                            "count": results.len(),
                            "query": request.query,
                            "search_type": "semantic"
                        }));
                    }
                    Err(e) => {
                        warn!("Semantic search failed, falling back to text: {}", e);
                        // Fall through to text search
                    }
                }
            }
            Err(e) => {
                warn!("Failed to generate query embedding, falling back to text: {}", e);
                // Fall through to text search
            }
        }
    }

    // Fallback: text search (when embeddings not available or semantic search fails)
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
                "query": request.query,
                "search_type": "text"
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to search patterns: {}", e)
        })),
    }
}

/// Backfill embeddings for patterns that don't have them
/// Uses embed_batch for efficient bulk processing
async fn backfill_embeddings(
    State(state): State<DaemonState>,
) -> Json<serde_json::Value> {
    // Check prerequisites
    let emb_service = match &state.embedding_service {
        Some(svc) => svc,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Embedding service not configured"
            }));
        }
    };

    let postgres = match &state.postgres {
        Some(pg) => pg,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "PostgreSQL not available"
            }));
        }
    };

    // Get patterns without embeddings (batch of 10)
    let patterns = match postgres.patterns.get_without_embeddings(10).await {
        Ok(p) => p,
        Err(e) => {
            return Json(serde_json::json!({
                "success": false,
                "error": format!("Failed to get patterns: {}", e)
            }));
        }
    };

    if patterns.is_empty() {
        return Json(serde_json::json!({
            "success": true,
            "message": "No patterns need embedding backfill",
            "processed": 0
        }));
    }

    // Prepare texts for batch embedding
    let texts: Vec<&str> = patterns
        .iter()
        .map(|p| p.content.as_str())
        .collect();

    // Generate embeddings in batch
    let embeddings = match emb_service.embed_batch(&texts).await {
        Ok(embs) => embs,
        Err(e) => {
            return Json(serde_json::json!({
                "success": false,
                "error": format!("Failed to generate embeddings: {}", e)
            }));
        }
    };

    // Update patterns with embeddings
    let mut updated = 0;
    let mut errors = 0;
    for (pattern, embedding) in patterns.iter().zip(embeddings.iter()) {
        match postgres.patterns.update_embedding(pattern.id, embedding).await {
            Ok(()) => updated += 1,
            Err(e) => {
                warn!("Failed to update embedding for pattern {}: {}", pattern.id, e);
                errors += 1;
            }
        }
    }

    Json(serde_json::json!({
        "success": true,
        "processed": updated,
        "errors": errors,
        "remaining": patterns.len() as i32 - updated - errors
    }))
}

// ============================================================================
// Codebase Indexing Endpoints
// ============================================================================

/// Start a codebase indexing job
async fn start_indexing(
    State(state): State<DaemonState>,
    Json(request): Json<StartIndexingRequest>,
) -> Json<serde_json::Value> {
    let indexing_service = match &state.indexing_service {
        Some(svc) => svc,
        None => {
            return Json(serde_json::json!({
                "job_id": "",
                "status": "error",
                "message": "Indexing service not available (requires embeddings + postgres)"
            }));
        }
    };

    match indexing_service.start_indexing(request).await {
        Ok(job_id) => Json(serde_json::json!({
            "job_id": job_id.to_string(),
            "status": "started",
            "message": "Indexing job started in background"
        })),
        Err(e) => Json(serde_json::json!({
            "job_id": "",
            "status": "error",
            "message": format!("Failed to start indexing: {}", e)
        })),
    }
}

/// Get indexing job status
async fn get_indexing_status(
    State(state): State<DaemonState>,
    Path(job_id): Path<String>,
) -> Json<serde_json::Value> {
    let indexing_service = match &state.indexing_service {
        Some(svc) => svc,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Indexing service not available"
            }));
        }
    };

    let job_uuid = match Uuid::parse_str(&job_id) {
        Ok(id) => id,
        Err(_) => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Invalid job ID format"
            }));
        }
    };

    match indexing_service.get_job_status(job_uuid).await {
        Ok(Some(status)) => Json(serde_json::json!({
            "success": true,
            "job": status
        })),
        Ok(None) => Json(serde_json::json!({
            "success": false,
            "error": "Job not found"
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to get job status: {}", e)
        })),
    }
}

/// Cancel a running indexing job
async fn cancel_indexing(
    State(state): State<DaemonState>,
    Path(job_id): Path<String>,
) -> Json<serde_json::Value> {
    let indexing_service = match &state.indexing_service {
        Some(svc) => svc,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Indexing service not available"
            }));
        }
    };

    let job_uuid = match Uuid::parse_str(&job_id) {
        Ok(id) => id,
        Err(_) => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Invalid job ID format"
            }));
        }
    };

    match indexing_service.cancel_job(job_uuid).await {
        Ok(true) => Json(serde_json::json!({
            "success": true,
            "message": "Job cancelled"
        })),
        Ok(false) => Json(serde_json::json!({
            "success": false,
            "error": "Job not found or not running"
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to cancel job: {}", e)
        })),
    }
}

/// List recent indexing jobs
async fn list_indexing_jobs(
    State(state): State<DaemonState>,
) -> Json<serde_json::Value> {
    let indexing_service = match &state.indexing_service {
        Some(svc) => svc,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Indexing service not available"
            }));
        }
    };

    match indexing_service.list_jobs(20).await {
        Ok(jobs) => Json(serde_json::json!({
            "success": true,
            "jobs": jobs,
            "count": jobs.len()
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to list jobs: {}", e)
        })),
    }
}

/// Search request for indexed code
#[derive(Debug, Deserialize)]
struct SearchCodeRequest {
    query: String,
    #[serde(default = "default_limit")]
    limit: i32,
    #[serde(default)]
    language: Option<String>,
}

/// Search indexed code chunks
async fn search_code(
    State(state): State<DaemonState>,
    Json(request): Json<SearchCodeRequest>,
) -> Json<serde_json::Value> {
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

    let indexing_service = match &state.indexing_service {
        Some(svc) => svc,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Indexing service not available"
            }));
        }
    };

    match indexing_service
        .search_code(&request.query, request.limit, request.language.as_deref())
        .await
    {
        Ok(results) => Json(serde_json::json!({
            "success": true,
            "results": results,
            "count": results.len(),
            "query": request.query
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Search failed: {}", e)
        })),
    }
}

/// Get code indexing statistics
async fn code_stats(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let indexing_service = match &state.indexing_service {
        Some(svc) => svc,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Indexing service not available"
            }));
        }
    };

    match indexing_service.get_stats().await {
        Ok(stats) => Json(serde_json::json!({
            "success": true,
            "stats": stats
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to get stats: {}", e)
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

/// ACP disconnect request
#[derive(Debug, Clone, Deserialize)]
pub struct AcpDisconnectRequest {
    pub agent_id: String,
}

/// Disconnect an agent worker
async fn acp_disconnect(
    State(state): State<DaemonState>,
    Json(request): Json<AcpDisconnectRequest>,
) -> Json<serde_json::Value> {
    let agent_id = match Uuid::parse_str(&request.agent_id) {
        Ok(uuid) => AgentId(uuid),
        Err(_) => {
            return Json(serde_json::json!({
                "success": false,
                "error": format!("Invalid agent ID: {}", request.agent_id)
            }));
        }
    };

    // Get the agent's role before disconnecting (for tmux tracking cleanup)
    let agent_role = state
        .acp_server
        .agents_with_roles()
        .await
        .into_iter()
        .find(|(id, _)| *id == agent_id)
        .and_then(|(_, role)| role);

    match state.acp_server.disconnect(agent_id).await {
        Ok(()) => {
            info!("Agent {} disconnected via API", agent_id);

            // If this was a tmux-spawned agent, remove it from tracking
            if let Some(role) = agent_role {
                state.tmux_manager.remove_agent_by_role(&role).await;
            }

            // Also remove from busy agents if it was marked busy
            state.busy_agents.write().await.remove(&agent_id);

            Json(serde_json::json!({
                "success": true,
                "message": format!("Agent {} disconnected", agent_id)
            }))
        }
        Err(e) => {
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

/// ACP send task request
#[derive(Debug, Clone, Deserialize)]
pub struct AcpSendTaskRequest {
    pub agent_id: String,
    pub task: String,
    pub context: Option<String>,
}

/// Send a task to a specific worker
async fn acp_send_task(
    State(state): State<DaemonState>,
    Json(request): Json<AcpSendTaskRequest>,
) -> Json<serde_json::Value> {
    // Input validation - check task length
    if request.task.len() > MAX_TASK_DESCRIPTION_LEN {
        return Json(serde_json::json!({
            "success": false,
            "error": format!(
                "Task too long: {} bytes (max: {} bytes)",
                request.task.len(),
                MAX_TASK_DESCRIPTION_LEN
            )
        }));
    }

    // SEC-008: Input validation - check context length
    if let Some(ref ctx) = request.context {
        if ctx.len() > MAX_TASK_DESCRIPTION_LEN {
            return Json(serde_json::json!({
                "success": false,
                "error": format!(
                    "Context too long: {} bytes (max: {} bytes)",
                    ctx.len(),
                    MAX_TASK_DESCRIPTION_LEN
                )
            }));
        }
    }

    let agent_id = match Uuid::parse_str(&request.agent_id) {
        Ok(uuid) => AgentId(uuid),
        Err(_) => {
            return Json(serde_json::json!({
                "success": false,
                "error": format!("Invalid agent ID: {}", request.agent_id)
            }));
        }
    };

    let timeout = std::time::Duration::from_secs(state.config.agents.default_timeout_seconds);

    match state.acp_server.send_task(
        agent_id,
        &request.task,
        request.context.as_deref(),
        timeout,
    ).await {
        Ok(output) => {
            Json(serde_json::json!({
                "success": true,
                "output": output
            }))
        }
        Err(e) => {
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
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

    // SEC-009: Sanitize message content before forwarding to agents
    let sanitized_message = sanitize_broadcast_message(&request.message);
    if sanitized_message.is_empty() {
        return Json(serde_json::json!({
            "success": false,
            "error": "Message is empty after sanitization"
        }));
    }

    match &state.redis {
        Some(redis) => {
            let msg = PubSubMessage::Broadcast {
                from: AgentId::new(), // System broadcast
                message: sanitized_message,
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

    // SEC-009: Sanitize message content before forwarding to agents
    let sanitized_message = sanitize_broadcast_message(&request.message);
    if sanitized_message.is_empty() {
        return Json(serde_json::json!({
            "success": false,
            "error": "Message is empty after sanitization"
        }));
    }

    let mut acp_count = 0;
    let mut redis_success = false;

    // Broadcast via ACP WebSocket
    let acp_message = cca_acp::AcpMessage::notification(
        cca_acp::methods::BROADCAST,
        serde_json::json!({
            "message_type": "announcement",
            "content": { "message": sanitized_message }
        }),
    );

    match state.acp_server.broadcast(acp_message).await {
        Ok(result) => {
            acp_count = result.sent;
            if result.had_backpressure() {
                warn!("Broadcast had backpressure: {}", result);
            }
        }
        Err(e) => {
            warn!("Failed to broadcast via ACP: {}", e);
        }
    }

    // Also broadcast via Redis pub/sub
    if let Some(redis) = &state.redis {
        let msg = PubSubMessage::Broadcast {
            from: AgentId::new(),
            message: sanitized_message.clone(),
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
    let tasks = state.tasks.read().await;

    // Sync ACP-connected agents to orchestrator
    let orchestrator = state.orchestrator.read().await;
    let agents_with_roles = state.acp_server.agents_with_roles().await;
    for (agent_id, role_opt) in agents_with_roles {
        if let Some(role) = role_opt {
            let workloads = orchestrator.get_agent_workloads().await;
            if !workloads.iter().any(|w| w.agent_id == agent_id) {
                orchestrator.register_agent(
                    agent_id,
                    role.clone(),
                    vec![role.clone()],
                    5,
                ).await;
            }
        }
    }

    // Get workloads from orchestrator (includes ACP-connected workers)
    let orchestrator_workloads = orchestrator.get_agent_workloads().await;

    let agents: Vec<serde_json::Value> = orchestrator_workloads
        .iter()
        .map(|w| {
            serde_json::json!({
                "agent_id": w.agent_id.to_string(),
                "role": w.role,
                "current_tasks": w.current_tasks,
                "max_tasks": w.max_tasks,
                "capabilities": w.capabilities,
                "success_rate": w.success_rate,
                "avg_completion_time": w.avg_completion_time,
                "tasks_completed": w.tasks_completed,
                "tasks_failed": w.tasks_failed
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

    // Build agent metrics with field names matching MCP client expectations
    let agents: Vec<serde_json::Value> = agent_metrics
        .iter()
        .map(|(id, m)| {
            let tokens_used = m.total_input + m.total_output;
            let efficiency = if tokens_used > 0 {
                (m.compression_savings as f64 / tokens_used as f64) * 100.0
            } else {
                0.0
            };
            serde_json::json!({
                "agent_id": id.to_string(),
                "tokens_used": tokens_used,
                "tokens_saved": m.compression_savings,
                "requests": m.message_count,
                "efficiency": efficiency,
                // Also include detailed breakdown
                "total_input": m.total_input,
                "total_output": m.total_output,
                "total_context": m.total_context,
                "avg_input_per_message": m.avg_input_per_message,
                "avg_output_per_message": m.avg_output_per_message,
                "peak_context_size": m.peak_context_size
            })
        })
        .collect();

    let efficiency_percent = if summary.total_tokens_used > 0 {
        (summary.total_tokens_saved as f64 / summary.total_tokens_used as f64) * 100.0
    } else {
        0.0
    };

    // Return flat structure matching MCP TokenMetricsResponse
    Json(serde_json::json!({
        "success": true,
        "total_tokens_used": summary.total_tokens_used,
        "total_tokens_saved": summary.total_tokens_saved,
        "efficiency_percent": efficiency_percent,
        "agent_count": agent_metrics.len(),
        "agents": agents,
        "error": null
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
