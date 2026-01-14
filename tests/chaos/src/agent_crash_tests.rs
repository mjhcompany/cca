//! Agent Crash Recovery Tests
//!
//! Tests for verifying system resilience when agents crash or become unresponsive.
//! These tests validate:
//! - Detection of crashed agents
//! - Automatic recovery and respawning
//! - Task reassignment after agent failure
//! - State preservation during recovery

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::sleep;

use async_trait::async_trait;
use crate::{ChaosConfig, ChaosError, ChaosMetrics, ChaosResult, ChaosTestable, FaultType};

/// Simulated agent for testing crash recovery
#[derive(Debug)]
pub struct MockAgent {
    pub id: String,
    pub role: String,
    pub is_alive: Arc<AtomicBool>,
    pub tasks_completed: Arc<AtomicU32>,
    pub crash_count: Arc<AtomicU32>,
}

impl MockAgent {
    pub fn new(id: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            role: role.into(),
            is_alive: Arc::new(AtomicBool::new(true)),
            tasks_completed: Arc::new(AtomicU32::new(0)),
            crash_count: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn simulate_crash(&self) {
        self.is_alive.store(false, Ordering::SeqCst);
        self.crash_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn recover(&self) {
        self.is_alive.store(true, Ordering::SeqCst);
    }

    pub fn is_alive(&self) -> bool {
        self.is_alive.load(Ordering::SeqCst)
    }
}

/// Agent manager for chaos testing
pub struct ChaosAgentManager {
    agents: Arc<RwLock<HashMap<String, MockAgent>>>,
    max_agents: usize,
    config: ChaosConfig,
    metrics: Arc<RwLock<ChaosMetrics>>,
}

impl ChaosAgentManager {
    pub fn new(max_agents: usize) -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            max_agents,
            config: ChaosConfig::default(),
            metrics: Arc::new(RwLock::new(ChaosMetrics::default())),
        }
    }

    pub fn with_config(mut self, config: ChaosConfig) -> Self {
        self.config = config;
        self
    }

    /// Spawn a new agent
    pub async fn spawn_agent(&self, role: &str) -> ChaosResult<String> {
        let mut agents = self.agents.write().await;

        if agents.len() >= self.max_agents {
            return Err(ChaosError::PreconditionFailed(format!(
                "Max agents ({}) reached",
                self.max_agents
            )));
        }

        let id = format!("agent-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let agent = MockAgent::new(&id, role);
        agents.insert(id.clone(), agent);

        Ok(id)
    }

    /// Kill an agent by ID
    pub async fn kill_agent(&self, agent_id: &str) -> ChaosResult<()> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(agent_id)
            .ok_or_else(|| ChaosError::ServiceUnavailable(format!("Agent {agent_id} not found")))?;

        agent.simulate_crash();

        let mut metrics = self.metrics.write().await;
        metrics.faults_injected += 1;

        Ok(())
    }

    /// Check if an agent is alive
    pub async fn is_agent_alive(&self, agent_id: &str) -> ChaosResult<bool> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(agent_id)
            .ok_or_else(|| ChaosError::ServiceUnavailable(format!("Agent {agent_id} not found")))?;

        Ok(agent.is_alive())
    }

    /// Detect and recover crashed agents
    pub async fn detect_and_recover_crashed(&self) -> ChaosResult<Vec<String>> {
        let start = Instant::now();
        let agents = self.agents.read().await;
        let crashed: Vec<String> = agents
            .iter()
            .filter(|(_, a)| !a.is_alive())
            .map(|(id, _)| id.clone())
            .collect();
        drop(agents);

        let mut recovered = Vec::new();
        for agent_id in &crashed {
            if self.recover_agent(agent_id).await.is_ok() {
                recovered.push(agent_id.clone());
            }
        }

        let elapsed = start.elapsed();
        let mut metrics = self.metrics.write().await;
        if !recovered.is_empty() {
            metrics.recoveries_successful += recovered.len() as u32;
            metrics.recovery_times_ms.push(elapsed.as_millis() as u64);
        }

        Ok(recovered)
    }

    /// Recover a specific crashed agent
    pub async fn recover_agent(&self, agent_id: &str) -> ChaosResult<()> {
        let agents = self.agents.read().await;
        let agent = agents
            .get(agent_id)
            .ok_or_else(|| ChaosError::ServiceUnavailable(format!("Agent {agent_id} not found")))?;

        if agent.is_alive() {
            return Ok(()); // Already alive
        }

        // Simulate recovery delay
        sleep(self.config.injection_delay).await;
        agent.recover();

        Ok(())
    }

    /// Get all agent IDs
    pub async fn list_agents(&self) -> Vec<String> {
        let agents = self.agents.read().await;
        agents.keys().cloned().collect()
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
impl ChaosTestable for ChaosAgentManager {
    async fn health_check(&self) -> ChaosResult<bool> {
        let agents = self.agents.read().await;
        Ok(agents.values().all(MockAgent::is_alive))
    }

    async fn inject_fault(&self, fault: FaultType) -> ChaosResult<()> {
        match fault {
            FaultType::ProcessKill { signal: _ } => {
                // Kill a random agent
                let agents = self.agents.read().await;
                if let Some(agent_id) = agents.keys().next().cloned() {
                    drop(agents);
                    self.kill_agent(&agent_id).await?;
                }
            }
            FaultType::PartialFailure { failure_rate } => {
                // Kill a percentage of agents
                let agents = self.agents.read().await;
                let count = (agents.len() as f64 * failure_rate).ceil() as usize;
                let to_kill: Vec<String> = agents.keys().take(count).cloned().collect();
                drop(agents);

                for agent_id in to_kill {
                    self.kill_agent(&agent_id).await?;
                }
            }
            _ => {
                return Err(ChaosError::PreconditionFailed(
                    "Unsupported fault type for agent manager".into(),
                ))
            }
        }
        Ok(())
    }

    async fn restore(&self) -> ChaosResult<()> {
        self.detect_and_recover_crashed().await?;
        Ok(())
    }
}

/// Process-based agent for integration tests (spawns real processes)
pub struct ProcessAgent {
    pub id: String,
    process: Option<Child>,
}

impl ProcessAgent {
    /// Spawn a new process-based agent
    pub fn spawn(id: impl Into<String>, command: &str, args: &[&str]) -> ChaosResult<Self> {
        let child = Command::new(command)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ChaosError::ProcessError(e.to_string()))?;

        Ok(Self {
            id: id.into(),
            process: Some(child),
        })
    }

    /// Check if the process is still running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.process {
            match child.try_wait() {
                Ok(Some(_)) => false, // Process has exited
                Ok(None) => true,     // Still running
                Err(_) => false,      // Error checking status
            }
        } else {
            false
        }
    }

    /// Kill the process
    pub fn kill(&mut self) -> ChaosResult<()> {
        if let Some(ref mut child) = self.process {
            child
                .kill()
                .map_err(|e| ChaosError::ProcessError(e.to_string()))?;
        }
        Ok(())
    }

    /// Send a signal to the process (Unix only)
    #[cfg(unix)]
    pub fn signal(&self, signal: i32) -> ChaosResult<()> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        if let Some(ref child) = self.process {
            let pid = Pid::from_raw(child.id() as i32);
            let sig = Signal::try_from(signal)
                .map_err(|e| ChaosError::ProcessError(format!("Invalid signal: {e}")))?;
            kill(pid, sig).map_err(|e: nix::Error| ChaosError::ProcessError(e.to_string()))?;
        }
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn signal(&self, _signal: i32) -> ChaosResult<()> {
        Err(ChaosError::PreconditionFailed(
            "Signal not supported on this platform".into(),
        ))
    }
}

impl Drop for ProcessAgent {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

// ============================================================================
// Test Cases
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_spawn_and_list() {
        let manager = ChaosAgentManager::new(10);

        let agent1 = manager.spawn_agent("worker").await.unwrap();
        let agent2 = manager.spawn_agent("analyzer").await.unwrap();

        let agents = manager.list_agents().await;
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&agent1));
        assert!(agents.contains(&agent2));
    }

    #[tokio::test]
    async fn test_agent_crash_detection() {
        let manager = ChaosAgentManager::new(10);

        let agent_id = manager.spawn_agent("worker").await.unwrap();
        assert!(manager.is_agent_alive(&agent_id).await.unwrap());

        // Simulate crash
        manager.kill_agent(&agent_id).await.unwrap();
        assert!(!manager.is_agent_alive(&agent_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_agent_recovery() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let manager = ChaosAgentManager::new(10).with_config(config);

        let agent_id = manager.spawn_agent("worker").await.unwrap();
        manager.kill_agent(&agent_id).await.unwrap();

        assert!(!manager.is_agent_alive(&agent_id).await.unwrap());

        // Recover
        let recovered = manager.detect_and_recover_crashed().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert!(recovered.contains(&agent_id));
        assert!(manager.is_agent_alive(&agent_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_multiple_agent_crash_recovery() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let manager = ChaosAgentManager::new(10).with_config(config);

        // Spawn multiple agents
        let mut agents = Vec::new();
        for _ in 0..5 {
            agents.push(manager.spawn_agent("worker").await.unwrap());
        }

        // Kill some agents
        manager.kill_agent(&agents[0]).await.unwrap();
        manager.kill_agent(&agents[2]).await.unwrap();
        manager.kill_agent(&agents[4]).await.unwrap();

        // Verify crash state
        assert!(!manager.is_agent_alive(&agents[0]).await.unwrap());
        assert!(manager.is_agent_alive(&agents[1]).await.unwrap());
        assert!(!manager.is_agent_alive(&agents[2]).await.unwrap());
        assert!(manager.is_agent_alive(&agents[3]).await.unwrap());
        assert!(!manager.is_agent_alive(&agents[4]).await.unwrap());

        // Recover all
        let recovered = manager.detect_and_recover_crashed().await.unwrap();
        assert_eq!(recovered.len(), 3);

        // Verify all recovered
        for agent in &agents {
            assert!(manager.is_agent_alive(agent).await.unwrap());
        }
    }

    #[tokio::test]
    async fn test_max_agents_limit() {
        let manager = ChaosAgentManager::new(3);

        manager.spawn_agent("worker").await.unwrap();
        manager.spawn_agent("worker").await.unwrap();
        manager.spawn_agent("worker").await.unwrap();

        // Should fail - max reached
        let result = manager.spawn_agent("worker").await;
        assert!(matches!(result, Err(ChaosError::PreconditionFailed(_))));
    }

    #[tokio::test]
    async fn test_partial_failure_injection() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let manager = ChaosAgentManager::new(10).with_config(config);

        // Spawn 10 agents
        for _ in 0..10 {
            manager.spawn_agent("worker").await.unwrap();
        }

        // Inject 50% failure
        manager
            .inject_fault(FaultType::PartialFailure { failure_rate: 0.5 })
            .await
            .unwrap();

        // Check that roughly half are dead
        let agents = manager.list_agents().await;
        let alive_count = {
            let mut count = 0;
            for agent in &agents {
                if manager.is_agent_alive(agent).await.unwrap() {
                    count += 1;
                }
            }
            count
        };

        assert!((4..=6).contains(&alive_count));
    }

    #[tokio::test]
    async fn test_chaos_metrics_collection() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let manager = ChaosAgentManager::new(10).with_config(config);

        let agent1 = manager.spawn_agent("worker").await.unwrap();
        let agent2 = manager.spawn_agent("worker").await.unwrap();

        // Inject faults
        manager.kill_agent(&agent1).await.unwrap();
        manager.kill_agent(&agent2).await.unwrap();

        // Recover
        manager.detect_and_recover_crashed().await.unwrap();

        let metrics = manager.get_metrics().await;
        assert_eq!(metrics.faults_injected, 2);
        assert_eq!(metrics.recoveries_successful, 2);
        assert!(!metrics.recovery_times_ms.is_empty());
    }

    #[tokio::test]
    async fn test_health_check_with_crashed_agents() {
        let manager = ChaosAgentManager::new(10);

        let agent_id = manager.spawn_agent("worker").await.unwrap();

        // Healthy state
        assert!(manager.health_check().await.unwrap());

        // After crash
        manager.kill_agent(&agent_id).await.unwrap();
        assert!(!manager.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_rapid_crash_recovery_cycles() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(1),
            ..Default::default()
        };
        let manager = ChaosAgentManager::new(10).with_config(config);

        let agent_id = manager.spawn_agent("worker").await.unwrap();

        // Rapid crash/recovery cycles
        for _ in 0..10 {
            manager.kill_agent(&agent_id).await.unwrap();
            assert!(!manager.is_agent_alive(&agent_id).await.unwrap());

            manager.recover_agent(&agent_id).await.unwrap();
            assert!(manager.is_agent_alive(&agent_id).await.unwrap());
        }

        let metrics = manager.get_metrics().await;
        assert_eq!(metrics.faults_injected, 10);
    }

    #[tokio::test]
    async fn test_concurrent_crash_recovery() {
        let config = ChaosConfig {
            injection_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let manager = Arc::new(ChaosAgentManager::new(100).with_config(config));

        // Spawn many agents
        let mut agent_ids = Vec::new();
        for _ in 0..50 {
            agent_ids.push(manager.spawn_agent("worker").await.unwrap());
        }

        // Concurrently crash and recover
        let mut handles = Vec::new();
        for agent_id in agent_ids.clone() {
            let manager = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                manager.kill_agent(&agent_id).await.unwrap();
                sleep(Duration::from_millis(5)).await;
                manager.recover_agent(&agent_id).await.unwrap();
            }));
        }

        // Wait for all operations
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all agents are alive
        for agent_id in &agent_ids {
            assert!(manager.is_agent_alive(agent_id).await.unwrap());
        }
    }
}
