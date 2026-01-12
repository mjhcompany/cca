//! ACP WebSocket server
//!
//! Provides the WebSocket server for inter-agent communication using JSON-RPC 2.0.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio::time::interval;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use cca_core::communication::{AcpError, AcpMessage};
use cca_core::AgentId;

use crate::message::{methods, HeartbeatParams, HeartbeatResponse};

/// Connection state for a single agent
pub struct AgentConnection {
    pub agent_id: AgentId,
    pub sender: mpsc::Sender<String>,
    pub connected_at: std::time::Instant,
    pub last_heartbeat: std::time::Instant,
    pub metadata: HashMap<String, String>,
}

impl AgentConnection {
    fn new(agent_id: AgentId, sender: mpsc::Sender<String>) -> Self {
        let now = std::time::Instant::now();
        Self {
            agent_id,
            sender,
            connected_at: now,
            last_heartbeat: now,
            metadata: HashMap::new(),
        }
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.connected_at.elapsed().as_secs()
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
}

impl DefaultHandler {
    pub fn new(connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>) -> Self {
        Self { connections }
    }
}

#[async_trait::async_trait]
impl MessageHandler for DefaultHandler {
    async fn handle(&self, from: AgentId, message: AcpMessage) -> Option<AcpMessage> {
        let method = message.method.as_deref()?;
        let id = message.id.as_ref()?;

        match method {
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

                Some(AcpMessage::response(id, serde_json::to_value(response).unwrap()))
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
                    Some(AcpMessage::response(id, serde_json::to_value(status).unwrap()))
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
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let (broadcast_tx, _) = broadcast::channel(1000);
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            bind_addr,
            connections: connections.clone(),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            message_handler: Arc::new(DefaultHandler::new(connections)),
            broadcast_tx,
            shutdown: shutdown_tx,
        }
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

                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(
                                    stream,
                                    addr,
                                    connections,
                                    pending,
                                    handler,
                                    broadcast_tx,
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

    /// Send a message to a specific agent
    pub async fn send_to(&self, agent_id: AgentId, message: AcpMessage) -> Result<()> {
        let connections = self.connections.read().await;

        if let Some(conn) = connections.get(&agent_id) {
            let json = serde_json::to_string(&message)?;
            conn.sender
                .send(json)
                .await
                .map_err(|_| anyhow::anyhow!("Failed to send to agent"))?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Agent not connected: {agent_id}"))
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

    /// Broadcast a message to all connected agents
    pub async fn broadcast(&self, message: AcpMessage) -> Result<usize> {
        let connections = self.connections.read().await;
        let json = serde_json::to_string(&message)?;
        let mut sent = 0;

        for conn in connections.values() {
            if conn.sender.send(json.clone()).await.is_ok() {
                sent += 1;
            }
        }

        // Also send to broadcast subscribers
        let _ = self.broadcast_tx.send(message);

        Ok(sent)
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

    /// Get count of connected agents
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }
}

async fn cleanup_pending_requests(pending: &Arc<RwLock<HashMap<String, PendingRequest>>>) {
    let mut pending = pending.write().await;
    let stale_timeout = Duration::from_secs(60);

    pending.retain(|_id, req| req.created_at.elapsed() < stale_timeout);
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>,
    pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
    handler: Arc<dyn MessageHandler>,
    broadcast_tx: broadcast::Sender<AcpMessage>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();

    debug!("New WebSocket connection from {}", addr);

    // Generate agent ID (in production, extract from URL path or initial handshake)
    let agent_id = AgentId::new();

    // Create channel for sending messages to this connection
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Register connection
    {
        let mut conns = connections.write().await;
        conns.insert(agent_id, AgentConnection::new(agent_id, tx));
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
                            let conns = connections.read().await;
                            if let Some(conn) = conns.get(&agent_id) {
                                let json = serde_json::to_string(&response)?;
                                let _ = conn.sender.send(json).await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse message from {}: {}", agent_id, e);
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                // Respond to ping with pong
                let conns = connections.read().await;
                if let Some(conn) = conns.get(&agent_id) {
                    let pong = Message::Pong(data);
                    let _ = conn.sender.send(pong.to_string()).await;
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
    }
}
