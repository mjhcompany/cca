//! MCP Tool implementations

use anyhow::{anyhow, Result};
use tracing::info;

use crate::client::{CreateTaskRequest, DaemonClient};
use crate::types::*;

/// Registry of available MCP tools
pub struct ToolRegistry {
    tools: Vec<McpTool>,
}

impl ToolRegistry {
    /// Create a new tool registry with default CCA tools
    pub fn new() -> Self {
        let tools = vec![
            McpTool {
                name: "cca_task".to_string(),
                description: "Send a task to the CCA system. The Coordinator will analyze the task and route it to appropriate agents.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "description": {
                            "type": "string",
                            "description": "The task description - what needs to be done"
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["low", "normal", "high", "critical"],
                            "description": "Task priority level (default: normal)"
                        }
                    },
                    "required": ["description"]
                }),
            },
            McpTool {
                name: "cca_status".to_string(),
                description: "Check the status of a running task or get overall system status.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "Optional task ID to check specific task status"
                        }
                    }
                }),
            },
            McpTool {
                name: "cca_activity".to_string(),
                description: "Get current activity of all agents - what each agent is working on.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "cca_agents".to_string(),
                description: "List all running agents and their status.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "cca_memory".to_string(),
                description: "Query the ReasoningBank for learned patterns relevant to the current task.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query for patterns"
                        },
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of results (default: 10)"
                        }
                    },
                    "required": ["query"]
                }),
            },
        ];

        Self { tools }
    }

    /// List all available tools
    pub fn list(&self) -> &[McpTool] {
        &self.tools
    }

    /// Call a tool by name
    pub async fn call(
        &self,
        name: &str,
        arguments: &serde_json::Value,
        daemon_url: &str,
    ) -> Result<String> {
        let client = DaemonClient::new(daemon_url);

        match name {
            "cca_task" => self.call_task(arguments, &client).await,
            "cca_status" => self.call_status(arguments, &client).await,
            "cca_activity" => self.call_activity(&client).await,
            "cca_agents" => self.call_agents(&client).await,
            "cca_memory" => self.call_memory(arguments, &client).await,
            _ => Err(anyhow!("Unknown tool: {}", name)),
        }
    }

    async fn call_task(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let description = arguments["description"]
            .as_str()
            .ok_or_else(|| anyhow!("description is required"))?;

        let priority = arguments["priority"].as_str().map(String::from);

        info!("Sending task to coordinator: {}", description);

        // Check daemon health first
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        let request = CreateTaskRequest {
            description: description.to_string(),
            priority,
            metadata: None,
        };

        match client.create_task(&request).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to create task: {}", e)
            }))?),
        }
    }

    async fn call_status(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let task_id = arguments["task_id"].as_str();

        // Check daemon health first
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        if let Some(task_id) = task_id {
            // Get specific task status
            match client.get_task(task_id).await {
                Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
                Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "error": format!("Failed to get task: {}", e)
                }))?),
            }
        } else {
            // Get overall status
            match client.status().await {
                Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
                Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "error": format!("Failed to get status: {}", e)
                }))?),
            }
        }
    }

    async fn call_activity(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        match client.get_activity().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to get activity: {}", e)
            }))?),
        }
    }

    async fn call_agents(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        match client.list_agents().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
            "error": format!("Failed to list agents: {}", e)
            }))?),
        }
    }

    async fn call_memory(
        &self,
        arguments: &serde_json::Value,
        _client: &DaemonClient,
    ) -> Result<String> {
        let query = arguments["query"]
            .as_str()
            .ok_or_else(|| anyhow!("query is required"))?;

        let limit = arguments["limit"].as_u64().unwrap_or(10) as usize;

        info!("Memory query: {} (limit: {})", query, limit);

        // Memory queries will be implemented when we add PostgreSQL/pgvector
        // For now, return empty results
        let response = MemoryResult { patterns: vec![] };
        Ok(serde_json::to_string_pretty(&response)?)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
