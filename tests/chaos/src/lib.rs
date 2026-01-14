//! CCA Chaos Testing Infrastructure
//!
//! This crate provides chaos testing capabilities for verifying system resilience:
//! - Agent crash recovery
//! - Redis disconnection handling
//! - `PostgreSQL` failover testing
//! - Graceful degradation scenarios

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::single_match_else)]
#![allow(clippy::format_push_string)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::if_not_else)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::unused_async)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::use_debug)]
#![allow(clippy::unused_self)]
#![allow(clippy::cast_lossless)]

pub mod agent_crash_tests;
pub mod degradation_tests;
pub mod postgres_chaos_tests;
pub mod redis_chaos_tests;

use async_trait::async_trait;
use std::time::Duration;

/// Configuration for chaos tests
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Timeout for test operations
    pub test_timeout: Duration,
    /// Number of reconnection attempts
    pub reconnect_attempts: u32,
    /// Delay between chaos injections
    pub injection_delay: Duration,
    /// Whether to run destructive tests
    pub enable_destructive: bool,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            test_timeout: Duration::from_secs(
                std::env::var("CHAOS_TEST_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60),
            ),
            reconnect_attempts: std::env::var("CHAOS_RECONNECT_ATTEMPTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            injection_delay: Duration::from_millis(
                std::env::var("CHAOS_AGENT_KILL_DELAY_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(100),
            ),
            enable_destructive: std::env::var("CHAOS_ENABLE_DESTRUCTIVE")
                .map(|s| s == "true" || s == "1")
                .unwrap_or(false),
        }
    }
}

/// Result type for chaos test operations
pub type ChaosResult<T> = Result<T, ChaosError>;

/// Error type for chaos test failures
#[derive(Debug, thiserror::Error)]
pub enum ChaosError {
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Recovery failed after {attempts} attempts: {reason}")]
    RecoveryFailed { attempts: u32, reason: String },

    #[error("Timeout waiting for {operation}")]
    Timeout { operation: String },

    #[error("Unexpected state: expected {expected}, got {actual}")]
    UnexpectedState { expected: String, actual: String },

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Process error: {0}")]
    ProcessError(String),

    #[error("Test precondition failed: {0}")]
    PreconditionFailed(String),
}

/// Trait for services that can be chaos-tested
#[async_trait]
pub trait ChaosTestable: Send + Sync {
    /// Check if the service is healthy
    async fn health_check(&self) -> ChaosResult<bool>;

    /// Inject a fault into the service
    async fn inject_fault(&self, fault: FaultType) -> ChaosResult<()>;

    /// Remove injected faults and restore normal operation
    async fn restore(&self) -> ChaosResult<()>;
}

/// Types of faults that can be injected
#[derive(Debug, Clone)]
pub enum FaultType {
    /// Kill a process
    ProcessKill { signal: i32 },
    /// Network disconnection
    NetworkDisconnect,
    /// High latency injection
    LatencyInjection { delay_ms: u64 },
    /// Connection pool exhaustion
    PoolExhaustion { concurrent_connections: u32 },
    /// Timeout injection
    TimeoutInjection { after_ms: u64 },
    /// Partial failure (some operations fail)
    PartialFailure { failure_rate: f64 },
}

/// Metrics collected during chaos tests
#[derive(Debug, Default)]
pub struct ChaosMetrics {
    /// Number of faults injected
    pub faults_injected: u32,
    /// Number of successful recoveries
    pub recoveries_successful: u32,
    /// Number of failed recoveries
    pub recoveries_failed: u32,
    /// Time to recovery in milliseconds
    pub recovery_times_ms: Vec<u64>,
    /// Number of requests during chaos
    pub requests_during_chaos: u32,
    /// Number of successful requests during chaos
    pub successful_requests: u32,
}

impl ChaosMetrics {
    /// Calculate average recovery time
    pub fn avg_recovery_time_ms(&self) -> Option<f64> {
        if self.recovery_times_ms.is_empty() {
            None
        } else {
            let sum: u64 = self.recovery_times_ms.iter().sum();
            Some(sum as f64 / self.recovery_times_ms.len() as f64)
        }
    }

    /// Calculate success rate during chaos
    pub fn success_rate(&self) -> f64 {
        if self.requests_during_chaos == 0 {
            1.0
        } else {
            f64::from(self.successful_requests) / f64::from(self.requests_during_chaos)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chaos_config_defaults() {
        let config = ChaosConfig::default();
        assert_eq!(config.test_timeout, Duration::from_secs(60));
        assert_eq!(config.reconnect_attempts, 5);
        assert!(!config.enable_destructive);
    }

    #[test]
    fn test_chaos_metrics_avg_recovery_time() {
        let mut metrics = ChaosMetrics::default();
        assert!(metrics.avg_recovery_time_ms().is_none());

        metrics.recovery_times_ms = vec![100, 200, 300];
        assert_eq!(metrics.avg_recovery_time_ms(), Some(200.0));
    }

    #[test]
    fn test_chaos_metrics_success_rate() {
        let mut metrics = ChaosMetrics::default();
        assert_eq!(metrics.success_rate(), 1.0);

        metrics.requests_during_chaos = 10;
        metrics.successful_requests = 8;
        assert!((metrics.success_rate() - 0.8).abs() < 0.001);
    }
}
