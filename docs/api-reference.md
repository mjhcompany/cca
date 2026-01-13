# API Reference

Complete API documentation for the CCA HTTP API and MCP tools.

## HTTP API

The CCA daemon exposes a REST API on the configured bind address (default: `http://127.0.0.1:9200`).

### Authentication

When `require_auth: true` is set in configuration, all endpoints except `/health` require authentication.

**Headers:**
```
X-API-Key: your-api-key
```
or
```
Authorization: Bearer your-api-key
```

## Health & Status

### GET /health

Health check endpoint (bypasses authentication).

**Response:**
```json
{
    "status": "healthy",
    "version": "0.1.0",
    "services": {
        "redis": true,
        "postgres": true,
        "acp": true
    }
}
```

Status values:
- `healthy` - All services available
- `degraded` - Some services unavailable

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

## Agent Management

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

### POST /api/v1/agents

Spawn a new agent.

**Request:**
```json
{
    "role": "backend"
}
```

Valid roles: `coordinator`, `frontend`, `backend`, `dba`, `devops`, `security`, `qa`

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

## Task Management

### POST /api/v1/tasks

Create a new task.

**Request:**
```json
{
    "description": "Implement JWT authentication for the API",
    "priority": "high"
}
```

Priority values: `low`, `normal`, `high`, `critical`

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

### GET /api/v1/tasks/{task_id}

Get specific task status.

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

**Status codes:**
- `200` - Task found
- `404` - Task not found

## Memory (ReasoningBank)

### POST /api/v1/memory/search

Search patterns in ReasoningBank with semantic search support. When embeddings are available, performs vector similarity search using pgvector. Falls back to text-based search when embeddings are unavailable.

**Request:**
```json
{
    "query": "authentication",
    "limit": 10
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `query` | string | Yes | - | Search query text (max 1,000 bytes) |
| `limit` | integer | No | 10 | Maximum number of results to return |

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

**Response (Text Search Fallback):**
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
            "created_at": "2024-01-10T10:00:00Z"
        }
    ],
    "count": 1,
    "query": "authentication",
    "search_type": "text"
}
```

| Response Field | Type | Description |
|----------------|------|-------------|
| `success` | boolean | Whether the search succeeded |
| `patterns` | array | Array of matching pattern objects |
| `patterns[].id` | string (UUID) | Unique pattern identifier |
| `patterns[].pattern_type` | string | Type classification of the pattern |
| `patterns[].content` | string | The pattern content text |
| `patterns[].success_rate` | float \| null | Success ratio (0.0-1.0) or null if no executions |
| `patterns[].success_count` | integer | Number of successful executions |
| `patterns[].failure_count` | integer | Number of failed executions |
| `patterns[].similarity` | float | Cosine similarity score (0.0-1.0), only present in semantic search |
| `patterns[].created_at` | string (ISO 8601) | Creation timestamp |
| `count` | integer | Number of results returned |
| `query` | string | Echo of the input query |
| `search_type` | string | Either `"semantic"` or `"text"` indicating search method used |

**Search Behavior:**

1. **Semantic Search (Primary):** When the embedding service is available:
   - Query text is converted to a 768-dimensional vector using Ollama's `nomic-embed-text` model
   - pgvector performs cosine similarity search against pattern embeddings
   - Results are filtered with a minimum similarity threshold of **0.3** (30%)
   - Results are ordered by similarity (highest first)

2. **Text Search (Fallback):** When embeddings are unavailable:
   - Uses case-insensitive substring matching (PostgreSQL `ILIKE`)
   - Results ordered by `success_rate DESC`, then `created_at DESC`

**Error Responses:**

Query too long (400):
```json
{
    "success": false,
    "error": "Query too long: 1500 bytes (max: 1000 bytes)"
}
```

PostgreSQL unavailable (503):
```json
{
    "success": false,
    "error": "PostgreSQL not available"
}
```

---

### POST /api/v1/memory/backfill-embeddings

Generate embeddings for existing patterns that don't have them. Processes patterns in batches of 10, prioritizing most recently created patterns.

**Request:**

No request body required.

**Response (Success):**
```json
{
    "success": true,
    "processed": 10,
    "errors": 0,
    "remaining": 45
}
```

| Response Field | Type | Description |
|----------------|------|-------------|
| `success` | boolean | Whether the backfill operation succeeded |
| `processed` | integer | Number of patterns successfully updated with embeddings |
| `errors` | integer | Number of patterns that failed to update |
| `remaining` | integer | Number of patterns still without embeddings |

**Response (Nothing to Process):**
```json
{
    "success": true,
    "message": "No patterns need embedding backfill",
    "processed": 0
}
```

**How It Works:**

1. Fetches up to 10 patterns where `embedding IS NULL`, ordered by `created_at DESC`
2. Sends pattern content to Ollama embedding service for batch processing
3. Updates each pattern with its 768-dimensional embedding vector
4. Returns counts of processed patterns and remaining work

**Usage Notes:**

- Call repeatedly until `remaining` reaches 0 to backfill all patterns
- Safe to run concurrently with normal operations
- Individual pattern errors don't abort the batch; check `errors` count
- Requires Ollama running with `nomic-embed-text` model available

**Error Responses:**

Embedding service not configured (503):
```json
{
    "success": false,
    "error": "Embedding service not configured"
}
```

PostgreSQL unavailable (503):
```json
{
    "success": false,
    "error": "PostgreSQL not available"
}
```

Failed to fetch patterns (500):
```json
{
    "success": false,
    "error": "Failed to get patterns: [error details]"
}
```

Failed to generate embeddings (500):
```json
{
    "success": false,
    "error": "Failed to generate embeddings: [error details]"
}
```

---

### Embedding Configuration

The semantic search feature requires:

| Component | Requirement |
|-----------|-------------|
| PostgreSQL | pgvector extension enabled |
| Embedding Model | Ollama with `nomic-embed-text:latest` |
| Vector Dimensions | 768 |
| Ollama URL | Default: `http://localhost:11434` |

**Database Schema (patterns table):**
```sql
embedding vector(768)  -- pgvector column

-- Index for fast similarity search
CREATE INDEX idx_patterns_embedding ON patterns
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
```

See [Configuration](./configuration.md) for embedding service setup options.

## Communication

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
        "550e8400-e29b-41d4-a716-446655440001",
        "550e8400-e29b-41d4-a716-446655440002"
    ]
}
```

### POST /api/v1/broadcast

Broadcast message to all agents.

**Request:**
```json
{
    "message": "System maintenance in 5 minutes"
}
```

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

**Request:**
```json
{
    "message": "Status update"
}
```

**Response:**
```json
{
    "success": true,
    "message": "Broadcast sent"
}
```

## Infrastructure Status

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

**Response (Disconnected):**
```json
{
    "connected": false,
    "error": "PostgreSQL not available"
}
```

## Reinforcement Learning

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

**Response (Success):**
```json
{
    "success": true,
    "loss": 0.021,
    "message": "Training complete"
}
```

**Response (Insufficient Data):**
```json
{
    "success": true,
    "loss": 0.0,
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

## Token Efficiency

### POST /api/v1/tokens/analyze

Analyze content for token usage.

**Request:**
```json
{
    "content": "Your content here...",
    "agent_id": "optional-agent-id"
}
```

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

## Error Responses

All endpoints may return error responses:

**400 Bad Request:**
```json
{
    "error": "Description too long: 150000 bytes (max: 100000 bytes)"
}
```

**401 Unauthorized:**
```json
{
    "error": "Missing or invalid API key"
}
```

**404 Not Found:**
```json
{
    "error": "Task not found"
}
```

**500 Internal Server Error:**
```json
{
    "error": "Internal server error: ..."
}
```

## Input Limits

| Field | Max Size |
|-------|----------|
| Task description | 100 KB |
| Broadcast message | 10 KB |
| Token content | 1 MB |
| Memory query | 1 KB |

## MCP Tools

See [cca-mcp documentation](./components/cca-mcp.md) for MCP tool reference.
