//! Daemon management commands

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use super::http;

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

/// Result of attempting to acquire the PID file lock
enum PidLockResult {
    /// Successfully acquired lock, no existing daemon
    Acquired(File),
    /// Another daemon is already running with this PID
    AlreadyRunning(u32),
    /// Error occurred
    Error(anyhow::Error),
}

/// Atomically acquire exclusive lock on PID file and write our PID.
/// This prevents race conditions where multiple processes try to start simultaneously.
///
/// The approach:
/// 1. Open/create the PID file with O_CREAT
/// 2. Acquire an exclusive (write) lock - this blocks or fails if another process holds it
/// 3. Read any existing PID and check if that process is running
/// 4. If no valid running process, truncate and write our PID
/// 5. Keep the lock held (file handle returned) until daemon exits
#[cfg(unix)]
fn acquire_pid_lock() -> PidLockResult {
    use libc::{flock, LOCK_EX, LOCK_NB};
    use std::os::unix::io::AsRawFd;

    let pid_file = get_pid_file();

    // Ensure parent directory exists
    if let Some(parent) = pid_file.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return PidLockResult::Error(anyhow!("Failed to create PID directory: {}", e));
        }
    }

    // Open or create the PID file with restrictive permissions (0600)
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&pid_file)
    {
        Ok(f) => f,
        Err(e) => return PidLockResult::Error(anyhow!("Failed to open PID file: {}", e)),
    };

    // Try to acquire exclusive lock (non-blocking)
    // LOCK_EX = exclusive lock, LOCK_NB = non-blocking (fail immediately if can't acquire)
    let fd = file.as_raw_fd();
    let lock_result = unsafe { flock(fd, LOCK_EX | LOCK_NB) };

    if lock_result != 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::WouldBlock {
            // Another process holds the lock - read the PID to report it
            if let Some(pid) = read_pid() {
                return PidLockResult::AlreadyRunning(pid);
            }
            return PidLockResult::Error(anyhow!(
                "PID file is locked by another process but PID is unreadable"
            ));
        }
        return PidLockResult::Error(anyhow!("Failed to acquire PID lock: {}", err));
    }

    // We have the exclusive lock. Now check if there's an existing PID
    let mut file = file;
    let mut contents = String::new();
    if file.read_to_string(&mut contents).is_ok() {
        if let Ok(existing_pid) = contents.trim().parse::<u32>() {
            // Check if that process is still running
            if is_process_running(existing_pid) {
                // This shouldn't normally happen since we have the lock,
                // but could occur if the previous daemon crashed without releasing lock
                // and the OS released it, but the process somehow survived
                return PidLockResult::AlreadyRunning(existing_pid);
            }
        }
    }

    // No valid running daemon - write our PID
    // Truncate file and write new PID
    if let Err(e) = file.set_len(0) {
        return PidLockResult::Error(anyhow!("Failed to truncate PID file: {}", e));
    }
    if let Err(e) = file.seek(SeekFrom::Start(0)) {
        return PidLockResult::Error(anyhow!("Failed to seek PID file: {}", e));
    }

    PidLockResult::Acquired(file)
}

#[cfg(not(unix))]
fn acquire_pid_lock() -> PidLockResult {
    // On non-Unix platforms, fall back to basic file operations
    // This is less safe but provides basic functionality
    let pid_file = get_pid_file();

    if let Some(parent) = pid_file.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return PidLockResult::Error(anyhow!("Failed to create PID directory: {}", e));
        }
    }

    // Check for existing daemon
    if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            return PidLockResult::AlreadyRunning(pid);
        }
    }

    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&pid_file)
    {
        Ok(f) => PidLockResult::Acquired(f),
        Err(e) => PidLockResult::Error(anyhow!("Failed to open PID file: {}", e)),
    }
}

/// Write PID to an already-locked PID file
fn write_pid_to_locked_file(mut file: &File, pid: u32) -> Result<()> {
    write!(file, "{}", pid).context("Failed to write PID")?;
    file.flush().context("Failed to flush PID file")?;
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
    // Atomically acquire PID file lock to prevent race conditions (SEC-011)
    // This ensures only one daemon can start even if multiple start commands run simultaneously
    let pid_lock = match acquire_pid_lock() {
        PidLockResult::Acquired(file) => file,
        PidLockResult::AlreadyRunning(pid) => {
            println!("CCA daemon is already running (PID: {pid})");
            return Ok(());
        }
        PidLockResult::Error(e) => {
            return Err(e.context("Failed to acquire PID lock"));
        }
    };

    // Check if port is in use via health endpoint (daemon might be running without our PID file)
    if let Ok(resp) = http::get(&format!("{}/api/v1/health", daemon_url())).await {
        if resp.status().is_success() {
            println!("CCA daemon appears to be running at {}", daemon_url());
            return Ok(());
        }
    }

    if foreground {
        println!("Starting CCA daemon in foreground...");
        // Write our PID before starting
        write_pid_to_locked_file(&pid_lock, std::process::id())?;
        // Execute ccad directly - this will block
        // Note: pid_lock is held until this process exits
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
        // Write the child's PID to the locked file
        write_pid_to_locked_file(&pid_lock, pid)?;
        // Drop the lock - the daemon process will manage its own lifecycle
        drop(pid_lock);

        // Wait a moment and check if it started successfully
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if is_process_running(pid) {
            println!("CCA daemon started (PID: {pid})");
            println!("Logs: {}", log_file.display());

            // Verify via health check
            for _ in 0..5 {
                if let Ok(resp) = http::get(&format!("{}/api/v1/health", daemon_url())).await {
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
    if let Ok(resp) = http::get(&format!("{}/api/v1/health", daemon_url())).await {
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

    match http::get(&format!("{}/api/v1/status", daemon_url())).await {
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
