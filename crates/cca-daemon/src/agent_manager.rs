//! Agent Manager - PTY management and process supervision
//!
//! Uses portable-pty to spawn Claude Code instances in pseudo-terminals,
//! enabling proper interactive communication with the agents.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::thread;

use anyhow::{anyhow, Context, Result};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use cca_core::{Agent, AgentId, AgentRole, AgentState};

use crate::config::Config;

/// Manages Claude Code agent instances with PTY support
pub struct AgentManager {
    agents: HashMap<AgentId, ManagedAgent>,
    config: Config,
}

/// A managed agent with its PTY handles
struct ManagedAgent {
    agent: Agent,
    pty_handle: Option<PtyHandle>,
}

/// Handles for PTY communication
struct PtyHandle {
    /// Sender to write to PTY stdin
    stdin_tx: mpsc::Sender<String>,
    /// Receiver for PTY stdout output
    stdout_rx: mpsc::Receiver<String>,
    /// Child process killer
    _child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl AgentManager {
    /// Create a new AgentManager
    pub fn new(config: &Config) -> Self {
        Self {
            agents: HashMap::new(),
            config: config.clone(),
        }
    }

    /// Spawn a new agent with the given role
    pub async fn spawn(&mut self, role: AgentRole) -> Result<AgentId> {
        if self.agents.len() >= self.config.daemon.max_agents {
            return Err(anyhow!(
                "Maximum number of agents ({}) reached",
                self.config.daemon.max_agents
            ));
        }

        let mut agent = Agent::new(role.clone());
        let agent_id = agent.id;

        info!("Spawning agent {} with role {:?}", agent_id, role);

        // Spawn Claude Code in a PTY
        let pty_handle = self.spawn_claude_pty(&agent).await?;

        agent.state = AgentState::Ready;

        self.agents.insert(
            agent_id,
            ManagedAgent {
                agent,
                pty_handle: Some(pty_handle),
            },
        );

        info!("Agent {} spawned successfully", agent_id);
        Ok(agent_id)
    }

    /// Spawn Claude Code in a PTY
    async fn spawn_claude_pty(&self, agent: &Agent) -> Result<PtyHandle> {
        // Create PTY system
        let pty_system = NativePtySystem::default();

        // Create PTY pair with reasonable terminal size
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to create PTY pair")?;

        // Build command for Claude Code
        let claude_md_path = format!("agents/{}.md", agent.role);

        let mut cmd = CommandBuilder::new("claude");
        cmd.arg("--dangerously-skip-permissions");
        cmd.env("CLAUDE_MD", &claude_md_path);
        // Ensure non-interactive mode indicators
        cmd.env("TERM", "dumb");
        cmd.env("NO_COLOR", "1");

        debug!("Spawning Claude Code with CLAUDE_MD={}", claude_md_path);

        // Spawn the child process
        let child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn Claude Code")?;

        // Get reader/writer from PTY master
        let mut writer = pair
            .master
            .take_writer()
            .context("Failed to get PTY writer")?;
        let reader = pair
            .master
            .try_clone_reader()
            .context("Failed to get PTY reader")?;

        // Create channels for async communication
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);
        let (stdout_tx, stdout_rx) = mpsc::channel::<String>(32);

        // Spawn thread to write to PTY stdin
        thread::spawn(move || {
            while let Some(msg) = stdin_rx.blocking_recv() {
                if let Err(e) = writeln!(writer, "{}", msg) {
                    error!("Failed to write to PTY: {}", e);
                    break;
                }
                if let Err(e) = writer.flush() {
                    error!("Failed to flush PTY: {}", e);
                    break;
                }
                debug!("Wrote to PTY: {}", msg);
            }
        });

        // Spawn thread to read from PTY stdout
        thread::spawn(move || {
            let buf_reader = BufReader::new(reader);
            for line in buf_reader.lines() {
                match line {
                    Ok(line) => {
                        debug!("Read from PTY: {}", line);
                        if stdout_tx.blocking_send(line).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to read from PTY: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(PtyHandle {
            stdin_tx,
            stdout_rx,
            _child: child,
        })
    }

    /// Stop an agent
    pub async fn stop(&mut self, agent_id: AgentId) -> Result<()> {
        let managed = self
            .agents
            .get_mut(&agent_id)
            .ok_or_else(|| anyhow!("Agent {} not found", agent_id))?;

        info!("Stopping agent {}", agent_id);

        managed.agent.state = AgentState::Stopping;

        // Drop the PTY handle to close connections
        managed.pty_handle = None;

        managed.agent.state = AgentState::Stopped;
        self.agents.remove(&agent_id);

        info!("Agent {} stopped", agent_id);
        Ok(())
    }

    /// Stop all agents
    pub async fn stop_all(&mut self) -> Result<()> {
        let agent_ids: Vec<_> = self.agents.keys().copied().collect();

        for agent_id in agent_ids {
            if let Err(e) = self.stop(agent_id).await {
                warn!("Error stopping agent {}: {}", agent_id, e);
            }
        }

        Ok(())
    }

    /// List all agents
    pub fn list(&self) -> Vec<&Agent> {
        self.agents.values().map(|m| &m.agent).collect()
    }

    /// Get an agent by ID
    pub fn get(&self, agent_id: AgentId) -> Option<&Agent> {
        self.agents.get(&agent_id).map(|m| &m.agent)
    }

    /// Send a message to an agent and wait for response
    pub async fn send(&mut self, agent_id: AgentId, message: &str) -> Result<String> {
        let managed = self
            .agents
            .get_mut(&agent_id)
            .ok_or_else(|| anyhow!("Agent {} not found", agent_id))?;

        let pty = managed
            .pty_handle
            .as_mut()
            .ok_or_else(|| anyhow!("Agent {} has no PTY handle", agent_id))?;

        // Send message to PTY
        pty.stdin_tx
            .send(message.to_string())
            .await
            .map_err(|_| anyhow!("Failed to send message to agent"))?;

        // Read response (with timeout)
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Self::read_until_complete(&mut pty.stdout_rx),
        )
        .await
        .map_err(|_| anyhow!("Timeout waiting for agent response"))??;

        Ok(response)
    }

    /// Read from stdout until we get a complete response
    async fn read_until_complete(rx: &mut mpsc::Receiver<String>) -> Result<String> {
        let mut output = Vec::new();
        let mut empty_count = 0;

        while let Some(line) = rx.recv().await {
            if line.is_empty() {
                empty_count += 1;
                if empty_count >= 2 {
                    // Two empty lines in a row means end of response
                    break;
                }
            } else {
                empty_count = 0;
                output.push(line);
            }
        }

        Ok(output.join("\n"))
    }

    /// Broadcast a message to all agents
    pub async fn broadcast(&self, message: &str) -> Result<()> {
        info!("Broadcasting message to all agents: {}", message);
        // TODO: Implement actual broadcast via Redis pub/sub
        Ok(())
    }
}
