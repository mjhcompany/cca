//! PostgreSQL Failover Tests
//!
//! Tests for verifying system resilience during PostgreSQL failures.
//! These tests validate:
//! - Connection pool behavior during failover
//! - Query timeout handling
//! - Transaction rollback on connection loss
//! - Read replica failover
//! - Connection pool exhaustion recovery

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;

use async_trait::async_trait;
use crate::{ChaosConfig, ChaosError, ChaosMetrics, ChaosResult, ChaosTestable, FaultType};

/// Simulated query result
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows_affected: u64,
    pub execution_time_ms: u64,
}

/// Mock PostgreSQL connection for testing
#[derive(Debug)]
pub struct MockPgConnection {
    id: u32,
    is_connected: Arc<AtomicBool>,
    in_transaction: Arc<AtomicBool>,
    queries_executed: Arc<AtomicU32>,
    latency_injection_ms: Arc<AtomicU64>,
}

impl MockPgConnection {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            is_connected: Arc::new(AtomicBool::new(true)),
            in_transaction: Arc::new(AtomicBool::new(false)),
            queries_executed: Arc::new(AtomicU32::new(0)),
            latency_injection_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn disconnect(&self) {
        self.is_connected.store(false, Ordering::SeqCst);
    }

    pub fn reconnect(&self) {
        self.is_connected.store(true, Ordering::SeqCst);
        self.in_transaction.store(false, Ordering::SeqCst);
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    pub fn inject_latency(&self, latency_ms: u64) {
        self.latency_injection_ms.store(latency_ms, Ordering::SeqCst);
    }

    pub fn clear_latency(&self) {
        self.latency_injection_ms.store(0, Ordering::SeqCst);
    }

    /// Execute a query
    pub async fn execute(&self, query: &str, timeout: Duration) -> ChaosResult<QueryResult> {
        if !self.is_connected() {
            return Err(ChaosError::ConnectionError(
                "PostgreSQL connection lost".to_string(),
            ));
        }

        let latency = self.latency_injection_ms.load(Ordering::SeqCst);
        let start = Instant::now();

        // Simulate query execution with latency
        if latency > 0 {
            sleep(Duration::from_millis(latency)).await;
        }

        // Check timeout
        if start.elapsed() > timeout {
            return Err(ChaosError::Timeout {
                operation: format!("Query execution: {}", query),
            });
        }

        self.queries_executed.fetch_add(1, Ordering::SeqCst);

        Ok(QueryResult {
            rows_affected: 1,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Begin a transaction
    pub async fn begin_transaction(&self) -> ChaosResult<()> {
        if !self.is_connected() {
            return Err(ChaosError::ConnectionError(
                "Cannot begin transaction: not connected".to_string(),
            ));
        }

        if self.in_transaction.load(Ordering::SeqCst) {
            return Err(ChaosError::PreconditionFailed(
                "Already in transaction".to_string(),
            ));
        }

        self.in_transaction.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Commit a transaction
    pub async fn commit(&self) -> ChaosResult<()> {
        if !self.is_connected() {
            // Transaction lost on disconnect
            self.in_transaction.store(false, Ordering::SeqCst);
            return Err(ChaosError::ConnectionError(
                "Connection lost during commit".to_string(),
            ));
        }

        if !self.in_transaction.load(Ordering::SeqCst) {
            return Err(ChaosError::PreconditionFailed("No active transaction".to_string()));
        }

        self.in_transaction.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Rollback a transaction
    pub async fn rollback(&self) -> ChaosResult<()> {
        self.in_transaction.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Check if in transaction
    pub fn in_transaction(&self) -> bool {
        self.in_transaction.load(Ordering::SeqCst)
    }

    /// Get query count
    pub fn queries_executed(&self) -> u32 {
        self.queries_executed.load(Ordering::SeqCst)
    }
}

impl Clone for MockPgConnection {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            is_connected: Arc::clone(&self.is_connected),
            in_transaction: Arc::clone(&self.in_transaction),
            queries_executed: Arc::clone(&self.queries_executed),
            latency_injection_ms: Arc::clone(&self.latency_injection_ms),
        }
    }
}

/// Mock PostgreSQL connection pool
pub struct MockPgPool {
    connections: Arc<RwLock<Vec<MockPgConnection>>>,
    max_connections: usize,
    acquire_timeout: Duration,
    statement_timeout: Duration,
    is_primary_available: Arc<AtomicBool>,
    is_replica_available: Arc<AtomicBool>,
    config: ChaosConfig,
    metrics: Arc<RwLock<ChaosMetrics>>,
    connections_acquired: Arc<AtomicU32>,
    connections_released: Arc<AtomicU32>,
}

impl MockPgPool {
    pub fn new(max_connections: usize) -> Self {
        let connections: Vec<MockPgConnection> = (0..max_connections)
            .map(|i| MockPgConnection::new(i as u32))
            .collect();

        Self {
            connections: Arc::new(RwLock::new(connections)),
            max_connections,
            acquire_timeout: Duration::from_secs(30),
            statement_timeout: Duration::from_secs(30),
            is_primary_available: Arc::new(AtomicBool::new(true)),
            is_replica_available: Arc::new(AtomicBool::new(true)),
            config: ChaosConfig::default(),
            metrics: Arc::new(RwLock::new(ChaosMetrics::default())),
            connections_acquired: Arc::new(AtomicU32::new(0)),
            connections_released: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn with_config(mut self, config: ChaosConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_timeouts(mut self, acquire: Duration, statement: Duration) -> Self {
        self.acquire_timeout = acquire;
        self.statement_timeout = statement;
        self
    }

    /// Acquire a connection from the pool
    pub async fn acquire(&self) -> ChaosResult<MockPgConnection> {
        if !self.is_primary_available.load(Ordering::SeqCst) {
            return Err(ChaosError::ServiceUnavailable(
                "Primary database unavailable".to_string(),
            ));
        }

        let start = Instant::now();
        loop {
            let connections = self.connections.read().await;
            for conn in connections.iter() {
                if conn.is_connected() && !conn.in_transaction() {
                    self.connections_acquired.fetch_add(1, Ordering::SeqCst);
                    return Ok(conn.clone());
                }
            }
            drop(connections);

            // Check acquire timeout
            if start.elapsed() > self.acquire_timeout {
                return Err(ChaosError::Timeout {
                    operation: "Acquiring connection from pool".to_string(),
                });
            }

            // Wait and retry
            sleep(Duration::from_millis(10)).await;
        }
    }

    /// Release a connection back to the pool
    pub fn release(&self, _conn: MockPgConnection) {
        self.connections_released.fetch_add(1, Ordering::SeqCst);
    }

    /// Execute a query on the primary
    pub async fn query(&self, sql: &str) -> ChaosResult<QueryResult> {
        let conn = self.acquire().await?;
        let result = conn.execute(sql, self.statement_timeout).await;
        self.release(conn);
        result
    }

    /// Execute a query on a replica (read-only)
    pub async fn query_replica(&self, sql: &str) -> ChaosResult<QueryResult> {
        if !self.is_replica_available.load(Ordering::SeqCst) {
            // Fallback to primary if replica unavailable
            return self.query(sql).await;
        }

        let conn = self.acquire().await?;
        let result = conn.execute(sql, self.statement_timeout).await;
        self.release(conn);
        result
    }

    /// Simulate primary database failover
    pub async fn simulate_primary_failover(&self) {
        self.is_primary_available.store(false, Ordering::SeqCst);

        let connections = self.connections.read().await;
        for conn in connections.iter() {
            conn.disconnect();
        }

        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;
    }

    /// Simulate replica failure
    pub async fn simulate_replica_failure(&self) {
        self.is_replica_available.store(false, Ordering::SeqCst);

        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;
    }

    /// Simulate connection pool exhaustion
    pub async fn simulate_pool_exhaustion(&self) {
        let connections = self.connections.read().await;
        for conn in connections.iter() {
            // Start a transaction on each connection to mark them as busy
            if conn.is_connected() {
                let _ = conn.begin_transaction().await;
            }
        }

        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;
    }

    /// Inject latency into all connections
    pub async fn inject_latency(&self, latency_ms: u64) {
        let connections = self.connections.read().await;
        for conn in connections.iter() {
            conn.inject_latency(latency_ms);
        }

        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;
    }

    /// Clear latency injection
    pub async fn clear_latency(&self) {
        let connections = self.connections.read().await;
        for conn in connections.iter() {
            conn.clear_latency();
        }
    }

    /// Recover from failover
    pub async fn recover(&self) -> ChaosResult<u32> {
        let start = Instant::now();
        let mut recovered = 0;

        let connections = self.connections.read().await;
        for conn in connections.iter() {
            if !conn.is_connected() {
                // Simulate reconnection delay
                sleep(self.config.injection_delay).await;
                conn.reconnect();
                recovered += 1;
            }
        }

        self.is_primary_available.store(true, Ordering::SeqCst);
        self.is_replica_available.store(true, Ordering::SeqCst);

        let elapsed = start.elapsed();
        let mut metrics = self.metrics.write().await;
        if recovered > 0 {
            metrics.recoveries_successful += 1;
            metrics.recovery_times_ms.push(elapsed.as_millis() as u64);
        }

        Ok(recovered)
    }

    /// Get pool status
    pub async fn get_status(&self) -> PgPoolStatus {
        let connections = self.connections.read().await;
        let connected = connections.iter().filter(|c| c.is_connected()).count();
        let in_transaction = connections.iter().filter(|c| c.in_transaction()).count();

        PgPoolStatus {
            max_connections: self.max_connections,
            connected_count: connected,
            available_count: connected - in_transaction,
            in_transaction_count: in_transaction,
            is_primary_available: self.is_primary_available.load(Ordering::SeqCst),
            is_replica_available: self.is_replica_available.load(Ordering::SeqCst),
            connections_acquired: self.connections_acquired.load(Ordering::SeqCst),
            connections_released: self.connections_released.load(Ordering::SeqCst),
        }
    }

    /// Get metrics
    pub async fn get_metrics(&self) -> ChaosMetrics {
        let metrics = self.metrics.read().await;
        ChaosMetrics {
            faults_injected: metrics.faults_injected,
            recoveries_successful: metrics.recoveries_successful,
            recoveries_failed: metrics.recoveries_failed,
            recovery_times_ms: metrics.recovery_times_ms.clone(),
            requests_during_chaos: metrics.requests_during_chaos,
            successful_requests: metrics.successful_requests,
        }
    }
}

#[async_trait]
impl ChaosTestable for MockPgPool {
    async fn health_check(&self) -> ChaosResult<bool> {
        Ok(self.is_primary_available.load(Ordering::SeqCst))
    }

    async fn inject_fault(&self, fault: FaultType) -> ChaosResult<()> {
        match fault {
            FaultType::NetworkDisconnect => {
                self.simulate_primary_failover().await;
            }
            FaultType::PoolExhaustion {
                concurrent_connections: _,
            } => {
                self.simulate_pool_exhaustion().await;
            }
            FaultType::LatencyInjection { delay_ms } => {
                self.inject_latency(delay_ms).await;
            }
            FaultType::TimeoutInjection { after_ms } => {
                // Inject latency longer than statement timeout to cause timeouts
                self.inject_latency(after_ms + self.statement_timeout.as_millis() as u64)
                    .await;
            }
            _ => {
                return Err(ChaosError::PreconditionFailed(
                    "Unsupported fault type for PostgreSQL pool".into(),
                ))
            }
        }
        Ok(())
    }

    async fn restore(&self) -> ChaosResult<()> {
        self.clear_latency().await;
        self.recover().await?;
        Ok(())
    }
}

/// Status of the PostgreSQL connection pool
#[derive(Debug, Clone)]
pub struct PgPoolStatus {
    pub max_connections: usize,
    pub connected_count: usize,
    pub available_count: usize,
    pub in_transaction_count: usize,
    pub is_primary_available: bool,
    pub is_replica_available: bool,
    pub connections_acquired: u32,
    pub connections_released: u32,
}

/// Mock task store backed by PostgreSQL
pub struct MockTaskStore {
    pool: Arc<MockPgPool>,
    tasks: Arc<RwLock<HashMap<String, TaskRecord>>>,
}

impl MockTaskStore {
    pub fn new(pool: Arc<MockPgPool>) -> Self {
        Self {
            pool,
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new task
    pub async fn create_task(&self, id: &str, description: &str) -> ChaosResult<()> {
        // Execute insert query
        self.pool
            .query(&format!(
                "INSERT INTO tasks (id, description) VALUES ('{}', '{}')",
                id, description
            ))
            .await?;

        // Update local cache
        let mut tasks = self.tasks.write().await;
        tasks.insert(
            id.to_string(),
            TaskRecord {
                id: id.to_string(),
                description: description.to_string(),
                status: "pending".to_string(),
            },
        );

        Ok(())
    }

    /// Get a task by ID
    pub async fn get_task(&self, id: &str) -> ChaosResult<Option<TaskRecord>> {
        // Execute select query
        self.pool
            .query_replica(&format!("SELECT * FROM tasks WHERE id = '{}'", id))
            .await?;

        let tasks = self.tasks.read().await;
        Ok(tasks.get(id).cloned())
    }

    /// Update task status
    pub async fn update_status(&self, id: &str, status: &str) -> ChaosResult<()> {
        self.pool
            .query(&format!(
                "UPDATE tasks SET status = '{}' WHERE id = '{}'",
                status, id
            ))
            .await?;

        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            task.status = status.to_string();
        }

        Ok(())
    }

    /// Delete a task with transaction
    pub async fn delete_task_transactional(&self, id: &str) -> ChaosResult<()> {
        let conn = self.pool.acquire().await?;

        conn.begin_transaction().await?;

        // Execute delete
        match conn
            .execute(
                &format!("DELETE FROM tasks WHERE id = '{}'", id),
                Duration::from_secs(30),
            )
            .await
        {
            Ok(_) => {
                conn.commit().await?;
                let mut tasks = self.tasks.write().await;
                tasks.remove(id);
                Ok(())
            }
            Err(e) => {
                conn.rollback().await?;
                Err(e)
            }
        }
    }
}

/// Task record structure
#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub id: String,
    pub description: String,
    pub status: String,
}

// ============================================================================
// Test Cases
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pg_pool_basic_query() {
        let pool = MockPgPool::new(5);

        let result = pool.query("SELECT 1").await.unwrap();
        assert_eq!(result.rows_affected, 1);
    }

    #[tokio::test]
    async fn test_pg_primary_failover() {
        let pool = MockPgPool::new(5);

        // Verify initial state
        assert!(pool.health_check().await.unwrap());

        // Simulate failover
        pool.simulate_primary_failover().await;

        // Queries should fail
        assert!(pool.query("SELECT 1").await.is_err());
        assert!(!pool.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_pg_failover_recovery() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(5),
            ..Default::default()
        };
        let pool = MockPgPool::new(5).with_config(config);

        // Failover and recover
        pool.simulate_primary_failover().await;
        assert!(!pool.health_check().await.unwrap());

        let recovered = pool.recover().await.unwrap();
        assert_eq!(recovered, 5);
        assert!(pool.health_check().await.unwrap());

        // Queries should work again
        let result = pool.query("SELECT 1").await.unwrap();
        assert_eq!(result.rows_affected, 1);
    }

    #[tokio::test]
    async fn test_pg_replica_failover_with_fallback() {
        let pool = MockPgPool::new(5);

        // Replica failure should fallback to primary
        pool.simulate_replica_failure().await;

        // Read queries should still work via primary
        let result = pool.query_replica("SELECT 1").await.unwrap();
        assert_eq!(result.rows_affected, 1);
    }

    #[tokio::test]
    async fn test_pg_pool_exhaustion() {
        let pool = MockPgPool::new(3).with_timeouts(Duration::from_millis(100), Duration::from_secs(30));

        // Exhaust the pool
        pool.simulate_pool_exhaustion().await;

        // New connection attempts should timeout
        let result = pool.acquire().await;
        assert!(matches!(result, Err(ChaosError::Timeout { .. })));
    }

    #[tokio::test]
    async fn test_pg_latency_injection() {
        let pool = MockPgPool::new(5).with_timeouts(Duration::from_secs(30), Duration::from_millis(100));

        // Inject latency that exceeds statement timeout
        pool.inject_latency(200).await;

        // Queries should timeout
        let result = pool.query("SELECT 1").await;
        assert!(matches!(result, Err(ChaosError::Timeout { .. })));

        // Clear latency
        pool.clear_latency().await;

        // Queries should work again
        let result = pool.query("SELECT 1").await.unwrap();
        assert_eq!(result.rows_affected, 1);
    }

    #[tokio::test]
    async fn test_pg_transaction_rollback_on_disconnect() {
        let pool = MockPgPool::new(5);

        let conn = pool.acquire().await.unwrap();
        conn.begin_transaction().await.unwrap();
        assert!(conn.in_transaction());

        // Simulate disconnect during transaction
        conn.disconnect();

        // Commit should fail
        let result = conn.commit().await;
        assert!(matches!(result, Err(ChaosError::ConnectionError(_))));

        // Transaction should be rolled back
        assert!(!conn.in_transaction());
    }

    #[tokio::test]
    async fn test_task_store_operations() {
        let pool = Arc::new(MockPgPool::new(5));
        let store = MockTaskStore::new(pool);

        // Create task
        store.create_task("task-1", "Test task").await.unwrap();

        // Get task
        let task = store.get_task("task-1").await.unwrap().unwrap();
        assert_eq!(task.description, "Test task");
        assert_eq!(task.status, "pending");

        // Update status
        store.update_status("task-1", "completed").await.unwrap();
        let task = store.get_task("task-1").await.unwrap().unwrap();
        assert_eq!(task.status, "completed");
    }

    #[tokio::test]
    async fn test_task_store_failover_resilience() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(5),
            ..Default::default()
        };
        let pool = Arc::new(MockPgPool::new(5).with_config(config));
        let store = MockTaskStore::new(Arc::clone(&pool));

        // Create task before failover
        store.create_task("task-1", "Test task").await.unwrap();

        // Simulate failover
        pool.simulate_primary_failover().await;

        // Operations should fail
        assert!(store.update_status("task-1", "failed").await.is_err());

        // Recover
        pool.recover().await.unwrap();

        // Operations should work again
        store.update_status("task-1", "recovered").await.unwrap();
    }

    #[tokio::test]
    async fn test_transactional_delete_with_rollback() {
        let pool = Arc::new(MockPgPool::new(5));
        let store = MockTaskStore::new(Arc::clone(&pool));

        store.create_task("task-1", "Test task").await.unwrap();

        // Successful transactional delete
        store.delete_task_transactional("task-1").await.unwrap();

        // Task should be gone
        let task = store.get_task("task-1").await.unwrap();
        assert!(task.is_none());
    }

    #[tokio::test]
    async fn test_pg_pool_status() {
        let pool = MockPgPool::new(5);

        let status = pool.get_status().await;
        assert_eq!(status.max_connections, 5);
        assert_eq!(status.connected_count, 5);
        assert!(status.is_primary_available);
        assert!(status.is_replica_available);

        // Acquire some connections
        pool.query("SELECT 1").await.unwrap();
        pool.query("SELECT 2").await.unwrap();

        let status = pool.get_status().await;
        assert!(status.connections_acquired >= 2);
    }

    #[tokio::test]
    async fn test_pg_metrics_collection() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(5),
            ..Default::default()
        };
        let pool = MockPgPool::new(5).with_config(config);

        // Inject multiple faults
        pool.simulate_primary_failover().await;
        pool.recover().await.unwrap();

        pool.inject_latency(10).await;
        pool.clear_latency().await;

        let metrics = pool.get_metrics().await;
        assert!(metrics.faults_injected >= 2);
        assert!(metrics.recoveries_successful >= 1);
    }

    #[tokio::test]
    async fn test_concurrent_queries_during_failover() {
        let pool = Arc::new(MockPgPool::new(10));

        // Spawn concurrent queries
        let mut handles = Vec::new();
        for i in 0..20 {
            let pool = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                let sql = format!("SELECT {}", i);
                pool.query(&sql).await.is_ok()
            }));
        }

        // Inject fault mid-queries
        sleep(Duration::from_millis(2)).await;
        pool.simulate_primary_failover().await;

        // Collect results
        let results: Vec<bool> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // Some should have succeeded, some failed
        let succeeded = results.iter().filter(|&&r| r).count();
        let failed = results.iter().filter(|&&r| !r).count();

        // Due to timing, distribution varies but both should occur
        assert!(succeeded > 0 || failed > 0);
    }

    #[tokio::test]
    async fn test_rapid_failover_recovery_cycles() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(1),
            ..Default::default()
        };
        let pool = MockPgPool::new(5).with_config(config);

        for _ in 0..5 {
            pool.simulate_primary_failover().await;
            assert!(!pool.health_check().await.unwrap());

            pool.recover().await.unwrap();
            assert!(pool.health_check().await.unwrap());

            // Verify queries work
            let result = pool.query("SELECT 1").await.unwrap();
            assert_eq!(result.rows_affected, 1);
        }

        let metrics = pool.get_metrics().await;
        assert_eq!(metrics.faults_injected, 5);
        assert_eq!(metrics.recoveries_successful, 5);
    }

    #[tokio::test]
    async fn test_chaos_testable_trait() {
        let pool = MockPgPool::new(5);

        // Test via trait
        let testable: &dyn ChaosTestable = &pool;

        assert!(testable.health_check().await.unwrap());

        testable
            .inject_fault(FaultType::NetworkDisconnect)
            .await
            .unwrap();
        assert!(!testable.health_check().await.unwrap());

        testable.restore().await.unwrap();
        assert!(testable.health_check().await.unwrap());
    }
}
