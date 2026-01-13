//! Agent management commands

use anyhow::{Context, Result};
use clap::Subcommand;
use cca_core::util::safe_truncate;
use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncBufReadExt;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use super::http;

/// Get the daemon URL from environment or use default
fn daemon_url() -> String {
    std::env::var("CCA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8580".to_string())
}

/// Get the ACP WebSocket URL from environment or use default
fn acp_url() -> String {
    std::env::var("CCA_ACP_URL").unwrap_or_else(|_| "ws://127.0.0.1:8581".to_string())
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// List connected agent workers
    List,
    /// Stop/disconnect a worker
    Stop {
        /// Agent ID (short or full) or role name
        id: String,
    },
    /// Send a task to a specific worker
    Send {
        /// Agent ID (short or full) or role name
        id: String,

        /// Task message to send
        message: String,
    },
    /// Run system diagnostics
    Diag,
    /// Run as an agent worker (connects via WebSocket)
    Worker {
        /// Agent role (coordinator, frontend, backend, dba, devops, security, qa)
        role: String,
    },
}

async fn check_daemon() -> Result<()> {
    let resp = http::get(&format!("{}/api/v1/health", daemon_url()))
        .await
        .context("Could not connect to CCA daemon. Is it running?")?;

    if !resp.status().is_success() {
        anyhow::bail!("CCA daemon is not healthy. Status: {}", resp.status());
    }
    Ok(())
}

pub async fn run(cmd: AgentCommands) -> Result<()> {
    match cmd {
        AgentCommands::List => list().await,
        AgentCommands::Stop { id } => stop(&id).await,
        AgentCommands::Send { id, message } => send(&id, &message).await,
        AgentCommands::Diag => diag().await,
        AgentCommands::Worker { role } => worker(&role).await,
    }
}

/// Resolve agent identifier (short ID, full ID, or role name) to full agent ID
async fn resolve_worker_id(id_or_role: &str) -> Result<String> {
    let resp = http::get(&format!("{}/api/v1/acp/status", daemon_url()))
        .await
        .context("Failed to fetch workers")?;

    let data: serde_json::Value = resp.json().await?;

    if let Some(workers) = data["workers"].as_array() {
        for worker in workers {
            let agent_id = worker["agent_id"].as_str().unwrap_or("");
            let role = worker["role"].as_str().unwrap_or("");

            // Match by role name (case-insensitive)
            if role.to_lowercase() == id_or_role.to_lowercase() {
                return Ok(agent_id.to_string());
            }

            // Match by full ID
            if agent_id == id_or_role {
                return Ok(agent_id.to_string());
            }

            // Match by short ID (prefix match)
            if agent_id.starts_with(id_or_role) {
                return Ok(agent_id.to_string());
            }
        }
    }

    Err(anyhow::anyhow!(
        "Worker '{}' not found. Use 'cca agent list' to see connected workers.",
        id_or_role
    ))
}

/// List connected agent workers
async fn list() -> Result<()> {
    check_daemon().await?;

    let resp = http::get(&format!("{}/api/v1/acp/status", daemon_url())).await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await?;
            let count = data["connected_agents"].as_u64().unwrap_or(0);

            println!("\nConnected Workers: {}\n", count);

            if let Some(workers) = data["workers"].as_array() {
                if workers.is_empty() {
                    println!("No workers connected.\n");
                    println!("Start workers with:");
                    println!("  cca agent worker coordinator");
                    println!("  cca agent worker backend");
                    println!("  cca agent worker frontend");
                    println!("  cca agent worker dba");
                    println!("  cca agent worker devops");
                    println!("  cca agent worker security");
                    println!("  cca agent worker qa");
                } else {
                    println!("{:<10} {:<12} {:<40}", "ID", "ROLE", "FULL ID");
                    println!("{}", "-".repeat(65));
                    for worker in workers {
                        let id = worker["agent_id"].as_str().unwrap_or("-");
                        let short_id = safe_truncate(id, 8);
                        let role = worker["role"].as_str().unwrap_or("unregistered");
                        println!("{:<10} {:<12} {}", short_id, role, id);
                    }
                    println!();
                    println!("Use role name (e.g., 'backend') or short ID to interact with workers");
                }
            } else if count == 0 {
                println!("No workers connected.\n");
                println!("Start workers with: cca agent worker <role>");
            }
        }
        Ok(r) => {
            println!("Failed to get workers: HTTP {}", r.status());
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    Ok(())
}

/// Stop/disconnect a worker
async fn stop(id: &str) -> Result<()> {
    check_daemon().await?;

    let agent_id = resolve_worker_id(id).await?;
    let short_id = safe_truncate(&agent_id, 8);

    println!("Disconnecting worker {}...", short_id);

    // Send disconnect request to daemon
    let resp = http::post_json(
        &format!("{}/api/v1/acp/disconnect", daemon_url()),
        &serde_json::json!({ "agent_id": agent_id }),
    )
    .await
    .context("Failed to send disconnect request")?;

    if resp.status().is_success() {
        println!("Worker {} disconnected successfully", short_id);
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        println!("Failed to disconnect worker: {} - {}", status, body);
    }

    Ok(())
}

/// Send a task to a specific worker
async fn send(id: &str, message: &str) -> Result<()> {
    check_daemon().await?;

    let agent_id = resolve_worker_id(id).await?;
    let short_id = safe_truncate(&agent_id, 8);

    println!("Sending task to worker {}...", short_id);

    // Send task via daemon API
    let resp = http::post_json(
        &format!("{}/api/v1/acp/send", daemon_url()),
        &serde_json::json!({
            "agent_id": agent_id,
            "task": message
        }),
    )
    .await
    .context("Failed to send task")?;

    if resp.status().is_success() {
        let data: serde_json::Value = resp.json().await?;
        println!("\nResponse from worker {}:", short_id);
        if let Some(output) = data["output"].as_str() {
            println!("{}", output);
        } else if let Some(error) = data["error"].as_str() {
            println!("Error: {}", error);
        } else {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        println!("Failed to send task: {} - {}", status, body);
    }

    Ok(())
}

/// Run system diagnostics
async fn diag() -> Result<()> {
    println!("CCA System Diagnostics");
    println!("{}\n", "=".repeat(50));

    // 1. Check daemon health
    print!("Daemon health........... ");
    let health_resp = http::get(&format!("{}/api/v1/health", daemon_url())).await;
    match health_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let status = data["status"].as_str().unwrap_or("unknown");
            println!("[OK] status={}", status);
        }
        Ok(r) => println!("[FAIL] HTTP {}", r.status()),
        Err(e) => println!("[FAIL] {}", e),
    }

    // 2. Check ACP WebSocket server
    print!("ACP WebSocket........... ");
    let acp_resp = http::get(&format!("{}/api/v1/acp/status", daemon_url())).await;
    match acp_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let running = data["running"].as_bool().unwrap_or(false);
            let port = data["port"].as_u64().unwrap_or(0);
            let agents = data["connected_agents"].as_u64().unwrap_or(0);
            if running {
                println!("[OK] port={}, connected_workers={}", port, agents);
            } else {
                println!("[WARN] not running");
            }
        }
        Ok(r) => println!("[FAIL] HTTP {}", r.status()),
        Err(e) => println!("[FAIL] {}", e),
    }

    // 3. Check Redis
    print!("Redis................... ");
    let redis_resp = http::get(&format!("{}/api/v1/redis/status", daemon_url())).await;
    match redis_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let connected = data["connected"].as_bool().unwrap_or(false);
            if connected {
                println!("[OK] connected");
            } else {
                println!("[WARN] not connected");
            }
        }
        Ok(r) => println!("[SKIP] HTTP {}", r.status()),
        Err(e) => println!("[FAIL] {}", e),
    }

    // 4. Check PostgreSQL
    print!("PostgreSQL.............. ");
    let pg_resp = http::get(&format!("{}/api/v1/postgres/status", daemon_url())).await;
    match pg_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let connected = data["connected"].as_bool().unwrap_or(false);
            let patterns = data["patterns_count"].as_i64().unwrap_or(0);
            if connected {
                println!("[OK] patterns={}", patterns);
            } else {
                println!("[WARN] not connected");
            }
        }
        Ok(r) => println!("[SKIP] HTTP {}", r.status()),
        Err(e) => println!("[FAIL] {}", e),
    }

    // 5. Check RL Engine
    print!("RL Engine............... ");
    let rl_resp = http::get(&format!("{}/api/v1/rl/stats", daemon_url())).await;
    match rl_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let algo = data["algorithm"].as_str().unwrap_or("unknown");
            let steps = data["total_steps"].as_u64().unwrap_or(0);
            let exp = data["experience_count"].as_u64().unwrap_or(0);
            println!("[OK] algo={}, steps={}, experiences={}", algo, steps, exp);
        }
        Ok(r) => println!("[SKIP] HTTP {}", r.status()),
        Err(e) => println!("[FAIL] {}", e),
    }

    // 6. List connected workers
    println!("\nConnected Workers:");
    let workers_resp = http::get(&format!("{}/api/v1/acp/status", daemon_url())).await;
    match workers_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(workers) = data["workers"].as_array() {
                if workers.is_empty() {
                    println!("  (none - start workers with: cca agent worker <role>)");
                } else {
                    for worker in workers {
                        let id = worker["agent_id"].as_str().unwrap_or("-");
                        let short_id = safe_truncate(id, 8);
                        let role = worker["role"].as_str().unwrap_or("unregistered");
                        println!("  {} ({})", short_id, role);
                    }
                }
            } else {
                println!("  (none)");
            }
        }
        _ => println!("  (failed to fetch)"),
    }

    // 7. List recent tasks
    println!("\nRecent Tasks:");
    let tasks_resp = http::get(&format!("{}/api/v1/tasks?limit=5", daemon_url())).await;
    match tasks_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(tasks) = data["tasks"].as_array() {
                if tasks.is_empty() {
                    println!("  (none)");
                } else {
                    for task in tasks {
                        let id = task["id"].as_str().unwrap_or("-");
                        let short_id = safe_truncate(id, 8);
                        let status = task["status"].as_str().unwrap_or("-");
                        let desc = task["description"].as_str().unwrap_or("-");
                        let short_desc = truncate_line(desc, 40);
                        println!("  {} [{}] {}", short_id, status, short_desc);
                    }
                }
            }
        }
        _ => println!("  (failed to fetch)"),
    }

    // 8. Check workloads
    println!("\nWorkloads:");
    let workload_resp = http::get(&format!("{}/api/v1/workloads", daemon_url())).await;
    match workload_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let total = data["total_tasks"].as_u64().unwrap_or(0);
            let pending = data["pending_tasks"].as_u64().unwrap_or(0);
            println!("  Total tasks: {}, Pending: {}", total, pending);
        }
        _ => println!("  (failed to fetch)"),
    }

    println!("\n{}", "=".repeat(50));
    println!("Diagnostics complete.");

    Ok(())
}

/// Truncate a file path for display (returns owned String for UTF-8 safety)
fn truncate_path(path: &str) -> String {
    // Show last component or last 40 chars
    let char_count = path.chars().count();
    if char_count <= 50 {
        path.to_string()
    } else if let Some(pos) = path.rfind('/') {
        // Safe: rfind returns byte position of '/', and '/' is ASCII (1 byte)
        // so pos + 1 is a valid UTF-8 boundary
        path[pos + 1..].to_string()
    } else {
        // Take the last 40 characters safely
        let skip_chars = char_count.saturating_sub(40);
        path.chars().skip(skip_chars).collect()
    }
}

/// Truncate a line for display (safe for UTF-8)
fn truncate_line(line: &str, max_len: usize) -> String {
    let char_count = line.chars().count();
    if char_count <= max_len {
        line.to_string()
    } else {
        format!("{}...", safe_truncate(line, max_len.saturating_sub(3)))
    }
}

/// Format a tool action for display
fn format_tool_action(tool_name: &str, input: Option<&serde_json::Value>) -> String {
    let icon = match tool_name {
        "Read" => "📖",
        "Edit" => "✏️ ",
        "Write" => "💾",
        "Grep" => "🔍",
        "Glob" => "📂",
        "Bash" => "⚙️ ",
        "Task" => "🤖",
        "WebFetch" => "🌐",
        "WebSearch" => "🔎",
        "TodoWrite" => "📋",
        "LSP" => "🔧",
        _ => "🔧",
    };

    let detail = match tool_name {
        "Read" => {
            input.and_then(|i| i.get("file_path"))
                .and_then(|p| p.as_str())
                .map(truncate_path)
                .unwrap_or_default()
        }
        "Edit" | "Write" => {
            input.and_then(|i| i.get("file_path"))
                .and_then(|p| p.as_str())
                .map(truncate_path)
                .unwrap_or_default()
        }
        "Grep" => {
            input.and_then(|i| i.get("pattern"))
                .and_then(|p| p.as_str())
                .map(|p| format!("'{}'", truncate_line(p, 30)))
                .unwrap_or_default()
        }
        "Glob" => {
            input.and_then(|i| i.get("pattern"))
                .and_then(|p| p.as_str())
                .map(|p| truncate_line(&p, 40))
                .unwrap_or_default()
        }
        "Bash" => {
            input.and_then(|i| i.get("command"))
                .and_then(|c| c.as_str())
                .map(|c| truncate_line(c, 50))
                .unwrap_or_default()
        }
        "Task" => {
            input.and_then(|i| i.get("description"))
                .and_then(|d| d.as_str())
                .map(|d| truncate_line(d, 40))
                .unwrap_or_else(|| "spawning agent".to_string())
        }
        _ => String::new(),
    };

    if detail.is_empty() {
        format!("{} {}", icon, tool_name)
    } else {
        format!("{} {} {}", icon, tool_name, detail)
    }
}

/// Run as a persistent agent worker connected via WebSocket
async fn worker(role: &str) -> Result<()> {
    let agent_id = Uuid::new_v4();
    let ws_url = acp_url();

    println!("Starting {} agent worker (ID: {})", role, agent_id);
    println!("Connecting to ACP server at {}...", ws_url);

    // Connect to WebSocket
    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .context("Failed to connect to ACP WebSocket server")?;

    let (mut write, mut read) = ws_stream.split();

    // Authenticate with the server first (load API key from config)
    if let Some(api_key) = http::load_api_key() {
        println!("Authenticating with ACP server...");
        let auth_msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "agent.authenticate",
            "params": {
                "api_key": api_key
            },
            "id": Uuid::new_v4().to_string()
        });

        write
            .send(Message::Text(auth_msg.to_string().into()))
            .await
            .context("Failed to send authentication message")?;

        // Wait for auth response
        if let Some(Ok(Message::Text(text))) = read.next().await {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if json.get("error").is_some() {
                    let err = json["error"]["message"].as_str().unwrap_or("Authentication failed");
                    anyhow::bail!("ACP authentication failed: {}", err);
                }
                println!("Authenticated successfully");
            }
        }
    } else {
        println!("Warning: No API key found in config - registration may fail");
    }

    // Register with the server
    let register_msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "agent.register",
        "params": {
            "agent_id": agent_id.to_string(),
            "role": role,
            "capabilities": ["execute_task"]
        },
        "id": Uuid::new_v4().to_string()
    });

    write
        .send(Message::Text(register_msg.to_string().into()))
        .await
        .context("Failed to send registration message")?;

    // Wait for registration response
    if let Some(Ok(Message::Text(text))) = read.next().await {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(result) = json.get("result") {
                if result.get("success").and_then(|s| s.as_bool()) == Some(false) {
                    let err = result.get("error").and_then(|e| e.as_str()).unwrap_or("Registration failed");
                    anyhow::bail!("Role registration failed: {}", err);
                }
            }
        }
    }

    println!("Registered as {} worker. Waiting for tasks...", role);
    println!("Press Ctrl+C to stop.\n");

    // Get claude path from environment or default
    let claude_path = std::env::var("CCA_CLAUDE_PATH").unwrap_or_else(|_| "claude".to_string());

    // Get data dir for agent markdown files
    let data_dir =
        std::env::var("CCA_DATA_DIR").unwrap_or_else(|_| "/usr/local/share/cca".to_string());
    let claude_md_path = format!("{}/agents/{}.md", data_dir, role);

    // Main message loop
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    // Check if this is a task assignment
                    if json.get("method").and_then(|m| m.as_str()) == Some("task.execute") {
                        let request_id = json.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let params = json.get("params").cloned().unwrap_or_default();
                        let task = params.get("task").and_then(|t| t.as_str()).unwrap_or("");
                        let context = params.get("context").and_then(|c| c.as_str());

                        println!("\n{}", "=".repeat(60));
                        println!("[TASK] Request ID: {}", request_id);
                        println!("[TASK] Task ({} chars):", task.len());
                        println!("{}", task);
                        if let Some(ctx) = context {
                            println!("[TASK] Context provided ({} chars)", ctx.len());
                        }
                        println!("{}", "-".repeat(60));

                        // Build the full message with context if provided
                        let full_message = if role == "coordinator" {
                            // Coordinator gets explicit JSON-only instruction prepended
                            let json_instruction = r#"OUTPUT ONLY JSON. Your response must be a single JSON object starting with { and ending with }. No text, no markdown, no explanations.

Format: {"action": "delegate", "delegations": [{"role": "backend|frontend|dba|devops|security|qa", "task": "specific task", "context": "context"}], "summary": "brief summary"}

Task: "#;
                            if let Some(ctx) = context {
                                format!("{}{}\n\nContext:\n{}", json_instruction, task, ctx)
                            } else {
                                format!("{}{}", json_instruction, task)
                            }
                        } else if let Some(ctx) = context {
                            format!("{}\n\nContext:\n{}", task, ctx)
                        } else {
                            task.to_string()
                        };

                        println!("[EXEC] Starting claude --print...");
                        let start_time = std::time::Instant::now();

                        // Execute claude --print with stream-json for real-time progress
                        // Coordinator gets NO tools - it only outputs JSON delegation decisions
                        // Specialist workers get full tool access
                        let mut cmd = tokio::process::Command::new(&claude_path);

                        // Coordinator mode: no tools, only outputs delegation JSON
                        if role == "coordinator" {
                            println!("[MODE] Coordinator: delegation-only mode (no tools)");
                            // Explicitly deny ALL tools including MCP tools for coordinator
                            // Coordinator must ONLY output JSON delegation decisions
                            let disallowed = [
                                // Standard tools
                                "Read", "Write", "Edit", "Bash", "Glob", "Grep", "Task",
                                "WebFetch", "WebSearch", "TodoWrite", "LSP", "NotebookEdit", "NotebookRead",
                                // CCA MCP tools - prevent recursive task delegation
                                "mcp__cca__cca_task", "mcp__cca__cca_status", "mcp__cca__cca_activity",
                                "mcp__cca__cca_agents", "mcp__cca__cca_memory", "mcp__cca__cca_acp_status",
                                "mcp__cca__cca_broadcast", "mcp__cca__cca_workloads", "mcp__cca__cca_rl_status",
                                "mcp__cca__cca_rl_train", "mcp__cca__cca_rl_algorithm",
                                "mcp__cca__cca_tokens_analyze", "mcp__cca__cca_tokens_compress",
                                "mcp__cca__cca_tokens_metrics", "mcp__cca__cca_tokens_recommendations",
                                // IDE MCP tools
                                "mcp__ide__getDiagnostics", "mcp__ide__executeCode",
                            ].join(",");
                            cmd.arg("--disallowedTools").arg(&disallowed);
                        } else {
                            // SEC-007: Apply permission configuration for specialist workers
                            // Default to allowlist mode for security; use "dangerous" only in sandboxed envs
                            let permission_mode = std::env::var("CCA_PERMISSION_MODE")
                                .unwrap_or_else(|_| "allowlist".to_string());

                            match permission_mode.as_str() {
                                "dangerous" => {
                                    println!("[SEC-007] WARNING: Using dangerous permission mode - ensure sandboxing!");
                                    cmd.arg("--dangerously-skip-permissions");
                                }
                                "sandbox" => {
                                    println!("[SEC-007] Using sandbox mode with minimal permissions");
                                    cmd.arg("--allowedTools").arg("Read,Glob,Grep");
                                }
                                _ => {
                                    // Allowlist mode (default): Use configured tools or safe defaults
                                    let allowed_tools = std::env::var("CCA_ALLOWED_TOOLS")
                                        .unwrap_or_else(|_| {
                                            "Read,Glob,Grep,Write(src/**),Write(tests/**),Write(docs/**),Bash(git status),Bash(git diff*),Bash(git log*)"
                                                .to_string()
                                        });
                                    let denied_tools = std::env::var("CCA_DENIED_TOOLS")
                                        .unwrap_or_else(|_| {
                                            "Bash(rm -rf *),Bash(sudo *),Read(.env*),Write(.env*)"
                                                .to_string()
                                        });

                                    println!("[SEC-007] Using allowlist permission mode");

                                    if !allowed_tools.is_empty() {
                                        cmd.arg("--allowedTools").arg(&allowed_tools);
                                    }
                                    if !denied_tools.is_empty() {
                                        cmd.arg("--disallowedTools").arg(&denied_tools);
                                    }
                                }
                            }
                        }

                        cmd.arg("--print")
                            .arg("--output-format")
                            .arg("stream-json")
                            .arg("--verbose")
                            .arg("--")  // Ensure prompt is treated as positional arg
                            .arg(&full_message)
                            .env("CLAUDE_MD", &claude_md_path)
                            .env("NO_COLOR", "1")
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped());

                        let mut child = match cmd.spawn()
                        {
                            Ok(c) => c,
                            Err(e) => {
                                println!("[FAIL] Failed to spawn claude: {}", e);
                                let response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": {
                                        "code": -32001,
                                        "message": "Failed to execute claude",
                                        "data": e.to_string()
                                    },
                                    "id": request_id
                                });
                                let _ = write.send(Message::Text(response.to_string().into())).await;
                                continue;
                            }
                        };

                        // Stream stdout in real-time
                        let stdout = child.stdout.take().expect("stdout piped");
                        let stderr = child.stderr.take().expect("stderr piped");

                        let mut stdout_reader = tokio::io::BufReader::new(stdout).lines();
                        let mut stderr_reader = tokio::io::BufReader::new(stderr).lines();

                        let mut stderr_lines: Vec<String> = Vec::new();
                        let mut last_progress_update = std::time::Instant::now();
                        let mut current_action = String::from("Starting...");
                        let mut final_result: Option<String> = None;
                        let mut total_tokens_used: u64 = 0;

                        println!("[STREAM] Reading claude output...\n");

                        loop {
                            tokio::select! {
                                line = stdout_reader.next_line() => {
                                    match line {
                                        Ok(Some(line)) => {
                                            // Parse stream-json event
                                            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                                                let elapsed = start_time.elapsed().as_secs();

                                                match event.get("type").and_then(|t| t.as_str()) {
                                                    Some("system") => {
                                                        if event.get("subtype").and_then(|s| s.as_str()) == Some("init") {
                                                            println!("  [{:>3}s] 🚀 Session initialized", elapsed);
                                                        }
                                                    }
                                                    Some("user") => {
                                                        println!("  [{:>3}s] 📝 Processing task...", elapsed);
                                                        current_action = "Processing task".to_string();
                                                    }
                                                    Some("assistant") => {
                                                        // Check for tool use in the message
                                                        if let Some(msg) = event.get("message") {
                                                            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                                                                for item in content {
                                                                    if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                                                        let tool_name = item.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                                                                        let action = format_tool_action(tool_name, item.get("input"));
                                                                        println!("  [{:>3}s] {}", elapsed, action);
                                                                        current_action = action;
                                                                    } else if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                                                        // Text output - show a preview
                                                                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                                                            if text.len() > 20 {
                                                                                let preview = text.lines().next().unwrap_or("");
                                                                                if !preview.is_empty() && preview.len() > 5 {
                                                                                    let display = truncate_line(preview, 60);
                                                                                    println!("  [{:>3}s] 💬 {}", elapsed, display);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    Some("result") => {
                                                        let success = !event.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);
                                                        if success {
                                                            final_result = event.get("result").and_then(|r| r.as_str()).map(|s| s.to_string());
                                                            let duration = event.get("duration_ms").and_then(|d| d.as_u64()).unwrap_or(0);

                                                            // Extract token usage from result event
                                                            if let Some(usage) = event.get("usage") {
                                                                let input = usage.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
                                                                let output = usage.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
                                                                total_tokens_used = input + output;
                                                                println!("  [{:>3}s] ✅ Task completed ({}ms, {} tokens)", elapsed, duration, total_tokens_used);
                                                            } else {
                                                                // Try alternate location for tokens
                                                                let input = event.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
                                                                let output = event.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
                                                                if input > 0 || output > 0 {
                                                                    total_tokens_used = input + output;
                                                                    println!("  [{:>3}s] ✅ Task completed ({}ms, {} tokens)", elapsed, duration, total_tokens_used);
                                                                } else {
                                                                    println!("  [{:>3}s] ✅ Task completed ({}ms)", elapsed, duration);
                                                                }
                                                            }
                                                        } else {
                                                            println!("  [{:>3}s] ❌ Task failed", elapsed);
                                                        }
                                                    }
                                                    // Handle usage event (Claude may send this separately)
                                                    Some("usage") => {
                                                        let input = event.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
                                                        let output = event.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
                                                        total_tokens_used = input + output;
                                                        println!("  [{:>3}s] 📊 Token usage: {} input + {} output = {} total",
                                                                 elapsed, input, output, total_tokens_used);
                                                    }
                                                    _ => {}
                                                }

                                                // Send progress update via WebSocket every 2 seconds
                                                if last_progress_update.elapsed().as_secs() >= 2 {
                                                    let progress_msg = serde_json::json!({
                                                        "jsonrpc": "2.0",
                                                        "method": "task.progress",
                                                        "params": {
                                                            "request_id": request_id,
                                                            "action": current_action,
                                                            "elapsed_secs": elapsed,
                                                            "status": "running"
                                                        }
                                                    });
                                                    let _ = write.send(Message::Text(progress_msg.to_string().into())).await;
                                                    last_progress_update = std::time::Instant::now();
                                                }
                                            }
                                        }
                                        Ok(None) => break,
                                        Err(e) => {
                                            eprintln!("\n[ERROR] Reading stdout: {}", e);
                                            break;
                                        }
                                    }
                                }
                                line = stderr_reader.next_line() => {
                                    match line {
                                        Ok(Some(line)) => {
                                            stderr_lines.push(line);
                                        }
                                        Ok(None) => {}
                                        Err(_) => {}
                                    }
                                }
                            }
                        }

                        println!(); // New line after progress

                        // Wait for process to complete
                        let status = child.wait().await;
                        let elapsed = start_time.elapsed();
                        println!("[EXEC] Claude completed in {:.1}s", elapsed.as_secs_f64());

                        // Build result from stream-json final_result
                        let stderr_output = stderr_lines.join("\n");
                        let output = final_result.unwrap_or_default();

                        let response = match status {
                            Ok(s) if s.success() => {
                                println!("[DONE] Success! Output: {} bytes, Tokens: {}", output.len(), total_tokens_used);
                                println!("[SEND] Sending response for request {}", request_id);
                                // Print first 200 chars of output for debugging
                                let preview = truncate_line(&output, 200);
                                println!("[PREVIEW] {}", preview.replace('\n', " "));
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "result": {
                                        "success": true,
                                        "output": output,
                                        "tokens_used": total_tokens_used
                                    },
                                    "id": request_id
                                })
                            }
                            Ok(s) => {
                                println!("[FAIL] Task failed (exit code: {:?})", s.code());
                                println!("[STDERR] {}", stderr_output);
                                if !output.is_empty() {
                                    println!("[STDOUT] {}", output);
                                }
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": {
                                        "code": -32000,
                                        "message": "Task execution failed",
                                        "data": stderr_output
                                    },
                                    "id": request_id
                                })
                            }
                            Err(e) => {
                                println!("[FAIL] Failed to wait for claude: {}", e);
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": {
                                        "code": -32001,
                                        "message": "Failed to execute claude",
                                        "data": e.to_string()
                                    },
                                    "id": request_id
                                })
                            }
                        };

                        println!("[SEND] Sending response via WebSocket...");
                        if let Err(e) = write.send(Message::Text(response.to_string().into())).await
                        {
                            eprintln!("[ERROR] Failed to send response: {}", e);
                        } else {
                            println!("[SEND] Response sent successfully");
                        }
                        println!("{}", "=".repeat(60));
                    } else if json.get("method").and_then(|m| m.as_str()) == Some("heartbeat") {
                        // Respond to heartbeat
                        let request_id = json.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "result": {
                                "status": "ok",
                                "agent_id": agent_id.to_string()
                            },
                            "id": request_id
                        });
                        let _ = write.send(Message::Text(response.to_string().into())).await;
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = write.send(Message::Pong(data)).await;
            }
            Ok(Message::Close(_)) => {
                println!("Server closed connection");
                break;
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    println!("Worker stopped.");
    Ok(())
}
