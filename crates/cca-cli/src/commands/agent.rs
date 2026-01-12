//! Agent management commands

use anyhow::{Context, Result};
use clap::Subcommand;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

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
    let resp = reqwest::get(format!("{}/api/v1/health", daemon_url()))
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
    let resp = reqwest::get(format!("{}/api/v1/acp/status", daemon_url()))
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

    let resp = reqwest::get(format!("{}/api/v1/acp/status", daemon_url())).await;

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
                        let short_id = if id.len() > 8 { &id[..8] } else { id };
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
    let short_id = &agent_id[..8.min(agent_id.len())];
    let client = Client::new();

    println!("Disconnecting worker {}...", short_id);

    // Send disconnect request to daemon
    let resp = client
        .post(format!("{}/api/v1/acp/disconnect", daemon_url()))
        .json(&serde_json::json!({ "agent_id": agent_id }))
        .send()
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
    let short_id = &agent_id[..8.min(agent_id.len())];
    let client = Client::new();

    println!("Sending task to worker {}...", short_id);

    // Send task via daemon API
    let resp = client
        .post(format!("{}/api/v1/acp/send", daemon_url()))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "task": message
        }))
        .send()
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
    let health_resp = reqwest::get(format!("{}/api/v1/health", daemon_url())).await;
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
    let acp_resp = reqwest::get(format!("{}/api/v1/acp/status", daemon_url())).await;
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
    let redis_resp = reqwest::get(format!("{}/api/v1/redis/status", daemon_url())).await;
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
    let pg_resp = reqwest::get(format!("{}/api/v1/postgres/status", daemon_url())).await;
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
    let rl_resp = reqwest::get(format!("{}/api/v1/rl/stats", daemon_url())).await;
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
    let workers_resp = reqwest::get(format!("{}/api/v1/acp/status", daemon_url())).await;
    match workers_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(workers) = data["workers"].as_array() {
                if workers.is_empty() {
                    println!("  (none - start workers with: cca agent worker <role>)");
                } else {
                    for worker in workers {
                        let id = worker["agent_id"].as_str().unwrap_or("-");
                        let short_id = if id.len() > 8 { &id[..8] } else { id };
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
    let tasks_resp = reqwest::get(format!("{}/api/v1/tasks?limit=5", daemon_url())).await;
    match tasks_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(tasks) = data["tasks"].as_array() {
                if tasks.is_empty() {
                    println!("  (none)");
                } else {
                    for task in tasks {
                        let id = task["id"].as_str().unwrap_or("-");
                        let short_id = if id.len() > 8 { &id[..8] } else { id };
                        let status = task["status"].as_str().unwrap_or("-");
                        let desc = task["description"].as_str().unwrap_or("-");
                        let short_desc = if desc.len() > 40 {
                            format!("{}...", &desc[..37])
                        } else {
                            desc.to_string()
                        };
                        println!("  {} [{}] {}", short_id, status, short_desc);
                    }
                }
            }
        }
        _ => println!("  (failed to fetch)"),
    }

    // 8. Check workloads
    println!("\nWorkloads:");
    let workload_resp = reqwest::get(format!("{}/api/v1/workloads", daemon_url())).await;
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
                        let full_message = if let Some(ctx) = context {
                            format!("{}\n\nContext:\n{}", task, ctx)
                        } else {
                            task.to_string()
                        };

                        println!("[EXEC] Starting claude --print...");
                        let start_time = std::time::Instant::now();

                        // Execute claude --print
                        let output = tokio::process::Command::new(&claude_path)
                            .arg("--dangerously-skip-permissions")
                            .arg("--print")
                            .arg("--output-format")
                            .arg("text")
                            .arg(&full_message)
                            .env("CLAUDE_MD", &claude_md_path)
                            .env("NO_COLOR", "1")
                            .output()
                            .await;

                        let elapsed = start_time.elapsed();
                        println!("[EXEC] Claude completed in {:.1}s", elapsed.as_secs_f64());

                        // Send result back
                        let response = match output {
                            Ok(out) if out.status.success() => {
                                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                                println!("[DONE] Success! Output: {} bytes", stdout.len());
                                println!("[SEND] Sending response for request {}", request_id);
                                // Print first 200 chars of output for debugging
                                let preview = if stdout.len() > 200 {
                                    format!("{}...", &stdout[..200])
                                } else {
                                    stdout.clone()
                                };
                                println!("[PREVIEW] {}", preview.replace('\n', " "));
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "result": {
                                        "success": true,
                                        "output": stdout
                                    },
                                    "id": request_id
                                })
                            }
                            Ok(out) => {
                                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                                println!("[FAIL] Task failed (exit code: {:?})", out.status.code());
                                println!("[STDERR] {}", stderr);
                                if !stdout.is_empty() {
                                    println!("[STDOUT] {}", stdout);
                                }
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": {
                                        "code": -32000,
                                        "message": "Task execution failed",
                                        "data": stderr
                                    },
                                    "id": request_id
                                })
                            }
                            Err(e) => {
                                println!("[FAIL] Failed to execute claude: {}", e);
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
