//! ACP WebSocket client


use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

use cca_core::communication::AcpMessage;
use cca_core::AgentId;

/// ACP WebSocket client for agent communication
pub struct AcpClient {
    agent_id: AgentId,
    server_url: String,
    sender: Option<mpsc::Sender<String>>,
}

impl AcpClient {
    /// Create a new ACP client
    pub fn new(agent_id: AgentId, server_url: impl Into<String>) -> Self {
        Self {
            agent_id,
            server_url: server_url.into(),
            sender: None,
        }
    }

    /// Connect to the ACP server
    pub async fn connect(&mut self) -> Result<mpsc::Receiver<AcpMessage>> {
        let url = format!("{}/ws/{}", self.server_url, self.agent_id);
        info!("Connecting to ACP server: {}", url);

        let (ws_stream, _) = connect_async(&url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Channel for sending messages
        let (tx, mut rx) = mpsc::channel::<String>(100);
        self.sender = Some(tx);

        // Channel for receiving messages
        let (msg_tx, msg_rx) = mpsc::channel::<AcpMessage>(100);

        // Spawn write task
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = write.send(Message::Text(msg)).await {
                    error!("Failed to send message: {}", e);
                    break;
                }
            }
        });

        // Spawn read task
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => match serde_json::from_str::<AcpMessage>(&text) {
                        Ok(acp_msg) => {
                            if msg_tx.send(acp_msg).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse ACP message: {}", e);
                        }
                    },
                    Ok(Message::Close(_)) => {
                        info!("WebSocket connection closed");
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        info!("Connected to ACP server");
        Ok(msg_rx)
    }

    /// Send a message to the server
    pub async fn send(&self, message: AcpMessage) -> Result<()> {
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let json = serde_json::to_string(&message)?;
        sender.send(json).await?;

        Ok(())
    }

    /// Send a request and wait for response
    pub async fn request(
        &self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let message = AcpMessage::request(&id, method, params);

        self.send(message).await?;

        // TODO: Implement response waiting with timeout
        // For now, return a placeholder
        Ok(serde_json::Value::Null)
    }

    /// Send a notification (no response expected)
    pub async fn notify(&self, method: impl Into<String>, params: serde_json::Value) -> Result<()> {
        let message = AcpMessage::notification(method, params);
        self.send(message).await
    }
}
