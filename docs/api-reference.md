# API Reference

Complete API documentation for the CCA system, including HTTP REST API, WebSocket (ACP) protocol, and MCP tools.

## Overview

CCA exposes three API interfaces:

| Interface | Protocol | Default Address | Purpose |
|-----------|----------|-----------------|---------|
| REST API | HTTP/HTTPS | `127.0.0.1:9200` | Task management, system control |
| ACP | WebSocket | `127.0.0.1:9100` | Real-time agent communication |
| MCP | JSON-RPC over stdio | N/A | Claude Code integration |

---

## HTTP REST API

The CCA daemon exposes a REST API on the configured bind address (default: `http://127.0.0.1:9200`).

### Authentication

When `require_auth: true` is set in configuration, all endpoints except health checks require authentication.

**Supported Methods:**

| Method | Header | Example |
|--------|--------|---------|
| API Key | `X-API-Key` | `X-API-Key: your-api-key` |
| Bearer Token | `Authorization` | `Authorization: Bearer your-api-key` |

**Bypass Paths (no authentication required):**
- `/health`
- `/api/v1/health`

**Security Features:**
- Constant-time comparison for API key validation (timing attack prevention)
- Rate limiting (per-IP, per-API-key, and global)

### Rate Limiting

CCA implements multi-tier rate limiting to prevent DoS attacks.

| Tier | Default Limit | Burst | Description |
|------|---------------|-------|-------------|
| Per-IP | 100 req/s | 50 | Base protection for all requests |
| Per-API-Key | 200 req/s | 100 | Higher limits for authenticated clients |
| Global | 1000 req/s | - | Absolute cap across all clients |

**Rate Limit Response Headers:**
```
Retry-After: <seconds>
X-RateLimit-Limit: <limit>
X-RateLimit-Remaining: <remaining>
X-RateLimit-Type: <ip|api_key|global>
```

**Rate Limit Exceeded Response (429):**
```json
{
    "error": "Too many requests",
    "message": "Rate limit exceeded. Please slow down.",
    "limit_type": "ip",
    "retry_after_seconds": 1
}
```

---

## Health & Status Endpoints

### GET /health

Health check endpoint (bypasses authentication). Response is cached for performance.

**Response:**
```json
{
    "status": "healthy",
    "version": "0.1.0",
    "services": {
        "redis": true,
        "postgres": true,
        "acp": true,
        "embeddings": true
    }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | string | `"healthy"` or `"degraded"` |
| `version` | string | CCA version |
| `services.redis` | boolean | Redis connection status |
| `services.postgres` | boolean | PostgreSQL connection status |
| `services.acp` | boolean | ACP WebSocket server status |
| `services.embeddings` | boolean | Embedding service availability |

### GET /api/v1/health

Alias for `/health`.

### GET /metrics

Prometheus metrics endpoint (bypasses authentication).

**Response:** Prometheus text format metrics.

### GET /api/v1/status

System status with task and agent counts.

**Response:**
```json
{
    "status": "running",
    "version": "0.1.0",
    "agents_count": 3,
    "tasks_pending": 5,
    "tasks_completed": 42
}
```

---

## Agent Management Endpoints

### GET /api/v1/agents

List all running agents.

**Response:**
```json
{
    "agents": [
        {
            "agent_id": "550e8400-e29b-41d4-a716-446655440000",
            "role": "coordinator",
            "status": "Ready",
            "current_task": null
        },
        {
            "agent_id": "550e8400-e29b-41d4-a716-446655440001",
            "role": "backend",
            "status": "Busy",
            "current_task": "task-123"
        }
    ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `agent_id` | string (UUID) | Unique agent identifier |
| `role` | string | Agent role |
| `status` | string | `"Ready"`, `"Busy"`, `"Offline"` |
| `current_task` | string \| null | Currently assigned task ID |

### POST /api/v1/agents

Spawn a new agent.

**Request:**
```json
{
    "role": "backend"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `role` | string | Yes | Agent role to spawn |

**Valid Roles:** `coordinator`, `frontend`, `backend`, `dba`, `devops`, `security`, `qa`

**Response (Success):**
```json
{
    "agent_id": "550e8400-e29b-41d4-a716-446655440002",
    "role": "backend",
    "status": "running"
}
```

**Response (Error):**
```json
{
    "error": "Maximum number of agents (10) reached"
}
```

### POST /api/v1/agents/:agent_id/send

Send a message directly to a specific agent.

**Path Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `agent_id` | string | Target agent ID |

**Request:**
```json
{
    "message": "Process this data...",
    "timeout_seconds": 60
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `message` | string | Yes | - | Message to send |
| `timeout_seconds` | integer | No | 30 | Response timeout |

**Response:**
```json
{
    "success": true,
    "output": "Processing complete...",
    "error": null,
    "duration_ms": 1234,
    "tokens_used": 500
}
```

### POST /api/v1/agents/:agent_id/attach

Start an interactive session with an agent.

**Path Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `agent_id` | string | Target agent ID |

**Response:**
```json
{
    "session_id": "session-uuid",
    "status": "attached"
}
```

### GET /api/v1/agents/:agent_id/logs

Get recent logs from a specific agent.

**Path Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `agent_id` | string | Target agent ID |

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lines` | integer | 100 | Number of log lines to return |

**Response:**
```json
{
    "agent_id": "550e8400-e29b-41d4-a716-446655440000",
    "logs": [
        "2024-01-10T12:00:00Z INFO Starting task processing",
        "2024-01-10T12:00:01Z DEBUG Received message from coordinator"
    ]
}
```

### GET /api/v1/activity

Get current activity of all agents with detailed metrics.

**Response:**
```json
{
    "agents": [
        {
            "agent_id": "550e8400-e29b-41d4-a716-446655440000",
            "role": "coordinator",
            "status": "Ready",
            "current_task": null,
            "last_activity": "2024-01-10T12:00:00Z",
            "tokens_used": 15000,
            "tasks_completed": 25
        }
    ]
}
```

### GET /api/v1/workloads

Get workload distribution across agents.

**Response:**
```json
{
    "agents": [
        {
            "agent_id": "550e8400-e29b-41d4-a716-446655440000",
            "role": "coordinator",
            "current_tasks": 2,
            "max_tasks": 10,
            "capabilities": []
        }
    ],
    "total_tasks": 50,
    "pending_tasks": 5
}
```

---

## Task Management Endpoints

### POST /api/v1/tasks

Create a new task for the coordinator to route.

**Request:**
```json
{
    "description": "Implement JWT authentication for the API",
    "priority": "high"
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `description` | string | Yes | - | Task description (max 100KB) |
| `priority` | string | No | `"normal"` | Priority level |

**Priority Values:** `low`, `normal`, `high`, `critical`

**Response (Success):**
```json
{
    "task_id": "task-550e8400",
    "status": "completed",
    "output": "Authentication implemented successfully...",
    "error": null,
    "assigned_agent": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Response (Error):**
```json
{
    "task_id": "task-550e8400",
    "status": "failed",
    "output": null,
    "error": "Timeout waiting for agent response",
    "assigned_agent": null
}
```

### GET /api/v1/tasks

List all tasks.

**Response:**
```json
{
    "tasks": [
        {
            "task_id": "task-001",
            "status": "completed",
            "output": "...",
            "error": null,
            "assigned_agent": "agent-001"
        }
    ]
}
```

### GET /api/v1/tasks/:task_id

Get specific task status.

**Path Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `task_id` | string | Task ID |

**Response:**
```json
{
    "task_id": "task-001",
    "status": "completed",
    "output": "Task completed successfully...",
    "error": null,
    "assigned_agent": "agent-001"
}
```

**Task Status Values:** `pending`, `assigned`, `in_progress`, `completed`, `failed`

### POST /api/v1/delegate

Delegate a task to a specific role with additional context.

**Request:**
```json
{
    "role": "backend",
    "task": "Implement user authentication",
    "context": "Use JWT tokens with 24h expiry",
    "timeout_seconds": 120
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `role` | string | Yes | - | Target agent role |
| `task` | string | Yes | - | Task description |
| `context` | string | No | null | Additional context |
| `timeout_seconds` | integer | No | 60 | Task timeout |

**Response:**
```json
{
    "success": true,
    "agent_id": "550e8400-e29b-41d4-a716-446655440000",
    "role": "backend",
    "output": "Implementation complete...",
    "error": null,
    "duration_ms": 5432,
    "tokens_used": 2500
}
```

---

## Memory (ReasoningBank) Endpoints

### POST /api/v1/memory/search

Search patterns in ReasoningBank with semantic search support.

**Request:**
```json
{
    "query": "authentication",
    "limit": 10
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `query` | string | Yes | - | Search query (max 1KB) |
| `limit` | integer | No | 10 | Maximum results |

**Response (Semantic Search):**
```json
{
    "success": true,
    "patterns": [
        {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "pattern_type": "code",
            "content": "JWT authentication pattern...",
            "success_rate": 0.95,
            "success_count": 19,
            "failure_count": 1,
            "similarity": 0.87,
            "created_at": "2024-01-10T10:00:00Z"
        }
    ],
    "count": 1,
    "query": "authentication",
    "search_type": "semantic"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `search_type` | string | `"semantic"` (vector) or `"text"` (fallback) |
| `similarity` | float | Cosine similarity (0.0-1.0), semantic only |
| `success_rate` | float \| null | Success ratio or null if no executions |

**Search Behavior:**
1. **Semantic Search:** Uses pgvector with `nomic-embed-text` embeddings (768 dimensions). Minimum similarity threshold: 0.3.
2. **Text Fallback:** Case-insensitive substring matching when embeddings unavailable.

### POST /api/v1/memory/backfill-embeddings

Generate embeddings for patterns that don't have them.

**Response:**
```json
{
    "success": true,
    "processed": 10,
    "errors": 0,
    "remaining": 45
}
```

---

## Code Indexing Endpoints

### POST /api/v1/memory/index

Start indexing a codebase for semantic code search.

**Request:**
```json
{
    "path": "/home/user/project",
    "extensions": [".rs", ".py", ".ts"],
    "exclude_patterns": ["**/node_modules/**", "**/target/**"]
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `path` | string | Yes | - | Directory to index |
| `extensions` | array | No | Common extensions | File extensions to include |
| `exclude_patterns` | array | No | [] | Glob patterns to exclude |

**Response:**
```json
{
    "success": true,
    "job_id": "job-550e8400",
    "message": "Indexing started"
}
```

### GET /api/v1/memory/index/:job_id

Get status of an indexing job.

**Response:**
```json
{
    "job_id": "job-550e8400",
    "status": "running",
    "progress": 45,
    "files_processed": 120,
    "files_total": 267,
    "symbols_indexed": 1543
}
```

**Job Status Values:** `pending`, `running`, `completed`, `failed`, `cancelled`

### POST /api/v1/memory/index/:job_id/cancel

Cancel a running indexing job.

**Response:**
```json
{
    "success": true,
    "message": "Job cancelled"
}
```

### GET /api/v1/memory/index/jobs

List all indexing jobs.

**Response:**
```json
{
    "jobs": [
        {
            "job_id": "job-001",
            "status": "completed",
            "path": "/home/user/project",
            "started_at": "2024-01-10T10:00:00Z",
            "completed_at": "2024-01-10T10:05:00Z"
        }
    ]
}
```

### POST /api/v1/code/search

Search indexed code using semantic similarity.

**Request:**
```json
{
    "query": "function that validates user input",
    "limit": 10,
    "language": "rust"
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `query` | string | Yes | - | Natural language query |
| `limit` | integer | No | 10 | Maximum results |
| `language` | string | No | null | Filter by language |

**Response:**
```json
{
    "success": true,
    "results": [
        {
            "file_path": "/src/validation.rs",
            "symbol_name": "validate_input",
            "symbol_type": "function",
            "language": "rust",
            "content": "pub fn validate_input(input: &str) -> Result<(), ValidationError>...",
            "line_start": 45,
            "line_end": 67,
            "similarity": 0.89
        }
    ],
    "count": 1
}
```

### GET /api/v1/code/stats

Get code indexing statistics.

**Response:**
```json
{
    "success": true,
    "total_files": 500,
    "total_symbols": 3500,
    "languages": {
        "rust": 200,
        "python": 150,
        "typescript": 150
    },
    "last_indexed": "2024-01-10T10:05:00Z"
}
```

---

## Communication Endpoints

### GET /api/v1/acp/status

Get ACP WebSocket server status.

**Response:**
```json
{
    "running": true,
    "port": 9100,
    "connected_agents": 3,
    "agent_ids": [
        "550e8400-e29b-41d4-a716-446655440000",
        "550e8400-e29b-41d4-a716-446655440001"
    ],
    "workers": [
        {
            "agent_id": "550e8400-e29b-41d4-a716-446655440000",
            "role": "backend",
            "status": "connected"
        }
    ]
}
```

### POST /api/v1/acp/disconnect

Disconnect an agent from the ACP server.

**Request:**
```json
{
    "agent_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Response:**
```json
{
    "success": true,
    "message": "Agent disconnected"
}
```

### POST /api/v1/acp/send

Send a task to a specific agent via ACP.

**Request:**
```json
{
    "agent_id": "550e8400-e29b-41d4-a716-446655440000",
    "task": "Process this request",
    "context": "Additional context here"
}
```

**Response:**
```json
{
    "success": true,
    "output": "Task completed...",
    "tokens_used": 500,
    "duration_ms": 1234
}
```

### POST /api/v1/broadcast

Broadcast message to all agents (ACP + Redis).

**Request:**
```json
{
    "message": "System maintenance in 5 minutes"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `message` | string | Yes | Message to broadcast (max 10KB) |

**Response:**
```json
{
    "success": true,
    "agents_notified": 3,
    "message": "Broadcast sent to 3 agents via ACP, Redis: true"
}
```

### POST /api/v1/pubsub/broadcast

Broadcast via Redis pub/sub only.

**Response:**
```json
{
    "success": true,
    "message": "Broadcast sent"
}
```

---

## Infrastructure Status Endpoints

### GET /api/v1/redis/status

Get Redis connection status.

**Response (Connected):**
```json
{
    "connected": true,
    "pool_size": 10,
    "context_ttl_seconds": 3600,
    "agents_tracked": 3
}
```

**Response (Disconnected):**
```json
{
    "connected": false,
    "error": "Redis not available"
}
```

### GET /api/v1/postgres/status

Get PostgreSQL connection status.

**Response (Connected):**
```json
{
    "connected": true,
    "pool_size": 20,
    "patterns_count": 150
}
```

---

## Reinforcement Learning Endpoints

### GET /api/v1/rl/stats

Get RL engine statistics.

**Response:**
```json
{
    "algorithm": "q_learning",
    "total_steps": 1000,
    "total_rewards": 850.5,
    "average_reward": 0.85,
    "buffer_size": 500,
    "last_training_loss": 0.023,
    "experience_count": 1000,
    "algorithms_available": ["q_learning", "ppo", "dqn"]
}
```

### POST /api/v1/rl/train

Trigger training on collected experiences.

**Response:**
```json
{
    "success": true,
    "loss": 0.021,
    "message": "Training complete"
}
```

### POST /api/v1/rl/algorithm

Set the active RL algorithm.

**Request:**
```json
{
    "algorithm": "dqn"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `algorithm` | string | Yes | `"q_learning"`, `"dqn"`, or `"ppo"` |

**Response:**
```json
{
    "success": true,
    "algorithm": "dqn",
    "message": "Switched to algorithm: dqn"
}
```

### GET /api/v1/rl/params

Get current algorithm parameters.

**Response:**
```json
{
    "success": true,
    "params": {
        "learning_rate": 0.1,
        "discount_factor": 0.99,
        "epsilon": 0.08,
        "q_table_size": 150
    }
}
```

### POST /api/v1/rl/params

Set algorithm parameters.

**Request:**
```json
{
    "learning_rate": 0.05,
    "epsilon": 0.1
}
```

**Response:**
```json
{
    "success": true,
    "message": "Parameters updated"
}
```

---

## Token Efficiency Endpoints

### POST /api/v1/tokens/analyze

Analyze content for token usage.

**Request:**
```json
{
    "content": "Your content here...",
    "agent_id": "optional-agent-id"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `content` | string | Yes | Content to analyze (max 1MB) |
| `agent_id` | string | No | Associate with agent |

**Response:**
```json
{
    "success": true,
    "total_tokens": 1500,
    "repeated_tokens": 200,
    "code_blocks": 3,
    "long_lines": 5,
    "compression_potential": "25.5%",
    "repeated_lines": 10
}
```

### POST /api/v1/tokens/compress

Compress content to reduce tokens.

**Request:**
```json
{
    "content": "Your content here...",
    "compress_code": true,
    "target_reduction": 0.3,
    "agent_id": "optional-agent-id"
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `content` | string | Yes | - | Content to compress |
| `compress_code` | boolean | No | true | Remove code comments |
| `target_reduction` | float | No | 0.3 | Target reduction (0.0-1.0) |
| `agent_id` | string | No | null | Associate with agent |

**Response:**
```json
{
    "success": true,
    "original_tokens": 1500,
    "final_tokens": 1050,
    "tokens_saved": 450,
    "reduction": "30.0%",
    "compressed_content": "Compressed content..."
}
```

### GET /api/v1/tokens/metrics

Get token efficiency metrics.

**Response:**
```json
{
    "success": true,
    "summary": {
        "total_tokens_used": 150000,
        "total_tokens_saved": 45000,
        "compression_ratio": "30.0%",
        "agents_tracked": 5,
        "target_reduction": "30%",
        "current_reduction": "30.0%",
        "on_track": true
    },
    "agents": [
        {
            "agent_id": "agent-001",
            "total_input": 50000,
            "total_output": 10000,
            "total_context": 30000,
            "message_count": 100,
            "avg_input_per_message": 500,
            "avg_output_per_message": 100,
            "peak_context_size": 35000,
            "compression_savings": 15000
        }
    ]
}
```

### GET /api/v1/tokens/recommendations

Get token efficiency recommendations.

**Response:**
```json
{
    "success": true,
    "recommendations": [
        {
            "agent_id": "agent-001",
            "category": "high_context",
            "severity": "warning",
            "message": "Agent has high context size, consider compression",
            "potential_savings": 5000
        }
    ],
    "count": 1,
    "total_potential_savings": 5000
}
```

---

## Error Responses

All endpoints may return error responses:

### 400 Bad Request
```json
{
    "error": "Description too long: 150000 bytes (max: 100000 bytes)"
}
```

### 401 Unauthorized
```json
{
    "error": "Missing or invalid API key"
}
```

### 404 Not Found
```json
{
    "error": "Task not found"
}
```

### 429 Too Many Requests
```json
{
    "error": "Too many requests",
    "message": "Rate limit exceeded. Please slow down.",
    "limit_type": "ip",
    "retry_after_seconds": 1
}
```

### 500 Internal Server Error
```json
{
    "error": "Internal server error: ..."
}
```

### 503 Service Unavailable
```json
{
    "error": "PostgreSQL not available"
}
```

---

## Input Limits

| Field | Max Size |
|-------|----------|
| Task description | 100 KB |
| Broadcast message | 10 KB |
| Token content | 1 MB |
| Memory query | 1 KB |

---

## WebSocket API (ACP - Agent Control Protocol)

The ACP server provides real-time bidirectional communication using JSON-RPC 2.0 over WebSocket.

### Connection

**WebSocket URL:** `ws://127.0.0.1:9100` (configurable)

### Authentication

ACP supports three authentication methods:

| Method | Example |
|--------|---------|
| Query Parameter | `ws://127.0.0.1:9100?token=your-api-key` |
| Header (X-API-Key) | During WebSocket handshake |
| Header (Bearer) | `Authorization: Bearer your-api-key` |

When authentication is required, agents must authenticate before sending other messages.

### JSON-RPC 2.0 Format

**Request:**
```json
{
    "jsonrpc": "2.0",
    "method": "methodName",
    "params": { ... },
    "id": 1
}
```

**Response (Success):**
```json
{
    "jsonrpc": "2.0",
    "result": { ... },
    "id": 1
}
```

**Response (Error):**
```json
{
    "jsonrpc": "2.0",
    "error": {
        "code": -32600,
        "message": "Invalid Request",
        "data": null
    },
    "id": 1
}
```

**Notification (no response expected):**
```json
{
    "jsonrpc": "2.0",
    "method": "methodName",
    "params": { ... }
}
```

### ACP Methods

#### registerAgent

Register an agent with the server. Must be called after connecting.

**Parameters:**
```json
{
    "agent_id": "550e8400-e29b-41d4-a716-446655440000",
    "role": "backend",
    "capabilities": ["rust", "api", "database"],
    "metadata": {}
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `agent_id` | string (UUID) | Yes | Unique agent identifier |
| `role` | string | Yes | Agent role |
| `capabilities` | array | No | Agent capabilities |
| `metadata` | object | No | Additional metadata |

**Response:**
```json
{
    "accepted": true,
    "message": "Agent registered successfully",
    "assigned_tasks_channel": "tasks:backend"
}
```

#### heartbeat

Keep-alive and time synchronization.

**Parameters:**
```json
{
    "timestamp": 1704888000000
}
```

**Response:**
```json
{
    "timestamp": 1704888000000,
    "server_time": 1704888000005
}
```

#### getStatus

Get current agent status.

**Response:**
```json
{
    "agent_id": "550e8400-e29b-41d4-a716-446655440000",
    "state": "Ready",
    "current_task": null,
    "uptime_seconds": 3600
}
```

#### sendMessage

Send a message to another agent.

**Parameters:**
```json
{
    "to": "550e8400-e29b-41d4-a716-446655440001",
    "content": "Process this request",
    "metadata": {}
}
```

#### executeTask

Request immediate task execution.

**Parameters:**
```json
{
    "description": "Implement user validation",
    "priority": 1,
    "metadata": {}
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `description` | string | Yes | - | Task description |
| `priority` | integer | No | 0 | 0=low, 1=normal, 2=high, 3=critical |
| `metadata` | object | No | {} | Additional task data |

#### cancelTask

Cancel a running task.

**Parameters:**
```json
{
    "task_id": "task-550e8400"
}
```

#### queryAgent

Query agent information.

**Parameters:**
```json
{
    "query_type": "status",
    "agent_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

| Query Type | Description |
|------------|-------------|
| `list_all` | List all agents |
| `status` | Get agent status |
| `capabilities` | Get agent capabilities |
| `workload` | Get agent workload |

### ACP Notifications (Server → Agent)

#### taskAssign

Server assigns a task to an agent.

```json
{
    "task_id": "task-550e8400",
    "description": "Implement authentication",
    "priority": 2,
    "parent_task": null,
    "token_budget": 5000,
    "metadata": {}
}
```

#### broadcast

Server broadcasts a message to all agents.

```json
{
    "message_type": "announcement",
    "content": { "message": "System maintenance in 5 minutes" }
}
```

**Broadcast Types:** `announcement`, `config_update`, `health_check`, `task_notification`, `custom`

### ACP Notifications (Agent → Server)

#### taskResult

Agent reports task completion.

```json
{
    "task_id": "task-550e8400",
    "success": true,
    "output": "Authentication implemented...",
    "tokens_used": 2500,
    "duration_ms": 5432,
    "error": null,
    "metadata": {}
}
```

#### taskProgress

Agent reports progress update.

```json
{
    "task_id": "task-550e8400",
    "progress_pct": 50,
    "message": "Implementing token validation...",
    "subtasks_completed": 2,
    "subtasks_total": 4
}
```

### ACP Error Codes

| Code | Message | Description |
|------|---------|-------------|
| -32700 | Parse error | Invalid JSON |
| -32600 | Invalid Request | Malformed JSON-RPC request |
| -32601 | Method not found | Unknown method name |
| -32602 | Invalid params | Invalid method parameters |
| -32603 | Internal error | Server internal error |

### Backpressure Handling

ACP implements backpressure protection for slow consumers:

| Setting | Default | Description |
|---------|---------|-------------|
| `channel_capacity` | 100 | Outbound message buffer size |
| `max_consecutive_drops` | 10 | Drops before disconnection |
| `warning_threshold` | 0.8 | Channel fullness warning level |

When a client can't keep up, messages are dropped. After too many consecutive drops, the slow consumer is disconnected.

---

## MCP Tools Reference

MCP (Model Context Protocol) tools enable Claude Code integration via JSON-RPC over stdio.

### cca_task

Send a task to the CCA system.

**Parameters:**
```json
{
    "description": "Implement user authentication",
    "priority": "high"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `description` | string | Yes | Task description |
| `priority` | string | No | `low`, `normal`, `high`, `critical` |

### cca_status

Check task or system status.

**Parameters:**
```json
{
    "task_id": "task-550e8400"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | No | Specific task ID (omit for system status) |

### cca_activity

Get current activity of all agents.

**Parameters:** None

### cca_agents

List all running agents.

**Parameters:** None

### cca_memory

Query the ReasoningBank for learned patterns.

**Parameters:**
```json
{
    "query": "authentication patterns",
    "limit": 10
}
```

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `query` | string | Yes | - | Search query |
| `limit` | number | No | 10 | Maximum results |

### cca_acp_status

Get ACP WebSocket server status.

**Parameters:** None

### cca_broadcast

Broadcast a message to all connected agents.

**Parameters:**
```json
{
    "message": "System update complete"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `message` | string | Yes | Message to broadcast |

### cca_workloads

Get current workload distribution.

**Parameters:** None

### cca_rl_status

Get RL engine status and statistics.

**Parameters:** None

### cca_rl_train

Trigger RL training on collected experiences.

**Parameters:** None

### cca_rl_algorithm

Set the RL algorithm.

**Parameters:**
```json
{
    "algorithm": "dqn"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `algorithm` | string | Yes | `q_learning`, `dqn`, or `ppo` |

### cca_tokens_analyze

Analyze content for token usage.

**Parameters:**
```json
{
    "content": "Content to analyze...",
    "agent_id": "agent-001"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `content` | string | Yes | Content to analyze |
| `agent_id` | string | No | Associate with agent |

### cca_tokens_compress

Compress content to reduce tokens.

**Parameters:**
```json
{
    "content": "Content to compress...",
    "strategies": ["code_comments", "deduplicate"],
    "target_reduction": 0.3,
    "agent_id": "agent-001"
}
```

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `content` | string | Yes | - | Content to compress |
| `strategies` | array | No | all | Compression strategies |
| `target_reduction` | number | No | 0.3 | Target reduction (0.0-1.0) |
| `agent_id` | string | No | null | Associate with agent |

**Available Strategies:** `code_comments`, `history`, `summarize`, `deduplicate`

### cca_tokens_metrics

Get token efficiency metrics.

**Parameters:** None

### cca_tokens_recommendations

Get recommendations for improving token efficiency.

**Parameters:** None

### cca_index_codebase

Index a codebase for semantic code search.

**Parameters:**
```json
{
    "path": "/home/user/project",
    "extensions": [".rs", ".py"],
    "exclude_patterns": ["**/target/**"]
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Directory to index |
| `extensions` | array | No | File extensions to include |
| `exclude_patterns` | array | No | Glob patterns to exclude |

### cca_search_code

Search indexed code using semantic similarity.

**Parameters:**
```json
{
    "query": "function that handles authentication",
    "limit": 10,
    "language": "rust"
}
```

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `query` | string | Yes | - | Natural language query |
| `limit` | number | No | 10 | Maximum results |
| `language` | string | No | null | Filter by language |

---

## Usage Examples

### cURL Examples

**Create a task:**
```bash
curl -X POST http://127.0.0.1:9200/api/v1/tasks \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{"description": "Implement user authentication", "priority": "high"}'
```

**List agents:**
```bash
curl http://127.0.0.1:9200/api/v1/agents \
  -H "X-API-Key: your-api-key"
```

**Search memory:**
```bash
curl -X POST http://127.0.0.1:9200/api/v1/memory/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{"query": "database optimization", "limit": 5}'
```

**Broadcast message:**
```bash
curl -X POST http://127.0.0.1:9200/api/v1/broadcast \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{"message": "System maintenance starting"}'
```

### WebSocket Example (JavaScript)

```javascript
const ws = new WebSocket('ws://127.0.0.1:9100?token=your-api-key');

ws.onopen = () => {
  // Register agent
  ws.send(JSON.stringify({
    jsonrpc: '2.0',
    method: 'registerAgent',
    params: {
      agent_id: '550e8400-e29b-41d4-a716-446655440000',
      role: 'backend',
      capabilities: ['rust', 'api']
    },
    id: 1
  }));
};

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.method === 'taskAssign') {
    // Handle task assignment
    console.log('Received task:', message.params);

    // Report completion
    ws.send(JSON.stringify({
      jsonrpc: '2.0',
      method: 'taskResult',
      params: {
        task_id: message.params.task_id,
        success: true,
        output: 'Task completed',
        tokens_used: 500,
        duration_ms: 1000
      }
    }));
  }
};
```

---

## See Also

- [Configuration Guide](./configuration.md) - Server and authentication configuration
- [Deployment Guide](./deployment.md) - Production deployment instructions
- [Architecture Overview](./architecture.md) - System design and components
- [Security Hardening](./security-hardening.md) - Security best practices
