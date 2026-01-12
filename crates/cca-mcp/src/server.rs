//! MCP Server implementation

use std::io::{BufRead, BufReader, Write};

use anyhow::Result;
use tracing::{debug, error, info};

use crate::tools::ToolRegistry;
use crate::types::{JsonRpcRequest, JsonRpcResponse};

/// MCP Server for CCA
pub struct McpServer {
    tools: ToolRegistry,
    daemon_url: String,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new(daemon_url: impl Into<String>) -> Self {
        Self {
            tools: ToolRegistry::new(),
            daemon_url: daemon_url.into(),
        }
    }

    /// Run the MCP server (stdio mode)
    pub async fn run_stdio(&self) -> Result<()> {
        info!("Starting MCP server in stdio mode");

        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let reader = BufReader::new(stdin.lock());

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(request) => {
                    let response = self.handle_request(request).await;
                    let response_json = serde_json::to_string(&response)?;
                    debug!("Sending: {}", response_json);
                    writeln!(stdout, "{response_json}")?;
                    stdout.flush()?;
                }
                Err(e) => {
                    error!("Failed to parse request: {}", e);
                    let error_response =
                        JsonRpcResponse::error(serde_json::Value::Null, -32700, "Parse error");
                    let response_json = serde_json::to_string(&error_response)?;
                    writeln!(stdout, "{response_json}")?;
                    stdout.flush()?;
                }
            }
        }

        Ok(())
    }

    /// Handle a JSON-RPC request
    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "resources/list" => self.handle_resources_list(request.id),
            "resources/read" => self.handle_resources_read(request.id, request.params),
            _ => JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            ),
        }
    }

    fn handle_initialize(&self, id: serde_json::Value) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": true
                    }
                },
                "serverInfo": {
                    "name": "cca",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
    }

    fn handle_tools_list(&self, id: serde_json::Value) -> JsonRpcResponse {
        let tools = self.tools.list();
        JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
    }

    async fn handle_tools_call(
        &self,
        id: serde_json::Value,
        params: serde_json::Value,
    ) -> JsonRpcResponse {
        let name = params["name"].as_str().unwrap_or("");
        let arguments = &params["arguments"];

        match self.tools.call(name, arguments, &self.daemon_url).await {
            Ok(result) => JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": result
                    }]
                }),
            ),
            Err(e) => JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error: {}", e)
                    }],
                    "isError": true
                }),
            ),
        }
    }

    fn handle_resources_list(&self, id: serde_json::Value) -> JsonRpcResponse {
        JsonRpcResponse::success(id, serde_json::json!({ "resources": [] }))
    }

    fn handle_resources_read(
        &self,
        id: serde_json::Value,
        _params: serde_json::Value,
    ) -> JsonRpcResponse {
        JsonRpcResponse::error(id, -32602, "Resource not found")
    }
}
