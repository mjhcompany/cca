//! Memory types for ReasoningBank and context management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentId;
use crate::types::PatternId;

/// Pattern stored in ReasoningBank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: PatternId,
    pub agent_id: Option<AgentId>,
    pub pattern_type: PatternType,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub success_count: u32,
    pub failure_count: u32,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Pattern {
    pub fn new(pattern_type: PatternType, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: PatternId::new(),
            agent_id: None,
            pattern_type,
            content: content.into(),
            embedding: None,
            success_count: 0,
            failure_count: 0,
            metadata: serde_json::Value::Null,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            0.0
        } else {
            self.success_count as f64 / total as f64
        }
    }

    pub fn record_success(&mut self) {
        self.success_count += 1;
        self.updated_at = Utc::now();
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.updated_at = Utc::now();
    }
}

/// Types of patterns in ReasoningBank
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// Code pattern (implementation strategy)
    Code,
    /// Task routing pattern
    Routing,
    /// Error handling pattern
    ErrorHandling,
    /// Communication pattern
    Communication,
    /// Optimization pattern
    Optimization,
    /// Custom pattern type
    Custom(String),
}

/// Context snapshot for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub id: Uuid,
    pub agent_id: AgentId,
    pub context_hash: String,
    pub compressed_context: Vec<u8>,
    pub token_count: u64,
    pub created_at: DateTime<Utc>,
}

/// Search match result from similarity search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub pattern: Pattern,
    pub score: f64,
}

/// Context for an agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub agent_id: AgentId,
    pub conversation_history: Vec<ContextMessage>,
    pub working_directory: String,
    pub active_files: Vec<String>,
    pub token_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A message in the context history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// Role in a conversation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}
