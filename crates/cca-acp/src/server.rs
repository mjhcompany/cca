//! ACP WebSocket server

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use cca_core::communication::AcpMessage;
use cca_core::AgentId;

/// Connection to a single agent
struct AgentConnection {
    agent_id: AgentId,
    sender: mpsc::Sender<String>,
}

/// ACP WebSocket server
pub struct AcpServer {
    bind_addr: SocketAddr,
    connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>,
    message_handler: Arc<dyn MessageHandler>,
}

/// Handler for incoming ACP messages
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle(&self, from: AgentId, message: AcpMessage) -> Option<AcpMessage>;
}

/// Default message handler that echoes messages
pub struct EchoHandler;

#[async_trait::async_trait]
impl MessageHandler for EchoHandler {
    async fn handle(&self, _from: AgentId, message: AcpMessage) -> Option<AcpMessage> {
        if let Some(id) = &message.id {
            Some(AcpMessage::response(id, serde_json::json!({"echo": true})))
        } else {
            None
        }
    }
}

impl AcpServer {
    /// Create a new ACP server
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            connections: Arc::new(RwLock::new(HashMap::new())),
            message_handler: Arc::new(EchoHandler),
        }
    }

    /// Set the message handler
    pub fn with_handler(mut self, handler: impl MessageHandler + 'static) -> Self {
        self.message_handler = Arc::new(handler);
        self
    }

    /// Start the server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(self.bind_addr).await?;
        info!("ACP server listening on {}", self.bind_addr);

        while let Ok((stream, addr)) = listener.accept().await {
            let connections = self.connections.clone();
            let handler = self.message_handler.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, addr, connections, handler).await {
                    error!("Connection error: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Send a message to a specific agent
    pub async fn send_to(&self, agent_id: AgentId, message: AcpMessage) -> Result<()> {
        let connections = self.connections.read().await;

        if let Some(conn) = connections.get(&agent_id) {
            let json = serde_json::to_string(&message)?;
            conn.sender.send(json).await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Agent not connected: {}", agent_id))
        }
    }

    /// Broadcast a message to all connected agents
    pub async fn broadcast(&self, message: AcpMessage) -> Result<()> {
        let connections = self.connections.read().await;
        let json = serde_json::to_string(&message)?;

        for conn in connections.values() {
            let _ = conn.sender.send(json.clone()).await;
        }

        Ok(())
    }

    /// Get list of connected agents
    pub async fn connected_agents(&self) -> Vec<AgentId> {
        self.connections.read().await.keys().copied().collect()
    }
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    connections: Arc<RwLock<HashMap<AgentId, AgentConnection>>>,
    handler: Arc<dyn MessageHandler>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();

    info!("New WebSocket connection from {}", addr);

    // For now, generate a temporary agent ID
    // In production, this would be extracted from the URL or initial message
    let agent_id = AgentId::new();

    // Create channel for sending messages to this connection
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Register connection
    {
        let mut conns = connections.write().await;
        conns.insert(
            agent_id,
            AgentConnection {
                agent_id,
                sender: tx,
            },
        );
    }

    info!("Agent {} connected", agent_id);

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
            Ok(Message::Text(text)) => match serde_json::from_str::<AcpMessage>(&text) {
                Ok(acp_msg) => {
                    debug!("Received from {}: {:?}", agent_id, acp_msg.method);

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
            },
            Ok(Message::Close(_)) => {
                info!("Agent {} disconnected", agent_id);
                break;
            }
            Err(e) => {
                error!("Error from {}: {}", agent_id, e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    {
        let mut conns = connections.write().await;
        conns.remove(&agent_id);
    }

    write_task.abort();

    Ok(())
}
