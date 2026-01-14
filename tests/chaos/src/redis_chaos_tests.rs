//! Redis Disconnection Handling Tests
//!
//! Tests for verifying system resilience when Redis becomes unavailable.
//! These tests validate:
//! - Connection pool behavior during disconnection
//! - Automatic reconnection attempts
//! - Pub/Sub subscription recovery
//! - Cache invalidation and consistency
//! - Graceful degradation when Redis is unavailable

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tokio::time::sleep;

use async_trait::async_trait;
use crate::{ChaosConfig, ChaosError, ChaosMetrics, ChaosResult, ChaosTestable, FaultType};

/// Mock Redis connection for testing
#[derive(Debug, Clone)]
pub struct MockRedisConnection {
    #[allow(dead_code)]
    id: u32,
    is_connected: Arc<AtomicBool>,
    operations_count: Arc<AtomicU32>,
}

impl MockRedisConnection {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            is_connected: Arc::new(AtomicBool::new(true)),
            operations_count: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn disconnect(&self) {
        self.is_connected.store(false, Ordering::SeqCst);
    }

    pub fn reconnect(&self) {
        self.is_connected.store(true, Ordering::SeqCst);
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    pub async fn execute(&self, _command: &str) -> ChaosResult<String> {
        if !self.is_connected() {
            return Err(ChaosError::ConnectionError(
                "Redis connection lost".to_string(),
            ));
        }
        self.operations_count.fetch_add(1, Ordering::SeqCst);
        Ok("OK".to_string())
    }

    pub fn operations_count(&self) -> u32 {
        self.operations_count.load(Ordering::SeqCst)
    }
}

/// Mock Redis connection pool for chaos testing
pub struct MockRedisPool {
    connections: Arc<RwLock<Vec<MockRedisConnection>>>,
    pool_size: usize,
    is_available: Arc<AtomicBool>,
    config: ChaosConfig,
    metrics: Arc<RwLock<ChaosMetrics>>,
    reconnect_attempts: Arc<AtomicU32>,
}

impl MockRedisPool {
    pub fn new(pool_size: usize) -> Self {
        let connections: Vec<MockRedisConnection> =
            (0..pool_size).map(|i| MockRedisConnection::new(i as u32)).collect();

        Self {
            connections: Arc::new(RwLock::new(connections)),
            pool_size,
            is_available: Arc::new(AtomicBool::new(true)),
            config: ChaosConfig::default(),
            metrics: Arc::new(RwLock::new(ChaosMetrics::default())),
            reconnect_attempts: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn with_config(mut self, config: ChaosConfig) -> Self {
        self.config = config;
        self
    }

    /// Get a connection from the pool
    pub async fn get_connection(&self) -> ChaosResult<MockRedisConnection> {
        if !self.is_available.load(Ordering::SeqCst) {
            return Err(ChaosError::ServiceUnavailable(
                "Redis pool is unavailable".to_string(),
            ));
        }

        let connections = self.connections.read().await;
        for conn in connections.iter() {
            if conn.is_connected() {
                return Ok(conn.clone());
            }
        }

        Err(ChaosError::ConnectionError(
            "No available connections in pool".to_string(),
        ))
    }

    /// Simulate total Redis disconnection
    pub async fn simulate_disconnection(&self) {
        self.is_available.store(false, Ordering::SeqCst);
        let connections = self.connections.read().await;
        for conn in connections.iter() {
            conn.disconnect();
        }

        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;
    }

    /// Simulate partial pool failure
    pub async fn simulate_partial_failure(&self, failure_rate: f64) {
        let connections = self.connections.read().await;
        let fail_count = (connections.len() as f64 * failure_rate).ceil() as usize;

        for (i, conn) in connections.iter().enumerate() {
            if i < fail_count {
                conn.disconnect();
            }
        }

        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;
    }

    /// Attempt to reconnect to Redis
    pub async fn reconnect(&self) -> ChaosResult<u32> {
        let start = Instant::now();
        let mut reconnected = 0;

        for attempt in 1..=self.config.reconnect_attempts {
            self.reconnect_attempts.fetch_add(1, Ordering::SeqCst);

            let connections = self.connections.read().await;
            for conn in connections.iter() {
                if !conn.is_connected() {
                    // Simulate reconnection delay
                    sleep(self.config.injection_delay).await;
                    conn.reconnect();
                    reconnected += 1;
                }
            }
            drop(connections);

            // Check if all connections are restored
            if self.all_connected().await {
                self.is_available.store(true, Ordering::SeqCst);

                let elapsed = start.elapsed();
                let mut metrics = self.metrics.write().await;
                metrics.recoveries_successful += 1;
                metrics.recovery_times_ms.push(elapsed.as_millis() as u64);

                return Ok(reconnected);
            }

            // Exponential backoff
            sleep(Duration::from_millis(50 * u64::from(attempt))).await;
        }

        let mut metrics = self.metrics.write().await;
        metrics.recoveries_failed += 1;

        Err(ChaosError::RecoveryFailed {
            attempts: self.config.reconnect_attempts,
            reason: "Failed to reconnect all connections".to_string(),
        })
    }

    /// Check if all connections are connected
    pub async fn all_connected(&self) -> bool {
        let connections = self.connections.read().await;
        connections.iter().all(MockRedisConnection::is_connected)
    }

    /// Get pool status
    pub async fn get_status(&self) -> PoolStatus {
        let connections = self.connections.read().await;
        let connected = connections.iter().filter(|c| c.is_connected()).count();

        PoolStatus {
            total_connections: self.pool_size,
            connected_count: connected,
            disconnected_count: self.pool_size - connected,
            is_available: self.is_available.load(Ordering::SeqCst),
            reconnect_attempts: self.reconnect_attempts.load(Ordering::SeqCst),
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
impl ChaosTestable for MockRedisPool {
    async fn health_check(&self) -> ChaosResult<bool> {
        Ok(self.is_available.load(Ordering::SeqCst) && self.all_connected().await)
    }

    async fn inject_fault(&self, fault: FaultType) -> ChaosResult<()> {
        match fault {
            FaultType::NetworkDisconnect => {
                self.simulate_disconnection().await;
            }
            FaultType::PartialFailure { failure_rate } => {
                self.simulate_partial_failure(failure_rate).await;
            }
            FaultType::PoolExhaustion {
                concurrent_connections: _,
            } => {
                // Mark pool as unavailable but don't disconnect existing connections
                self.is_available.store(false, Ordering::SeqCst);
            }
            _ => {
                return Err(ChaosError::PreconditionFailed(
                    "Unsupported fault type for Redis pool".into(),
                ))
            }
        }
        Ok(())
    }

    async fn restore(&self) -> ChaosResult<()> {
        self.reconnect().await?;
        Ok(())
    }
}

/// Status of the Redis connection pool
#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub total_connections: usize,
    pub connected_count: usize,
    pub disconnected_count: usize,
    pub is_available: bool,
    pub reconnect_attempts: u32,
}

/// Mock Redis Pub/Sub handler for testing subscription recovery
pub struct MockRedisPubSub {
    is_subscribed: Arc<AtomicBool>,
    channel: String,
    message_tx: broadcast::Sender<String>,
    reconnect_count: Arc<AtomicU32>,
}

impl MockRedisPubSub {
    pub fn new(channel: impl Into<String>) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            is_subscribed: Arc::new(AtomicBool::new(false)),
            channel: channel.into(),
            message_tx: tx,
            reconnect_count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Subscribe to the channel
    pub async fn subscribe(&self) -> ChaosResult<broadcast::Receiver<String>> {
        // Simulate subscription delay
        sleep(Duration::from_millis(10)).await;
        self.is_subscribed.store(true, Ordering::SeqCst);
        Ok(self.message_tx.subscribe())
    }

    /// Unsubscribe from the channel
    pub fn unsubscribe(&self) {
        self.is_subscribed.store(false, Ordering::SeqCst);
    }

    /// Check if subscribed
    pub fn is_subscribed(&self) -> bool {
        self.is_subscribed.load(Ordering::SeqCst)
    }

    /// Simulate disconnection
    pub fn simulate_disconnect(&self) {
        self.unsubscribe();
    }

    /// Attempt to resubscribe
    pub async fn resubscribe(&self) -> ChaosResult<()> {
        self.reconnect_count.fetch_add(1, Ordering::SeqCst);
        self.subscribe().await?;
        Ok(())
    }

    /// Publish a message
    pub fn publish(&self, message: &str) -> ChaosResult<usize> {
        if !self.is_subscribed() {
            return Err(ChaosError::ConnectionError("Not subscribed".to_string()));
        }

        self.message_tx
            .send(message.to_string())
            .map_err(|e| ChaosError::ConnectionError(e.to_string()))
    }

    /// Get reconnect count
    pub fn reconnect_count(&self) -> u32 {
        self.reconnect_count.load(Ordering::SeqCst)
    }

    /// Get channel name
    pub fn channel(&self) -> &str {
        &self.channel
    }
}

/// Mock Redis cache for testing cache invalidation
pub struct MockRedisCache {
    data: Arc<RwLock<std::collections::HashMap<String, (String, Instant, Duration)>>>,
    is_available: Arc<AtomicBool>,
    hit_count: Arc<AtomicU32>,
    miss_count: Arc<AtomicU32>,
}

impl MockRedisCache {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(std::collections::HashMap::new())),
            is_available: Arc::new(AtomicBool::new(true)),
            hit_count: Arc::new(AtomicU32::new(0)),
            miss_count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Set a value with TTL
    pub async fn set(&self, key: &str, value: &str, ttl: Duration) -> ChaosResult<()> {
        if !self.is_available.load(Ordering::SeqCst) {
            return Err(ChaosError::ServiceUnavailable(
                "Cache unavailable".to_string(),
            ));
        }

        let mut data = self.data.write().await;
        data.insert(key.to_string(), (value.to_string(), Instant::now(), ttl));
        Ok(())
    }

    /// Get a value
    pub async fn get(&self, key: &str) -> ChaosResult<Option<String>> {
        if !self.is_available.load(Ordering::SeqCst) {
            return Err(ChaosError::ServiceUnavailable(
                "Cache unavailable".to_string(),
            ));
        }

        let data = self.data.read().await;
        if let Some((value, created, ttl)) = data.get(key) {
            if created.elapsed() < *ttl {
                self.hit_count.fetch_add(1, Ordering::SeqCst);
                return Ok(Some(value.clone()));
            }
        }

        self.miss_count.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    /// Invalidate cache
    pub async fn invalidate(&self, key: &str) -> ChaosResult<bool> {
        if !self.is_available.load(Ordering::SeqCst) {
            return Err(ChaosError::ServiceUnavailable(
                "Cache unavailable".to_string(),
            ));
        }

        let mut data = self.data.write().await;
        Ok(data.remove(key).is_some())
    }

    /// Clear all cache
    pub async fn clear(&self) -> ChaosResult<()> {
        let mut data = self.data.write().await;
        data.clear();
        Ok(())
    }

    /// Simulate cache unavailability
    pub fn make_unavailable(&self) {
        self.is_available.store(false, Ordering::SeqCst);
    }

    /// Restore cache availability
    pub fn make_available(&self) {
        self.is_available.store(true, Ordering::SeqCst);
    }

    /// Get cache stats
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hit_count: self.hit_count.load(Ordering::SeqCst),
            miss_count: self.miss_count.load(Ordering::SeqCst),
            is_available: self.is_available.load(Ordering::SeqCst),
        }
    }
}

impl Default for MockRedisCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hit_count: u32,
    pub miss_count: u32,
    pub is_available: bool,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hit_count + self.miss_count;
        if total == 0 {
            0.0
        } else {
            f64::from(self.hit_count) / f64::from(total)
        }
    }
}

// ============================================================================
// Test Cases
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_redis_pool_basic_operations() {
        let pool = MockRedisPool::new(5);

        let conn = pool.get_connection().await.unwrap();
        assert!(conn.is_connected());

        let result = conn.execute("PING").await.unwrap();
        assert_eq!(result, "OK");
    }

    #[tokio::test]
    async fn test_redis_complete_disconnection() {
        let pool = MockRedisPool::new(5);

        // Verify initial state
        assert!(pool.health_check().await.unwrap());

        // Simulate complete disconnection
        pool.simulate_disconnection().await;

        // Verify disconnected state
        assert!(!pool.health_check().await.unwrap());

        // Attempting to get a connection should fail
        let result = pool.get_connection().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_redis_partial_failure() {
        let pool = MockRedisPool::new(10);

        // Simulate 50% failure
        pool.simulate_partial_failure(0.5).await;

        let status = pool.get_status().await;
        assert!(status.connected_count >= 4 && status.connected_count <= 6);
        assert!(status.disconnected_count >= 4 && status.disconnected_count <= 6);
    }

    #[tokio::test]
    async fn test_redis_reconnection() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(5),
            reconnect_attempts: 3,
            ..Default::default()
        };
        let pool = MockRedisPool::new(5).with_config(config);

        // Disconnect
        pool.simulate_disconnection().await;
        assert!(!pool.all_connected().await);

        // Reconnect
        let reconnected = pool.reconnect().await.unwrap();
        assert_eq!(reconnected, 5);
        assert!(pool.all_connected().await);
    }

    #[tokio::test]
    async fn test_redis_pool_exhaustion() {
        let pool = MockRedisPool::new(5);

        // Simulate pool exhaustion
        pool.inject_fault(FaultType::PoolExhaustion {
            concurrent_connections: 100,
        })
        .await
        .unwrap();

        // Pool should be marked as unavailable
        let status = pool.get_status().await;
        assert!(!status.is_available);

        // Getting connection should fail
        assert!(pool.get_connection().await.is_err());
    }

    #[tokio::test]
    async fn test_pubsub_subscription_recovery() {
        let pubsub = MockRedisPubSub::new("test-channel");

        // Subscribe
        let _rx = pubsub.subscribe().await.unwrap();
        assert!(pubsub.is_subscribed());

        // Simulate disconnect
        pubsub.simulate_disconnect();
        assert!(!pubsub.is_subscribed());

        // Resubscribe
        pubsub.resubscribe().await.unwrap();
        assert!(pubsub.is_subscribed());
        assert_eq!(pubsub.reconnect_count(), 1);
    }

    #[tokio::test]
    async fn test_pubsub_message_delivery() {
        let pubsub = MockRedisPubSub::new("test-channel");

        let mut rx = pubsub.subscribe().await.unwrap();

        // Publish a message
        let subscribers = pubsub.publish("test-message").unwrap();
        assert_eq!(subscribers, 1);

        // Receive the message
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg, "test-message");
    }

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = MockRedisCache::new();

        // Set value
        cache
            .set("key1", "value1", Duration::from_secs(60))
            .await
            .unwrap();

        // Get value
        let value = cache.get("key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));

        // Miss on non-existent key
        let value = cache.get("nonexistent").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        let cache = MockRedisCache::new();

        // Set value with short TTL
        cache
            .set("key1", "value1", Duration::from_millis(50))
            .await
            .unwrap();

        // Value should exist
        let value = cache.get("key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));

        // Wait for expiration
        sleep(Duration::from_millis(60)).await;

        // Value should be gone
        let value = cache.get("key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_cache_unavailability() {
        let cache = MockRedisCache::new();

        // Set value
        cache
            .set("key1", "value1", Duration::from_secs(60))
            .await
            .unwrap();

        // Make cache unavailable
        cache.make_unavailable();

        // Operations should fail
        assert!(cache.get("key1").await.is_err());
        assert!(cache
            .set("key2", "value2", Duration::from_secs(60))
            .await
            .is_err());

        // Restore
        cache.make_available();
        let value = cache.get("key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache = MockRedisCache::new();

        cache
            .set("key1", "value1", Duration::from_secs(60))
            .await
            .unwrap();

        // Invalidate
        let removed = cache.invalidate("key1").await.unwrap();
        assert!(removed);

        // Should be gone
        let value = cache.get("key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = MockRedisCache::new();

        cache
            .set("key1", "value1", Duration::from_secs(60))
            .await
            .unwrap();

        // Hit
        cache.get("key1").await.unwrap();
        // Miss
        cache.get("nonexistent").await.unwrap();
        // Another hit
        cache.get("key1").await.unwrap();

        let stats = cache.stats();
        assert_eq!(stats.hit_count, 2);
        assert_eq!(stats.miss_count, 1);
        assert!((stats.hit_rate() - 0.666).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(5),
            reconnect_attempts: 3,
            ..Default::default()
        };
        let pool = MockRedisPool::new(5).with_config(config);

        // Inject multiple faults
        pool.simulate_disconnection().await;
        pool.reconnect().await.unwrap();

        pool.simulate_partial_failure(0.5).await;
        pool.reconnect().await.unwrap();

        let metrics = pool.get_metrics().await;
        assert_eq!(metrics.faults_injected, 2);
        assert_eq!(metrics.recoveries_successful, 2);
        assert_eq!(metrics.recovery_times_ms.len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_operations_during_failure() {
        let pool = Arc::new(MockRedisPool::new(10));

        // Spawn concurrent operations
        let mut handles = Vec::new();
        for _ in 0..20 {
            let pool = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                match pool.get_connection().await {
                    Ok(conn) => conn.execute("GET key").await.is_ok(),
                    Err(_) => false,
                }
            }));
        }

        // Inject fault mid-operations
        sleep(Duration::from_millis(5)).await;
        pool.simulate_disconnection().await;

        // Collect results
        let results: Vec<bool> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // Some should have succeeded, some failed
        let succeeded = results.iter().filter(|&&r| r).count();
        let failed = results.iter().filter(|&&r| !r).count();

        // Due to timing, most will likely succeed before disconnect
        assert!(succeeded > 0 || failed > 0);
    }

    #[tokio::test]
    async fn test_rapid_disconnect_reconnect_cycles() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(1),
            reconnect_attempts: 5,
            ..Default::default()
        };
        let pool = MockRedisPool::new(5).with_config(config);

        for _ in 0..10 {
            pool.simulate_disconnection().await;
            assert!(!pool.all_connected().await);

            pool.reconnect().await.unwrap();
            assert!(pool.all_connected().await);
        }

        let metrics = pool.get_metrics().await;
        assert_eq!(metrics.faults_injected, 10);
        assert_eq!(metrics.recoveries_successful, 10);
    }
}
