//! Agent management commands

use anyhow::{Context, Result};
use clap::Subcommand;
use reqwest::Client;
use std::io::{self, Write};

const DAEMON_URL: &str = "http://127.0.0.1:9200";

#[derive(Subcommand)]
pub enum AgentCommands {
    /// Spawn a new agent
    Spawn {
        /// Agent role (coordinator, frontend, backend, dba, devops, security, qa)
        role: String,

        /// Custom name for the agent
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Stop an agent
    Stop {
        /// Agent ID (short or full) or role name
        id: String,
    },
    /// List all agents
    List,
    /// Attach to agent PTY for manual intervention
    Attach {
        /// Agent ID (short or full) or role name
        id: String,
    },
    /// Send a message to an agent
    Send {
        /// Agent ID (short or full) or role name
        id: String,

        /// Message to send
        message: String,
    },
    /// View agent logs
    Logs {
        /// Agent ID (short or full) or role name
        id: String,

        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },
    /// Run diagnostics to check system health
    Diag,
}

async fn check_daemon() -> Result<()> {
    let resp = reqwest::get(format!("{DAEMON_URL}/health"))
        .await
        .context("Could not connect to CCA daemon. Is it running?")?;

    if !resp.status().is_success() {
        anyhow::bail!("CCA daemon is not healthy. Status: {}", resp.status());
    }
    Ok(())
}

pub async fn run(cmd: AgentCommands) -> Result<()> {
    match cmd {
        AgentCommands::Spawn { role, name } => spawn(&role, name).await,
        AgentCommands::Stop { id } => stop(&id).await,
        AgentCommands::List => list().await,
        AgentCommands::Attach { id } => attach(&id).await,
        AgentCommands::Send { id, message } => send(&id, &message).await,
        AgentCommands::Logs { id, lines } => logs(&id, lines).await,
        AgentCommands::Diag => diag().await,
    }
}

/// Resolve agent identifier (short ID, full ID, or role name) to full agent ID
async fn resolve_agent_id(id_or_role: &str) -> Result<String> {
    let resp = reqwest::get(format!("{DAEMON_URL}/api/v1/agents"))
        .await
        .context("Failed to fetch agents")?;

    let data: serde_json::Value = resp.json().await?;
    let agents = data["agents"].as_array();

    if let Some(agents) = agents {
        for agent in agents {
            let agent_id = agent["agent_id"].as_str().unwrap_or("");
            let role = agent["role"].as_str().unwrap_or("");

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

    Err(anyhow::anyhow!("Agent '{}' not found. Use 'cca agent list' to see available agents.", id_or_role))
}

async fn spawn(role: &str, name: Option<String>) -> Result<()> {
    check_daemon().await?;

    let client = Client::new();

    let mut body = serde_json::json!({
        "role": role
    });

    if let Some(n) = &name {
        body["name"] = serde_json::json!(n);
    }

    println!("Spawning {} agent{}...", role, name.as_ref().map(|n| format!(" ({n})")).unwrap_or_default());

    let resp = client
        .post(format!("{DAEMON_URL}/api/v1/agents"))
        .json(&body)
        .send()
        .await
        .context("Failed to send spawn request")?;

    if resp.status().is_success() {
        let data: serde_json::Value = resp.json().await?;
        println!("Agent spawned successfully");
        println!("ID: {}", data["agent_id"].as_str().unwrap_or("unknown"));
        if let Some(state) = data["state"].as_str() {
            println!("State: {state}");
        }
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        println!("Failed to spawn agent: {status} - {body}");
    }

    Ok(())
}

async fn stop(id: &str) -> Result<()> {
    check_daemon().await?;

    let agent_id = resolve_agent_id(id).await?;
    let client = Client::new();

    println!("Stopping agent {}...", &agent_id[..8.min(agent_id.len())]);

    let resp = client
        .delete(format!("{DAEMON_URL}/api/v1/agents/{agent_id}"))
        .send()
        .await
        .context("Failed to send stop request")?;

    if resp.status().is_success() {
        println!("Agent stopped successfully");
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        println!("Failed to stop agent: {status} - {body}");
    }

    Ok(())
}

async fn list() -> Result<()> {
    check_daemon().await?;

    let resp = reqwest::get(format!("{DAEMON_URL}/api/v1/agents"))
        .await
        .context("Failed to fetch agents")?;

    if !resp.status().is_success() {
        println!("Failed to fetch agents: {}", resp.status());
        return Ok(());
    }

    let data: serde_json::Value = resp.json().await?;
    let agents = data["agents"].as_array();

    println!();
    if let Some(agents) = agents {
        if agents.is_empty() {
            println!("No agents running");
        } else {
            println!("{:<10} {:<12} {:<10} CURRENT TASK", "ID", "ROLE", "STATE");
            println!("{}", "-".repeat(60));
            for agent in agents {
                let task = agent["current_task"].as_str().map_or_else(
                    || "-".to_string(),
                    |s| {
                        if s.len() > 25 {
                            format!("{}...", &s[..22])
                        } else {
                            s.to_string()
                        }
                    },
                );

                // Show short ID (first 8 chars) for readability
                let full_id = agent["agent_id"].as_str().unwrap_or("-");
                let short_id = if full_id.len() > 8 { &full_id[..8] } else { full_id };

                println!(
                    "{:<10} {:<12} {:<10} {}",
                    short_id,
                    agent["role"].as_str().unwrap_or("-"),
                    agent["status"].as_str().unwrap_or("-"),
                    task
                );
            }
            println!();
            println!("Tip: Use role name (e.g., 'coordinator') or short ID to interact with agents");
        }
    }

    Ok(())
}

async fn attach(id: &str) -> Result<()> {
    check_daemon().await?;

    let agent_id = resolve_agent_id(id).await?;
    let short_id = &agent_id[..8.min(agent_id.len())];

    println!("Attaching to agent {short_id}...");
    println!("(Press Ctrl+D to detach)\n");
    println!("Connected to agent: {short_id}");
    println!("Type messages and press Enter to send.\n");

    // Simple interactive loop
    let client = Client::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut input = String::new();
        match stdin.read_line(&mut input) {
            Ok(0) => {
                // EOF (Ctrl+D)
                println!("\nDetaching...");
                break;
            }
            Ok(_) => {
                let message = input.trim();
                if message.is_empty() {
                    continue;
                }

                // Send message to agent
                let body = serde_json::json!({
                    "message": message
                });

                match client
                    .post(format!("{DAEMON_URL}/api/v1/agents/{agent_id}/send"))
                    .json(&body)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        let data: serde_json::Value = resp.json().await?;
                        if let Some(output) = data["output"].as_str() {
                            println!("{output}");
                        }
                    }
                    Ok(resp) => {
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error: {body}");
                    }
                    Err(e) => {
                        println!("Failed to send: {e}");
                    }
                }
            }
            Err(e) => {
                println!("Error reading input: {e}");
                break;
            }
        }
    }

    Ok(())
}

async fn send(id: &str, message: &str) -> Result<()> {
    check_daemon().await?;

    let agent_id = resolve_agent_id(id).await?;
    let short_id = &agent_id[..8.min(agent_id.len())];
    let client = Client::new();

    println!("Sending message to agent {short_id}...");

    let body = serde_json::json!({
        "message": message
    });

    let resp = client
        .post(format!("{DAEMON_URL}/api/v1/agents/{agent_id}/send"))
        .json(&body)
        .send()
        .await
        .context("Failed to send message")?;

    if resp.status().is_success() {
        let data: serde_json::Value = resp.json().await?;
        println!("\nResponse:");
        if let Some(output) = data["output"].as_str() {
            println!("{output}");
        } else {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        println!("Failed to send message: {status} - {body}");
    }

    Ok(())
}

async fn logs(id: &str, lines: usize) -> Result<()> {
    check_daemon().await?;

    let agent_id = resolve_agent_id(id).await?;
    let short_id = &agent_id[..8.min(agent_id.len())];

    println!("Viewing logs for agent {short_id} (last {lines} lines)...\n");

    let resp = reqwest::get(format!(
        "{DAEMON_URL}/api/v1/agents/{agent_id}/logs?lines={lines}"
    ))
    .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await?;
            if let Some(logs) = data["logs"].as_array() {
                for log in logs {
                    if let Some(line) = log.as_str() {
                        println!("{line}");
                    }
                }
            }
        }
        Ok(r) if r.status() == 404 => {
            println!("Logs not available for agent {short_id}");
        }
        Ok(r) => {
            println!("Failed to fetch logs: {}", r.status());
        }
        Err(e) => {
            println!("Error: {e}");
        }
    }

    Ok(())
}

/// Run system diagnostics
async fn diag() -> Result<()> {
    println!("CCA System Diagnostics");
    println!("{}\n", "=".repeat(50));

    // 1. Check daemon health
    print!("Daemon health........... ");
    let health_resp = reqwest::get(format!("{DAEMON_URL}/health")).await;
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
    let acp_resp = reqwest::get(format!("{DAEMON_URL}/api/v1/acp/status")).await;
    match acp_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let running = data["running"].as_bool().unwrap_or(false);
            let port = data["port"].as_u64().unwrap_or(0);
            let agents = data["connected_agents"].as_u64().unwrap_or(0);
            if running {
                println!("[OK] port={}, connected_agents={}", port, agents);
            } else {
                println!("[WARN] not running");
            }
        }
        Ok(r) => println!("[FAIL] HTTP {}", r.status()),
        Err(e) => println!("[FAIL] {}", e),
    }

    // 3. Check Redis
    print!("Redis................... ");
    let redis_resp = reqwest::get(format!("{DAEMON_URL}/api/v1/redis/status")).await;
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
    let pg_resp = reqwest::get(format!("{DAEMON_URL}/api/v1/postgres/status")).await;
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
    let rl_resp = reqwest::get(format!("{DAEMON_URL}/api/v1/rl/stats")).await;
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

    // 6. List agents
    println!("\nAgents:");
    let agents_resp = reqwest::get(format!("{DAEMON_URL}/api/v1/agents")).await;
    match agents_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            if let Some(agents) = data["agents"].as_array() {
                if agents.is_empty() {
                    println!("  (none)");
                } else {
                    for agent in agents {
                        let id = agent["agent_id"].as_str().unwrap_or("-");
                        let short_id = if id.len() > 8 { &id[..8] } else { id };
                        let role = agent["role"].as_str().unwrap_or("-");
                        let status = agent["status"].as_str().unwrap_or("-");
                        println!("  {} ({}) - {}", short_id, role, status);
                    }
                }
            }
        }
        _ => println!("  (failed to fetch)"),
    }

    // 7. List recent tasks
    println!("\nRecent Tasks:");
    let tasks_resp = reqwest::get(format!("{DAEMON_URL}/api/v1/tasks?limit=5")).await;
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
    let workload_resp = reqwest::get(format!("{DAEMON_URL}/api/v1/workloads")).await;
    match workload_resp {
        Ok(r) if r.status().is_success() => {
            let data: serde_json::Value = r.json().await.unwrap_or_default();
            let total = data["total_tasks"].as_u64().unwrap_or(0);
            let pending = data["pending_tasks"].as_u64().unwrap_or(0);
            println!("  Total tasks: {}, Pending: {}", total, pending);
            if let Some(agents) = data["agents"].as_array() {
                for agent in agents {
                    let id = agent["agent_id"].as_str().unwrap_or("-");
                    let short_id = if id.len() > 8 { &id[..8] } else { id };
                    let role = agent["role"].as_str().unwrap_or("-");
                    let current = agent["current_tasks"].as_u64().unwrap_or(0);
                    let max = agent["max_tasks"].as_u64().unwrap_or(0);
                    println!("  {} ({}): {}/{} tasks", short_id, role, current, max);
                }
            }
        }
        _ => println!("  (failed to fetch)"),
    }

    println!("\n{}", "=".repeat(50));
    println!("Diagnostics complete.");

    Ok(())
}
