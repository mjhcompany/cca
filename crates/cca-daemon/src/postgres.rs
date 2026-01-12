//! PostgreSQL integration for CCA Daemon
//!
//! Provides connection pooling and repositories for persistent storage,
//! including the ReasoningBank with pgvector similarity search.
//!
//! Note: Many methods are infrastructure for future features and not yet called.
#![allow(dead_code)]

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::FromRow;
use tracing::{debug, info};
use uuid::Uuid;

use crate::config::PostgresConfig;

/// PostgreSQL database connection pool
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Create a new database connection pool
    pub async fn new(config: &PostgresConfig) -> Result<Self> {
        info!("Connecting to PostgreSQL at {}", config.url);

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        // Test connection
        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .context("Failed to execute test query")?;

        info!(
            "PostgreSQL connection established (max connections: {})",
            config.max_connections
        );

        Ok(Self { pool })
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

// ============================================================================
// Agent Repository
// ============================================================================

/// Agent record from the database
#[derive(Debug, Clone, FromRow)]
pub struct AgentRecord {
    pub id: Uuid,
    pub role: String,
    pub name: Option<String>,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Repository for agent persistence
pub struct AgentRepository {
    pool: PgPool,
}

impl AgentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new agent
    pub async fn create(&self, role: &str, name: Option<&str>, config: serde_json::Value) -> Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r"
            INSERT INTO agents (id, role, name, config)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(id)
        .bind(role)
        .bind(name)
        .bind(&config)
        .execute(&self.pool)
        .await
        .context("Failed to create agent")?;

        debug!("Created agent {} with role {}", id, role);
        Ok(id)
    }

    /// Get an agent by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<AgentRecord>> {
        let agent = sqlx::query_as::<_, AgentRecord>(
            r"
            SELECT id, role, name, config, created_at, updated_at
            FROM agents
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get agent")?;

        Ok(agent)
    }

    /// List all agents
    pub async fn list(&self) -> Result<Vec<AgentRecord>> {
        let agents = sqlx::query_as::<_, AgentRecord>(
            r"
            SELECT id, role, name, config, created_at, updated_at
            FROM agents
            ORDER BY created_at DESC
            ",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to list agents")?;

        Ok(agents)
    }

    /// Update agent config
    pub async fn update_config(&self, id: Uuid, config: serde_json::Value) -> Result<()> {
        sqlx::query(
            r"
            UPDATE agents SET config = $2 WHERE id = $1
            ",
        )
        .bind(id)
        .bind(&config)
        .execute(&self.pool)
        .await
        .context("Failed to update agent config")?;

        Ok(())
    }

    /// Delete an agent
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete agent")?;

        Ok(())
    }
}

// ============================================================================
// Pattern Repository (ReasoningBank)
// ============================================================================

/// Pattern types for the ReasoningBank
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternType {
    /// Successful problem-solving approach
    Solution,
    /// Error recovery pattern
    ErrorRecovery,
    /// Code refactoring pattern
    Refactoring,
    /// Optimization pattern
    Optimization,
    /// Testing pattern
    Testing,
    /// General reasoning pattern
    Reasoning,
}

impl PatternType {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn as_str(&self) -> &'static str {
        match self {
            PatternType::Solution => "solution",
            PatternType::ErrorRecovery => "error_recovery",
            PatternType::Refactoring => "refactoring",
            PatternType::Optimization => "optimization",
            PatternType::Testing => "testing",
            PatternType::Reasoning => "reasoning",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "solution" => Some(PatternType::Solution),
            "error_recovery" => Some(PatternType::ErrorRecovery),
            "refactoring" => Some(PatternType::Refactoring),
            "optimization" => Some(PatternType::Optimization),
            "testing" => Some(PatternType::Testing),
            "reasoning" => Some(PatternType::Reasoning),
            _ => None,
        }
    }
}

/// Pattern record from the database
#[derive(Debug, Clone, FromRow)]
pub struct PatternRecord {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub pattern_type: String,
    pub content: String,
    pub success_count: i32,
    pub failure_count: i32,
    pub success_rate: Option<f64>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Pattern with similarity score from vector search
#[derive(Debug, Clone)]
pub struct PatternWithScore {
    pub pattern: PatternRecord,
    pub similarity: f64,
}

/// Repository for pattern storage (ReasoningBank)
pub struct PatternRepository {
    pool: PgPool,
}

impl PatternRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Store a new pattern with optional embedding
    pub async fn create(
        &self,
        agent_id: Option<Uuid>,
        pattern_type: PatternType,
        content: &str,
        embedding: Option<&[f32]>,
        metadata: serde_json::Value,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();

        if let Some(emb) = embedding {
            // Store with embedding
            let embedding_str = format!(
                "[{}]",
                emb.iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );

            sqlx::query(
                r"
                INSERT INTO patterns (id, agent_id, pattern_type, content, embedding, metadata)
                VALUES ($1, $2, $3, $4, $5::vector, $6)
                ",
            )
            .bind(id)
            .bind(agent_id)
            .bind(pattern_type.as_str())
            .bind(content)
            .bind(&embedding_str)
            .bind(&metadata)
            .execute(&self.pool)
            .await
            .context("Failed to create pattern with embedding")?;
        } else {
            // Store without embedding
            sqlx::query(
                r"
                INSERT INTO patterns (id, agent_id, pattern_type, content, metadata)
                VALUES ($1, $2, $3, $4, $5)
                ",
            )
            .bind(id)
            .bind(agent_id)
            .bind(pattern_type.as_str())
            .bind(content)
            .bind(&metadata)
            .execute(&self.pool)
            .await
            .context("Failed to create pattern")?;
        }

        debug!("Created pattern {} of type {:?}", id, pattern_type);
        Ok(id)
    }

    /// Get a pattern by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<PatternRecord>> {
        let pattern = sqlx::query_as::<_, PatternRecord>(
            r"
            SELECT id, agent_id, pattern_type, content, success_count, failure_count,
                   success_rate, metadata, created_at, updated_at
            FROM patterns
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get pattern")?;

        Ok(pattern)
    }

    /// Search patterns by similarity using pgvector
    pub async fn search_similar(
        &self,
        embedding: &[f32],
        limit: i32,
        min_similarity: f64,
    ) -> Result<Vec<PatternWithScore>> {
        let embedding_str = format!(
            "[{}]",
            embedding
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );

        // Use cosine similarity (1 - cosine_distance)
        let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, i32, i32, Option<f64>, serde_json::Value, DateTime<Utc>, DateTime<Utc>, f64)>(
            r"
            SELECT id, agent_id, pattern_type, content, success_count, failure_count,
                   success_rate, metadata, created_at, updated_at,
                   1 - (embedding <=> $1::vector) as similarity
            FROM patterns
            WHERE embedding IS NOT NULL
              AND 1 - (embedding <=> $1::vector) >= $3
            ORDER BY embedding <=> $1::vector
            LIMIT $2
            ",
        )
        .bind(&embedding_str)
        .bind(limit)
        .bind(min_similarity)
        .fetch_all(&self.pool)
        .await
        .context("Failed to search similar patterns")?;

        let patterns = rows
            .into_iter()
            .map(|row| PatternWithScore {
                pattern: PatternRecord {
                    id: row.0,
                    agent_id: row.1,
                    pattern_type: row.2,
                    content: row.3,
                    success_count: row.4,
                    failure_count: row.5,
                    success_rate: row.6,
                    metadata: row.7,
                    created_at: row.8,
                    updated_at: row.9,
                },
                similarity: row.10,
            })
            .collect();

        Ok(patterns)
    }

    /// Search patterns by text content (full-text search fallback)
    pub async fn search_text(&self, query: &str, limit: i32) -> Result<Vec<PatternRecord>> {
        let patterns = sqlx::query_as::<_, PatternRecord>(
            r"
            SELECT id, agent_id, pattern_type, content, success_count, failure_count,
                   success_rate, metadata, created_at, updated_at
            FROM patterns
            WHERE content ILIKE '%' || $1 || '%'
            ORDER BY success_rate DESC NULLS LAST, created_at DESC
            LIMIT $2
            ",
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to search patterns by text")?;

        Ok(patterns)
    }

    /// Get patterns by type
    pub async fn get_by_type(&self, pattern_type: PatternType, limit: i32) -> Result<Vec<PatternRecord>> {
        let patterns = sqlx::query_as::<_, PatternRecord>(
            r"
            SELECT id, agent_id, pattern_type, content, success_count, failure_count,
                   success_rate, metadata, created_at, updated_at
            FROM patterns
            WHERE pattern_type = $1
            ORDER BY success_rate DESC NULLS LAST, created_at DESC
            LIMIT $2
            ",
        )
        .bind(pattern_type.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get patterns by type")?;

        Ok(patterns)
    }

    /// Record success for a pattern
    pub async fn record_success(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            r"
            UPDATE patterns
            SET success_count = success_count + 1
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to record success")?;

        Ok(())
    }

    /// Record failure for a pattern
    pub async fn record_failure(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            r"
            UPDATE patterns
            SET failure_count = failure_count + 1
            WHERE id = $1
            ",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to record failure")?;

        Ok(())
    }

    /// Update pattern embedding
    pub async fn update_embedding(&self, id: Uuid, embedding: &[f32]) -> Result<()> {
        let embedding_str = format!(
            "[{}]",
            embedding
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );

        sqlx::query(
            r"
            UPDATE patterns
            SET embedding = $2::vector
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(&embedding_str)
        .execute(&self.pool)
        .await
        .context("Failed to update embedding")?;

        Ok(())
    }

    /// Get top performing patterns
    pub async fn get_top_patterns(&self, limit: i32) -> Result<Vec<PatternRecord>> {
        let patterns = sqlx::query_as::<_, PatternRecord>(
            r"
            SELECT id, agent_id, pattern_type, content, success_count, failure_count,
                   success_rate, metadata, created_at, updated_at
            FROM patterns
            WHERE success_count + failure_count >= 5
            ORDER BY success_rate DESC NULLS LAST
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get top patterns")?;

        Ok(patterns)
    }

    /// Delete a pattern
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM patterns WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete pattern")?;

        Ok(())
    }

    /// Count all patterns
    pub async fn count(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM patterns")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count patterns")?;

        Ok(count.0)
    }
}

// ============================================================================
// Task Repository
// ============================================================================

/// Task record from the database
#[derive(Debug, Clone, FromRow)]
pub struct TaskRecord {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub description: String,
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub tokens_used: Option<i32>,
    pub duration_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Repository for task history
pub struct TaskRepository {
    pool: PgPool,
}

impl TaskRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new task record
    pub async fn create(&self, agent_id: Option<Uuid>, description: &str) -> Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r"
            INSERT INTO tasks (id, agent_id, description)
            VALUES ($1, $2, $3)
            ",
        )
        .bind(id)
        .bind(agent_id)
        .bind(description)
        .execute(&self.pool)
        .await
        .context("Failed to create task")?;

        debug!("Created task {}", id);
        Ok(id)
    }

    /// Get a task by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<TaskRecord>> {
        let task = sqlx::query_as::<_, TaskRecord>(
            r"
            SELECT id, agent_id, description, status, result, tokens_used, duration_ms,
                   created_at, completed_at
            FROM tasks
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get task")?;

        Ok(task)
    }

    /// Update task completion
    pub async fn complete(
        &self,
        id: Uuid,
        status: &str,
        result: serde_json::Value,
        tokens_used: i32,
        duration_ms: i32,
    ) -> Result<()> {
        sqlx::query(
            r"
            UPDATE tasks
            SET status = $2, result = $3, tokens_used = $4, duration_ms = $5,
                completed_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(status)
        .bind(&result)
        .bind(tokens_used)
        .bind(duration_ms)
        .execute(&self.pool)
        .await
        .context("Failed to complete task")?;

        Ok(())
    }

    /// List recent tasks
    pub async fn list_recent(&self, limit: i32) -> Result<Vec<TaskRecord>> {
        let tasks = sqlx::query_as::<_, TaskRecord>(
            r"
            SELECT id, agent_id, description, status, result, tokens_used, duration_ms,
                   created_at, completed_at
            FROM tasks
            ORDER BY created_at DESC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list tasks")?;

        Ok(tasks)
    }

    /// Get tasks by agent
    pub async fn get_by_agent(&self, agent_id: Uuid, limit: i32) -> Result<Vec<TaskRecord>> {
        let tasks = sqlx::query_as::<_, TaskRecord>(
            r"
            SELECT id, agent_id, description, status, result, tokens_used, duration_ms,
                   created_at, completed_at
            FROM tasks
            WHERE agent_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            ",
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get tasks by agent")?;

        Ok(tasks)
    }

    /// Get task statistics
    pub async fn get_stats(&self) -> Result<TaskStats> {
        let stats = sqlx::query_as::<_, TaskStats>(
            r"
            SELECT
                COUNT(*) as total_tasks,
                COUNT(*) FILTER (WHERE status = 'completed') as completed_tasks,
                COUNT(*) FILTER (WHERE status = 'failed') as failed_tasks,
                COALESCE(SUM(tokens_used), 0) as total_tokens,
                COALESCE(AVG(duration_ms), 0) as avg_duration_ms
            FROM tasks
            ",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to get task stats")?;

        Ok(stats)
    }
}

/// Task statistics
#[derive(Debug, Clone, FromRow)]
pub struct TaskStats {
    pub total_tasks: i64,
    pub completed_tasks: i64,
    pub failed_tasks: i64,
    pub total_tokens: i64,
    pub avg_duration_ms: f64,
}

// ============================================================================
// Context Snapshot Repository
// ============================================================================

/// Context snapshot record
#[derive(Debug, Clone, FromRow)]
pub struct ContextSnapshotRecord {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub context_hash: String,
    pub compressed_context: Option<Vec<u8>>,
    pub token_count: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// Repository for context snapshots
pub struct ContextSnapshotRepository {
    pool: PgPool,
}

impl ContextSnapshotRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Save a context snapshot
    pub async fn save(
        &self,
        agent_id: Uuid,
        context_hash: &str,
        compressed_context: &[u8],
        token_count: i32,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r"
            INSERT INTO context_snapshots (id, agent_id, context_hash, compressed_context, token_count)
            VALUES ($1, $2, $3, $4, $5)
            ",
        )
        .bind(id)
        .bind(agent_id)
        .bind(context_hash)
        .bind(compressed_context)
        .bind(token_count)
        .execute(&self.pool)
        .await
        .context("Failed to save context snapshot")?;

        debug!("Saved context snapshot {} for agent {}", id, agent_id);
        Ok(id)
    }

    /// Get the latest snapshot for an agent
    pub async fn get_latest(&self, agent_id: Uuid) -> Result<Option<ContextSnapshotRecord>> {
        let snapshot = sqlx::query_as::<_, ContextSnapshotRecord>(
            r"
            SELECT id, agent_id, context_hash, compressed_context, token_count, created_at
            FROM context_snapshots
            WHERE agent_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get latest snapshot")?;

        Ok(snapshot)
    }

    /// Get snapshot by hash
    pub async fn get_by_hash(&self, context_hash: &str) -> Result<Option<ContextSnapshotRecord>> {
        let snapshot = sqlx::query_as::<_, ContextSnapshotRecord>(
            r"
            SELECT id, agent_id, context_hash, compressed_context, token_count, created_at
            FROM context_snapshots
            WHERE context_hash = $1
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(context_hash)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get snapshot by hash")?;

        Ok(snapshot)
    }

    /// Delete old snapshots, keeping only the N most recent per agent
    pub async fn cleanup(&self, keep_count: i32) -> Result<u64> {
        let result = sqlx::query(
            r"
            DELETE FROM context_snapshots
            WHERE id NOT IN (
                SELECT id FROM (
                    SELECT id, ROW_NUMBER() OVER (PARTITION BY agent_id ORDER BY created_at DESC) as rn
                    FROM context_snapshots
                ) ranked
                WHERE rn <= $1
            )
            ",
        )
        .bind(keep_count)
        .execute(&self.pool)
        .await
        .context("Failed to cleanup snapshots")?;

        Ok(result.rows_affected())
    }
}

// ============================================================================
// RL Experience Repository
// ============================================================================

/// RL experience record
#[derive(Debug, Clone, FromRow)]
pub struct RLExperienceRecord {
    pub id: Uuid,
    pub state: serde_json::Value,
    pub action: serde_json::Value,
    pub reward: f64,
    pub next_state: Option<serde_json::Value>,
    pub done: bool,
    pub algorithm: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Repository for RL experience replay
pub struct RLExperienceRepository {
    pool: PgPool,
}

impl RLExperienceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Store an experience
    pub async fn store(
        &self,
        state: serde_json::Value,
        action: serde_json::Value,
        reward: f64,
        next_state: Option<serde_json::Value>,
        done: bool,
        algorithm: Option<&str>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r"
            INSERT INTO rl_experiences (id, state, action, reward, next_state, done, algorithm)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
        )
        .bind(id)
        .bind(&state)
        .bind(&action)
        .bind(reward)
        .bind(&next_state)
        .bind(done)
        .bind(algorithm)
        .execute(&self.pool)
        .await
        .context("Failed to store RL experience")?;

        Ok(id)
    }

    /// Sample random experiences for training
    pub async fn sample(&self, count: i32, algorithm: Option<&str>) -> Result<Vec<RLExperienceRecord>> {
        let experiences = if let Some(alg) = algorithm {
            sqlx::query_as::<_, RLExperienceRecord>(
                r"
                SELECT id, state, action, reward, next_state, done, algorithm, created_at
                FROM rl_experiences
                WHERE algorithm = $2
                ORDER BY RANDOM()
                LIMIT $1
                ",
            )
            .bind(count)
            .bind(alg)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, RLExperienceRecord>(
                r"
                SELECT id, state, action, reward, next_state, done, algorithm, created_at
                FROM rl_experiences
                ORDER BY RANDOM()
                LIMIT $1
                ",
            )
            .bind(count)
            .fetch_all(&self.pool)
            .await
        }
        .context("Failed to sample experiences")?;

        Ok(experiences)
    }

    /// Get recent experiences
    pub async fn get_recent(&self, count: i32) -> Result<Vec<RLExperienceRecord>> {
        let experiences = sqlx::query_as::<_, RLExperienceRecord>(
            r"
            SELECT id, state, action, reward, next_state, done, algorithm, created_at
            FROM rl_experiences
            ORDER BY created_at DESC
            LIMIT $1
            ",
        )
        .bind(count)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get recent experiences")?;

        Ok(experiences)
    }

    /// Count experiences by algorithm
    pub async fn count_by_algorithm(&self) -> Result<Vec<(String, i64)>> {
        let counts = sqlx::query_as::<_, (Option<String>, i64)>(
            r"
            SELECT algorithm, COUNT(*) as count
            FROM rl_experiences
            GROUP BY algorithm
            ",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to count by algorithm")?;

        Ok(counts
            .into_iter()
            .map(|(alg, count)| (alg.unwrap_or_else(|| "unknown".to_string()), count))
            .collect())
    }

    /// Delete old experiences
    pub async fn cleanup(&self, keep_count: i64) -> Result<u64> {
        let result = sqlx::query(
            r"
            DELETE FROM rl_experiences
            WHERE id NOT IN (
                SELECT id FROM rl_experiences
                ORDER BY created_at DESC
                LIMIT $1
            )
            ",
        )
        .bind(keep_count)
        .execute(&self.pool)
        .await
        .context("Failed to cleanup experiences")?;

        Ok(result.rows_affected())
    }
}

// ============================================================================
// Combined Database Services
// ============================================================================

/// Combined PostgreSQL services
pub struct PostgresServices {
    pub db: Arc<Database>,
    pub agents: AgentRepository,
    pub patterns: PatternRepository,
    pub tasks: TaskRepository,
    pub snapshots: ContextSnapshotRepository,
    pub experiences: RLExperienceRepository,
}

impl PostgresServices {
    /// Initialize all PostgreSQL services
    pub async fn new(config: &PostgresConfig) -> Result<Self> {
        let db = Arc::new(Database::new(config).await?);
        let pool = db.pool().clone();

        let agents = AgentRepository::new(pool.clone());
        let patterns = PatternRepository::new(pool.clone());
        let tasks = TaskRepository::new(pool.clone());
        let snapshots = ContextSnapshotRepository::new(pool.clone());
        let experiences = RLExperienceRepository::new(pool);

        Ok(Self {
            db,
            agents,
            patterns,
            tasks,
            snapshots,
            experiences,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_type_conversion() {
        assert_eq!(PatternType::Solution.as_str(), "solution");
        assert_eq!(PatternType::from_str("solution"), Some(PatternType::Solution));
        assert_eq!(PatternType::from_str("unknown"), None);
    }
}
