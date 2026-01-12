//! Daemon management commands

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Get the daemon URL from environment or use default
fn daemon_url() -> String {
    std::env::var("CCA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8580".to_string())
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Start the CCA daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },
    /// Stop the CCA daemon
    Stop,
    /// Show daemon status
    Status,
    /// View daemon logs
    Logs {
        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },
}

fn get_pid_file() -> PathBuf {
    dirs::runtime_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("cca")
        .join("ccad.pid")
}

fn get_log_file() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("cca")
        .join("ccad.log")
}

fn read_pid() -> Option<u32> {
    let pid_file = get_pid_file();
    fs::read_to_string(pid_file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn write_pid(pid: u32) -> Result<()> {
    let pid_file = get_pid_file();
    if let Some(parent) = pid_file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(pid_file, format!("{pid}"))?;
    Ok(())
}

fn remove_pid() -> Result<()> {
    let pid_file = get_pid_file();
    if pid_file.exists() {
        fs::remove_file(pid_file)?;
    }
    Ok(())
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Try to send signal 0 to check if process exists
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        // On non-Unix, check /proc or use other methods
        false
    }
}

pub async fn run(cmd: DaemonCommands) -> Result<()> {
    match cmd {
        DaemonCommands::Start { foreground } => start(foreground).await,
        DaemonCommands::Stop => stop().await,
        DaemonCommands::Status => status().await,
        DaemonCommands::Logs { lines, follow } => logs(lines, follow).await,
    }
}

async fn start(foreground: bool) -> Result<()> {
    // Check if already running
    if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            println!("CCA daemon is already running (PID: {pid})");
            return Ok(());
        }
        // Stale PID file
        remove_pid()?;
    }

    // Check if port is in use via health endpoint
    if let Ok(resp) = reqwest::get(format!("{}/health", daemon_url())).await {
        if resp.status().is_success() {
            println!("CCA daemon appears to be running at {}", daemon_url());
            return Ok(());
        }
    }

    if foreground {
        println!("Starting CCA daemon in foreground...");
        // Execute ccad directly - this will block
        let status = Command::new("ccad").status()?;
        std::process::exit(status.code().unwrap_or(1));
    } else {
        println!("Starting CCA daemon...");

        // Create log directory
        let log_file = get_log_file();
        if let Some(parent) = log_file.parent() {
            fs::create_dir_all(parent)?;
        }

        // Start daemon with output redirected to log file
        let log = fs::File::create(&log_file).context("Failed to create log file")?;
        let err_log = log.try_clone()?;

        let child = Command::new("ccad")
            .stdout(log)
            .stderr(err_log)
            .stdin(Stdio::null())
            .spawn()
            .context("Failed to spawn ccad. Is it installed?")?;

        let pid = child.id();
        write_pid(pid)?;

        // Wait a moment and check if it started successfully
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if is_process_running(pid) {
            println!("CCA daemon started (PID: {pid})");
            println!("Logs: {}", log_file.display());

            // Verify via health check
            for _ in 0..5 {
                if let Ok(resp) = reqwest::get(format!("{}/health", daemon_url())).await {
                    if resp.status().is_success() {
                        println!("Daemon is healthy and accepting connections");
                        return Ok(());
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
            println!("Warning: Daemon started but health check failed. Check logs.");
        } else {
            remove_pid()?;
            return Err(anyhow!(
                "Daemon failed to start. Check logs at {}",
                log_file.display()
            ));
        }
    }
    Ok(())
}

async fn stop() -> Result<()> {
    // First check via API
    if let Ok(resp) = reqwest::get(format!("{}/health", daemon_url())).await {
        if resp.status().is_success() {
            println!("Sending shutdown signal to daemon...");

            // Send SIGTERM via kill if we have the PID
            if let Some(pid) = read_pid() {
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .args(["-TERM", &pid.to_string()])
                        .status();
                }

                // Wait for graceful shutdown
                for i in 0..10 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                    if !is_process_running(pid) {
                        remove_pid()?;
                        println!("CCA daemon stopped");
                        return Ok(());
                    }
                    if i == 5 {
                        println!("Still waiting for daemon to stop...");
                    }
                }

                // Force kill if still running
                println!("Daemon not responding, sending SIGKILL...");
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .status();
                }
                remove_pid()?;
                println!("CCA daemon killed");
                return Ok(());
            }
        }
    }

    // No daemon running or no PID
    if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
            remove_pid()?;
            println!("CCA daemon stopped");
        } else {
            remove_pid()?;
            println!("CCA daemon was not running (stale PID file removed)");
        }
    } else {
        println!("CCA daemon is not running");
    }

    Ok(())
}

async fn status() -> Result<()> {
    println!("CCA Daemon Status");
    println!("=================\n");

    // Check PID file
    let pid_info = if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            format!("PID: {pid} (running)")
        } else {
            format!("PID: {pid} (stale)")
        }
    } else {
        "PID: none".to_string()
    };

    match reqwest::get(format!("{}/api/v1/status", daemon_url())).await {
        Ok(resp) if resp.status().is_success() => {
            let status: serde_json::Value = resp.json().await?;
            println!("Status: running");
            println!("{pid_info}");
            println!(
                "Version: {}",
                status["version"].as_str().unwrap_or("unknown")
            );
            println!(
                "Agents: {}",
                status["agents_count"].as_u64().unwrap_or(0)
            );
            println!(
                "Tasks Pending: {}",
                status["tasks_pending"].as_u64().unwrap_or(0)
            );
            println!(
                "Tasks Completed: {}",
                status["tasks_completed"].as_u64().unwrap_or(0)
            );
        }
        _ => {
            println!("Status: not running");
            println!("{pid_info}");
            println!("\nStart with: cca daemon start");
        }
    }

    Ok(())
}

async fn logs(lines: usize, follow: bool) -> Result<()> {
    let log_file = get_log_file();

    if !log_file.exists() {
        println!("No log file found at {}", log_file.display());
        println!("Daemon may not have been started or logs are elsewhere.");
        return Ok(());
    }

    if follow {
        println!("Following daemon logs (Ctrl+C to stop)...\n");

        // Use tail -f for following
        #[cfg(unix)]
        {
            let mut child = Command::new("tail")
                .args(["-f", "-n", &lines.to_string()])
                .arg(&log_file)
                .spawn()
                .context("Failed to execute tail")?;

            child.wait()?;
        }
        #[cfg(not(unix))]
        {
            println!("Log following not supported on this platform");
        }
    } else {
        println!("Last {lines} lines of daemon logs:\n");

        let file = fs::File::open(&log_file).context("Failed to open log file")?;
        let reader = BufReader::new(file);
        let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

        let start = if all_lines.len() > lines {
            all_lines.len() - lines
        } else {
            0
        };

        for line in &all_lines[start..] {
            println!("{line}");
        }
    }

    Ok(())
}
