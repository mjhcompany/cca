//! Redis integration for CCA Daemon
//!
//! Provides connection pooling, session state storage, context caching,
//! and Pub/Sub for inter-agent communication.
//!
//! Note: Many methods are infrastructure for future features and not yet called.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use deadpool_redis::{Config as PoolConfig, Connection, Pool, Runtime};
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use cca_core::{AgentId, TaskId};

use crate::config::RedisConfig;

/// Redis key prefixes
mod keys {
    pub const SESSION: &str = "cca:session:";
    pub const CONTEXT: &str = "cca:context:";
    pub const AGENT_STATE: &str = "cca:agent:state:";
    pub const TASK: &str = "cca:task:";
    pub const PUBSUB_AGENTS: &str = "cca:pubsub:agents";
    pub const PUBSUB_TASKS: &str = "cca:pubsub:tasks";
    pub const PUBSUB_BROADCAST: &str = "cca:pubsub:broadcast";
}

/// Redis client with connection pool
pub struct RedisClient {
    pool: Pool,
    config: RedisConfig,
}

impl RedisClient {
    /// Create a new Redis client with connection pool
    pub async fn new(config: &RedisConfig) -> Result<Self> {
        info!("Connecting to Redis at {}", config.url);

        let pool_config = PoolConfig::from_url(&config.url);
        let pool = pool_config
            .builder()
            .map_err(|e| anyhow::anyhow!("Failed to create pool builder: {e}"))?
            .max_size(config.pool_size)
            .runtime(Runtime::Tokio1)
            .build()
            .context("Failed to create Redis connection pool")?;

        // Test connection
        let mut conn = pool.get().await.context("Failed to get Redis connection")?;
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .context("Redis PING failed")?;

        info!("Redis connection established (pool size: {})", config.pool_size);

        Ok(Self {
            pool,
            config: config.clone(),
        })
    }

    /// Get a connection from the pool
    pub async fn get_conn(&self) -> Result<Connection> {
        self.pool
            .get()
            .await
            .context("Failed to get Redis connection from pool")
    }

    /// Get the default TTL for context entries
    pub fn context_ttl(&self) -> Duration {
        Duration::from_secs(self.config.context_ttl_seconds)
    }
}

/// Session state storage
#[derive(Clone)]
pub struct SessionStore {
    client: Arc<RedisClient>,
}

impl SessionStore {
    pub fn new(client: Arc<RedisClient>) -> Self {
        Self { client }
    }

    /// Store session data for an agent
    pub async fn set<T: Serialize>(&self, agent_id: AgentId, data: &T) -> Result<()> {
        let key = format!("{}{}", keys::SESSION, agent_id);
        let json = serde_json::to_string(data)?;
        let mut conn = self.client.get_conn().await?;

        conn.set_ex::<_, _, ()>(&key, &json, self.client.context_ttl().as_secs())
            .await
            .context("Failed to store session")?;

        debug!("Stored session for agent {}", agent_id);
        Ok(())
    }

    /// Get session data for an agent
    pub async fn get<T: DeserializeOwned>(&self, agent_id: AgentId) -> Result<Option<T>> {
        let key = format!("{}{}", keys::SESSION, agent_id);
        let mut conn = self.client.get_conn().await?;

        let result: Option<String> = conn.get(&key).await?;

        match result {
            Some(json) => {
                let data = serde_json::from_str(&json)?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    /// Delete session data for an agent
    pub async fn delete(&self, agent_id: AgentId) -> Result<()> {
        let key = format!("{}{}", keys::SESSION, agent_id);
        let mut conn = self.client.get_conn().await?;

        conn.del::<_, ()>(&key).await?;
        debug!("Deleted session for agent {}", agent_id);
        Ok(())
    }

    /// Refresh session TTL
    pub async fn touch(&self, agent_id: AgentId) -> Result<()> {
        let key = format!("{}{}", keys::SESSION, agent_id);
        let mut conn = self.client.get_conn().await?;

        conn.expire::<_, ()>(&key, self.client.context_ttl().as_secs() as i64)
            .await?;
        Ok(())
    }
}

/// Context caching for agent contexts
#[derive(Clone)]
pub struct ContextCache {
    client: Arc<RedisClient>,
}

/// Cached context entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedContext {
    pub agent_id: AgentId,
    pub context_hash: String,
    pub compressed_context: Vec<u8>,
    pub token_count: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ContextCache {
    pub fn new(client: Arc<RedisClient>) -> Self {
        Self { client }
    }

    /// Store a context snapshot
    pub async fn store(&self, agent_id: AgentId, context: &CachedContext) -> Result<()> {
        let key = format!("{}{}", keys::CONTEXT, agent_id);
        let json = serde_json::to_string(context)?;
        let mut conn = self.client.get_conn().await?;

        conn.set_ex::<_, _, ()>(&key, &json, self.client.context_ttl().as_secs())
            .await
            .context("Failed to cache context")?;

        debug!(
            "Cached context for agent {} (hash: {}, tokens: {})",
            agent_id, context.context_hash, context.token_count
        );
        Ok(())
    }

    /// Retrieve a cached context
    pub async fn get(&self, agent_id: AgentId) -> Result<Option<CachedContext>> {
        let key = format!("{}{}", keys::CONTEXT, agent_id);
        let mut conn = self.client.get_conn().await?;

        let result: Option<String> = conn.get(&key).await?;

        match result {
            Some(json) => {
                let context = serde_json::from_str(&json)?;
                Ok(Some(context))
            }
            None => Ok(None),
        }
    }

    /// Check if a context hash is still valid
    pub async fn is_valid(&self, agent_id: AgentId, context_hash: &str) -> Result<bool> {
        if let Some(cached) = self.get(agent_id).await? {
            return Ok(cached.context_hash == context_hash);
        }
        Ok(false)
    }

    /// Invalidate cached context
    pub async fn invalidate(&self, agent_id: AgentId) -> Result<()> {
        let key = format!("{}{}", keys::CONTEXT, agent_id);
        let mut conn = self.client.get_conn().await?;

        conn.del::<_, ()>(&key).await?;
        debug!("Invalidated context cache for agent {}", agent_id);
        Ok(())
    }
}

/// Agent state stored in Redis
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RedisAgentState {
    pub agent_id: AgentId,
    pub role: String,
    pub state: String,
    pub current_task: Option<TaskId>,
    pub tokens_used: u64,
    pub tasks_completed: u64,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

/// Agent state storage in Redis
#[derive(Clone)]
pub struct AgentStateStore {
    client: Arc<RedisClient>,
}

impl AgentStateStore {
    pub fn new(client: Arc<RedisClient>) -> Self {
        Self { client }
    }

    /// Update agent state
    pub async fn update(&self, state: &RedisAgentState) -> Result<()> {
        let key = format!("{}{}", keys::AGENT_STATE, state.agent_id);
        let json = serde_json::to_string(state)?;
        let mut conn = self.client.get_conn().await?;

        // State expires after 5 minutes without heartbeat
        conn.set_ex::<_, _, ()>(&key, &json, 300).await?;
        Ok(())
    }

    /// Get agent state
    pub async fn get(&self, agent_id: AgentId) -> Result<Option<RedisAgentState>> {
        let key = format!("{}{}", keys::AGENT_STATE, agent_id);
        let mut conn = self.client.get_conn().await?;

        let result: Option<String> = conn.get(&key).await?;

        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Get all active agents
    pub async fn get_all(&self) -> Result<Vec<RedisAgentState>> {
        let pattern = format!("{}*", keys::AGENT_STATE);
        let mut conn = self.client.get_conn().await?;

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;

        let mut states = Vec::new();
        for key in keys {
            if let Ok(Some(json)) = conn.get::<_, Option<String>>(&key).await {
                if let Ok(state) = serde_json::from_str(&json) {
                    states.push(state);
                }
            }
        }

        Ok(states)
    }

    /// Remove agent state
    pub async fn remove(&self, agent_id: AgentId) -> Result<()> {
        let key = format!("{}{}", keys::AGENT_STATE, agent_id);
        let mut conn = self.client.get_conn().await?;

        conn.del::<_, ()>(&key).await?;
        Ok(())
    }
}

/// Message types for Pub/Sub
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum PubSubMessage {
    /// Task assigned to an agent
    TaskAssigned {
        task_id: TaskId,
        agent_id: AgentId,
        description: String,
    },
    /// Task completed
    TaskCompleted {
        task_id: TaskId,
        agent_id: AgentId,
        success: bool,
    },
    /// Agent status change
    AgentStatusChange {
        agent_id: AgentId,
        old_state: String,
        new_state: String,
    },
    /// Broadcast message to all agents
    Broadcast {
        from: AgentId,
        message: String,
    },
    /// Direct message to specific agent
    DirectMessage {
        from: AgentId,
        to: AgentId,
        message: String,
    },
    /// Coordinator delegation
    Delegation {
        from: AgentId,
        to: AgentId,
        task_id: TaskId,
        instruction: String,
    },
}

/// Pub/Sub handler for inter-agent communication
pub struct PubSubHandler {
    client: Arc<RedisClient>,
    tx: broadcast::Sender<PubSubMessage>,
}

impl PubSubHandler {
    /// Create a new Pub/Sub handler
    pub async fn new(client: Arc<RedisClient>) -> Result<Self> {
        let (tx, _) = broadcast::channel(1000);

        Ok(Self { client, tx })
    }

    /// Get a receiver for messages
    pub fn subscribe(&self) -> broadcast::Receiver<PubSubMessage> {
        self.tx.subscribe()
    }

    /// Publish a message to the agents channel
    pub async fn publish_agent(&self, message: &PubSubMessage) -> Result<()> {
        self.publish_to(keys::PUBSUB_AGENTS, message).await
    }

    /// Publish a message to the tasks channel
    pub async fn publish_task(&self, message: &PubSubMessage) -> Result<()> {
        self.publish_to(keys::PUBSUB_TASKS, message).await
    }

    /// Publish a broadcast message
    pub async fn broadcast(&self, message: &PubSubMessage) -> Result<()> {
        self.publish_to(keys::PUBSUB_BROADCAST, message).await
    }

    /// Publish a raw string message to any channel
    pub async fn publish(&self, channel: &str, message: &str) -> Result<()> {
        let mut conn = self.client.get_conn().await?;

        redis::cmd("PUBLISH")
            .arg(channel)
            .arg(message)
            .query_async::<()>(&mut conn)
            .await?;

        debug!("Published message to channel {}", channel);
        Ok(())
    }

    /// Internal publish method for typed messages
    async fn publish_to(&self, channel: &str, message: &PubSubMessage) -> Result<()> {
        let json = serde_json::to_string(message)?;
        let mut conn = self.client.get_conn().await?;

        redis::cmd("PUBLISH")
            .arg(channel)
            .arg(&json)
            .query_async::<()>(&mut conn)
            .await?;

        debug!("Published message to channel {}", channel);

        // Also send to local subscribers
        let _ = self.tx.send(message.clone());

        Ok(())
    }

    /// Start listening for messages (runs in background)
    pub async fn start_listener(client: Arc<RedisClient>, tx: broadcast::Sender<PubSubMessage>) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = Self::listen_loop(&client, &tx).await {
                    error!("Pub/Sub listener error: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });
    }

    async fn listen_loop(
        client: &RedisClient,
        tx: &broadcast::Sender<PubSubMessage>,
    ) -> Result<()> {
        let redis_client = redis::Client::open(client.config.url.as_str())?;
        let mut pubsub = redis_client.get_async_pubsub().await?;

        pubsub.subscribe(keys::PUBSUB_AGENTS).await?;
        pubsub.subscribe(keys::PUBSUB_TASKS).await?;
        pubsub.subscribe(keys::PUBSUB_BROADCAST).await?;

        info!("Pub/Sub listener started");

        loop {
            let msg = pubsub.on_message().next().await;
            if let Some(msg) = msg {
                let payload: String = msg.get_payload()?;
                match serde_json::from_str::<PubSubMessage>(&payload) {
                    Ok(message) => {
                        let _ = tx.send(message);
                    }
                    Err(e) => {
                        warn!("Failed to parse Pub/Sub message: {}", e);
                    }
                }
            }
        }
    }
}

/// Combined Redis services
pub struct RedisServices {
    pub client: Arc<RedisClient>,
    pub sessions: SessionStore,
    pub contexts: ContextCache,
    pub agent_states: AgentStateStore,
    pub pubsub: PubSubHandler,
}

impl RedisServices {
    /// Initialize all Redis services
    pub async fn new(config: &RedisConfig) -> Result<Self> {
        let client = Arc::new(RedisClient::new(config).await?);
        let sessions = SessionStore::new(client.clone());
        let contexts = ContextCache::new(client.clone());
        let agent_states = AgentStateStore::new(client.clone());
        let pubsub = PubSubHandler::new(client.clone()).await?;

        // Start the Pub/Sub listener
        PubSubHandler::start_listener(client.clone(), pubsub.tx.clone()).await;

        Ok(Self {
            client,
            sessions,
            contexts,
            agent_states,
            pubsub,
        })
    }
}

// Needed for the listen_loop
use futures_util::StreamExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_prefixes() {
        assert!(keys::SESSION.starts_with("cca:"));
        assert!(keys::CONTEXT.starts_with("cca:"));
        assert!(keys::AGENT_STATE.starts_with("cca:"));
    }

    #[test]
    fn test_pubsub_message_serialization() {
        let msg = PubSubMessage::TaskAssigned {
            task_id: TaskId::new(),
            agent_id: AgentId::new(),
            description: "Test task".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: PubSubMessage = serde_json::from_str(&json).unwrap();

        assert!(matches!(parsed, PubSubMessage::TaskAssigned { .. }));
    }

    #[test]
    fn test_cached_context_serialization() {
        let ctx = CachedContext {
            agent_id: AgentId::new(),
            context_hash: "abc123".to_string(),
            compressed_context: vec![1, 2, 3],
            token_count: 1000,
            created_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: CachedContext = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.context_hash, "abc123");
        assert_eq!(parsed.token_count, 1000);
    }
}
