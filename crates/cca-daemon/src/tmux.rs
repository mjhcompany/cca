//! Tmux integration for auto-spawning agent workers
//!
//! Manages agent workers in tmux windows with 2x2 pane layouts.
//! Maximum 2 windows ("Auto CCA", "Auto CCA #2") = 8 agent slots.

use std::collections::HashMap;
use std::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Maximum number of auto-created windows
const MAX_WINDOWS: usize = 2;
/// Panes per window (2x2 grid)
const PANES_PER_WINDOW: usize = 4;
/// Maximum total auto-spawned agents
pub const MAX_AUTO_AGENTS: usize = MAX_WINDOWS * PANES_PER_WINDOW;

/// Tracks a spawned agent in tmux
#[derive(Debug, Clone)]
pub struct TmuxAgent {
    pub role: String,
    pub window_name: String,
    pub pane_id: String,
    pub spawned_at: std::time::Instant,
}

/// Manages tmux windows and panes for auto-spawning agents
#[derive(Debug)]
pub struct TmuxManager {
    /// Track spawned agents by pane_id
    agents: RwLock<HashMap<String, TmuxAgent>>,
    /// Track windows we've created
    windows: RwLock<Vec<String>>,
    /// Whether tmux is available
    tmux_available: bool,
}

impl TmuxManager {
    pub fn new() -> Self {
        let tmux_available = Self::check_tmux_available();
        if tmux_available {
            info!("Tmux detected - auto-spawn feature enabled");
        } else {
            warn!("Tmux not detected - auto-spawn feature disabled");
        }

        Self {
            agents: RwLock::new(HashMap::new()),
            windows: RwLock::new(Vec::new()),
            tmux_available,
        }
    }

    /// Check if we're running inside tmux
    fn check_tmux_available() -> bool {
        std::env::var("TMUX").is_ok()
    }

    /// Check if tmux auto-spawn is available
    pub fn is_available(&self) -> bool {
        self.tmux_available
    }

    /// Get count of currently spawned agents
    pub async fn spawned_count(&self) -> usize {
        self.agents.read().await.len()
    }

    /// Get spawned agents by role
    pub async fn agents_by_role(&self, role: &str) -> Vec<TmuxAgent> {
        self.agents
            .read()
            .await
            .values()
            .filter(|a| a.role == role)
            .cloned()
            .collect()
    }

    /// Spawn a new agent worker in tmux
    /// Returns the pane_id if successful
    pub async fn spawn_agent(&self, role: &str) -> Result<String, String> {
        if !self.tmux_available {
            return Err("Tmux not available".to_string());
        }

        let spawned = self.spawned_count().await;
        if spawned >= MAX_AUTO_AGENTS {
            return Err(format!(
                "Maximum auto-spawned agents reached ({})",
                MAX_AUTO_AGENTS
            ));
        }

        // Determine which window to use
        let window_index = spawned / PANES_PER_WINDOW;
        let pane_in_window = spawned % PANES_PER_WINDOW;
        let window_name = if window_index == 0 {
            "Auto CCA".to_string()
        } else {
            format!("Auto CCA #{}", window_index + 1)
        };

        // Create window if needed
        let mut windows = self.windows.write().await;
        if !windows.contains(&window_name) {
            self.create_window(&window_name)?;
            windows.push(window_name.clone());
        }
        drop(windows);

        // Create pane if not the first one in the window
        let pane_id = if pane_in_window == 0 {
            // First pane - use the window's default pane
            self.get_window_pane(&window_name)?
        } else {
            // Split to create new pane
            self.create_pane(&window_name, pane_in_window)?
        };

        // Run the agent command in the pane
        let cmd = format!("cca agent worker {}", role);
        self.run_in_pane(&pane_id, &cmd)?;

        // Track the agent
        let agent = TmuxAgent {
            role: role.to_string(),
            window_name: window_name.clone(),
            pane_id: pane_id.clone(),
            spawned_at: std::time::Instant::now(),
        };

        self.agents.write().await.insert(pane_id.clone(), agent);

        info!(
            "Spawned {} agent in tmux window '{}' pane {}",
            role, window_name, pane_id
        );

        Ok(pane_id)
    }

    /// Create a new tmux window
    fn create_window(&self, name: &str) -> Result<(), String> {
        let output = Command::new("tmux")
            .args(["new-window", "-n", name, "-d"])
            .output()
            .map_err(|e| format!("Failed to create tmux window: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Tmux new-window failed: {}", stderr));
        }

        debug!("Created tmux window: {}", name);

        // Set up 2x2 layout
        self.setup_window_layout(name)?;

        Ok(())
    }

    /// Set up 2x2 pane layout in a window
    fn setup_window_layout(&self, window_name: &str) -> Result<(), String> {
        // Split horizontally first (creates 2 panes side by side)
        let _ = Command::new("tmux")
            .args(["split-window", "-h", "-t", &format!(":{}", window_name)])
            .output();

        // Split each pane vertically (creates 4 panes in 2x2)
        let _ = Command::new("tmux")
            .args([
                "split-window",
                "-v",
                "-t",
                &format!(":{}.0", window_name),
            ])
            .output();

        let _ = Command::new("tmux")
            .args([
                "split-window",
                "-v",
                "-t",
                &format!(":{}.2", window_name),
            ])
            .output();

        // Select tiled layout for even distribution
        let _ = Command::new("tmux")
            .args([
                "select-layout",
                "-t",
                &format!(":{}", window_name),
                "tiled",
            ])
            .output();

        debug!("Set up 2x2 layout for window: {}", window_name);
        Ok(())
    }

    /// Get the first pane ID of a window
    fn get_window_pane(&self, window_name: &str) -> Result<String, String> {
        let output = Command::new("tmux")
            .args([
                "list-panes",
                "-t",
                &format!(":{}", window_name),
                "-F",
                "#{pane_id}",
            ])
            .output()
            .map_err(|e| format!("Failed to list panes: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Tmux list-panes failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pane_id = stdout
            .lines()
            .next()
            .ok_or("No panes found")?
            .trim()
            .to_string();

        Ok(pane_id)
    }

    /// Create a new pane by splitting
    fn create_pane(&self, window_name: &str, pane_index: usize) -> Result<String, String> {
        // The layout is already set up, just get the pane at the index
        let output = Command::new("tmux")
            .args([
                "list-panes",
                "-t",
                &format!(":{}", window_name),
                "-F",
                "#{pane_id}",
            ])
            .output()
            .map_err(|e| format!("Failed to list panes: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Tmux list-panes failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let panes: Vec<&str> = stdout.lines().collect();

        if pane_index >= panes.len() {
            return Err(format!(
                "Pane index {} out of range (have {} panes)",
                pane_index,
                panes.len()
            ));
        }

        Ok(panes[pane_index].trim().to_string())
    }

    /// Run a command in a specific pane
    fn run_in_pane(&self, pane_id: &str, command: &str) -> Result<(), String> {
        let output = Command::new("tmux")
            .args(["send-keys", "-t", pane_id, command, "Enter"])
            .output()
            .map_err(|e| format!("Failed to send keys to pane: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Tmux send-keys failed: {}", stderr));
        }

        debug!("Ran command in pane {}: {}", pane_id, command);
        Ok(())
    }

    /// Remove the oldest tracked agent for a given role
    /// Returns the removed agent's pane_id if found
    pub async fn remove_agent_by_role(&self, role: &str) -> Option<String> {
        let mut agents = self.agents.write().await;
        // Find the oldest agent with this role
        let oldest_pane_id = agents
            .iter()
            .filter(|(_, a)| a.role.to_lowercase() == role.to_lowercase())
            .min_by_key(|(_, a)| a.spawned_at)
            .map(|(pane_id, _)| pane_id.clone());

        if let Some(pane_id) = oldest_pane_id {
            agents.remove(&pane_id);
            info!("Removed tracked {} agent from pane {}", role, pane_id);
            Some(pane_id)
        } else {
            None
        }
    }

    /// Get all spawned agents info
    pub async fn list_agents(&self) -> Vec<TmuxAgent> {
        self.agents.read().await.values().cloned().collect()
    }

    /// Kill all auto-spawned agents
    pub async fn cleanup(&self) {
        let agents = self.agents.read().await;
        for (pane_id, agent) in agents.iter() {
            info!("Cleaning up agent {} in pane {}", agent.role, pane_id);
            // Send Ctrl+C to stop the agent
            let _ = Command::new("tmux")
                .args(["send-keys", "-t", pane_id, "C-c"])
                .output();
        }
        drop(agents);

        // Clear tracking
        self.agents.write().await.clear();

        // Kill the windows we created
        let windows = self.windows.read().await;
        for window in windows.iter() {
            let _ = Command::new("tmux")
                .args(["kill-window", "-t", &format!(":{}", window)])
                .output();
        }
    }
}

impl Default for TmuxManager {
    fn default() -> Self {
        Self::new()
    }
}
