//! ACP WebSocket client
//!
//! Provides WebSocket client with automatic reconnection and JSON-RPC 2.0 support.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::{interval, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use cca_core::communication::AcpMessage;
use cca_core::AgentId;

use crate::message::{methods, HeartbeatParams};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Pending request awaiting response
struct PendingRequest {
    sender: oneshot::Sender<AcpMessage>,
    #[allow(dead_code)] // Preserved for future timeout monitoring
    created_at: std::time::Instant,
}

/// Configuration for the ACP client
#[derive(Debug, Clone)]
pub struct AcpClientConfig {
    /// Server URL (ws:// or wss://)
    pub server_url: String,
    /// Reconnection interval (starts at this, uses exponential backoff)
    pub reconnect_interval: Duration,
    /// Maximum reconnection attempts (0 = unlimited)
    pub max_reconnect_attempts: u32,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Request timeout
    pub request_timeout: Duration,
}

impl Default for AcpClientConfig {
    fn default() -> Self {
        Self {
            server_url: "ws://127.0.0.1:8581".to_string(),
            reconnect_interval: Duration::from_secs(1),
            max_reconnect_attempts: 0, // Unlimited
            heartbeat_interval: Duration::from_secs(30),
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// ACP WebSocket client for agent communication
pub struct AcpClient {
    agent_id: AgentId,
    config: AcpClientConfig,
    sender: Arc<RwLock<Option<mpsc::Sender<String>>>>,
    state: Arc<RwLock<ConnectionState>>,
    pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
    message_tx: mpsc::Sender<AcpMessage>,
    shutdown: tokio::sync::broadcast::Sender<()>,
}

impl AcpClient {
    /// Create a new ACP client with default configuration
    pub fn new(agent_id: AgentId, server_url: impl Into<String>) -> Self {
        Self::with_config(
            agent_id,
            AcpClientConfig {
                server_url: server_url.into(),
                ..Default::default()
            },
        )
    }

    /// Create a new ACP client with custom configuration
    pub fn with_config(agent_id: AgentId, config: AcpClientConfig) -> Self {
        let (message_tx, _) = mpsc::channel(100);
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        Self {
            agent_id,
            config,
            sender: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            message_tx,
            shutdown: shutdown_tx,
        }
    }

    /// Get current connection state
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == ConnectionState::Connected
    }

    /// Connect to the ACP server with automatic reconnection
    #[allow(clippy::too_many_lines)]
    pub async fn connect(&mut self) -> Result<mpsc::Receiver<AcpMessage>> {
        let (message_tx, message_rx) = mpsc::channel::<AcpMessage>(100);
        self.message_tx = message_tx.clone();

        let agent_id = self.agent_id;
        let config = self.config.clone();
        let sender = self.sender.clone();
        let state = self.state.clone();
        let pending = self.pending_requests.clone();
        let mut shutdown_rx = self.shutdown.subscribe();

        // Spawn connection manager task
        tokio::spawn(async move {
            let mut reconnect_attempts = 0u32;
            let mut reconnect_delay = config.reconnect_interval;

            loop {
                // Check for shutdown
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                // Update state
                {
                    let mut s = state.write().await;
                    *s = if reconnect_attempts > 0 {
                        ConnectionState::Reconnecting
                    } else {
                        ConnectionState::Connecting
                    };
                }

                let url = format!("{}/ws/{}", config.server_url, agent_id);
                info!("Connecting to ACP server: {} (attempt {})", url, reconnect_attempts + 1);

                match connect_async(&url).await {
                    Ok((ws_stream, _)) => {
                        info!("Connected to ACP server");
                        reconnect_attempts = 0;
                        reconnect_delay = config.reconnect_interval;

                        {
                            let mut s = state.write().await;
                            *s = ConnectionState::Connected;
                        }

                        let (mut write, mut read) = ws_stream.split();

                        // Create send channel
                        let (tx, mut rx) = mpsc::channel::<String>(100);
                        {
                            let mut s = sender.write().await;
                            *s = Some(tx);
                        }

                        // Spawn write task
                        let write_task = tokio::spawn(async move {
                            while let Some(msg) = rx.recv().await {
                                if write.send(Message::Text(msg)).await.is_err() {
                                    break;
                                }
                            }
                        });

                        // Spawn heartbeat task
                        let heartbeat_sender = sender.clone();
                        let heartbeat_interval = config.heartbeat_interval;
                        let heartbeat_task = tokio::spawn(async move {
                            let mut ticker = interval(heartbeat_interval);
                            loop {
                                ticker.tick().await;
                                let sender_guard = heartbeat_sender.read().await;
                                if let Some(ref tx) = *sender_guard {
                                    let heartbeat = AcpMessage::request(
                                        uuid::Uuid::new_v4().to_string(),
                                        methods::HEARTBEAT,
                                        serde_json::to_value(HeartbeatParams {
                                            timestamp: chrono::Utc::now().timestamp(),
                                        })
                                        .unwrap(),
                                    );
                                    if let Ok(json) = serde_json::to_string(&heartbeat) {
                                        if tx.send(json).await.is_err() {
                                            break;
                                        }
                                    }
                                } else {
                                    break;
                                }
                            }
                        });

                        // Handle incoming messages
                        let pending_clone = pending.clone();
                        let message_tx_clone = message_tx.clone();

                        loop {
                            tokio::select! {
                                msg = read.next() => {
                                    match msg {
                                        Some(Ok(Message::Text(text))) => {
                                            match serde_json::from_str::<AcpMessage>(&text) {
                                                Ok(acp_msg) => {
                                                    debug!("Received: {:?}", acp_msg.method);

                                                    // Check if this is a response to a pending request
                                                    if acp_msg.id.is_some() && acp_msg.method.is_none() {
                                                        let id = acp_msg.id.as_ref().unwrap();
                                                        let mut pending = pending_clone.write().await;
                                                        if let Some(req) = pending.remove(id) {
                                                            let _ = req.sender.send(acp_msg.clone());
                                                            continue;
                                                        }
                                                    }

                                                    // Forward to message channel
                                                    if message_tx_clone.send(acp_msg).await.is_err() {
                                                        break;
                                                    }
                                                }
                                                Err(e) => {
                                                    warn!("Failed to parse ACP message: {}", e);
                                                }
                                            }
                                        }
                                        Some(Ok(Message::Ping(data))) => {
                                            let sender_guard = sender.read().await;
                                            if let Some(ref tx) = *sender_guard {
                                                let pong = Message::Pong(data);
                                                let _ = tx.send(pong.to_string()).await;
                                            }
                                        }
                                        Some(Ok(Message::Close(_))) => {
                                            info!("Server closed connection");
                                            break;
                                        }
                                        Some(Err(e)) => {
                                            error!("WebSocket error: {}", e);
                                            break;
                                        }
                                        None => {
                                            info!("WebSocket stream ended");
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                                _ = shutdown_rx.recv() => {
                                    info!("Shutdown requested");
                                    write_task.abort();
                                    heartbeat_task.abort();
                                    return;
                                }
                            }
                        }

                        // Cleanup on disconnect
                        write_task.abort();
                        heartbeat_task.abort();
                        {
                            let mut s = sender.write().await;
                            *s = None;
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect: {}", e);
                    }
                }

                // Update state to disconnected
                {
                    let mut s = state.write().await;
                    *s = ConnectionState::Disconnected;
                }

                // Check max reconnect attempts
                reconnect_attempts += 1;
                if config.max_reconnect_attempts > 0
                    && reconnect_attempts >= config.max_reconnect_attempts
                {
                    error!(
                        "Max reconnection attempts ({}) reached, giving up",
                        config.max_reconnect_attempts
                    );
                    break;
                }

                // Exponential backoff with jitter
                let jitter = Duration::from_millis(rand_jitter());
                let delay = reconnect_delay + jitter;
                warn!(
                    "Reconnecting in {:?} (attempt {})",
                    delay, reconnect_attempts
                );
                sleep(delay).await;

                // Increase delay for next attempt (max 60 seconds)
                reconnect_delay = std::cmp::min(reconnect_delay * 2, Duration::from_secs(60));
            }
        });

        Ok(message_rx)
    }

    /// Disconnect from the server
    pub async fn disconnect(&self) {
        let _ = self.shutdown.send(());
        let mut sender = self.sender.write().await;
        *sender = None;
        let mut state = self.state.write().await;
        *state = ConnectionState::Disconnected;
    }

    /// Send a message to the server
    pub async fn send(&self, message: AcpMessage) -> Result<()> {
        let sender = self.sender.read().await;
        let tx = sender.as_ref().ok_or_else(|| anyhow!("Not connected"))?;

        let json = serde_json::to_string(&message)?;
        tx.send(json)
            .await
            .map_err(|_| anyhow!("Failed to send message"))?;

        Ok(())
    }

    /// Send a request and wait for response
    pub async fn request(
        &self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<AcpMessage> {
        self.request_with_timeout(method, params, self.config.request_timeout)
            .await
    }

    /// Send a request with custom timeout
    pub async fn request_with_timeout(
        &self,
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
        self.send(message).await?;

        // Wait for response with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(anyhow!("Request channel closed")),
            Err(_) => {
                // Remove pending request on timeout
                let mut pending = self.pending_requests.write().await;
                pending.remove(&id);
                Err(anyhow!("Request timeout"))
            }
        }
    }

    /// Send a notification (no response expected)
    pub async fn notify(&self, method: impl Into<String>, params: serde_json::Value) -> Result<()> {
        let message = AcpMessage::notification(method, params);
        self.send(message).await
    }

    /// Get status from server
    pub async fn get_status(&self) -> Result<crate::message::StatusResponse> {
        let response = self
            .request(methods::GET_STATUS, serde_json::json!({}))
            .await?;

        if let Some(result) = response.result {
            Ok(serde_json::from_value(result)?)
        } else if let Some(error) = response.error {
            Err(anyhow!("Server error: {} - {}", error.code, error.message))
        } else {
            Err(anyhow!("Invalid response"))
        }
    }

    /// Send a heartbeat
    pub async fn heartbeat(&self) -> Result<crate::message::HeartbeatResponse> {
        let params = HeartbeatParams {
            timestamp: chrono::Utc::now().timestamp(),
        };

        let response = self
            .request(methods::HEARTBEAT, serde_json::to_value(params)?)
            .await?;

        if let Some(result) = response.result {
            Ok(serde_json::from_value(result)?)
        } else if let Some(error) = response.error {
            Err(anyhow!("Heartbeat error: {} - {}", error.code, error.message))
        } else {
            Err(anyhow!("Invalid heartbeat response"))
        }
    }
}

/// Generate random jitter for reconnection backoff (0-500ms)
fn rand_jitter() -> u64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    u64::from(nanos % 500)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = AcpClientConfig::default();
        assert_eq!(config.reconnect_interval, Duration::from_secs(1));
        assert_eq!(config.max_reconnect_attempts, 0);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
    }

    #[test]
    fn test_rand_jitter() {
        let jitter = rand_jitter();
        assert!(jitter < 500);
    }

    #[tokio::test]
    async fn test_client_state() {
        let client = AcpClient::new(AgentId::new(), "ws://localhost:8581");
        assert_eq!(client.state().await, ConnectionState::Disconnected);
        assert!(!client.is_connected().await);
    }
}
