//! Agent Manager - Claude Code process management
//!
//! Manages Claude Code agent instances with two modes:
//! 1. Task mode: Uses -p (print) for reliable non-interactive task execution
//! 2. Interactive mode: Uses PTY for attach/detach console access
//!
//! Note: Some methods are infrastructure for future features and not yet called.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use cca_core::{Agent, AgentId, AgentRole, AgentState};
use cca_core::util::{safe_truncate, safe_truncate_with_ellipsis};

use crate::config::{Config, PermissionsConfig};

/// `SEC-007`: Apply permission configuration to a tokio Command
/// This replaces the blanket `--dangerously-skip-permissions` with granular control
pub fn apply_permissions_to_command(cmd: &mut Command, permissions: &PermissionsConfig, role: &str) {
    let mode = permissions.get_mode(role);

    match mode {
        "dangerous" => {
            // Legacy mode - NOT RECOMMENDED
            // Only use this in fully sandboxed environments (containers/VMs)
            warn!(
                "SEC-007: Using dangerous permission mode for role '{}'. \
                 This bypasses all permission checks. Ensure environment is sandboxed.",
                role
            );
            cmd.arg("--dangerously-skip-permissions");
        }
        "sandbox" => {
            // Sandbox mode - expects external sandboxing, uses minimal permissions
            info!(
                "SEC-007: Using sandbox permission mode for role '{}'. \
                 External sandboxing (container/VM) is expected.",
                role
            );
            // In sandbox mode, we still use allowlist but with minimal tools
            // This provides defense-in-depth even when sandboxed
            cmd.arg("--allowedTools");
            cmd.arg("Read,Glob,Grep");  // Minimal read-only access

            // Apply denials
            let denied = permissions.get_denied_tools(role);
            if !denied.is_empty() {
                cmd.arg("--disallowedTools");
                cmd.arg(denied.join(","));
            }
        }
        _ => {
            // Allowlist mode - secure default
            // Uses --allowedTools to specify exactly what's permitted
            let allowed = permissions.get_allowed_tools(role);
            let denied = permissions.get_denied_tools(role);

            info!(
                "SEC-007: Using allowlist permission mode for role '{}'. \
                 Allowed: {} tools, Denied: {} patterns.",
                role,
                allowed.len(),
                denied.len()
            );

            if !allowed.is_empty() {
                cmd.arg("--allowedTools");
                cmd.arg(allowed.join(","));
            }

            if !denied.is_empty() {
                cmd.arg("--disallowedTools");
                cmd.arg(denied.join(","));
            }
        }
    }
}

/// `SEC-007`: Apply permission configuration to a `portable_pty` `CommandBuilder`
/// This is the PTY variant for interactive sessions
pub fn apply_permissions_to_pty_command(cmd: &mut CommandBuilder, permissions: &PermissionsConfig, role: &str) {
    let mode = permissions.get_mode(role);

    match mode {
        "dangerous" => {
            cmd.arg("--dangerously-skip-permissions");
        }
        "sandbox" => {
            cmd.arg("--allowedTools");
            cmd.arg("Read,Glob,Grep");

            let denied = permissions.get_denied_tools(role);
            if !denied.is_empty() {
                cmd.arg("--disallowedTools");
                cmd.arg(denied.join(","));
            }
        }
        _ => {
            let allowed = permissions.get_allowed_tools(role);
            let denied = permissions.get_denied_tools(role);

            if !allowed.is_empty() {
                cmd.arg("--allowedTools");
                cmd.arg(allowed.join(","));
            }

            if !denied.is_empty() {
                cmd.arg("--disallowedTools");
                cmd.arg(denied.join(","));
            }
        }
    }
}

/// Manages Claude Code agent instances
pub struct AgentManager {
    agents: HashMap<AgentId, ManagedAgent>,
    config: Config,
}

/// Maximum number of log entries to keep per agent
const MAX_LOG_ENTRIES: usize = 100;

/// A managed agent with optional interactive PTY session
struct ManagedAgent {
    agent: Agent,
    /// PTY handle for interactive sessions (attach/detach)
    interactive_session: Option<InteractiveSession>,
    /// Current task being executed (if any)
    current_task: Option<String>,
    /// Recent log entries for this agent
    logs: Vec<LogEntry>,
}

/// A log entry for an agent
#[derive(Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub message: String,
}

/// Configuration needed to execute a task for an agent
#[derive(Clone)]
pub struct TaskConfig {
    pub role: AgentRole,
    pub claude_path: String,
    pub claude_md_path: String,
    pub system_prompt: Option<String>,
}

/// Handles for interactive PTY communication
struct InteractiveSession {
    /// Sender to write to PTY stdin
    stdin_tx: mpsc::Sender<String>,
    /// Receiver for PTY stdout output
    stdout_rx: mpsc::Receiver<String>,
    /// Child process
    _child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl AgentManager {
    /// Create a new `AgentManager`
    pub fn new(config: &Config) -> Self {
        Self {
            agents: HashMap::new(),
            config: config.clone(),
        }
    }

    /// Spawn a new agent with the given role
    /// This registers the agent - actual Claude Code processes are spawned
    /// per-task in `send()` or on-demand via `start_interactive_session()`
    pub async fn spawn(&mut self, role: AgentRole) -> Result<AgentId> {
        if self.agents.len() >= self.config.daemon.max_agents {
            return Err(anyhow!(
                "Maximum number of agents ({}) reached",
                self.config.daemon.max_agents
            ));
        }

        let mut agent = Agent::new(role.clone());
        let agent_id = agent.id;

        info!("Registering agent {} with role {:?}", agent_id, role);

        agent.state = AgentState::Ready;

        self.agents.insert(
            agent_id,
            ManagedAgent {
                agent,
                interactive_session: None,
                current_task: None,
                logs: Vec::new(),
            },
        );

        info!("Agent {} registered successfully", agent_id);
        Ok(agent_id)
    }

    /// Start an interactive PTY session for an agent (for attach functionality)
    pub async fn start_interactive_session(&mut self, agent_id: AgentId) -> Result<()> {
        let managed = self
            .agents
            .get_mut(&agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

        if managed.interactive_session.is_some() {
            return Ok(()); // Already has an interactive session
        }

        let role = &managed.agent.role;
        let claude_path = &self.config.agents.claude_path;
        let data_dir = self.config.daemon.get_data_dir();
        let claude_md_path = data_dir.join("agents").join(format!("{role}.md"))
            .to_string_lossy().to_string();

        info!("Starting interactive session for agent {}", agent_id);

        // Create PTY system
        let pty_system = NativePtySystem::default();

        // Create PTY pair
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to create PTY pair")?;

        // Build command for Claude Code (interactive mode)
        let mut cmd = CommandBuilder::new(claude_path);

        // SEC-007: Apply permission configuration instead of blanket --dangerously-skip-permissions
        let role_str = role.to_string();
        apply_permissions_to_pty_command(&mut cmd, &self.config.agents.permissions, &role_str);

        cmd.env("CLAUDE_MD", &claude_md_path);
        cmd.env("TERM", "dumb");
        cmd.env("NO_COLOR", "1");

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

        // Spawn blocking task to write to PTY stdin
        tokio::task::spawn_blocking(move || {
            while let Some(msg) = stdin_rx.blocking_recv() {
                if let Err(e) = writeln!(writer, "{msg}") {
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

        // Spawn blocking task to read from PTY stdout
        tokio::task::spawn_blocking(move || {
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

        managed.interactive_session = Some(InteractiveSession {
            stdin_tx,
            stdout_rx,
            _child: child,
        });

        info!("Interactive session started for agent {}", agent_id);
        Ok(())
    }

    /// Send a message to an interactive session and wait for response
    pub async fn send_interactive(
        &mut self,
        agent_id: AgentId,
        message: &str,
    ) -> Result<String> {
        let managed = self
            .agents
            .get_mut(&agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

        let session = managed
            .interactive_session
            .as_mut()
            .ok_or_else(|| anyhow!("No interactive session for agent {agent_id}"))?;

        // Send message to PTY
        session
            .stdin_tx
            .send(message.to_string())
            .await
            .map_err(|_| anyhow!("Failed to send message to agent"))?;

        // Read response (with timeout)
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            Self::read_until_complete(&mut session.stdout_rx),
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

    /// Stop an agent's interactive session
    pub async fn stop_interactive_session(&mut self, agent_id: AgentId) -> Result<()> {
        let managed = self
            .agents
            .get_mut(&agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

        if managed.interactive_session.is_some() {
            managed.interactive_session = None;
            info!("Interactive session stopped for agent {}", agent_id);
        }

        Ok(())
    }

    /// Stop an agent
    pub async fn stop(&mut self, agent_id: AgentId) -> Result<()> {
        let managed = self
            .agents
            .get_mut(&agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

        info!("Stopping agent {}", agent_id);

        managed.agent.state = AgentState::Stopping;

        // Drop interactive session if any
        managed.interactive_session = None;

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

    /// Check if agent has an interactive session
    pub fn has_interactive_session(&self, agent_id: AgentId) -> bool {
        self.agents
            .get(&agent_id)
            .is_some_and(|m| m.interactive_session.is_some())
    }

    /// Prepare an agent for task execution (call before releasing lock)
    /// Returns the config needed to execute the task
    pub fn prepare_task(&mut self, agent_id: AgentId, message: &str) -> Result<TaskConfig> {
        let managed = self
            .agents
            .get_mut(&agent_id)
            .ok_or_else(|| anyhow!("Agent {agent_id} not found"))?;

        let role = managed.agent.role.clone();
        let claude_path = self.config.agents.claude_path.clone();
        let data_dir = self.config.daemon.get_data_dir();
        let claude_md_path = data_dir.join("agents").join(format!("{role}.md"))
            .to_string_lossy().to_string();

        // Set current task
        let task_preview = safe_truncate_with_ellipsis(message, 100);
        managed.current_task = Some(task_preview.clone());

        // Add log entry for task start
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: "INFO".to_string(),
            message: format!("Starting task: {task_preview}"),
        };
        managed.logs.push(entry);
        if managed.logs.len() > MAX_LOG_ENTRIES {
            managed.logs.remove(0);
        }

        Ok(TaskConfig {
            role,
            claude_path,
            claude_md_path,
            system_prompt: None,
        })
    }

    /// Record task completion (call after task finishes, re-acquire lock first)
    pub fn record_task_result(&mut self, agent_id: AgentId, success: bool, output: &str, error: Option<&str>) {
        if let Some(managed) = self.agents.get_mut(&agent_id) {
            managed.current_task = None;

            let entry = if success {
                LogEntry {
                    timestamp: Utc::now(),
                    level: "INFO".to_string(),
                    message: format!("Task completed successfully ({} bytes)", output.len()),
                }
            } else {
                LogEntry {
                    timestamp: Utc::now(),
                    level: "ERROR".to_string(),
                    message: format!("Task failed: {}", error.unwrap_or("unknown error")),
                }
            };
            managed.logs.push(entry);
            if managed.logs.len() > MAX_LOG_ENTRIES {
                managed.logs.remove(0);
            }

            // Add output preview for successful tasks
            if success {
                let output_preview = safe_truncate_with_ellipsis(output, 200);
                let debug_entry = LogEntry {
                    timestamp: Utc::now(),
                    level: "DEBUG".to_string(),
                    message: format!("Output: {}", output_preview.replace('\n', "\\n")),
                };
                managed.logs.push(debug_entry);
                if managed.logs.len() > MAX_LOG_ENTRIES {
                    managed.logs.remove(0);
                }
            }
        }
    }

    /// Send a task to an agent using print mode (`-p`) for reliable execution
    /// This spawns a new Claude Code process for each task
    /// WARNING: This method holds the lock during execution - use `prepare_task`/`record_task_result`
    /// for concurrent execution.
    pub async fn send(&mut self, agent_id: AgentId, message: &str) -> Result<String> {
        // Get agent info and update current task
        let config = self.prepare_task(agent_id, message)?;

        info!(
            "Sending task to {} agent {}: {}",
            config.role,
            agent_id,
            safe_truncate(message, 100)
        );

        // SEC-007: Build Claude Code command with proper permission configuration
        let mut cmd = Command::new(&config.claude_path);

        // Apply permission configuration (replaces blanket --dangerously-skip-permissions)
        let role_str = config.role.to_string();
        apply_permissions_to_command(&mut cmd, &self.config.agents.permissions, &role_str);

        // Non-interactive mode
        let output = cmd
            .arg("--print")
            .arg("--output-format")
            .arg("text")
            .arg(message) // The prompt/task
            .env("CLAUDE_MD", &config.claude_md_path)
            .env("NO_COLOR", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()
            .await?;

        if output.status.success() {
            let response = String::from_utf8_lossy(&output.stdout).to_string();
            debug!("Agent {} response length: {} bytes", agent_id, response.len());
            self.record_task_result(agent_id, true, &response, None);
            Ok(response)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            self.record_task_result(agent_id, false, "", Some(&stderr));
            Err(anyhow!("Claude Code failed: {stderr}"))
        }
    }

    /// Add a log entry for an agent (public for external use)
    pub fn add_log(&mut self, agent_id: AgentId, level: &str, message: &str) {
        if let Some(managed) = self.agents.get_mut(&agent_id) {
            let entry = LogEntry {
                timestamp: Utc::now(),
                level: level.to_string(),
                message: message.to_string(),
            };
            managed.logs.push(entry);

            // Keep only the last MAX_LOG_ENTRIES
            if managed.logs.len() > MAX_LOG_ENTRIES {
                managed.logs.remove(0);
            }
        }
    }

    /// Get current task for an agent
    pub fn get_current_task(&self, agent_id: AgentId) -> Option<String> {
        self.agents.get(&agent_id).and_then(|m| m.current_task.clone())
    }

    /// Clear current task for an agent (used when task times out or is cancelled)
    pub fn clear_current_task(&mut self, agent_id: AgentId) {
        if let Some(managed) = self.agents.get_mut(&agent_id) {
            managed.current_task = None;
        }
    }

    /// Get logs for an agent
    pub fn get_logs(&self, agent_id: AgentId, limit: usize) -> Vec<LogEntry> {
        self.agents
            .get(&agent_id)
            .map(|m| {
                let logs = &m.logs;
                if logs.len() > limit {
                    logs[logs.len() - limit..].to_vec()
                } else {
                    logs.clone()
                }
            })
            .unwrap_or_default()
    }

    /// Send a task to an agent with custom timeout
    pub async fn send_with_timeout(
        &mut self,
        agent_id: AgentId,
        message: &str,
        timeout: Duration,
    ) -> Result<String> {
        tokio::time::timeout(timeout, self.send(agent_id, message))
            .await
            .map_err(|_| anyhow!("Timeout waiting for agent response"))?
    }

    /// Broadcast a message to all agents
    pub async fn broadcast(&self, message: &str) -> Result<()> {
        info!("Broadcasting message to all agents: {}", message);
        // Broadcast would need to send to all agents with interactive sessions
        // For now, this is a placeholder
        Ok(())
    }
}
