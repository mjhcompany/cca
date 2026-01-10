//! Task management commands

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    Create {
        /// Task description
        description: String,

        /// Target agent (optional, coordinator will route if not specified)
        #[arg(short, long)]
        agent: Option<String>,
    },
    /// Check task status
    Status {
        /// Task ID
        id: String,
    },
    /// List recent tasks
    List {
        /// Number of tasks to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Cancel a task
    Cancel {
        /// Task ID
        id: String,
    },
}

pub async fn run(cmd: TaskCommands) -> Result<()> {
    match cmd {
        TaskCommands::Create { description, agent } => create(&description, agent).await,
        TaskCommands::Status { id } => status(&id).await,
        TaskCommands::List { limit } => list(limit).await,
        TaskCommands::Cancel { id } => cancel(&id).await,
    }
}

async fn create(description: &str, agent: Option<String>) -> Result<()> {
    println!("Creating task: {}", description);
    if let Some(a) = agent {
        println!("Target agent: {}", a);
    } else {
        println!("Coordinator will route this task");
    }
    // TODO: Call daemon API
    println!("Task created: <task-id>");
    Ok(())
}

async fn status(id: &str) -> Result<()> {
    println!("Task: {}", id);
    println!("Status: pending");
    // TODO: Call daemon API
    Ok(())
}

async fn list(limit: usize) -> Result<()> {
    println!("Recent tasks (last {}):\n", limit);
    println!("{:<36} {:<12} {:<20}", "ID", "STATUS", "DESCRIPTION");
    println!("{}", "-".repeat(70));
    // TODO: Call daemon API and list tasks
    println!("No tasks found");
    Ok(())
}

async fn cancel(id: &str) -> Result<()> {
    println!("Cancelling task {}...", id);
    // TODO: Call daemon API
    println!("Task cancelled");
    Ok(())
}
