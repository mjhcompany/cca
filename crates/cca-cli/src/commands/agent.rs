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
        /// Agent ID
        id: String,
    },
    /// List all agents
    List,
    /// Attach to agent PTY for manual intervention
    Attach {
        /// Agent ID or role name
        id: String,
    },
    /// Send a message to an agent
    Send {
        /// Agent ID
        id: String,

        /// Message to send
        message: String,
    },
    /// View agent logs
    Logs {
        /// Agent ID
        id: String,

        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },
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
    }
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

    let client = Client::new();

    println!("Stopping agent {id}...");

    let resp = client
        .delete(format!("{DAEMON_URL}/api/v1/agents/{id}"))
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
            println!("{:<36} {:<12} {:<10} CURRENT TASK", "ID", "ROLE", "STATE");
            println!("{}", "-".repeat(80));
            for agent in agents {
                let task = agent["current_task"].as_str().map_or_else(
                    || "-".to_string(),
                    |s| {
                        if s.len() > 20 {
                            format!("{}...", &s[..17])
                        } else {
                            s.to_string()
                        }
                    },
                );

                println!(
                    "{:<36} {:<12} {:<10} {}",
                    agent["agent_id"].as_str().unwrap_or("-"),
                    agent["role"].as_str().unwrap_or("-"),
                    agent["status"].as_str().unwrap_or("-"),
                    task
                );
            }
        }
    }

    Ok(())
}

async fn attach(id: &str) -> Result<()> {
    check_daemon().await?;

    println!("Attaching to agent {id}...");
    println!("(Press Ctrl+D to detach)\n");

    // First verify the agent exists
    let resp = reqwest::get(format!("{DAEMON_URL}/api/v1/agents"))
        .await?;

    let data: serde_json::Value = resp.json().await?;
    let agents = data["agents"].as_array();

    let agent = agents
        .and_then(|arr| {
            arr.iter().find(|a| {
                a["agent_id"].as_str() == Some(id) || a["role"].as_str().map(str::to_lowercase) == Some(id.to_lowercase())
            })
        });

    let agent_id = if let Some(a) = agent { a["agent_id"].as_str().unwrap_or(id).to_string() } else {
        println!("Agent '{id}' not found");
        return Ok(());
    };

    println!("Connected to agent: {agent_id}");
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

    let client = Client::new();

    println!("Sending message to agent {id}...");

    let body = serde_json::json!({
        "message": message
    });

    let resp = client
        .post(format!("{DAEMON_URL}/api/v1/agents/{id}/send"))
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

    println!("Viewing logs for agent {id} (last {lines} lines)...\n");

    let resp = reqwest::get(format!(
        "{DAEMON_URL}/api/v1/agents/{id}/logs?lines={lines}"
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
            println!("Agent '{id}' not found or logs not available");
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
