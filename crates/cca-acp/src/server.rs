//! ACP WebSocket server
//!
//! Provides the WebSocket server for inter-agent communication using JSON-RPC 2.0.
//!
//! # Authentication
//!
//! When authentication is enabled, agents must authenticate before sending other messages.
//! Authentication can be done via:
//! 1. Query parameter `?token=<api_key>` in the WebSocket URL (validated during handshake)
//! 2. `X-API-Key` or `Authorization: Bearer <token>` header during WebSocket handshake
//! 3. The `agent.authenticate` method with a valid API key (post-connection fallback)

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use cca_core::util::constant_time_eq;
use serde::{Deserialize, Serialize};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio::time::interval;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
    },
};
use tracing::{debug, error, info, warn};

use cca_core::communication::{AcpError, AcpMessage};
use cca_core::AgentId;

use crate::message::{methods, HeartbeatParams, HeartbeatResponse};

/// Metadata for an API key including permissions
#[derive(Debug, Clone, Default)]
pub struct ApiKeyMetadata {
    /// The API key value
    pub key: String,
    /// Roles this key is authorized to register as (empty = all roles allowed for backwards compat)
    pub allowed_roles: Vec<String>,
    /// Optional identifier for this key (for logging)
    pub key_id: Option<String>,
}

/// Authentication configuration for the ACP server
#[derive(Debug, Clone, Default)]
pub struct AcpAuthConfig {
    /// Valid API keys that can authenticate (legacy - use api_key_metadata for role restrictions)
    pub api_keys: Vec<String>,
    /// API keys with associated metadata and permissions
    pub api_key_metadata: Vec<ApiKeyMetadata>,
    /// Whether authentication is required
    pub require_auth: bool,
}

impl AcpAuthConfig {
    /// Check if a role is authorized for a given API key
    /// Returns true if:
    /// 1. The key has no role restrictions (allowed_roles is empty), OR
    /// 2. The requested role is in the key's allowed_roles list
    pub fn is_role_authorized(&self, api_key: &str, role: &str) -> bool {
        // Check in api_key_metadata first
        for meta in &self.api_key_metadata {
            if constant_time_eq(&meta.key, api_key) {
                // Empty allowed_roles means all roles permitted (backwards compat)
                if meta.allowed_roles.is_empty() {
                    return true;
                }
                return meta.allowed_roles.iter().any(|r| r == role);
            }
        }
        // Legacy api_keys have no role restrictions
        if self.api_keys.iter().any(|k| constant_time_eq(k, api_key)) {
            return true;
        }
        false
    }

    /// Get the key_id for an API key (for logging)
    pub fn get_key_id(&self, api_key: &str) -> Option<String> {
        for meta in &self.api_key_metadata {
            if constant_time_eq(&meta.key, api_key) {
                return meta.key_id.clone();
            }
        }
        None
    }
}

/// Configuration for backpressure handling
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// Channel capacity for outbound messages
    pub channel_capacity: usize,
    /// Maximum consecutive dropped messages before disconnecting slow consumer
    pub max_consecutive_drops: u32,
    /// Warning threshold for channel fullness (0.0 to 1.0)
    pub warning_threshold: f32,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 100,
            max_consecutive_drops: 10,
            warning_threshold: 0.8,
        }
    }
}

/// Backpressure metrics for a connection
#[derive(Debug, Default)]
pub struct BackpressureMetrics {
    /// Total messages dropped due to full channel
    pub messages_dropped: u64,
    /// Consecutive messages dropped (resets on successful send)
    pub consecutive_drops: u32,
    /// Total messages sent successfully
    pub messages_sent: u64,
    /// Last time a message was dropped
    pub last_drop_time: Option<std::time::Instant>,
}

impl BackpressureMetrics {
    /// Record a successful send
    pub fn record_send(&mut self) {
        self.messages_sent += 1;
        self.consecutive_drops = 0;
    }

    /// Record a dropped message
    pub fn record_drop(&mut self) {
        self.messages_dropped += 1;
        self.consecutive_drops += 1;
        self.last_drop_time = Some(std::time::Instant::now());
    }

    /// Check if the connection should be disconnected due to slow consumption
    pub fn should_disconnect(&self, max_consecutive_drops: u32) -> bool {
        self.consecutive_drops >= max_consecutive_drops
    }
}

/// Result of a send operation with backpressure handling
#[derive(Debug)]
pub enum SendResult {
    /// Message sent successfully
    Sent,
    /// Message dropped due to full channel (slow consumer)
    Dropped,
    /// Consumer should be disconnected (too many consecutive drops)
    DisconnectSlowConsumer,
}

/// Response from a task execution including output and token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    /// The task output
    pub output: String,
    /// Tokens used during task execution (0 if not reported by worker)
    pub tokens_used: u64,
    /// Whether the task was successful
    pub success: bool,
}

/// Result of a broadcast operation
#[derive(Debug)]
pub struct BroadcastResult {
    /// Number of agents that received the message
    pub sent: usize,
    /// Number of messages dropped due to backpressure
    pub dropped: usize,
    /// Agents that were disconnected due to being slow consumers
    pub disconnected: Vec<AgentId>,
}

impl BroadcastResult {
    /// Returns the total number of successfully sent messages
    pub fn sent_count(&self) -> usize {
        self.sent
    }

    /// Returns true if any messages were dropped or consumers disconnected
    pub fn had_backpressure(&self) -> bool {
        self.dropped > 0 || !self.disconnected.is_empty()
    }
}

impl std::fmt::Display for BroadcastResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sent: {}, dropped: {}, disconnected: {}",
               self.sent, self.dropped, self.disconnected.len())
    }
}

/// Backpressure information for a single connection
#[derive(Debug, Clone)]
pub struct ConnectionBackpressureInfo {
    /// Agent ID
    pub agent_id: AgentId,
    /// Total messages successfully sent
    pub messages_sent: u64,
    /// Total messages dropped due to backpressure
    pub messages_dropped: u64,
    /// Current consecutive drops (resets on successful send)
    pub consecutive_drops: u32,
    /// Current channel fullness (0.0 to 1.0)
    pub channel_fullness: f32,
    /// Whether the connection is above the warning threshold
    pub is_warning: bool,
}

/// Connection state for a single agent
pub struct AgentConnection {
    pub agent_id: AgentId,
    pub role: Option<String>,
    pub sender: mpsc::Sender<String>,
    pub connected_at: std::time::Instant,
    pub last_heartbeat: std::time::Instant,
    pub metadata: HashMap<String, String>,
    /// Whether this connection has been authenticated
    pub authenticated: bool,
    /// The API key used to authenticate this connection (for role authorization)
    pub authenticated_key: Option<String>,
    /// Backpressure metrics for this connection
    pub backpressure: BackpressureMetrics,
}

impl AgentConnection {
    fn new(agent_id: AgentId, sender: mpsc::Sender<String>) -> Self {
        let now = std::time::Instant::now();
        Self {
            agent_id,
            role: None,
            sender,
            connected_at: now,
            last_heartbeat: now,
            metadata: HashMap::new(),
            authenticated: false, // Must authenticate if auth is required
            authenticated_key: None,
            backpressure: BackpressureMetrics::default(),
        }
    }

    /// Mark this connection as authenticated with the given API key
    pub fn set_authenticated(&mut self, api_key: Option<String>) {
        self.authenticated = true;
        self.authenticated_key = api_key;
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.connected_at.elapsed().as_secs()
    }

    /// Try to send a message with backpressure handling.
    /// Uses try_send to avoid blocking. Returns the result of the send operation.
    pub fn try_send_with_backpressure(
        &mut self,
        message: String,
        max_consecutive_drops: u32,
    ) -> SendResult {
        match self.sender.try_send(message) {
            Ok(()) => {
                self.backpressure.record_send();
                SendResult::Sent
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.backpressure.record_drop();
                warn!(
                    "Backpressure: dropped message for agent {} (consecutive: {}, total: {})",
                    self.agent_id,
                    self.backpressure.consecutive_drops,
                    self.backpressure.messages_dropped
                );
                if self.backpressure.should_disconnect(max_consecutive_drops) {
                    warn!(
                        "Slow consumer detected: agent {} exceeded {} consecutive drops, disconnecting",
                        self.agent_id, max_consecutive_drops
                    );
                    SendResult::DisconnectSlowConsumer
                } else {
                    SendResult::Dropped
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Channel closed, connection is dead
                SendResult::DisconnectSlowConsumer
            }
        }
    }

    /// Get the current channel capacity usage as a ratio (0.0 to 1.0)
    pub fn channel_fullness(&self) -> f32 {
        let capacity = self.sender.capacity();
        let max_capacity = self.sender.max_capacity();
        if max_capacity == 0 {
            return 0.0;
        }
        1.0 - (capacity as f32 / max_capacity as f32)
    }

    /// Check if the channel is above the warning threshold
    pub fn is_channel_warning(&self, threshold: f32) -> bool {
        self.channel_fullness() >= threshold
    }
}

/// Pending request awaiting response
struct PendingRequest {
    sender: oneshot::Sender<AcpMessage>,
    created_at: std::time::Instant,
}

/// ACP WebSocket server
pub struct AcpServer {
    bind_addr: SocketAddr,
    connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>,
    pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
    message_handler: Arc<dyn MessageHandler>,
    broadcast_tx: broadcast::Sender<AcpMessage>,
    shutdown: broadcast::Sender<()>,
    /// Authentication configuration
    auth_config: AcpAuthConfig,
    /// Backpressure configuration
    backpressure_config: BackpressureConfig,
}

/// Handler for incoming ACP messages
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message and optionally return a response
    async fn handle(&self, from: AgentId, message: AcpMessage) -> Option<AcpMessage>;

    /// Called when an agent connects
    async fn on_connect(&self, _agent_id: AgentId) {}

    /// Called when an agent disconnects
    async fn on_disconnect(&self, _agent_id: AgentId) {}
}

/// Default message handler that handles standard ACP methods
pub struct DefaultHandler {
    connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>,
    auth_config: AcpAuthConfig,
}

impl DefaultHandler {
    pub fn new(connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>, auth_config: AcpAuthConfig) -> Self {
        Self { connections, auth_config }
    }

    async fn handle_register(&self, from: AgentId, params: Option<&serde_json::Value>) -> Option<serde_json::Value> {
        if let Some(params) = params {
            if let Some(role) = params.get("role").and_then(|r| r.as_str()) {
                let mut conns = self.connections.write().await;
                if let Some(conn) = conns.get_mut(&from) {
                    // SECURITY: Check role authorization if authentication is required
                    if self.auth_config.require_auth {
                        // Get the API key used to authenticate this connection
                        let authorized = match &conn.authenticated_key {
                            Some(api_key) => self.auth_config.is_role_authorized(api_key, role),
                            None => {
                                // No key stored means either:
                                // 1. Auth wasn't required (allowed - backwards compat)
                                // 2. Connection isn't authenticated (should have been rejected earlier)
                                !self.auth_config.require_auth
                            }
                        };

                        if !authorized {
                            // Log the unauthorized attempt with key_id if available
                            let key_id = conn.authenticated_key.as_ref()
                                .and_then(|k| self.auth_config.get_key_id(k));

                            if let Some(kid) = key_id {
                                warn!(
                                    "Agent {} (key_id: {}) unauthorized to register as role '{}'",
                                    from, kid, role
                                );
                            } else {
                                warn!(
                                    "Agent {} unauthorized to register as role '{}'",
                                    from, role
                                );
                            }

                            // SECURITY: Don't reveal which roles exist or are valid
                            return Some(serde_json::json!({
                                "success": false,
                                "error": "Role registration not authorized"
                            }));
                        }
                    }

                    conn.role = Some(role.to_string());
                    info!("Agent {} registered with role: {}", from, role);
                }
                return Some(serde_json::json!({
                    "success": true,
                    "agent_id": from.to_string(),
                    "role": role
                }));
            }
        }
        Some(serde_json::json!({
            "success": false,
            "error": "Missing role parameter"
        }))
    }
}

#[async_trait::async_trait]
impl MessageHandler for DefaultHandler {
    async fn handle(&self, from: AgentId, message: AcpMessage) -> Option<AcpMessage> {
        let method = message.method.as_deref()?;
        let id = message.id.as_ref()?;

        match method {
            "agent.register" => {
                let result = self.handle_register(from, message.params.as_ref()).await;
                result.map(|r| AcpMessage::response(id, r))
            }
            methods::HEARTBEAT => {
                // Parse heartbeat params
                let params: HeartbeatParams = message
                    .params
                    .as_ref()
                    .and_then(|p| serde_json::from_value(p.clone()).ok())
                    .unwrap_or(HeartbeatParams {
                        timestamp: chrono::Utc::now().timestamp(),
                    });

                // Update last heartbeat time
                {
                    let mut conns = self.connections.write().await;
                    if let Some(conn) = conns.get_mut(&from) {
                        conn.last_heartbeat = std::time::Instant::now();
                    }
                }

                let response = HeartbeatResponse {
                    timestamp: params.timestamp,
                    server_time: chrono::Utc::now().timestamp(),
                };

                match serde_json::to_value(response) {
                    Ok(value) => Some(AcpMessage::response(id, value)),
                    Err(e) => {
                        error!("Failed to serialize heartbeat response: {}", e);
                        Some(AcpMessage::error_response(id, AcpError::internal_error("Serialization failed")))
                    }
                }
            }

            methods::GET_STATUS => {
                let conns = self.connections.read().await;
                if let Some(conn) = conns.get(&from) {
                    let status = crate::message::StatusResponse {
                        agent_id: from.to_string(),
                        state: "connected".to_string(),
                        current_task: None,
                        uptime_seconds: conn.uptime_seconds(),
                    };
                    match serde_json::to_value(status) {
                        Ok(value) => Some(AcpMessage::response(id, value)),
                        Err(e) => {
                            error!("Failed to serialize status response: {}", e);
                            Some(AcpMessage::error_response(id, AcpError::internal_error("Serialization failed")))
                        }
                    }
                } else {
                    Some(AcpMessage::error_response(
                        id,
                        AcpError::invalid_request(),
                    ))
                }
            }

            _ => {
                // Unknown method
                Some(AcpMessage::error_response(
                    id,
                    AcpError::method_not_found(),
                ))
            }
        }
    }

    async fn on_connect(&self, agent_id: AgentId) {
        info!("Agent {} connected via ACP", agent_id);
    }

    async fn on_disconnect(&self, agent_id: AgentId) {
        info!("Agent {} disconnected from ACP", agent_id);
    }
}

impl AcpServer {
    /// Create a new ACP server
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self::with_auth(bind_addr, AcpAuthConfig::default())
    }

    /// Create a new ACP server with authentication configuration
    pub fn with_auth(bind_addr: SocketAddr, auth_config: AcpAuthConfig) -> Self {
        Self::with_config(bind_addr, auth_config, BackpressureConfig::default())
    }

    /// Create a new ACP server with full configuration
    pub fn with_config(
        bind_addr: SocketAddr,
        auth_config: AcpAuthConfig,
        backpressure_config: BackpressureConfig,
    ) -> Self {
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let (broadcast_tx, _) = broadcast::channel(1000);
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            bind_addr,
            connections: connections.clone(),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            message_handler: Arc::new(DefaultHandler::new(connections, auth_config.clone())),
            broadcast_tx,
            shutdown: shutdown_tx,
            auth_config,
            backpressure_config,
        }
    }

    /// Get the backpressure configuration
    pub fn backpressure_config(&self) -> &BackpressureConfig {
        &self.backpressure_config
    }

    /// Check if authentication is required
    pub fn requires_auth(&self) -> bool {
        self.auth_config.require_auth
    }

    /// Validate an API key using constant-time comparison
    pub fn validate_api_key(&self, key: &str) -> bool {
        self.auth_config
            .api_keys
            .iter()
            .any(|k| constant_time_eq(k, key))
    }

    /// Set a custom message handler
    pub fn with_handler(mut self, handler: impl MessageHandler + 'static) -> Self {
        self.message_handler = Arc::new(handler);
        self
    }

    /// Get a broadcast receiver for all messages
    pub fn subscribe(&self) -> broadcast::Receiver<AcpMessage> {
        self.broadcast_tx.subscribe()
    }

    /// Start the server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(self.bind_addr).await?;
        info!("ACP server listening on {}", self.bind_addr);

        let mut shutdown_rx = self.shutdown.subscribe();

        // Spawn cleanup task for stale pending requests
        let pending = self.pending_requests.clone();
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(30));
            loop {
                cleanup_interval.tick().await;
                cleanup_pending_requests(&pending).await;
            }
        });

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            let connections = self.connections.clone();
                            let pending = self.pending_requests.clone();
                            let handler = self.message_handler.clone();
                            let broadcast_tx = self.broadcast_tx.clone();
                            let auth_config = self.auth_config.clone();
                            let backpressure_config = self.backpressure_config.clone();

                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(
                                    stream,
                                    addr,
                                    connections,
                                    pending,
                                    handler,
                                    broadcast_tx,
                                    auth_config,
                                    backpressure_config,
                                ).await {
                                    error!("Connection error from {}: {}", addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("ACP server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shutdown the server
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }

    /// Send a message to a specific agent with backpressure handling.
    /// Returns an error if the agent is not connected or if the message was dropped
    /// due to backpressure (slow consumer).
    pub async fn send_to(&self, agent_id: AgentId, message: AcpMessage) -> Result<()> {
        let json = serde_json::to_string(&message)?;
        let max_drops = self.backpressure_config.max_consecutive_drops;

        let mut connections = self.connections.write().await;

        if let Some(conn) = connections.get_mut(&agent_id) {
            match conn.try_send_with_backpressure(json, max_drops) {
                SendResult::Sent => Ok(()),
                SendResult::Dropped => {
                    Err(anyhow::anyhow!(
                        "Message dropped due to backpressure for agent {}",
                        agent_id
                    ))
                }
                SendResult::DisconnectSlowConsumer => {
                    // Remove the slow consumer connection
                    let agent_id_to_remove = agent_id;
                    drop(connections);
                    self.disconnect(agent_id_to_remove).await.ok();
                    Err(anyhow::anyhow!(
                        "Agent {} disconnected due to slow consumption",
                        agent_id
                    ))
                }
            }
        } else {
            Err(anyhow::anyhow!("Agent not connected: {agent_id}"))
        }
    }

    /// Send a message to a specific agent, ignoring backpressure errors.
    /// Use this for best-effort delivery where message loss is acceptable.
    /// Still disconnects slow consumers that exceed the threshold.
    pub async fn send_to_best_effort(&self, agent_id: AgentId, message: AcpMessage) -> bool {
        let json = match serde_json::to_string(&message) {
            Ok(j) => j,
            Err(_) => return false,
        };
        let max_drops = self.backpressure_config.max_consecutive_drops;

        let mut connections = self.connections.write().await;

        if let Some(conn) = connections.get_mut(&agent_id) {
            match conn.try_send_with_backpressure(json, max_drops) {
                SendResult::Sent => true,
                SendResult::Dropped => false,
                SendResult::DisconnectSlowConsumer => {
                    let agent_id_to_remove = agent_id;
                    drop(connections);
                    let _ = self.disconnect(agent_id_to_remove);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Send a request to an agent and wait for response
    pub async fn request(
        &self,
        agent_id: AgentId,
        method: impl Into<String>,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<AcpMessage> {
        let id = uuid::Uuid::new_v4().to_string();
        let message = AcpMessage::request(&id, method, params);

        // Create pending request
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(
                id.clone(),
                PendingRequest {
                    sender: tx,
                    created_at: std::time::Instant::now(),
                },
            );
        }

        // Send the request
        self.send_to(agent_id, message).await?;

        // Wait for response with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(anyhow::anyhow!("Request channel closed")),
            Err(_) => {
                // Remove pending request on timeout
                let mut pending = self.pending_requests.write().await;
                pending.remove(&id);
                Err(anyhow::anyhow!("Request timeout"))
            }
        }
    }

    /// Broadcast a message to all connected agents with backpressure handling.
    /// Returns the number of agents that received the message and a list of
    /// slow consumers that were disconnected.
    pub async fn broadcast(&self, message: AcpMessage) -> Result<BroadcastResult> {
        let json = serde_json::to_string(&message)?;
        let max_drops = self.backpressure_config.max_consecutive_drops;

        let mut connections = self.connections.write().await;
        let mut sent = 0;
        let mut dropped = 0;
        let mut slow_consumers = Vec::new();

        for conn in connections.values_mut() {
            match conn.try_send_with_backpressure(json.clone(), max_drops) {
                SendResult::Sent => sent += 1,
                SendResult::Dropped => dropped += 1,
                SendResult::DisconnectSlowConsumer => {
                    slow_consumers.push(conn.agent_id);
                }
            }
        }

        // Remove slow consumers
        for agent_id in &slow_consumers {
            connections.remove(agent_id);
            info!("Disconnected slow consumer {} during broadcast", agent_id);
        }

        // Also send to broadcast subscribers
        let _ = self.broadcast_tx.send(message);

        Ok(BroadcastResult {
            sent,
            dropped,
            disconnected: slow_consumers,
        })
    }

    /// Get list of connected agents
    pub async fn connected_agents(&self) -> Vec<AgentId> {
        self.connections.read().await.keys().copied().collect()
    }

    /// Get connection info for an agent
    pub async fn get_connection(&self, agent_id: AgentId) -> Option<(u64, std::time::Duration)> {
        let connections = self.connections.read().await;
        connections.get(&agent_id).map(|conn| {
            (
                conn.uptime_seconds(),
                conn.last_heartbeat.elapsed(),
            )
        })
    }

    /// Get backpressure metrics for an agent
    pub async fn get_backpressure_metrics(&self, agent_id: AgentId) -> Option<ConnectionBackpressureInfo> {
        let connections = self.connections.read().await;
        connections.get(&agent_id).map(|conn| ConnectionBackpressureInfo {
            agent_id: conn.agent_id,
            messages_sent: conn.backpressure.messages_sent,
            messages_dropped: conn.backpressure.messages_dropped,
            consecutive_drops: conn.backpressure.consecutive_drops,
            channel_fullness: conn.channel_fullness(),
            is_warning: conn.is_channel_warning(self.backpressure_config.warning_threshold),
        })
    }

    /// Get backpressure status for all connections
    pub async fn get_all_backpressure_metrics(&self) -> Vec<ConnectionBackpressureInfo> {
        let connections = self.connections.read().await;
        connections.values().map(|conn| ConnectionBackpressureInfo {
            agent_id: conn.agent_id,
            messages_sent: conn.backpressure.messages_sent,
            messages_dropped: conn.backpressure.messages_dropped,
            consecutive_drops: conn.backpressure.consecutive_drops,
            channel_fullness: conn.channel_fullness(),
            is_warning: conn.is_channel_warning(self.backpressure_config.warning_threshold),
        }).collect()
    }

    /// Get count of connected agents
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Find an agent by role
    pub async fn find_agent_by_role(&self, role: &str) -> Option<AgentId> {
        let connections = self.connections.read().await;
        connections
            .values()
            .find(|conn| conn.role.as_deref() == Some(role))
            .map(|conn| conn.agent_id)
    }

    /// Get all agents with their roles
    pub async fn agents_with_roles(&self) -> Vec<(AgentId, Option<String>)> {
        let connections = self.connections.read().await;
        connections
            .values()
            .map(|conn| (conn.agent_id, conn.role.clone()))
            .collect()
    }

    /// Register an agent with a role (called when agent sends register message)
    pub async fn register_agent_role(&self, agent_id: AgentId, role: &str) {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(&agent_id) {
            conn.role = Some(role.to_string());
            info!("Agent {} registered with role: {}", agent_id, role);
        }
    }

    /// Disconnect an agent
    pub async fn disconnect(&self, agent_id: AgentId) -> Result<()> {
        let mut connections = self.connections.write().await;
        if connections.remove(&agent_id).is_some() {
            info!("Agent {} disconnected", agent_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Agent {} not found", agent_id))
        }
    }

    /// Send a task to an agent and wait for response
    /// Returns TaskResponse with output, tokens_used, and success status
    pub async fn send_task(
        &self,
        agent_id: AgentId,
        task: &str,
        context: Option<&str>,
        timeout: Duration,
    ) -> Result<TaskResponse> {
        let params = serde_json::json!({
            "task": task,
            "context": context
        });

        let response = self
            .request(agent_id, "task.execute", params, timeout)
            .await?;

        // Extract result from response
        if let Some(result) = response.result {
            // Extract tokens_used from response (default to 0 if not present)
            let tokens_used = result
                .get("tokens_used")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            if let Some(output) = result.get("output").and_then(|v: &serde_json::Value| v.as_str()) {
                return Ok(TaskResponse {
                    output: output.to_string(),
                    tokens_used,
                    success: true,
                });
            }
            if result.get("success").and_then(|v: &serde_json::Value| v.as_bool()) == Some(true) {
                return Ok(TaskResponse {
                    output: result.to_string(),
                    tokens_used,
                    success: true,
                });
            }
        }

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "Task execution failed: {}",
                error.message
            ));
        }

        Err(anyhow::anyhow!("Invalid response from agent"))
    }
}

/// Handle the agent.authenticate message
async fn handle_authenticate(
    agent_id: AgentId,
    msg: &AcpMessage,
    connections: &Arc<RwLock<HashMap<AgentId, AgentConnection>>>,
    auth_config: &AcpAuthConfig,
) -> AcpMessage {
    let id = match &msg.id {
        Some(id) => id.clone(),
        None => return AcpMessage::error_response("0", AcpError::invalid_request()),
    };

    // Extract API key from params
    let api_key = msg
        .params
        .as_ref()
        .and_then(|p| p.get("api_key"))
        .and_then(|k| k.as_str());

    let api_key = match api_key {
        Some(key) => key,
        None => {
            return AcpMessage::error_response(
                &id,
                AcpError::custom(-32602, "Missing api_key parameter"),
            );
        }
    };

    // Validate API key using constant-time comparison
    // Check both legacy api_keys and api_key_metadata
    let is_valid_legacy = auth_config
        .api_keys
        .iter()
        .any(|k| constant_time_eq(k, api_key));

    let is_valid_metadata = auth_config
        .api_key_metadata
        .iter()
        .any(|meta| constant_time_eq(&meta.key, api_key));

    let is_valid = is_valid_legacy || is_valid_metadata;

    if is_valid {
        // Get key_id for logging (if available)
        let key_id = auth_config.get_key_id(api_key);

        // Mark connection as authenticated and store the API key for role authorization
        {
            let mut conns = connections.write().await;
            if let Some(conn) = conns.get_mut(&agent_id) {
                conn.set_authenticated(Some(api_key.to_string()));
                if let Some(ref kid) = key_id {
                    info!("Agent {} authenticated successfully (key_id: {})", agent_id, kid);
                } else {
                    info!("Agent {} authenticated successfully", agent_id);
                }
            }
        }
        AcpMessage::response(&id, serde_json::json!({"success": true, "agent_id": agent_id.to_string()}))
    } else {
        warn!("Agent {} authentication failed - invalid API key", agent_id);
        AcpMessage::error_response(&id, AcpError::custom(-32001, "Invalid API key"))
    }
}

async fn cleanup_pending_requests(pending: &Arc<RwLock<HashMap<String, PendingRequest>>>) {
    let mut pending = pending.write().await;
    // Use 15 minutes for stale timeout - must be longer than task execution timeout
    let stale_timeout = Duration::from_secs(900);

    let before = pending.len();
    pending.retain(|_id, req| req.created_at.elapsed() < stale_timeout);
    let removed = before - pending.len();
    if removed > 0 {
        info!("Cleaned up {} stale pending requests", removed);
    }
}

/// Simple percent-decoding for URL query parameters
/// Handles common cases like %20 for space, %3D for =, etc.
fn percent_decode(input: &str) -> Option<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Read two hex digits
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    return None; // Invalid hex
                }
            } else {
                return None; // Incomplete escape sequence
            }
        } else if c == '+' {
            result.push(' '); // + is space in query strings
        } else {
            result.push(c);
        }
    }
    Some(result)
}

/// Extract API key from WebSocket handshake request
/// Checks: 1) ?token= query param, 2) X-API-Key header, 3) Authorization: Bearer header
fn extract_api_key_from_request(request: &Request) -> Option<String> {
    // 1. Check query parameter ?token=<api_key>
    let uri = request.uri();
    if let Some(query) = uri.query() {
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("token=") {
                let decoded = percent_decode(value)?;
                return Some(decoded);
            }
        }
    }

    // 2. Check X-API-Key header
    if let Some(key) = request.headers().get("X-API-Key") {
        if let Ok(key_str) = key.to_str() {
            return Some(key_str.to_string());
        }
    }

    // 3. Check Authorization: Bearer header
    if let Some(auth) = request.headers().get("Authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    None
}

/// Validate an API key against the auth config using constant-time comparison
fn validate_api_key_for_handshake(auth_config: &AcpAuthConfig, key: &str) -> bool {
    // Check legacy api_keys
    let is_valid_legacy = auth_config
        .api_keys
        .iter()
        .any(|k| constant_time_eq(k, key));

    // Check api_key_metadata
    let is_valid_metadata = auth_config
        .api_key_metadata
        .iter()
        .any(|meta| constant_time_eq(&meta.key, key));

    is_valid_legacy || is_valid_metadata
}

/// Result of WebSocket handshake authentication
struct HandshakeAuthResult {
    authenticated: bool,
    api_key: Option<String>,
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>,
    pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
    handler: Arc<dyn MessageHandler>,
    broadcast_tx: broadcast::Sender<AcpMessage>,
    auth_config: AcpAuthConfig,
    backpressure_config: BackpressureConfig,
) -> Result<()> {
    // Track authentication state from handshake using Arc<Mutex>
    let auth_result = Arc::new(std::sync::Mutex::new(HandshakeAuthResult {
        authenticated: false,
        api_key: None,
    }));
    let auth_result_clone = auth_result.clone();
    let auth_config_clone = auth_config.clone();

    // Use accept_hdr_async to access HTTP request headers during WebSocket handshake
    // SEC: Validate API key during handshake to prevent unauthenticated connections
    let ws_stream = accept_hdr_async(stream, move |request: &Request, response: Response| {
        debug!("WebSocket handshake from {}: {:?}", addr, request.uri());

        // If auth is not required, allow all connections
        if !auth_config_clone.require_auth {
            let mut result = auth_result_clone.lock().unwrap();
            result.authenticated = true;
            return Ok(response);
        }

        // Extract and validate API key from request
        if let Some(api_key) = extract_api_key_from_request(request) {
            if validate_api_key_for_handshake(&auth_config_clone, &api_key) {
                info!("Worker authenticated via handshake from {}", addr);
                let mut result = auth_result_clone.lock().unwrap();
                result.api_key = Some(api_key);
                result.authenticated = true;
                return Ok(response);
            } else {
                warn!("Invalid API key in WebSocket handshake from {}", addr);
            }
        } else {
            debug!("No API key in handshake from {} - will require post-connect auth", addr);
        }

        // Allow connection but mark as unauthenticated
        // Worker must authenticate via agent.authenticate message
        Ok(response)
    }).await?;

    // Extract auth result after handshake
    let handshake_result = {
        let result = auth_result.lock().unwrap();
        HandshakeAuthResult {
            authenticated: result.authenticated,
            api_key: result.api_key.clone(),
        }
    };

    let (mut write, mut read) = ws_stream.split();

    debug!("New WebSocket connection from {}", addr);

    // Generate agent ID (in production, extract from URL path or initial handshake)
    let agent_id = AgentId::new();

    // Create channel for sending messages to this connection
    // Channel capacity is configurable via BackpressureConfig
    let (tx, mut rx) = mpsc::channel::<String>(backpressure_config.channel_capacity);

    // Register connection with authentication state from handshake
    {
        let mut conns = connections.write().await;
        let mut conn = AgentConnection::new(agent_id, tx);

        // Set authentication state based on handshake result
        if handshake_result.authenticated {
            conn.set_authenticated(handshake_result.api_key.clone());
            if handshake_result.api_key.is_some() {
                info!("Agent {} pre-authenticated via handshake", agent_id);
            }
        } else if !auth_config.require_auth {
            // Auth not required - mark as authenticated with no key
            conn.set_authenticated(None);
        }
        // Otherwise: auth required but not provided in handshake
        // Worker must authenticate via agent.authenticate message

        conns.insert(agent_id, conn);
    }

    // Notify handler of connection
    handler.on_connect(agent_id).await;

    // Spawn write task
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<AcpMessage>(&text) {
                    Ok(acp_msg) => {
                        debug!("Received from {}: {:?}", agent_id, acp_msg.method);

                        // Handle authentication message
                        if acp_msg.method.as_deref() == Some("agent.authenticate") {
                            let response = handle_authenticate(
                                agent_id,
                                &acp_msg,
                                &connections,
                                &auth_config,
                            )
                            .await;
                            let should_disconnect = {
                                let mut conns = connections.write().await;
                                if let Some(conn) = conns.get_mut(&agent_id) {
                                    let json = serde_json::to_string(&response)?;
                                    matches!(
                                        conn.try_send_with_backpressure(json, backpressure_config.max_consecutive_drops),
                                        SendResult::DisconnectSlowConsumer
                                    )
                                } else {
                                    false
                                }
                            };
                            if should_disconnect {
                                warn!("Disconnecting slow consumer {} during auth response", agent_id);
                                break;
                            }
                            continue;
                        }

                        // Check if authenticated (for non-auth messages)
                        let is_authenticated = {
                            let conns = connections.read().await;
                            conns.get(&agent_id).map(|c| c.authenticated).unwrap_or(false)
                        };

                        if !is_authenticated {
                            warn!("Unauthenticated message from {}: {:?}", agent_id, acp_msg.method);
                            // Send error response if this is a request
                            if let Some(id) = &acp_msg.id {
                                let error_response = AcpMessage::error_response(
                                    id,
                                    AcpError::custom(-32001, "Authentication required"),
                                );
                                let should_disconnect = {
                                    let mut conns = connections.write().await;
                                    if let Some(conn) = conns.get_mut(&agent_id) {
                                        let json = serde_json::to_string(&error_response)?;
                                        matches!(
                                            conn.try_send_with_backpressure(json, backpressure_config.max_consecutive_drops),
                                            SendResult::DisconnectSlowConsumer
                                        )
                                    } else {
                                        false
                                    }
                                };
                                if should_disconnect {
                                    warn!("Disconnecting slow consumer {} during error response", agent_id);
                                    break;
                                }
                            }
                            continue;
                        }

                        // Check if this is a response to a pending request
                        if acp_msg.id.is_some() && acp_msg.method.is_none() {
                            // This is a response
                            let id = acp_msg.id.as_ref().unwrap();
                            let mut pending = pending_requests.write().await;
                            if let Some(req) = pending.remove(id) {
                                let _ = req.sender.send(acp_msg.clone());
                            }
                        }

                        // Broadcast to subscribers
                        let _ = broadcast_tx.send(acp_msg.clone());

                        // Let handler process the message
                        if let Some(response) = handler.handle(agent_id, acp_msg).await {
                            let should_disconnect = {
                                let mut conns = connections.write().await;
                                if let Some(conn) = conns.get_mut(&agent_id) {
                                    let json = serde_json::to_string(&response)?;
                                    matches!(
                                        conn.try_send_with_backpressure(json, backpressure_config.max_consecutive_drops),
                                        SendResult::DisconnectSlowConsumer
                                    )
                                } else {
                                    false
                                }
                            };
                            if should_disconnect {
                                warn!("Disconnecting slow consumer {} during handler response", agent_id);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse message from {}: {}", agent_id, e);
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                // Respond to ping with pong using backpressure-aware send
                let should_disconnect = {
                    let mut conns = connections.write().await;
                    if let Some(conn) = conns.get_mut(&agent_id) {
                        let pong = Message::Pong(data);
                        matches!(
                            conn.try_send_with_backpressure(pong.to_string(), backpressure_config.max_consecutive_drops),
                            SendResult::DisconnectSlowConsumer
                        )
                    } else {
                        false
                    }
                };
                if should_disconnect {
                    warn!("Disconnecting slow consumer {} during pong response", agent_id);
                    break;
                }
            }
            Ok(Message::Close(_)) => {
                info!("Agent {} disconnected (close frame)", agent_id);
                break;
            }
            Err(e) => {
                error!("WebSocket error from {}: {}", agent_id, e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    handler.on_disconnect(agent_id).await;
    {
        let mut conns = connections.write().await;
        conns.remove(&agent_id);
    }

    write_task.abort();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_connection() {
        let (tx, _rx) = mpsc::channel(10);
        let agent_id = AgentId::new();
        let conn = AgentConnection::new(agent_id, tx);

        assert_eq!(conn.agent_id, agent_id);
        assert!(conn.metadata.is_empty());
        assert!(!conn.authenticated);
        assert!(conn.authenticated_key.is_none());
    }

    #[test]
    fn test_agent_connection_authentication() {
        let (tx, _rx) = mpsc::channel(10);
        let agent_id = AgentId::new();
        let mut conn = AgentConnection::new(agent_id, tx);

        // Initially not authenticated
        assert!(!conn.authenticated);
        assert!(conn.authenticated_key.is_none());

        // After authentication
        conn.set_authenticated(Some("test-api-key".to_string()));
        assert!(conn.authenticated);
        assert_eq!(conn.authenticated_key.as_deref(), Some("test-api-key"));
    }

    #[test]
    fn test_role_authorization_legacy_keys() {
        // Legacy api_keys (no role restrictions)
        let config = AcpAuthConfig {
            api_keys: vec!["legacy-key".to_string()],
            api_key_metadata: vec![],
            require_auth: true,
        };

        // Legacy keys can register as any role
        assert!(config.is_role_authorized("legacy-key", "backend"));
        assert!(config.is_role_authorized("legacy-key", "frontend"));
        assert!(config.is_role_authorized("legacy-key", "coordinator"));

        // Unknown key is not authorized
        assert!(!config.is_role_authorized("unknown-key", "backend"));
    }

    #[test]
    fn test_role_authorization_with_restrictions() {
        let config = AcpAuthConfig {
            api_keys: vec![],
            api_key_metadata: vec![
                ApiKeyMetadata {
                    key: "backend-key".to_string(),
                    allowed_roles: vec!["backend".to_string(), "worker".to_string()],
                    key_id: Some("backend-agent".to_string()),
                },
                ApiKeyMetadata {
                    key: "admin-key".to_string(),
                    allowed_roles: vec![], // Empty = all roles allowed
                    key_id: Some("admin-agent".to_string()),
                },
            ],
            require_auth: true,
        };

        // backend-key can only register as backend or worker
        assert!(config.is_role_authorized("backend-key", "backend"));
        assert!(config.is_role_authorized("backend-key", "worker"));
        assert!(!config.is_role_authorized("backend-key", "coordinator"));
        assert!(!config.is_role_authorized("backend-key", "security"));

        // admin-key (empty allowed_roles) can register as any role
        assert!(config.is_role_authorized("admin-key", "backend"));
        assert!(config.is_role_authorized("admin-key", "coordinator"));
        assert!(config.is_role_authorized("admin-key", "security"));

        // Unknown key is not authorized
        assert!(!config.is_role_authorized("unknown-key", "backend"));
    }

    #[test]
    fn test_get_key_id() {
        let config = AcpAuthConfig {
            api_keys: vec!["legacy-key".to_string()],
            api_key_metadata: vec![ApiKeyMetadata {
                key: "tracked-key".to_string(),
                allowed_roles: vec![],
                key_id: Some("my-agent-id".to_string()),
            }],
            require_auth: true,
        };

        // Legacy keys have no key_id
        assert!(config.get_key_id("legacy-key").is_none());

        // Metadata keys have key_id
        assert_eq!(config.get_key_id("tracked-key").as_deref(), Some("my-agent-id"));

        // Unknown keys have no key_id
        assert!(config.get_key_id("unknown-key").is_none());
    }

    #[test]
    fn test_default_deny_unknown_keys() {
        // With require_auth = true, unknown keys should be denied
        let config = AcpAuthConfig {
            api_keys: vec!["known-key".to_string()],
            api_key_metadata: vec![],
            require_auth: true,
        };

        // Unknown key should be denied for any role
        assert!(!config.is_role_authorized("attacker-key", "coordinator"));
        assert!(!config.is_role_authorized("attacker-key", "security"));
        assert!(!config.is_role_authorized("attacker-key", "backend"));
    }

    #[test]
    fn test_percent_decode_basic() {
        // Basic decoding
        assert_eq!(percent_decode("hello"), Some("hello".to_string()));
        assert_eq!(percent_decode("hello%20world"), Some("hello world".to_string()));
        assert_eq!(percent_decode("hello+world"), Some("hello world".to_string()));
    }

    #[test]
    fn test_percent_decode_special_chars() {
        // Special characters commonly found in API keys
        assert_eq!(percent_decode("abc%3D123"), Some("abc=123".to_string()));
        assert_eq!(percent_decode("key%2Fvalue"), Some("key/value".to_string()));
        assert_eq!(percent_decode("test%26test"), Some("test&test".to_string()));
    }

    #[test]
    fn test_percent_decode_invalid() {
        // Invalid escape sequences
        assert_eq!(percent_decode("hello%2"), None); // Incomplete
        assert_eq!(percent_decode("hello%ZZ"), None); // Invalid hex
    }

    #[test]
    fn test_validate_api_key_for_handshake() {
        let config = AcpAuthConfig {
            api_keys: vec!["legacy-key-123".to_string()],
            api_key_metadata: vec![ApiKeyMetadata {
                key: "metadata-key-456".to_string(),
                allowed_roles: vec!["worker".to_string()],
                key_id: Some("worker-1".to_string()),
            }],
            require_auth: true,
        };

        // Valid keys
        assert!(validate_api_key_for_handshake(&config, "legacy-key-123"));
        assert!(validate_api_key_for_handshake(&config, "metadata-key-456"));

        // Invalid keys
        assert!(!validate_api_key_for_handshake(&config, "invalid-key"));
        assert!(!validate_api_key_for_handshake(&config, ""));
        assert!(!validate_api_key_for_handshake(&config, "legacy-key-12")); // Close but not exact
    }

    #[test]
    fn test_validate_api_key_timing_safety() {
        // Ensure validation uses constant-time comparison (can't directly test timing,
        // but we verify the code path uses constant_time_eq)
        let config = AcpAuthConfig {
            api_keys: vec!["secret-key-12345678901234567890".to_string()],
            api_key_metadata: vec![],
            require_auth: true,
        };

        // Both should use constant-time comparison regardless of where they differ
        assert!(!validate_api_key_for_handshake(&config, "wrong-key-12345678901234567890"));
        assert!(!validate_api_key_for_handshake(&config, "secret-key-12345678901234567891"));
    }

    #[test]
    fn test_handshake_auth_result_default() {
        let result = HandshakeAuthResult {
            authenticated: false,
            api_key: None,
        };
        assert!(!result.authenticated);
        assert!(result.api_key.is_none());
    }

    // Backpressure tests

    #[test]
    fn test_backpressure_config_default() {
        let config = BackpressureConfig::default();
        assert_eq!(config.channel_capacity, 100);
        assert_eq!(config.max_consecutive_drops, 10);
        assert!((config.warning_threshold - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_backpressure_metrics_default() {
        let metrics = BackpressureMetrics::default();
        assert_eq!(metrics.messages_dropped, 0);
        assert_eq!(metrics.consecutive_drops, 0);
        assert_eq!(metrics.messages_sent, 0);
        assert!(metrics.last_drop_time.is_none());
    }

    #[test]
    fn test_backpressure_metrics_record_send() {
        let mut metrics = BackpressureMetrics::default();

        // Record some drops first
        metrics.record_drop();
        metrics.record_drop();
        assert_eq!(metrics.consecutive_drops, 2);
        assert_eq!(metrics.messages_dropped, 2);

        // Record a send - should reset consecutive_drops
        metrics.record_send();
        assert_eq!(metrics.messages_sent, 1);
        assert_eq!(metrics.consecutive_drops, 0);
        assert_eq!(metrics.messages_dropped, 2); // Total drops unchanged
    }

    #[test]
    fn test_backpressure_metrics_record_drop() {
        let mut metrics = BackpressureMetrics::default();

        metrics.record_drop();
        assert_eq!(metrics.messages_dropped, 1);
        assert_eq!(metrics.consecutive_drops, 1);
        assert!(metrics.last_drop_time.is_some());

        metrics.record_drop();
        assert_eq!(metrics.messages_dropped, 2);
        assert_eq!(metrics.consecutive_drops, 2);
    }

    #[test]
    fn test_backpressure_metrics_should_disconnect() {
        let mut metrics = BackpressureMetrics::default();

        // Below threshold
        for _ in 0..9 {
            metrics.record_drop();
        }
        assert!(!metrics.should_disconnect(10));

        // At threshold
        metrics.record_drop();
        assert!(metrics.should_disconnect(10));

        // Reset with a send
        metrics.record_send();
        assert!(!metrics.should_disconnect(10));
    }

    #[test]
    fn test_connection_try_send_with_backpressure_success() {
        let (tx, mut rx) = mpsc::channel(10);
        let agent_id = AgentId::new();
        let mut conn = AgentConnection::new(agent_id, tx);

        let result = conn.try_send_with_backpressure("test message".to_string(), 10);
        assert!(matches!(result, SendResult::Sent));
        assert_eq!(conn.backpressure.messages_sent, 1);
        assert_eq!(conn.backpressure.consecutive_drops, 0);

        // Verify message was sent
        let received = rx.try_recv();
        assert!(received.is_ok());
        assert_eq!(received.unwrap(), "test message");
    }

    #[test]
    fn test_connection_try_send_with_backpressure_channel_full() {
        // Create a channel with capacity 1
        let (tx, _rx) = mpsc::channel(1);
        let agent_id = AgentId::new();
        let mut conn = AgentConnection::new(agent_id, tx);

        // First message should succeed
        let result1 = conn.try_send_with_backpressure("msg1".to_string(), 10);
        assert!(matches!(result1, SendResult::Sent));

        // Second message should be dropped (channel full, receiver not consuming)
        let result2 = conn.try_send_with_backpressure("msg2".to_string(), 10);
        assert!(matches!(result2, SendResult::Dropped));
        assert_eq!(conn.backpressure.messages_dropped, 1);
        assert_eq!(conn.backpressure.consecutive_drops, 1);
    }

    #[test]
    fn test_connection_try_send_disconnect_slow_consumer() {
        // Create a channel with capacity 1
        let (tx, _rx) = mpsc::channel(1);
        let agent_id = AgentId::new();
        let mut conn = AgentConnection::new(agent_id, tx);

        // Fill the channel
        let _ = conn.try_send_with_backpressure("msg1".to_string(), 3);

        // Drop messages until we hit the threshold
        for i in 0..2 {
            let result = conn.try_send_with_backpressure(format!("dropped{}", i), 3);
            assert!(matches!(result, SendResult::Dropped));
        }

        // Next drop should trigger disconnect
        let result = conn.try_send_with_backpressure("final".to_string(), 3);
        assert!(matches!(result, SendResult::DisconnectSlowConsumer));
    }

    #[test]
    fn test_connection_channel_fullness() {
        let (tx, _rx) = mpsc::channel(10);
        let agent_id = AgentId::new();
        let mut conn = AgentConnection::new(agent_id, tx);

        // Empty channel should have 0% fullness
        let fullness = conn.channel_fullness();
        assert!(fullness < 0.01); // Near 0

        // Fill half the channel
        for _ in 0..5 {
            let _ = conn.try_send_with_backpressure("msg".to_string(), 10);
        }
        let fullness = conn.channel_fullness();
        assert!((fullness - 0.5).abs() < 0.1); // Approximately 50%
    }

    #[test]
    fn test_connection_is_channel_warning() {
        let (tx, _rx) = mpsc::channel(10);
        let agent_id = AgentId::new();
        let mut conn = AgentConnection::new(agent_id, tx);

        // Empty channel should not be in warning state
        assert!(!conn.is_channel_warning(0.8));

        // Fill to 80%+
        for _ in 0..9 {
            let _ = conn.try_send_with_backpressure("msg".to_string(), 10);
        }
        assert!(conn.is_channel_warning(0.8));
    }

    #[test]
    fn test_broadcast_result_display() {
        let result = BroadcastResult {
            sent: 5,
            dropped: 2,
            disconnected: vec![AgentId::new(), AgentId::new()],
        };
        let display = format!("{}", result);
        assert!(display.contains("sent: 5"));
        assert!(display.contains("dropped: 2"));
        assert!(display.contains("disconnected: 2"));
    }

    #[test]
    fn test_broadcast_result_had_backpressure() {
        // No backpressure
        let result1 = BroadcastResult {
            sent: 5,
            dropped: 0,
            disconnected: vec![],
        };
        assert!(!result1.had_backpressure());

        // Had drops
        let result2 = BroadcastResult {
            sent: 5,
            dropped: 1,
            disconnected: vec![],
        };
        assert!(result2.had_backpressure());

        // Had disconnections
        let result3 = BroadcastResult {
            sent: 5,
            dropped: 0,
            disconnected: vec![AgentId::new()],
        };
        assert!(result3.had_backpressure());
    }
}
