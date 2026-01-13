//! MCP Tool implementations

use anyhow::{anyhow, Result};
use tracing::info;

use crate::client::{CreateTaskRequest, DaemonClient};
use crate::types::{McpTool, PatternMatch, MemoryResult};

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
            McpTool {
                name: "cca_acp_status".to_string(),
                description: "Get ACP WebSocket server status - shows connected agents and real-time communication status.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "cca_broadcast".to_string(),
                description: "Broadcast a message to all connected agents.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to broadcast to all agents"
                        }
                    },
                    "required": ["message"]
                }),
            },
            McpTool {
                name: "cca_workloads".to_string(),
                description: "Get current workload distribution across all agents.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "cca_rl_status".to_string(),
                description: "Get RL (Reinforcement Learning) engine status - shows algorithm, training stats, and experience buffer.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "cca_rl_train".to_string(),
                description: "Trigger RL training on collected experiences.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "cca_rl_algorithm".to_string(),
                description: "Set the RL algorithm to use. Available: q_learning, dqn, ppo.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "algorithm": {
                            "type": "string",
                            "description": "The RL algorithm to use (q_learning, dqn, ppo)"
                        }
                    },
                    "required": ["algorithm"]
                }),
            },
            // Token efficiency tools
            McpTool {
                name: "cca_tokens_analyze".to_string(),
                description: "Analyze content for token usage - counts tokens, detects redundancy, and estimates compression potential.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The content to analyze for token usage"
                        },
                        "agent_id": {
                            "type": "string",
                            "description": "Optional agent ID to associate with analysis"
                        }
                    },
                    "required": ["content"]
                }),
            },
            McpTool {
                name: "cca_tokens_compress".to_string(),
                description: "Compress content using various strategies (code_comments, history, summarize, deduplicate). Targets 30%+ token reduction.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The content to compress"
                        },
                        "strategies": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Compression strategies: code_comments, history, summarize, deduplicate (default: all)"
                        },
                        "target_reduction": {
                            "type": "number",
                            "description": "Target reduction as decimal 0.0-1.0 (default: 0.3 for 30%)"
                        },
                        "agent_id": {
                            "type": "string",
                            "description": "Optional agent ID to track savings"
                        }
                    },
                    "required": ["content"]
                }),
            },
            McpTool {
                name: "cca_tokens_metrics".to_string(),
                description: "Get token efficiency metrics - total usage, savings, and per-agent breakdown.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "cca_tokens_recommendations".to_string(),
                description: "Get recommendations for improving token efficiency based on current usage patterns.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            // Codebase indexing tools
            McpTool {
                name: "cca_index_codebase".to_string(),
                description: "Index a codebase for semantic code search. Extracts functions, classes, and methods, generates embeddings for similarity search.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the directory to index"
                        },
                        "extensions": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "File extensions to include (default: common code files)"
                        },
                        "exclude_patterns": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Glob patterns to exclude (e.g., '**/node_modules/**')"
                        }
                    },
                    "required": ["path"]
                }),
            },
            McpTool {
                name: "cca_search_code".to_string(),
                description: "Search indexed code using semantic similarity. Finds functions, classes, and methods matching your query.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Natural language search query"
                        },
                        "limit": {
                            "type": "number",
                            "description": "Maximum results (default: 10)"
                        },
                        "language": {
                            "type": "string",
                            "description": "Filter by programming language (e.g., 'rust', 'python')"
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
            "cca_acp_status" => self.call_acp_status(&client).await,
            "cca_broadcast" => self.call_broadcast(arguments, &client).await,
            "cca_workloads" => self.call_workloads(&client).await,
            "cca_rl_status" => self.call_rl_status(&client).await,
            "cca_rl_train" => self.call_rl_train(&client).await,
            "cca_rl_algorithm" => self.call_rl_algorithm(arguments, &client).await,
            "cca_tokens_analyze" => self.call_tokens_analyze(arguments, &client).await,
            "cca_tokens_compress" => self.call_tokens_compress(arguments, &client).await,
            "cca_tokens_metrics" => self.call_tokens_metrics(&client).await,
            "cca_tokens_recommendations" => self.call_tokens_recommendations(&client).await,
            "cca_index_codebase" => self.call_index_codebase(arguments, &client).await,
            "cca_search_code" => self.call_search_code(arguments, &client).await,
            _ => Err(anyhow!("Unknown tool: {name}")),
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
            // Get overall status, including connected workers from ACP
            match client.status().await {
                Ok(mut response) => {
                    // Add connected workers count from ACP
                    if let Ok(acp) = client.get_acp_status().await {
                        response.agents_count = acp.connected_agents;
                    }
                    Ok(serde_json::to_string_pretty(&response)?)
                }
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

        // Get ACP-connected workers (WebSocket connections)
        match client.get_acp_status().await {
            Ok(acp_status) => {
                // Transform ACP status into agent list format
                let agents: Vec<serde_json::Value> = acp_status.workers
                    .iter()
                    .map(|w| serde_json::json!({
                        "agent_id": w.agent_id,
                        "role": w.role,
                        "status": "connected"
                    }))
                    .collect();

                Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "agents": agents,
                    "connected_count": acp_status.connected_agents
                }))?)
            }
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to list agents: {}", e)
            }))?),
        }
    }

    async fn call_memory(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let query = arguments["query"]
            .as_str()
            .ok_or_else(|| anyhow!("query is required"))?;

        let limit = arguments["limit"].as_i64().unwrap_or(10) as i32;

        info!("Memory query: {} (limit: {})", query, limit);

        // Check daemon health first
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        // Query the ReasoningBank via the daemon
        match client.search_memory(query, limit).await {
            Ok(response) => {
                // Convert to MemoryResult format for compatibility
                let patterns: Vec<PatternMatch> = response
                    .patterns
                    .iter()
                    .map(|p| PatternMatch {
                        id: p.id.clone(),
                        pattern_type: p.pattern_type.clone(),
                        content: p.content.clone(),
                        score: 1.0, // Text search doesn't have similarity score
                        success_rate: p.success_rate.unwrap_or(0.0),
                    })
                    .collect();

                let result = MemoryResult { patterns };
                Ok(serde_json::to_string_pretty(&result)?)
            }
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to search memory: {}", e)
            }))?),
        }
    }

    async fn call_acp_status(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        match client.get_acp_status().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to get ACP status: {}", e)
            }))?),
        }
    }

    async fn call_broadcast(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let message = arguments["message"]
            .as_str()
            .ok_or_else(|| anyhow!("message is required"))?;

        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        info!("Broadcasting message: {}", message);

        match client.broadcast(message).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to broadcast: {}", e)
            }))?),
        }
    }

    async fn call_workloads(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        match client.get_workloads().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to get workloads: {}", e)
            }))?),
        }
    }

    async fn call_rl_status(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        match client.get_rl_stats().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to get RL stats: {}", e)
            }))?),
        }
    }

    async fn call_rl_train(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        info!("Triggering RL training");

        match client.rl_train().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to train: {}", e)
            }))?),
        }
    }

    async fn call_rl_algorithm(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let algorithm = arguments["algorithm"]
            .as_str()
            .ok_or_else(|| anyhow!("algorithm is required"))?;

        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        info!("Setting RL algorithm to: {}", algorithm);

        match client.set_rl_algorithm(algorithm).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to set algorithm: {}", e)
            }))?),
        }
    }

    async fn call_tokens_analyze(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let content = arguments["content"]
            .as_str()
            .ok_or_else(|| anyhow!("content is required"))?;

        let agent_id = arguments["agent_id"].as_str();

        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        info!("Analyzing context for token usage");

        match client.tokens_analyze(content, agent_id).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to analyze tokens: {}", e)
            }))?),
        }
    }

    async fn call_tokens_compress(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let content = arguments["content"]
            .as_str()
            .ok_or_else(|| anyhow!("content is required"))?;

        let strategies: Option<Vec<String>> = arguments["strategies"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let target_reduction = arguments["target_reduction"].as_f64();
        let agent_id = arguments["agent_id"].as_str();

        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        info!("Compressing context");

        match client.tokens_compress(content, strategies, target_reduction, agent_id).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to compress: {}", e)
            }))?),
        }
    }

    async fn call_tokens_metrics(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        match client.tokens_metrics().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to get metrics: {}", e)
            }))?),
        }
    }

    async fn call_tokens_recommendations(&self, client: &DaemonClient) -> Result<String> {
        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        match client.tokens_recommendations().await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to get recommendations: {}", e)
            }))?),
        }
    }

    async fn call_index_codebase(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let path = arguments["path"]
            .as_str()
            .ok_or_else(|| anyhow!("path is required"))?;

        let extensions: Option<Vec<String>> = arguments["extensions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let exclude_patterns: Option<Vec<String>> = arguments["exclude_patterns"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        info!("Starting codebase indexing for: {}", path);

        match client.start_indexing(path, extensions, exclude_patterns, None).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to start indexing: {}", e)
            }))?),
        }
    }

    async fn call_search_code(
        &self,
        arguments: &serde_json::Value,
        client: &DaemonClient,
    ) -> Result<String> {
        let query = arguments["query"]
            .as_str()
            .ok_or_else(|| anyhow!("query is required"))?;

        let limit = arguments["limit"].as_i64().map(|l| l as i32);
        let language = arguments["language"].as_str();

        if !client.health().await? {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": "CCA daemon is not running. Start it with: cca daemon start"
            }))?);
        }

        info!("Searching code for: {}", query);

        match client.search_code(query, limit, language).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)?),
            Err(e) => Ok(serde_json::to_string_pretty(&serde_json::json!({
                "error": format!("Failed to search code: {}", e)
            }))?),
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
