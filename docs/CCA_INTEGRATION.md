# CCA MCP Tools Reference

> Claude Code Agents (CCA) - Intelligent code analysis, task routing, and continuous learning

## Overview

CCA is a multi-agent orchestration system that provides AI-powered code analysis, intelligent task routing to specialist agents, and continuous learning from development experiences. CCA exposes **17 MCP (Model Context Protocol) tools** for seamless integration with Claude Code.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Claude Code (Client)                           │
│                           │                                         │
│                    MCP Protocol (17 Tools)                          │
└───────────────────────────┬─────────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────────────┐
│                      CCA Daemon                                      │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │
│  │   Coordinator   │  │  ReasoningBank  │  │    RL Engine        │  │
│  │   (Task Router) │  │    (Memory)     │  │  (Q-Learning/DQN)   │  │
│  └────────┬────────┘  └─────────────────┘  └─────────────────────┘  │
│           │                                                          │
│  ┌────────▼────────────────────────────────────────────────────┐    │
│  │                    ACP WebSocket Hub                         │    │
│  └────────┬─────────────┬─────────────┬─────────────┬──────────┘    │
└───────────┼─────────────┼─────────────┼─────────────┼───────────────┘
            │             │             │             │
    ┌───────▼───────┐ ┌───▼───┐ ┌───────▼───────┐ ┌───▼───┐
    │ Go Backend    │ │ Rust  │ │ Frontend      │ │  DBA  │
    │ Specialist    │ │ Agent │ │ Specialist    │ │ Agent │
    └───────────────┘ └───────┘ └───────────────┘ └───────┘
```

## MCP Tools Reference

### Task Management Tools

#### `cca_task`
Submit tasks to the CCA coordinator for intelligent routing to specialist agents.

```json
{
  "description": "Analyze authentication middleware for security issues",
  "priority": "high"  // low | normal | high | critical
}
```

**Response:**
```json
{
  "task_id": "8699243c-aab3-4f14-a4e7-e6f66797124a",
  "status": "completed",
  "output": "Analysis results...",
  "assigned_agent": "backend-specialist"
}
```

#### `cca_status`
Check system status or specific task status.

```json
{
  "task_id": "optional-task-id"  // omit for system status
}
```

**Response:**
```json
{
  "status": "running",
  "version": "0.3.0",
  "agents_count": 5,
  "tasks_pending": 2,
  "tasks_completed": 15
}
```

#### `cca_activity`
Get current activity of all connected agents.

**Response:**
```json
{
  "agents": [
    {"id": "agent-1", "type": "backend", "status": "busy", "current_task": "..."},
    {"id": "agent-2", "type": "frontend", "status": "idle"}
  ]
}
```

---

### Agent Control Tools

#### `cca_agents`
List all running agents and their capabilities.

**Response:**
```json
{
  "agents": [
    {"id": "...", "type": "coordinator", "status": "running"},
    {"id": "...", "type": "backend", "status": "running"},
    {"id": "...", "type": "frontend", "status": "running"},
    {"id": "...", "type": "dba", "status": "running"}
  ]
}
```

#### `cca_broadcast`
Send a message to all connected agents.

```json
{
  "message": "Priority shift: focus on security audit"
}
```

#### `cca_workloads`
Get current workload distribution across agents.

**Response:**
```json
{
  "workloads": {
    "backend": {"tasks": 3, "utilization": 0.75},
    "frontend": {"tasks": 1, "utilization": 0.25},
    "dba": {"tasks": 0, "utilization": 0.0}
  }
}
```

---

### Memory & Learning Tools

#### `cca_memory`
Query the ReasoningBank for learned patterns relevant to the current task.

```json
{
  "query": "authentication error handling patterns",
  "limit": 10
}
```

**Response:**
```json
{
  "patterns": [
    {
      "id": "pattern-001",
      "pattern_type": "error_handling",
      "content": "JWT validation should check expiry before signature",
      "success_rate": 0.92,
      "success_count": 23,
      "failure_count": 2
    }
  ]
}
```

#### `cca_rl_status`
Get reinforcement learning engine status.

**Response:**
```json
{
  "algorithm": "q_learning",
  "total_steps": 150,
  "total_rewards": 142.5,
  "average_reward": 0.95,
  "buffer_size": 150,
  "experience_count": 150,
  "last_training_loss": 0.023,
  "algorithms_available": ["q_learning", "dqn", "ppo"]
}
```

#### `cca_rl_train`
Trigger training on collected experiences to update the learning model.

**Response:**
```json
{
  "success": true,
  "loss": 0.018,
  "message": "Training complete"
}
```

#### `cca_rl_algorithm`
Set the reinforcement learning algorithm.

```json
{
  "algorithm": "dqn"  // q_learning | dqn | ppo
}
```

**Algorithm Comparison:**

| Algorithm | Best For | Learning Speed | Stability |
|-----------|----------|----------------|-----------|
| `q_learning` | Simple tasks, small state space | Fast | High |
| `dqn` | Complex tasks, pattern recognition | Medium | Medium |
| `ppo` | Continuous learning, policy optimization | Slow | Very High |

---

### Token Optimization Tools

#### `cca_tokens_analyze`
Analyze content for token usage, detect redundancy, and estimate compression potential.

```json
{
  "content": "Your code or text content here...",
  "agent_id": "optional-agent-id"
}
```

**Response:**
```json
{
  "token_count": 1250,
  "redundancy_score": 0.23,
  "compression_potential": 0.35,
  "recommendations": ["Remove duplicate imports", "Consolidate error messages"]
}
```

#### `cca_tokens_compress`
Compress content using various strategies targeting 30%+ token reduction.

```json
{
  "content": "Content to compress...",
  "strategies": ["code_comments", "history", "summarize", "deduplicate"],
  "target_reduction": 0.3,
  "agent_id": "optional-agent-id"
}
```

**Strategies:**

| Strategy | Description | Typical Reduction |
|----------|-------------|-------------------|
| `code_comments` | Remove/minimize code comments | 10-20% |
| `history` | Compress conversation history | 20-40% |
| `summarize` | Summarize verbose content | 30-50% |
| `deduplicate` | Remove duplicate content | 15-30% |

#### `cca_tokens_metrics`
Get token efficiency metrics across all agents.

**Response:**
```json
{
  "total_tokens_used": 125000,
  "total_tokens_saved": 42000,
  "efficiency_percentage": 33.6,
  "per_agent": {
    "backend": {"used": 50000, "saved": 18000},
    "frontend": {"used": 45000, "saved": 15000}
  }
}
```

#### `cca_tokens_recommendations`
Get recommendations for improving token efficiency based on usage patterns.

**Response:**
```json
{
  "recommendations": [
    {"priority": "high", "action": "Enable history compression", "potential_savings": "25%"},
    {"priority": "medium", "action": "Use code summarization for large files", "potential_savings": "15%"}
  ]
}
```

---

### Code Intelligence Tools

#### `cca_index_codebase`
Index a codebase for semantic code search. Extracts functions, classes, and methods, generates embeddings for similarity search.

```json
{
  "path": "/path/to/your/project",
  "extensions": [".rs", ".go", ".ts", ".tsx", ".py"],
  "exclude_patterns": ["**/node_modules/**", "**/target/**", "**/.git/**"]
}
```

**Response:**
```json
{
  "job_id": "79bf2244-683f-4e7d-8d4e-c823d68a1d40",
  "status": "started",
  "message": "Indexing job started in background"
}
```

**Requirements:**
- Ollama must be running with an embedding model (e.g., `nomic-embed-text`)
- Sufficient disk space for vector index

#### `cca_search_code`
Search indexed code using semantic similarity.

```json
{
  "query": "authentication middleware JWT validation",
  "language": "go",  // optional: filter by language
  "limit": 10
}
```

**Response:**
```json
{
  "success": true,
  "results": [
    {
      "file_path": "/path/to/project/src/middleware/auth.rs",
      "chunk_type": "function",
      "name": "verify_token",
      "signature": "pub fn verify_token(token: &str) -> Result<Claims, AuthError>",
      "content": "pub fn verify_token...",
      "start_line": 45,
      "end_line": 82,
      "language": "rust",
      "similarity": 0.89
    }
  ],
  "count": 5
}
```

---

### Communication Tools

#### `cca_acp_status`
Get ACP (Agent Communication Protocol) WebSocket server status.

**Response:**
```json
{
  "status": "running",
  "connected_agents": 4,
  "messages_processed": 1250,
  "uptime_seconds": 3600
}
```

---

## Specialist Agents

### Available Specialists

| Agent | Expertise | Use Cases |
|-------|-----------|-----------|
| **coordinator** | Task analysis, routing | Orchestrates multi-agent workflows |
| **backend** | Go, Rust, APIs | Code review, architecture analysis |
| **frontend** | Templ, HTMX, Alpine.js | UI patterns, component analysis |
| **dba** | PostgreSQL, Redis | Schema review, query optimization |
| **devops** | Docker, CI/CD | Infrastructure, deployment |
| **security** | OWASP, auth/authz | Security audits, vulnerability scanning |
| **qa** | Testing | Test coverage, quality assurance |

### Starting Specialist Workers

```bash
# Start all specialists
cca agent worker coordinator &
cca agent worker backend &
cca agent worker frontend &
cca agent worker dba &
cca agent worker devops &
cca agent worker security &
cca agent worker qa &

# Check running agents
cca_agents
```

---

## Workflows

### Comprehensive Code Analysis

```bash
# 1. Start agents
cca agent worker coordinator &
cca agent worker backend &
cca agent worker frontend &
cca agent worker dba &

# 2. Submit analysis task
cca_task description="Comprehensive codebase analysis: backend architecture, frontend patterns, database optimization" priority="high"

# 3. Check progress
cca_activity

# 4. Train on results
cca_rl_train
```

### Semantic Code Search Workflow

```bash
# 1. Index the codebase (one-time or periodic)
cca_index_codebase path="/path/to/your/project" exclude_patterns='["**/node_modules/**", "**/target/**"]'

# 2. Search for patterns
cca_search_code query="error handling middleware" language="rust"
cca_search_code query="database connection pool" language="rust"
cca_search_code query="authentication patterns"
```

### Token Optimization Workflow

```bash
# 1. Analyze current usage
cca_tokens_metrics

# 2. Get recommendations
cca_tokens_recommendations

# 3. Compress specific content
cca_tokens_compress content="..." strategies='["history", "deduplicate"]'

# 4. Verify improvements
cca_tokens_metrics
```

### Continuous Learning Workflow

```bash
# 1. Check current learning state
cca_rl_status

# 2. Query existing patterns
cca_memory query="authentication patterns"

# 3. Run tasks to collect experiences
cca_task description="Review auth middleware"

# 4. Train on new experiences
cca_rl_train

# 5. Optionally switch algorithms for better learning
cca_rl_algorithm algorithm="dqn"
```

---

## Configuration

### CCA Daemon Configuration

CCA configuration file (`cca.toml`) or environment variables:

```toml
[daemon]
bind_address = "127.0.0.1:9200"
max_agents = 10
require_auth = false

[redis]
url = "redis://localhost:6380"

[postgres]
url = "postgres://cca:cca@localhost:5433/cca"

[embedding]
provider = "ollama"
base_url = "http://localhost:11434"
model = "nomic-embed-text"

[learning]
enabled = true
default_algorithm = "q_learning"
```

### Claude Code MCP Configuration

Register CCA using the CLI:

```bash
claude mcp add cca /path/to/cca/target/release/cca-mcp --args "--daemon-url" "http://127.0.0.1:9200"
```

Or add to Claude Code MCP settings (`~/.claude/settings.json`):

```json
{
  "mcpServers": {
    "cca": {
      "command": "/path/to/cca/target/release/cca-mcp",
      "args": ["--daemon-url", "http://127.0.0.1:9200"]
    }
  }
}
```

---

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "No coordinator worker connected" | Coordinator not running | Start: `cca agent worker coordinator` |
| "No specialist workers connected" | Specialists not running | Start required specialists |
| "Search failed: Ollama embedding error" | Ollama not accessible | Check Ollama is running with embedding model |
| Empty memory/patterns | Not enough training data | Run more tasks, then `cca_rl_train` |
| Low similarity scores | Index outdated | Re-run `cca_index_codebase` |

### Debugging Commands

```bash
# Check CCA system status
cca_status

# Check agent connectivity
cca_agents

# Check ACP WebSocket status
cca_acp_status

# Check RL engine state
cca_rl_status

# View token usage
cca_tokens_metrics
```

---

## Best Practices

### Task Submission
- Use clear, specific task descriptions
- Set appropriate priority levels
- Break large tasks into smaller, focused subtasks

### Code Indexing
- Re-index after major code changes
- Exclude build artifacts and dependencies
- Use language filters for targeted searches

### Learning Optimization
- Run `cca_rl_train` after completing significant tasks
- Start with `q_learning` for quick patterns
- Switch to `dqn` for complex pattern recognition
- Use `ppo` for stable long-term learning

### Token Efficiency
- Monitor `cca_tokens_metrics` regularly
- Apply compression strategies based on recommendations
- Use `history` compression for long conversations

---

## Example: Full Project Analysis

```bash
# Using CCA for comprehensive project analysis
cca_task description="
Analyze project codebase:
1. Backend - Architecture patterns, code quality
2. Frontend - Component structure, state management
3. Database - Schema design, query optimization
4. Security - Vulnerability scan, auth review
" priority="high"
```

See the main [README](../README.md) for more information about CCA.
