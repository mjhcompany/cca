//! Graceful Degradation Tests
//!
//! Tests for verifying system behavior when services are partially or fully unavailable.
//! These tests validate:
//! - System continues operating with reduced functionality
//! - Appropriate fallback behaviors are activated
//! - User-facing errors are graceful and informative
//! - Recovery to full functionality when services return

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::agent_crash_tests::ChaosAgentManager;
use crate::postgres_chaos_tests::MockPgPool;
use crate::redis_chaos_tests::{MockRedisCache, MockRedisPool};
use crate::{ChaosConfig, ChaosError, ChaosMetrics, ChaosResult, ChaosTestable, FaultType};

/// Service availability status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is fully operational
    Healthy,
    /// Service is degraded but partially functional
    Degraded,
    /// Service is completely unavailable
    Unavailable,
}

/// Overall system health status
#[derive(Debug, Clone)]
pub struct SystemHealth {
    pub agents: ServiceStatus,
    pub redis: ServiceStatus,
    pub postgres: ServiceStatus,
    pub overall: ServiceStatus,
    pub degraded_features: Vec<String>,
    pub available_features: Vec<String>,
}

impl SystemHealth {
    pub fn is_operational(&self) -> bool {
        self.overall != ServiceStatus::Unavailable
    }
}

/// Simulated CCA system for degradation testing
pub struct MockCCASystem {
    agent_manager: Arc<ChaosAgentManager>,
    redis_pool: Arc<MockRedisPool>,
    redis_cache: Arc<MockRedisCache>,
    pg_pool: Arc<MockPgPool>,
    config: ChaosConfig,
    metrics: Arc<RwLock<ChaosMetrics>>,
    request_count: Arc<AtomicU32>,
    successful_requests: Arc<AtomicU32>,
    degraded_requests: Arc<AtomicU32>,
    failed_requests: Arc<AtomicU32>,
}

impl MockCCASystem {
    pub fn new() -> Self {
        Self {
            agent_manager: Arc::new(ChaosAgentManager::new(10)),
            redis_pool: Arc::new(MockRedisPool::new(10)),
            redis_cache: Arc::new(MockRedisCache::new()),
            pg_pool: Arc::new(MockPgPool::new(10)),
            config: ChaosConfig::default(),
            metrics: Arc::new(RwLock::new(ChaosMetrics::default())),
            request_count: Arc::new(AtomicU32::new(0)),
            successful_requests: Arc::new(AtomicU32::new(0)),
            degraded_requests: Arc::new(AtomicU32::new(0)),
            failed_requests: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn with_config(mut self, config: ChaosConfig) -> Self {
        self.config = config;
        self
    }

    /// Initialize system with agents
    pub async fn initialize(&self, agent_count: usize) -> ChaosResult<()> {
        for _ in 0..agent_count {
            self.agent_manager.spawn_agent("worker").await?;
        }
        Ok(())
    }

    /// Check system health
    pub async fn check_health(&self) -> SystemHealth {
        let agents_healthy = self.agent_manager.health_check().await.unwrap_or(false);
        let redis_healthy = self.redis_pool.health_check().await.unwrap_or(false);
        let pg_healthy = self.pg_pool.health_check().await.unwrap_or(false);

        let agents_status = if agents_healthy {
            ServiceStatus::Healthy
        } else {
            let agents = self.agent_manager.list_agents().await;
            let alive_count = {
                let mut count = 0;
                for agent in &agents {
                    if self
                        .agent_manager
                        .is_agent_alive(agent)
                        .await
                        .unwrap_or(false)
                    {
                        count += 1;
                    }
                }
                count
            };
            if alive_count > 0 {
                ServiceStatus::Degraded
            } else {
                ServiceStatus::Unavailable
            }
        };

        let redis_status = if redis_healthy {
            ServiceStatus::Healthy
        } else if self.redis_pool.get_status().await.connected_count > 0 {
            ServiceStatus::Degraded
        } else {
            ServiceStatus::Unavailable
        };

        let pg_status = if pg_healthy {
            ServiceStatus::Healthy
        } else {
            ServiceStatus::Unavailable
        };

        // Determine overall status and features
        let (overall, degraded_features, available_features) = self.calculate_system_status(
            agents_status,
            redis_status,
            pg_status,
        );

        SystemHealth {
            agents: agents_status,
            redis: redis_status,
            postgres: pg_status,
            overall,
            degraded_features,
            available_features,
        }
    }

    fn calculate_system_status(
        &self,
        agents: ServiceStatus,
        redis: ServiceStatus,
        postgres: ServiceStatus,
    ) -> (ServiceStatus, Vec<String>, Vec<String>) {
        let mut degraded = Vec::new();
        let mut available = Vec::new();

        // Core features that require PostgreSQL
        if postgres == ServiceStatus::Healthy {
            available.push("task_persistence".to_string());
            available.push("pattern_storage".to_string());
            available.push("history".to_string());
        } else {
            degraded.push("task_persistence".to_string());
            degraded.push("pattern_storage".to_string());
            degraded.push("history".to_string());
        }

        // Features that require Redis
        if redis == ServiceStatus::Healthy {
            available.push("caching".to_string());
            available.push("pubsub".to_string());
            available.push("session_store".to_string());
        } else if redis == ServiceStatus::Degraded {
            available.push("caching".to_string()); // Partial caching
            degraded.push("pubsub".to_string());
            degraded.push("session_store".to_string());
        } else {
            degraded.push("caching".to_string());
            degraded.push("pubsub".to_string());
            degraded.push("session_store".to_string());
        }

        // Features that require agents
        if agents == ServiceStatus::Healthy {
            available.push("task_execution".to_string());
            available.push("code_analysis".to_string());
        } else if agents == ServiceStatus::Degraded {
            available.push("task_execution".to_string()); // Slower but available
            degraded.push("code_analysis".to_string());
        } else {
            degraded.push("task_execution".to_string());
            degraded.push("code_analysis".to_string());
        }

        // Basic API is always available
        available.push("health_check".to_string());
        available.push("status".to_string());

        // Determine overall status
        let overall = if postgres == ServiceStatus::Unavailable && agents == ServiceStatus::Unavailable
        {
            ServiceStatus::Unavailable
        } else if agents == ServiceStatus::Healthy
            && redis == ServiceStatus::Healthy
            && postgres == ServiceStatus::Healthy
        {
            ServiceStatus::Healthy
        } else {
            ServiceStatus::Degraded
        };

        (overall, degraded, available)
    }

    /// Execute a task with graceful degradation
    pub async fn execute_task(&self, task_id: &str, task_type: TaskType) -> ChaosResult<TaskResult> {
        self.request_count.fetch_add(1, Ordering::SeqCst);
        let start = Instant::now();

        let health = self.check_health().await;

        match task_type {
            TaskType::AgentTask => {
                if health.agents == ServiceStatus::Unavailable {
                    self.failed_requests.fetch_add(1, Ordering::SeqCst);
                    return Err(ChaosError::ServiceUnavailable(
                        "No agents available".to_string(),
                    ));
                }

                // Try to execute on an agent
                let agents = self.agent_manager.list_agents().await;
                for agent_id in agents {
                    if self
                        .agent_manager
                        .is_agent_alive(&agent_id)
                        .await
                        .unwrap_or(false)
                    {
                        self.successful_requests.fetch_add(1, Ordering::SeqCst);
                        return Ok(TaskResult {
                            task_id: task_id.to_string(),
                            success: true,
                            degraded: health.agents == ServiceStatus::Degraded,
                            execution_time_ms: start.elapsed().as_millis() as u64,
                            message: "Task executed successfully".to_string(),
                        });
                    }
                }

                self.failed_requests.fetch_add(1, Ordering::SeqCst);
                Err(ChaosError::ServiceUnavailable(
                    "No healthy agents found".to_string(),
                ))
            }

            TaskType::CachedQuery => {
                // Try cache first
                if let Ok(Some(_)) = self.redis_cache.get(task_id).await {
                    self.successful_requests.fetch_add(1, Ordering::SeqCst);
                    return Ok(TaskResult {
                        task_id: task_id.to_string(),
                        success: true,
                        degraded: false,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        message: "Cache hit".to_string(),
                    });
                }

                // Fallback to database
                if health.postgres == ServiceStatus::Healthy {
                    self.pg_pool.query("SELECT data FROM cache WHERE key = ?").await?;

                    // Try to populate cache (best effort)
                    let _ = self
                        .redis_cache
                        .set(task_id, "data", Duration::from_secs(300))
                        .await;

                    self.degraded_requests.fetch_add(1, Ordering::SeqCst);
                    return Ok(TaskResult {
                        task_id: task_id.to_string(),
                        success: true,
                        degraded: true,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        message: "Cache miss, fetched from database".to_string(),
                    });
                }

                self.failed_requests.fetch_add(1, Ordering::SeqCst);
                Err(ChaosError::ServiceUnavailable(
                    "Cache and database unavailable".to_string(),
                ))
            }

            TaskType::DatabaseWrite => {
                if health.postgres != ServiceStatus::Healthy {
                    self.failed_requests.fetch_add(1, Ordering::SeqCst);
                    return Err(ChaosError::ServiceUnavailable(
                        "Database unavailable for writes".to_string(),
                    ));
                }

                self.pg_pool.query("INSERT INTO tasks VALUES (?)").await?;

                // Invalidate cache (best effort)
                let _ = self.redis_cache.invalidate(task_id).await;

                self.successful_requests.fetch_add(1, Ordering::SeqCst);
                Ok(TaskResult {
                    task_id: task_id.to_string(),
                    success: true,
                    degraded: health.redis != ServiceStatus::Healthy,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    message: "Database write successful".to_string(),
                })
            }

            TaskType::HealthCheck => {
                // Always succeeds with current health status
                self.successful_requests.fetch_add(1, Ordering::SeqCst);
                Ok(TaskResult {
                    task_id: task_id.to_string(),
                    success: true,
                    degraded: health.overall == ServiceStatus::Degraded,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    message: format!("System status: {:?}", health.overall),
                })
            }
        }
    }

    /// Inject a fault into the system
    pub async fn inject_system_fault(&self, fault: SystemFault) -> ChaosResult<()> {
        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;
        drop(metrics);

        match fault {
            SystemFault::AgentsCrash { percentage } => {
                self.agent_manager
                    .inject_fault(FaultType::PartialFailure {
                        failure_rate: percentage,
                    })
                    .await?;
            }
            SystemFault::RedisDisconnect => {
                self.redis_pool
                    .inject_fault(FaultType::NetworkDisconnect)
                    .await?;
            }
            SystemFault::RedisPartialFailure { percentage } => {
                self.redis_pool
                    .inject_fault(FaultType::PartialFailure {
                        failure_rate: percentage,
                    })
                    .await?;
            }
            SystemFault::PostgresFailover => {
                self.pg_pool
                    .inject_fault(FaultType::NetworkDisconnect)
                    .await?;
            }
            SystemFault::CacheUnavailable => {
                self.redis_cache.make_unavailable();
            }
            SystemFault::FullOutage => {
                self.agent_manager
                    .inject_fault(FaultType::PartialFailure { failure_rate: 1.0 })
                    .await?;
                self.redis_pool
                    .inject_fault(FaultType::NetworkDisconnect)
                    .await?;
                self.pg_pool
                    .inject_fault(FaultType::NetworkDisconnect)
                    .await?;
            }
        }

        Ok(())
    }

    /// Recover from faults
    pub async fn recover_system(&self) -> ChaosResult<RecoveryReport> {
        let start = Instant::now();
        let mut report = RecoveryReport {
            agents_recovered: 0,
            redis_recovered: false,
            postgres_recovered: false,
            total_recovery_time_ms: 0,
        };

        // Recover agents
        if let Ok(recovered) = self.agent_manager.detect_and_recover_crashed().await {
            report.agents_recovered = recovered.len() as u32;
        }

        // Recover Redis
        if self.redis_pool.restore().await.is_ok() {
            report.redis_recovered = true;
        }
        self.redis_cache.make_available();

        // Recover PostgreSQL
        if self.pg_pool.restore().await.is_ok() {
            report.postgres_recovered = true;
        }

        report.total_recovery_time_ms = start.elapsed().as_millis() as u64;

        let mut metrics = self.metrics.write().await;
        metrics.recoveries_successful += 1;
        metrics
            .recovery_times_ms
            .push(report.total_recovery_time_ms);

        Ok(report)
    }

    /// Get system statistics
    pub fn get_stats(&self) -> SystemStats {
        SystemStats {
            total_requests: self.request_count.load(Ordering::SeqCst),
            successful_requests: self.successful_requests.load(Ordering::SeqCst),
            degraded_requests: self.degraded_requests.load(Ordering::SeqCst),
            failed_requests: self.failed_requests.load(Ordering::SeqCst),
        }
    }

    /// Get the agent manager for direct testing
    pub fn agent_manager(&self) -> &Arc<ChaosAgentManager> {
        &self.agent_manager
    }

    /// Get the Redis pool for direct testing
    pub fn redis_pool(&self) -> &Arc<MockRedisPool> {
        &self.redis_pool
    }

    /// Get the PostgreSQL pool for direct testing
    pub fn pg_pool(&self) -> &Arc<MockPgPool> {
        &self.pg_pool
    }
}

impl Default for MockCCASystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Types of tasks that can be executed
#[derive(Debug, Clone, Copy)]
pub enum TaskType {
    /// Task that requires an agent
    AgentTask,
    /// Query that should be cached
    CachedQuery,
    /// Write to the database
    DatabaseWrite,
    /// Health check (always available)
    HealthCheck,
}

/// Result of task execution
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub degraded: bool,
    pub execution_time_ms: u64,
    pub message: String,
}

/// Types of system faults that can be injected
#[derive(Debug, Clone)]
pub enum SystemFault {
    /// Kill a percentage of agents
    AgentsCrash { percentage: f64 },
    /// Complete Redis disconnection
    RedisDisconnect,
    /// Partial Redis failure
    RedisPartialFailure { percentage: f64 },
    /// PostgreSQL primary failover
    PostgresFailover,
    /// Make cache unavailable
    CacheUnavailable,
    /// Full system outage
    FullOutage,
}

/// Report of system recovery
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    pub agents_recovered: u32,
    pub redis_recovered: bool,
    pub postgres_recovered: bool,
    pub total_recovery_time_ms: u64,
}

/// System statistics
#[derive(Debug, Clone)]
pub struct SystemStats {
    pub total_requests: u32,
    pub successful_requests: u32,
    pub degraded_requests: u32,
    pub failed_requests: u32,
}

impl SystemStats {
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            1.0
        } else {
            (self.successful_requests + self.degraded_requests) as f64 / self.total_requests as f64
        }
    }

    pub fn degradation_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.degraded_requests as f64 / self.total_requests as f64
        }
    }

    pub fn failure_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.failed_requests as f64 / self.total_requests as f64
        }
    }
}

// ============================================================================
// Test Cases
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_system_healthy_state() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        let health = system.check_health().await;
        assert_eq!(health.overall, ServiceStatus::Healthy);
        assert_eq!(health.agents, ServiceStatus::Healthy);
        assert_eq!(health.redis, ServiceStatus::Healthy);
        assert_eq!(health.postgres, ServiceStatus::Healthy);
        assert!(health.is_operational());
    }

    #[tokio::test]
    async fn test_degraded_with_agent_failures() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Kill some agents
        system
            .inject_system_fault(SystemFault::AgentsCrash { percentage: 0.5 })
            .await
            .unwrap();

        let health = system.check_health().await;
        assert_eq!(health.agents, ServiceStatus::Degraded);
        assert_eq!(health.overall, ServiceStatus::Degraded);
        assert!(health.is_operational());
    }

    #[tokio::test]
    async fn test_degraded_with_redis_failure() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Disconnect Redis
        system
            .inject_system_fault(SystemFault::RedisDisconnect)
            .await
            .unwrap();

        let health = system.check_health().await;
        assert_eq!(health.redis, ServiceStatus::Unavailable);
        assert_eq!(health.overall, ServiceStatus::Degraded);
        assert!(health.is_operational());
        assert!(health.degraded_features.contains(&"caching".to_string()));
    }

    #[tokio::test]
    async fn test_degraded_with_postgres_failure() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Failover PostgreSQL
        system
            .inject_system_fault(SystemFault::PostgresFailover)
            .await
            .unwrap();

        let health = system.check_health().await;
        assert_eq!(health.postgres, ServiceStatus::Unavailable);
        assert_eq!(health.overall, ServiceStatus::Degraded);
        assert!(health.is_operational());
        assert!(health
            .degraded_features
            .contains(&"task_persistence".to_string()));
    }

    #[tokio::test]
    async fn test_unavailable_with_full_outage() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Full outage
        system
            .inject_system_fault(SystemFault::FullOutage)
            .await
            .unwrap();

        let health = system.check_health().await;
        assert_eq!(health.overall, ServiceStatus::Unavailable);
        assert!(!health.is_operational());
    }

    #[tokio::test]
    async fn test_task_execution_with_healthy_system() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        let result = system
            .execute_task("task-1", TaskType::AgentTask)
            .await
            .unwrap();
        assert!(result.success);
        assert!(!result.degraded);
    }

    #[tokio::test]
    async fn test_task_execution_with_degraded_agents() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Kill some agents
        system
            .inject_system_fault(SystemFault::AgentsCrash { percentage: 0.5 })
            .await
            .unwrap();

        // Tasks should still work but be degraded
        let result = system
            .execute_task("task-1", TaskType::AgentTask)
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.degraded);
    }

    #[tokio::test]
    async fn test_task_execution_with_no_agents() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Kill all agents
        system
            .inject_system_fault(SystemFault::AgentsCrash { percentage: 1.0 })
            .await
            .unwrap();

        let result = system.execute_task("task-1", TaskType::AgentTask).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cached_query_fallback() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Make cache unavailable
        system
            .inject_system_fault(SystemFault::CacheUnavailable)
            .await
            .unwrap();

        // Should fall back to database
        let result = system
            .execute_task("query-1", TaskType::CachedQuery)
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.degraded);
        assert!(result.message.contains("database"));
    }

    #[tokio::test]
    async fn test_database_write_requires_postgres() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Failover PostgreSQL
        system
            .inject_system_fault(SystemFault::PostgresFailover)
            .await
            .unwrap();

        let result = system.execute_task("task-1", TaskType::DatabaseWrite).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_check_always_works() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Even with full outage, health check returns
        system
            .inject_system_fault(SystemFault::FullOutage)
            .await
            .unwrap();

        let result = system
            .execute_task("health", TaskType::HealthCheck)
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_system_recovery() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(5),
            reconnect_attempts: 3,
            ..Default::default()
        };
        let system = MockCCASystem::new().with_config(config);
        system.initialize(5).await.unwrap();

        // Inject faults
        system
            .inject_system_fault(SystemFault::AgentsCrash { percentage: 0.5 })
            .await
            .unwrap();
        system
            .inject_system_fault(SystemFault::RedisDisconnect)
            .await
            .unwrap();
        system
            .inject_system_fault(SystemFault::PostgresFailover)
            .await
            .unwrap();

        let health = system.check_health().await;
        assert_ne!(health.overall, ServiceStatus::Healthy);

        // Recover
        let report = system.recover_system().await.unwrap();
        assert!(report.agents_recovered > 0);
        assert!(report.redis_recovered);
        assert!(report.postgres_recovered);

        // System should be healthy again
        let health = system.check_health().await;
        assert_eq!(health.overall, ServiceStatus::Healthy);
    }

    #[tokio::test]
    async fn test_system_stats_tracking() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Execute some tasks
        system
            .execute_task("task-1", TaskType::AgentTask)
            .await
            .unwrap();
        system
            .execute_task("task-2", TaskType::AgentTask)
            .await
            .unwrap();

        // Cause degradation
        system
            .inject_system_fault(SystemFault::CacheUnavailable)
            .await
            .unwrap();
        system
            .execute_task("query-1", TaskType::CachedQuery)
            .await
            .unwrap();

        // Kill all agents
        system
            .inject_system_fault(SystemFault::AgentsCrash { percentage: 1.0 })
            .await
            .unwrap();
        let _ = system.execute_task("task-3", TaskType::AgentTask).await;

        let stats = system.get_stats();
        assert_eq!(stats.total_requests, 4);
        assert_eq!(stats.successful_requests, 2);
        assert_eq!(stats.degraded_requests, 1);
        assert_eq!(stats.failed_requests, 1);
        assert!((stats.success_rate() - 0.75).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_concurrent_requests_during_degradation() {
        let system = Arc::new(MockCCASystem::new());
        system.initialize(10).await.unwrap();

        // Start concurrent requests
        let mut handles = Vec::new();
        for i in 0..20 {
            let system = Arc::clone(&system);
            handles.push(tokio::spawn(async move {
                let task_id = format!("task-{}", i);
                system.execute_task(&task_id, TaskType::AgentTask).await
            }));
        }

        // Inject fault mid-operation
        sleep(Duration::from_millis(5)).await;
        system
            .inject_system_fault(SystemFault::AgentsCrash { percentage: 0.5 })
            .await
            .unwrap();

        // Collect results
        let results: Vec<bool> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap().is_ok())
            .collect();

        // Most should still succeed due to remaining agents
        let succeeded = results.iter().filter(|&&r| r).count();
        assert!(succeeded > 10);
    }

    #[tokio::test]
    async fn test_feature_availability_tracking() {
        let system = MockCCASystem::new();
        system.initialize(5).await.unwrap();

        // Full health
        let health = system.check_health().await;
        assert!(health
            .available_features
            .contains(&"task_execution".to_string()));
        assert!(health.available_features.contains(&"caching".to_string()));
        assert!(health
            .available_features
            .contains(&"task_persistence".to_string()));

        // Degrade agents
        system
            .inject_system_fault(SystemFault::AgentsCrash { percentage: 0.5 })
            .await
            .unwrap();
        let health = system.check_health().await;
        assert!(health
            .degraded_features
            .contains(&"code_analysis".to_string()));
        assert!(health
            .available_features
            .contains(&"task_execution".to_string()));

        // Degrade Redis
        system
            .inject_system_fault(SystemFault::RedisDisconnect)
            .await
            .unwrap();
        let health = system.check_health().await;
        assert!(health.degraded_features.contains(&"caching".to_string()));
        assert!(health.degraded_features.contains(&"pubsub".to_string()));
    }

    #[tokio::test]
    async fn test_recovery_report_accuracy() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(5),
            reconnect_attempts: 3,
            ..Default::default()
        };
        let system = MockCCASystem::new().with_config(config);
        system.initialize(5).await.unwrap();

        // Full outage
        system
            .inject_system_fault(SystemFault::FullOutage)
            .await
            .unwrap();

        let report = system.recover_system().await.unwrap();

        // All services should have recovered
        assert_eq!(report.agents_recovered, 5);
        assert!(report.redis_recovered);
        assert!(report.postgres_recovered);
        assert!(report.total_recovery_time_ms > 0);
    }

    #[tokio::test]
    async fn test_rapid_degradation_recovery_cycles() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(1),
            reconnect_attempts: 3,
            ..Default::default()
        };
        let system = MockCCASystem::new().with_config(config);
        system.initialize(5).await.unwrap();

        for _ in 0..5 {
            // Degrade
            system
                .inject_system_fault(SystemFault::AgentsCrash { percentage: 0.5 })
                .await
                .unwrap();
            let health = system.check_health().await;
            assert_eq!(health.overall, ServiceStatus::Degraded);

            // Recover
            system.recover_system().await.unwrap();
            let health = system.check_health().await;
            assert_eq!(health.overall, ServiceStatus::Healthy);
        }
    }
}
