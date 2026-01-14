# CCA User Guide

**Claude Code Agentic (CCA)** - Multi-Agent Orchestration System for Claude Code

Version: 0.3.0 | Rust 1.81+ Required | License: MIT

---

## Table of Contents

1. [Overview](#overview)
2. [Installation](#installation)
3. [Quick Start](#quick-start)
4. [**Tmux Workflow (Recommended)**](#tmux-workflow-recommended)
5. [CLI Commands](#cli-commands)
6. [MCP Tools for Claude Code](#mcp-tools-for-claude-code)
7. [Configuration](#configuration)
8. [Agent Roles](#agent-roles)
9. [Semantic Search & Embeddings](#semantic-search--embeddings)
10. [Code Indexing](#code-indexing)
11. [Reinforcement Learning](#reinforcement-learning)
12. [Token Efficiency](#token-efficiency)
13. [Security](#security)
14. [HTTP API Reference](#http-api-reference)
15. [Troubleshooting](#troubleshooting)
16. [Advanced Usage](#advanced-usage)
17. [Resources](#resources)

---

## Overview

CCA is a next-generation multi-agent orchestration system that enables coordination of multiple independent Claude Code instances through a Command Center architecture. Unlike simulated agents, CCA spawns **real Claude Code processes** that collaborate on complex tasks.

### Key Features

| Feature | Description |
|---------|-------------|
| **Multi-Agent Orchestration** | Spawn and coordinate specialized Claude Code workers |
| **Reinforcement Learning** | Intelligent task routing that learns from experience |
| **ReasoningBank** | Pattern storage and retrieval with semantic search |
| **Semantic Search** | Vector similarity search using pgvector and embeddings |
| **Code Indexing** | Index and search codebases semantically |
| **Real-time Communication** | WebSocket-based agent-to-agent messaging (ACP) |
| **Token Efficiency** | Context compression and usage optimization |
| **MCP Integration** | Standard Claude Code plugin interface |

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Command Center (CC)                       │
│              Your Claude Code + CCA MCP Plugin               │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                     CCA Daemon (ccad)                        │
│         Orchestration • Task Routing • RL Engine             │
└──────┬──────────────────┬──────────────────┬────────────────┘
       │                  │                  │
       ▼                  ▼                  ▼
┌────────────┐    ┌────────────┐    ┌────────────┐
│ Coordinator│    │  Frontend  │    │  Backend   │    ...
│   Agent    │    │   Agent    │    │   Agent    │
└────────────┘    └────────────┘    └────────────┘
```

### How It Works

1. **Command Center**: Your primary Claude Code instance with the CCA MCP plugin
2. **CCA Daemon**: Orchestrates agents, routes tasks, manages state
3. **Worker Agents**: Specialized Claude Code instances for specific tasks
4. **ReasoningBank**: Stores patterns and solutions for reuse
5. **RL Engine**: Learns optimal task routing from experience

---

## Installation

### Prerequisites

Before installing CCA, ensure you have:

```bash
# Rust toolchain (1.81 or later)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version  # Verify: rustc 1.81.0 or higher

# Docker and Docker Compose (for infrastructure)
docker --version
docker-compose --version

# Claude Code CLI
claude --version

# Ollama (optional, for semantic search)
ollama --version
```

### Build from Source

1. **Clone the repository:**

```bash
git clone https://github.com/your-org/cca.git
cd cca
```

2. **Start infrastructure services:**

```bash
docker-compose up -d
```

This starts:
- PostgreSQL 18 with pgvector extension (port 5433)
- Redis 7 (port 6380)

3. **Build the project:**

```bash
# Debug build
cargo build

# Release build (recommended for production)
cargo build --release
```

4. **Verify the build:**

```bash
./target/release/cca --help
./target/release/ccad --help
```

### Binary Installation

If pre-built binaries are available:

```bash
# Download the latest release
curl -L https://github.com/your-org/cca/releases/latest/download/cca-linux-x86_64.tar.gz | tar xz

# Move to your PATH
sudo mv cca ccad cca-mcp /usr/local/bin/

# Verify
cca --version
```

### Post-Installation Setup

1. **Initialize configuration:**

```bash
cca config init
```

2. **Start Ollama for semantic search (optional but recommended):**

```bash
# Pull the embedding model
ollama pull nomic-embed-text

# Verify Ollama is running
curl http://localhost:11434/api/tags
```

3. **Configure Claude Code MCP integration** (see [MCP Tools](#mcp-tools-for-claude-code))

---

## Quick Start

### 1. Start Infrastructure

```bash
cd /path/to/cca
docker-compose up -d

# Verify services are running
docker-compose ps
```

### 2. Start Ollama (Optional - for Semantic Search)

```bash
# Start Ollama if not running
ollama serve &

# Pull embedding model
ollama pull nomic-embed-text
```

### 3. Start the Daemon

```bash
# Start in background
cca daemon start

# Or start in foreground for debugging
cca daemon start --foreground

# Or run directly
./target/release/ccad
```

### 4. Verify Daemon Status

```bash
cca daemon status

# Or use curl
curl http://localhost:9200/health
```

### 5. Start Worker Agents

Open separate terminal windows for each worker:

```bash
# Terminal 1 - Coordinator
cca agent worker coordinator

# Terminal 2 - Backend developer
cca agent worker backend

# Terminal 3 - Frontend developer
cca agent worker frontend
```

### 6. Check Connected Agents

```bash
cca agent list
```

### 7. Create Your First Task

```bash
cca task create "Analyze the codebase structure and suggest improvements"
```

### 8. Monitor Progress

```bash
cca task list
cca task status <task-id>
```

---

## Tmux Workflow (Recommended)

The **recommended way** to use CCA is through a tmux session with Claude Code. This provides:
- Organized agent management in separate windows/panes
- Persistent sessions that survive disconnections
- Visual overview of all running agents
- Easy monitoring of agent activity

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                        tmux session: cca                         │
├─────────────────────────────────────────────────────────────────┤
│ Window 0: claude-code     │ Window 1: coordinator               │
│ ┌─────────────────────┐   │ ┌─────────────────────────────────┐ │
│ │ $ claude            │   │ │ cca agent worker coordinator    │ │
│ │ > Use cca_task to:  │   │ │ [Coordinator running...]        │ │
│ │   "Build feature X" │   │ └─────────────────────────────────┘ │
│ └─────────────────────┘   │                                     │
├───────────────────────────┼─────────────────────────────────────┤
│ Window 2: agents-1        │ Window 3: agents-2 (if needed)      │
│ ┌──────────┬──────────┐   │ ┌──────────┬──────────┐             │
│ │ backend  │ frontend │   │ │ devops   │ security │             │
│ ├──────────┼──────────┤   │ └──────────┴──────────┘             │
│ │ dba      │ qa       │   │                                     │
│ └──────────┴──────────┘   │                                     │
└───────────────────────────┴─────────────────────────────────────┘
```

### Step-by-Step Setup

#### 1. Create a tmux Session

```bash
# Create a new tmux session named "cca"
tmux new-session -s cca -n claude-code
```

#### 2. Start Infrastructure

```bash
# In the claude-code window, ensure infrastructure is running
cd /path/to/cca
docker-compose up -d

# Start the CCA daemon
cca daemon start
```

#### 3. Start Claude Code

```bash
# Still in the claude-code window
claude
```

Now you're in Claude Code with CCA MCP tools available.

#### 4. Use cca_task - Automatic Agent Spawning

When you run `cca_task`, the system will:
1. Create a **coordinator window** (if not exists)
2. Spawn specialist agents in **agent windows** with **up to 4 panes each**
3. Only create panes for agents actually needed (no empty shells)

**Example in Claude Code:**
```
Use cca_task to: "Build a REST API for user authentication with frontend login form"
```

This might spawn:
- Window "coordinator": Coordinator agent
- Window "agents-1": backend (pane 1), frontend (pane 2)
- Window "agents-2": security (pane 1) - if security review needed

### Manual Agent Spawning with tmux

If you prefer manual control, use these commands:

```bash
# Create coordinator in new window
tmux new-window -n "coordinator" "cca agent worker coordinator"

# Create agents window with multiple panes (up to 4)
tmux new-window -n "agents-1" "cca agent worker backend"
tmux split-window -h "cca agent worker frontend"
tmux split-window -v -t 0 "cca agent worker dba"
tmux split-window -v -t 1 "cca agent worker devops"

# Add more agents in a second window if needed
tmux new-window -n "agents-2" "cca agent worker security"
tmux split-window -h "cca agent worker qa"
```

### Tmux Key Bindings Reference

| Key | Action |
|-----|--------|
| `Ctrl+b c` | Create new window |
| `Ctrl+b n` | Next window |
| `Ctrl+b p` | Previous window |
| `Ctrl+b 0-9` | Switch to window N |
| `Ctrl+b %` | Split pane horizontally |
| `Ctrl+b "` | Split pane vertically |
| `Ctrl+b o` | Cycle through panes |
| `Ctrl+b d` | Detach from session |
| `Ctrl+b w` | Window overview |

### Reattaching to Sessions

```bash
# List all sessions
tmux ls

# Attach to the cca session
tmux attach -t cca

# If session exists, attach; otherwise create
tmux new-session -A -s cca
```

### Agent Window Layout Rules

CCA follows these rules when spawning agents in tmux:

| Rule | Description |
|------|-------------|
| **Coordinator isolated** | Always gets its own window |
| **Max 4 panes per window** | Agent windows have up to 4 panes |
| **No empty panes** | Only create panes for actual agents |
| **Sequential filling** | Fill current window before creating new one |

**Example: 6 agents spawn request**
```
Window 1: coordinator
Window 2: backend, frontend, dba, devops (4 panes)
Window 3: security, qa (2 panes)
```

### Complete Workflow Example

```bash
# 1. Start tmux session
tmux new-session -s cca -n claude-code

# 2. Start infrastructure and daemon
cd ~/code/cca
docker-compose up -d
cca daemon start

# 3. Start Claude Code
claude

# 4. In Claude Code, submit a task using MCP
# Claude will automatically spawn agents in tmux windows:
#   "Use cca_task to: Build a user dashboard with charts"
#
# This creates:
#   - Window "coordinator" with coordinator agent
#   - Window "agents-1" with backend + frontend agents

# 5. Monitor with tmux
# Ctrl+b n  -> see coordinator window
# Ctrl+b n  -> see agents window with panes

# 6. When done, stop everything
cca daemon stop
docker-compose down
tmux kill-session -t cca
```

### Tips for Effective tmux Usage

1. **Use window names** - Name windows descriptively for easy navigation
2. **Watch the coordinator** - Coordinator window shows task delegation
3. **Pane synchronization** - Use `Ctrl+b :setw synchronize-panes on` to send commands to all panes
4. **Save layouts** - Consider using tmuxinator or tmux-resurrect for persistent layouts
5. **Log output** - Use `Ctrl+b :` then `capture-pane -S -3000` to capture scrollback

---

## CLI Commands

### Daemon Management

| Command | Description |
|---------|-------------|
| `cca daemon start` | Start the daemon in background |
| `cca daemon start --foreground` | Start daemon in foreground (blocking) |
| `cca daemon stop` | Stop the running daemon |
| `cca daemon status` | Show daemon status, PID, and health |
| `cca daemon logs` | View last 50 log lines |
| `cca daemon logs -n <count>` | View last N log lines |
| `cca daemon logs -f` | Follow logs in real-time |

**Examples:**

```bash
# Start daemon and check status
cca daemon start
cca daemon status

# View recent logs
cca daemon logs -n 100

# Monitor logs in real-time
cca daemon logs -f
```

### Agent Management

| Command | Description |
|---------|-------------|
| `cca agent list` | List all connected worker agents |
| `cca agent worker <role>` | Run as a persistent agent worker |
| `cca agent spawn <role>` | Spawn a new agent (daemon-managed) |
| `cca agent stop <id>` | Disconnect a worker by ID or role |
| `cca agent send <id> "message"` | Send a message to a specific agent |
| `cca agent attach <id>` | Attach to agent PTY for debugging |
| `cca agent diag` | Run comprehensive system diagnostics |

**Available Roles:** `coordinator`, `frontend`, `backend`, `dba`, `devops`, `security`, `qa`

**Examples:**

```bash
# Start a backend worker
cca agent worker backend

# List all connected agents
cca agent list

# Run diagnostics
cca agent diag

# Stop a specific agent
cca agent stop backend

# Send a message to an agent
cca agent send <agent-id> "Please review the authentication module"
```

### Task Management

| Command | Description |
|---------|-------------|
| `cca task create "description"` | Create a new task |
| `cca task create "desc" -a <role>` | Create task for specific agent |
| `cca task create "desc" --priority high` | Create task with priority |
| `cca task list` | List recent tasks (default: 10) |
| `cca task list --limit <n>` | List last N tasks |
| `cca task list --status pending` | Filter by status |
| `cca task status <id>` | Check task status |
| `cca task cancel <id>` | Cancel a pending task |

**Task Priorities:** `low`, `normal` (default), `high`, `critical`

**Examples:**

```bash
# Create a general task
cca task create "Implement user authentication API"

# Create a high-priority task for the frontend agent
cca task create "Build a login form component" -a frontend --priority high

# Check task status
cca task status abc123

# List recent tasks
cca task list --limit 20
```

### Memory Management

| Command | Description |
|---------|-------------|
| `cca memory search "query"` | Search the ReasoningBank (uses semantic search if available) |
| `cca memory search "query" -l <n>` | Search with custom limit |
| `cca memory store "pattern"` | Store a new pattern |
| `cca memory store "pattern" -t <type>` | Store with pattern type |
| `cca memory stats` | Show memory statistics |
| `cca memory export <file>` | Export patterns to JSON file |
| `cca memory import <file>` | Import patterns from JSON file |

**Examples:**

```bash
# Search for authentication patterns (semantic search)
cca memory search "authentication"

# Store a useful pattern
cca memory store "Use JWT tokens with refresh rotation for secure auth"

# Export all patterns for backup
cca memory export backup-patterns.json

# View memory statistics
cca memory stats
```

### Configuration

| Command | Description |
|---------|-------------|
| `cca config show` | Display current configuration |
| `cca config init` | Create default cca.toml |
| `cca config init --force` | Overwrite existing config |
| `cca config init --path <path>` | Create config at specific path |
| `cca config set <key> <value>` | Set individual config values |

**Examples:**

```bash
# View current config
cca config show

# Create initial config
cca config init

# Update a setting
cca config set daemon.max_agents 20
cca config set redis.url "redis://localhost:6380"
```

### Status

```bash
# Quick system status check
cca status
```

**Output:**
```
CCA Status
==========
Daemon: running
  Version: 0.3.0
  Address: 127.0.0.1:9200
  Uptime: 2h 15m

Agents: 3 running
  coordinator: Ready
  backend: Busy (task: abc123)
  frontend: Ready

Redis: connected
  Pool size: 10
  Agents tracked: 3

PostgreSQL: connected
  Pool size: 20
  Patterns: 150

Tasks:
  Pending: 2
  Completed: 45
  Failed: 3
```

### Global Options

```bash
# Enable verbose/debug logging for any command
cca --verbose daemon status
cca -v agent list
```

---

## MCP Tools for Claude Code

CCA provides MCP (Model Context Protocol) tools that integrate directly with Claude Code, allowing you to orchestrate agents from within your Claude Code sessions.

### Setup

Add CCA to your Claude Code MCP configuration:

**File:** `~/.config/claude-code/mcp_servers.json`

```json
{
  "mcpServers": {
    "cca": {
      "command": "/path/to/cca/target/release/cca-mcp",
      "args": ["--daemon-url", "http://127.0.0.1:9200"],
      "env": {
        "CCA_API_KEY": "your-api-key-if-auth-enabled"
      }
    }
  }
}
```

### Available MCP Tools

#### Task Management

| Tool | Description | Parameters |
|------|-------------|------------|
| `cca_task` | Send a task to the Coordinator | `description` (required), `priority` (low/normal/high/critical) |
| `cca_status` | Check system or task status | `task_id` (optional) |
| `cca_activity` | Get current activity of all agents | none |
| `cca_agents` | List all running agents | none |
| `cca_workloads` | Get workload distribution | none |

**Usage in Claude Code:**

```
Use cca_task to: "Build a REST API for user management"
Use cca_status to check the system status
Use cca_agents to see connected workers
```

#### Memory & Learning

| Tool | Description | Parameters |
|------|-------------|------------|
| `cca_memory` | Query ReasoningBank for patterns | `query` (required), `limit` (default: 10) |
| `cca_rl_status` | Get RL engine statistics | none |
| `cca_rl_train` | Trigger RL training | none |
| `cca_rl_algorithm` | Set RL algorithm | `algorithm` (q_learning/dqn/ppo) |

**Usage in Claude Code:**

```
Use cca_memory to search for: "database connection patterns"
Use cca_rl_status to check learning progress
Use cca_rl_algorithm to set algorithm to "ppo"
```

#### System & Communication

| Tool | Description | Parameters |
|------|-------------|------------|
| `cca_acp_status` | Get WebSocket server status | none |
| `cca_broadcast` | Send message to all agents | `message` (required) |

#### Token Efficiency

| Tool | Description | Parameters |
|------|-------------|------------|
| `cca_tokens_analyze` | Analyze content for token usage | `content` (required), `agent_id` (optional) |
| `cca_tokens_compress` | Compress content | `content` (required), `strategies`, `target_reduction`, `agent_id` |
| `cca_tokens_metrics` | Get token efficiency metrics | none |
| `cca_tokens_recommendations` | Get optimization recommendations | none |

**Compression Strategies:**
- `code_comments` - Remove redundant comments
- `history` - Compress git history
- `summarize` - Abstract verbose sections
- `deduplicate` - Remove exact duplicates

#### Code Indexing

| Tool | Description | Parameters |
|------|-------------|------------|
| `cca_index_codebase` | Index a codebase for semantic search | `path` (required), `extensions` (optional), `exclude_patterns` (optional) |
| `cca_search_code` | Search indexed code semantically | `query` (required), `language` (optional), `limit` (default: 10) |

**Usage in Claude Code:**

```
Use cca_index_codebase to index: "/path/to/project"
Use cca_search_code to find: "authentication middleware"
```

---

## Configuration

### Configuration File Location

CCA looks for configuration in this order:
1. `CCA_CONFIG` environment variable
2. `./cca.toml` (current directory)
3. `~/.config/cca/cca.toml` (user home)

### Full Configuration Reference

```toml
# =============================================================================
# CCA Configuration File
# =============================================================================

[daemon]
bind_address = "127.0.0.1:9200"    # HTTP API bind address
log_level = "info"                  # debug, info, warn, error
max_agents = 10                     # Maximum concurrent agents
require_auth = false                # Enable API key authentication
# api_keys = ["key1", "key2"]       # API keys (prefer env vars)

[redis]
url = "redis://localhost:6380"      # Redis connection URL (empty = disabled)
pool_size = 10                      # Connection pool size
context_ttl_seconds = 3600          # Cache TTL

[postgres]
url = "postgres://cca:cca@localhost:5433/cca"
pool_size = 10
max_connections = 20

[agents]
default_timeout_seconds = 300       # Task timeout
context_compression = true          # Enable compression
token_budget_per_task = 50000       # Token limit per task
claude_path = "claude"              # Path to Claude Code binary

[acp]
websocket_port = 9100               # WebSocket server port
reconnect_interval_ms = 1000        # Reconnection interval
max_reconnect_attempts = 5          # Max retries

[mcp]
enabled = true                      # Enable MCP server
bind_address = "127.0.0.1:9201"     # MCP bind address

[learning]
enabled = true                      # Enable RL engine
default_algorithm = "q_learning"    # q_learning, ppo, dqn
training_batch_size = 32            # Batch size for training
update_interval_seconds = 300       # Training interval

[agents.permissions]
mode = "allowlist"                  # allowlist, sandbox, dangerous
allowed_tools = ["Read", "Glob", "Grep"]
denied_tools = ["Bash(rm -rf *)"]
allow_network = false
working_dir = ""                    # Working directory restriction
```

### Environment Variables

All settings can be overridden via environment variables with `CCA__` prefix:

```bash
# Daemon settings
export CCA__DAEMON__BIND_ADDRESS="0.0.0.0:9200"
export CCA__DAEMON__MAX_AGENTS="20"
export CCA__DAEMON__REQUIRE_AUTH="true"
export CCA__DAEMON__API_KEYS="key1,key2,key3"

# Database connections
export CCA__REDIS__URL="redis://redis-host:6379"
export CCA__POSTGRES__URL="postgres://user:pass@pg:5432/cca"

# Agent settings
export CCA__AGENTS__CLAUDE_PATH="/usr/local/bin/claude"
export CCA__AGENTS__TOKEN_BUDGET_PER_TASK="100000"

# Security permissions
export CCA__AGENTS__PERMISSIONS__MODE="allowlist"
export CCA__AGENTS__PERMISSIONS__ALLOWED_TOOLS="Read,Glob,Grep,Write(src/**)"
```

### Configuration Precedence

1. **Environment variables** (highest priority)
2. **Config file**
3. **Default values** (lowest priority)

---

## Agent Roles

CCA supports specialized agent roles, each with specific capabilities:

| Role | Purpose | Typical Tools |
|------|---------|---------------|
| **coordinator** | Routes tasks to specialists; aggregates results | Read-only, JSON output |
| **frontend** | Frontend/UI development | Full development tools |
| **backend** | Backend/API development | Full development tools |
| **dba** | Database administration | Database tools, migrations |
| **devops** | Infrastructure/deployment | Bash, Docker, deployment |
| **security** | Security review and hardening | Read, Grep, analysis |
| **qa** | Testing and quality assurance | Testing frameworks |

### Starting Workers by Role

```bash
# Start specific role workers
cca agent worker coordinator
cca agent worker backend
cca agent worker frontend
cca agent worker dba
cca agent worker devops
cca agent worker security
cca agent worker qa
```

### Role-Specific Permission Overrides

```toml
[agents.permissions.role_overrides.coordinator]
mode = "sandbox"
allowed_tools = ["Read", "Glob", "Grep"]

[agents.permissions.role_overrides.backend]
mode = "allowlist"
allowed_tools = ["Read", "Glob", "Grep", "Write(src/**)", "Bash(cargo *)"]
denied_tools = ["Bash(cargo publish)"]

[agents.permissions.role_overrides.dba]
mode = "allowlist"
allowed_tools = ["Read", "Glob", "Grep", "Bash(psql *)"]
denied_tools = ["Bash(psql * DROP *)"]
```

---

## Semantic Search & Embeddings

CCA supports semantic search for the ReasoningBank, enabling intelligent pattern retrieval based on meaning rather than exact text matching.

### Prerequisites

1. **PostgreSQL with pgvector extension** (included in docker-compose.yml)
2. **Ollama with nomic-embed-text model**

### Setup

```bash
# Start Ollama
ollama serve &

# Pull the embedding model
ollama pull nomic-embed-text

# Verify the model is available
ollama list
```

### How Semantic Search Works

1. **Query Processing**: When you search, your query is converted to a 768-dimensional vector using the `nomic-embed-text` model
2. **Vector Similarity**: pgvector performs cosine similarity search against pattern embeddings
3. **Filtering**: Results are filtered with a minimum similarity threshold of 30%
4. **Ranking**: Results are ordered by similarity score (highest first)

### Search Types

| Search Type | When Used | Description |
|-------------|-----------|-------------|
| **Semantic** | Ollama available | Vector similarity search, finds conceptually related patterns |
| **Text** | Fallback | Case-insensitive substring matching (PostgreSQL ILIKE) |

### Usage

**CLI:**
```bash
# Semantic search for patterns
cca memory search "how to implement authentication"

# The response includes search_type
{
  "patterns": [...],
  "search_type": "semantic"  # or "text" if fallback
}
```

**MCP Tool:**
```
Use cca_memory to search for: "database connection pooling best practices"
```

### Backfilling Embeddings

If you have existing patterns without embeddings:

```bash
# Via API - processes 10 patterns at a time
curl -X POST http://localhost:9200/api/v1/memory/backfill-embeddings

# Repeat until remaining is 0
{
  "success": true,
  "processed": 10,
  "errors": 0,
  "remaining": 45
}
```

### Embedding Configuration

| Component | Requirement |
|-----------|-------------|
| PostgreSQL | pgvector extension enabled |
| Embedding Model | Ollama with `nomic-embed-text:latest` |
| Vector Dimensions | 768 |
| Ollama URL | Default: `http://localhost:11434` |

---

## Code Indexing

CCA can index your codebase for semantic code search, enabling natural language queries to find relevant functions, classes, and methods.

### Indexing a Codebase

**MCP Tool:**
```
Use cca_index_codebase with path: "/path/to/your/project"
```

**Parameters:**
| Parameter | Description | Default |
|-----------|-------------|---------|
| `path` | Directory to index | Required |
| `extensions` | File extensions to include | Common code files |
| `exclude_patterns` | Glob patterns to exclude | `**/node_modules/**`, etc. |

**Example:**
```json
{
  "path": "/home/user/project",
  "extensions": [".rs", ".py", ".ts"],
  "exclude_patterns": ["**/target/**", "**/node_modules/**", "**/.git/**"]
}
```

### Searching Indexed Code

**MCP Tool:**
```
Use cca_search_code to find: "error handling middleware"
```

**Parameters:**
| Parameter | Description | Default |
|-----------|-------------|---------|
| `query` | Natural language search query | Required |
| `language` | Filter by programming language | All languages |
| `limit` | Maximum results | 10 |

**Example Results:**
```json
{
  "results": [
    {
      "file": "src/middleware/error.rs",
      "function": "handle_error",
      "language": "rust",
      "similarity": 0.89,
      "snippet": "pub async fn handle_error(...)"
    }
  ]
}
```

---

## Reinforcement Learning

CCA uses reinforcement learning to optimize task routing and agent selection over time.

### Available Algorithms

| Algorithm | Best For | Pros | Cons |
|-----------|----------|------|------|
| **Q-Learning** | Simple state spaces | Fast, interpretable | Limited scalability |
| **DQN** | Complex state spaces | Handles high dimensions | Requires tuning |
| **PPO** | Continuous actions | Stable training | Higher complexity |

### RL Engine Status

```bash
# Via CLI
cca status

# Via API
curl http://localhost:9200/api/v1/rl/stats
```

**Response:**
```json
{
  "algorithm": "q_learning",
  "total_steps": 1000,
  "total_rewards": 850.5,
  "average_reward": 0.85,
  "buffer_size": 500,
  "experience_count": 1000,
  "algorithms_available": ["q_learning", "ppo", "dqn"]
}
```

### Triggering Training

```bash
# Via API
curl -X POST http://localhost:9200/api/v1/rl/train
```

### Changing Algorithms

```bash
# Via API
curl -X POST http://localhost:9200/api/v1/rl/algorithm \
  -H "Content-Type: application/json" \
  -d '{"algorithm": "dqn"}'
```

### Reward Computation

The system computes rewards based on:

| Component | Contribution | Range |
|-----------|--------------|-------|
| Task Success/Failure | Base reward | +1.0 / -0.5 |
| Token Efficiency | Bonus | 0.0 - 0.2 |
| Speed | Bonus | 0.0 - 0.1 |
| **Total** | | -0.5 to +1.3 |

### Configuration

```toml
[learning]
enabled = true
default_algorithm = "q_learning"
training_batch_size = 32
update_interval_seconds = 300
```

---

## Token Efficiency

CCA provides tools for analyzing and optimizing token usage across agents.

### Analyzing Token Usage

```bash
# Via API
curl -X POST http://localhost:9200/api/v1/tokens/analyze \
  -H "Content-Type: application/json" \
  -d '{"content": "Your content here..."}'
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

### Compressing Content

```bash
# Via API
curl -X POST http://localhost:9200/api/v1/tokens/compress \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Your content here...",
    "target_reduction": 0.3,
    "strategies": ["code_comments", "deduplicate"]
  }'
```

### Compression Strategies

| Strategy | Description |
|----------|-------------|
| `code_comments` | Remove redundant comments from code |
| `history` | Compress git history output |
| `summarize` | Abstract verbose sections |
| `deduplicate` | Remove exact duplicate content |

### Getting Recommendations

```bash
curl http://localhost:9200/api/v1/tokens/recommendations
```

**Response:**
```json
{
  "recommendations": [
    {
      "agent_id": "agent-001",
      "category": "high_context",
      "severity": "warning",
      "message": "Agent has high context size, consider compression",
      "potential_savings": 5000
    }
  ],
  "total_potential_savings": 5000
}
```

---

## Security

### Permission Modes

CCA supports three permission modes for agent security:

| Mode | Security Level | Description |
|------|---------------|-------------|
| **allowlist** | High | Granular control via `--allowedTools` and `--disallowedTools` |
| **sandbox** | Medium | Minimal read-only permissions |
| **dangerous** | None | **NOT RECOMMENDED** - Disables all permission checks |

#### Allowlist Mode (Recommended)

```toml
[agents.permissions]
mode = "allowlist"
allowed_tools = [
    "Read", "Glob", "Grep",
    "Write(src/**)", "Write(tests/**)",
    "Bash(git status)", "Bash(git diff*)"
]
denied_tools = [
    "Bash(rm -rf *)", "Bash(sudo *)",
    "Read(.env*)", "Write(.env*)"
]
allow_network = false
```

#### Sandbox Mode

Minimal read-only permissions for isolated environments:

```toml
[agents.permissions]
mode = "sandbox"
# Only Read, Glob, Grep allowed
```

#### Dangerous Mode (Not Recommended)

Full access - only use in fully isolated containers:

```toml
[agents.permissions]
mode = "dangerous"
# Uses --dangerously-skip-permissions
```

**Warning:** This mode:
- Disables ALL permission checks
- Allows arbitrary command execution
- Bypasses all safety prompts
- Creates severe security risks

### Tool Pattern Syntax

```
Simple:       "Read", "Glob", "Grep"
Patterns:     "Write(src/**)", "Bash(git *)"
Exclusions:   "Bash(rm -rf *)", "Read(.env*)"
```

### Network Restrictions

When `allow_network = false` (default), these commands are automatically blocked:
- `curl`, `wget`, `nc`, `netcat`

### API Authentication

Enable authentication in production:

```toml
[daemon]
require_auth = true
# Set API keys via environment variable
```

```bash
export CCA__DAEMON__API_KEYS="strong-random-key-1,strong-random-key-2"
```

---

## HTTP API Reference

CCA exposes a REST API for programmatic access.

### Health & Status

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (no auth required) |
| `/api/v1/status` | GET | System status |

### Agent Management

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/agents` | GET | List agents |
| `/api/v1/agents` | POST | Spawn agent |
| `/api/v1/activity` | GET | Agent activity |
| `/api/v1/workloads` | GET | Workload distribution |

### Task Management

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/tasks` | POST | Create task |
| `/api/v1/tasks` | GET | List tasks |
| `/api/v1/tasks/<id>` | GET | Get task details |
| `/api/v1/tasks/<id>/cancel` | POST | Cancel task |

### Memory (ReasoningBank)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/memory/search` | POST | Search patterns (semantic) |
| `/api/v1/memory/store` | POST | Store pattern |
| `/api/v1/memory/stats` | GET | Memory statistics |
| `/api/v1/memory/backfill-embeddings` | POST | Generate missing embeddings |

### Reinforcement Learning

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/rl/stats` | GET | RL engine status |
| `/api/v1/rl/train` | POST | Trigger training |
| `/api/v1/rl/algorithm` | POST | Set algorithm |
| `/api/v1/rl/params` | GET/POST | Get/set parameters |

### Token Efficiency

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/tokens/analyze` | POST | Analyze content |
| `/api/v1/tokens/compress` | POST | Compress content |
| `/api/v1/tokens/metrics` | GET | Get metrics |
| `/api/v1/tokens/recommendations` | GET | Get recommendations |

### Communication

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/acp/status` | GET | ACP WebSocket status |
| `/api/v1/broadcast` | POST | Broadcast to all agents |

### Infrastructure

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/redis/status` | GET | Redis status |
| `/api/v1/postgres/status` | GET | PostgreSQL status |

### Authentication

When `require_auth = true`, include API key in requests:

```bash
# Header option 1
curl -H "X-API-Key: your-api-key" http://localhost:9200/api/v1/status

# Header option 2
curl -H "Authorization: Bearer your-api-key" http://localhost:9200/api/v1/status
```

### Input Limits

| Field | Max Size |
|-------|----------|
| Task description | 100 KB |
| Broadcast message | 10 KB |
| Token content | 1 MB |
| Memory query | 1 KB |

---

## Troubleshooting

### Run Diagnostics

```bash
cca agent diag
```

This checks:
- Daemon health
- WebSocket server (ACP)
- Redis connectivity
- PostgreSQL connectivity
- RL engine status
- Connected workers
- Recent tasks
- Workload distribution

### Common Issues

#### Daemon Won't Start

```bash
# Check if port is already in use
lsof -i :9200
lsof -i :9100

# Verify infrastructure is running
docker-compose ps

# Check logs for errors
cca daemon logs -n 50

# Try foreground mode for debugging
cca daemon start --foreground
```

#### Workers Can't Connect

```bash
# Verify daemon is running
cca daemon status

# Check ACP WebSocket port
lsof -i :9100

# Check daemon logs
cca daemon logs -f

# Verify network connectivity
curl http://localhost:9200/health
```

#### Tasks Not Completing

```bash
# Check agent activity
cca agent list
cca agent diag

# View recent tasks
cca task list

# Check specific task
cca task status <task-id>

# Check worker logs
cca daemon logs -f
```

#### Database Connection Issues

```bash
# Check PostgreSQL
docker-compose ps postgres
docker-compose logs postgres

# Test PostgreSQL connection
psql -h localhost -p 5433 -U cca -d cca -c "SELECT 1"

# Check Redis
docker-compose ps redis
docker-compose logs redis

# Test Redis connection
redis-cli -p 6380 ping
```

#### Semantic Search Not Working

```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# Verify model is available
ollama list | grep nomic-embed-text

# Check search type in response
cca memory search "test"
# Look for "search_type": "semantic" vs "text"

# Backfill embeddings if needed
curl -X POST http://localhost:9200/api/v1/memory/backfill-embeddings
```

#### Permission Denied Errors

```bash
# Check current permission mode
cca config show | grep -A 10 permissions

# Verify allowed tools include needed operations
# Update config if necessary

# For debugging, check daemon logs
cca daemon logs -f | grep -i permission
```

### Log Locations

| Log | Location |
|-----|----------|
| Daemon logs | `~/.local/share/cca/ccad.log` |
| PID file | `~/.local/run/cca/ccad.pid` |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |
| 3 | Connection error |
| 4 | Not found |

### Getting Help

```bash
# CLI help
cca --help
cca <command> --help

# Example
cca daemon --help
cca agent --help
cca task --help
cca memory --help
cca config --help
```

---

## Advanced Usage

### Running Multiple Workers

For complex projects, run multiple specialized workers:

```bash
# Terminal 1 - Start coordinator
cca agent worker coordinator

# Terminal 2-4 - Start specialists
cca agent worker backend
cca agent worker frontend
cca agent worker dba

# Terminal 5-6 - Start reviewers
cca agent worker security
cca agent worker qa
```

### Task Delegation Patterns

**Direct Assignment:**
```bash
cca task create "Implement user model" -a backend
```

**Coordinator Routing:**
```bash
# Let the coordinator decide which agent should handle it
cca task create "Add user authentication with OAuth2"
```

**Priority Tasks:**
```bash
cca task create "Fix critical security vulnerability" --priority critical
```

### Integration with CI/CD

```bash
#!/bin/bash
# Example CI/CD script

# Start daemon if not running
cca daemon status || cca daemon start

# Run code review
cca task create "Review PR changes for security issues" -a security --priority high

# Wait for completion
sleep 30
cca task list --status completed --limit 1
```

### Using the HTTP API Programmatically

**Python Example:**
```python
import requests

CCA_URL = "http://localhost:9200"
API_KEY = "your-api-key"

headers = {"X-API-Key": API_KEY}

# Create a task
response = requests.post(
    f"{CCA_URL}/api/v1/tasks",
    json={"description": "Implement feature X", "priority": "high"},
    headers=headers
)
task = response.json()

# Check status
response = requests.get(
    f"{CCA_URL}/api/v1/tasks/{task['task_id']}",
    headers=headers
)
print(response.json())
```

### Custom Agent Configurations

Create custom CLAUDE.md files for specialized agents:

```bash
# agents/custom-agent/CLAUDE.md
# Custom Agent Configuration

## Role
You are a specialized agent for handling data migrations.

## Capabilities
- Database schema analysis
- Migration script generation
- Data validation

## Constraints
- Always backup before migration
- Validate data integrity after changes
```

### Performance Tuning

**For High Throughput:**
```toml
[daemon]
max_agents = 20

[redis]
pool_size = 20

[postgres]
pool_size = 20
max_connections = 50

[agents]
default_timeout_seconds = 600
token_budget_per_task = 100000
```

**For Low Latency:**
```toml
[learning]
training_batch_size = 16
update_interval_seconds = 60

[acp]
reconnect_interval_ms = 500
max_reconnect_attempts = 10
```

---

## Resources

### Documentation

| Document | Description |
|----------|-------------|
| [Architecture](./architecture.md) | System architecture with diagrams |
| [API Reference](./api-reference.md) | Complete HTTP API documentation |
| [Configuration](./configuration.md) | All configuration options |
| [Deployment](./deployment.md) | Production deployment guide |
| [Security Hardening](./security-hardening.md) | Security best practices |
| [Data Flow](./data-flow.md) | Data flow diagrams |

### Component Documentation

| Component | Description |
|-----------|-------------|
| [cca-core](./components/cca-core.md) | Core types and traits |
| [cca-daemon](./components/cca-daemon.md) | Main orchestration service |
| [cca-mcp](./components/cca-mcp.md) | MCP server plugin |
| [cca-acp](./components/cca-acp.md) | Agent Communication Protocol |
| [cca-rl](./components/cca-rl.md) | Reinforcement Learning engine |
| [cca-cli](./components/cca-cli.md) | Command-line interface |

### Project Structure

| Path | Description |
|------|-------------|
| `crates/cca-core/` | Core types and traits |
| `crates/cca-daemon/` | Main daemon (ccad) |
| `crates/cca-cli/` | CLI tool (cca) |
| `crates/cca-mcp/` | MCP server plugin |
| `crates/cca-acp/` | Agent Communication Protocol |
| `crates/cca-rl/` | Reinforcement Learning |
| `agents/` | Agent CLAUDE.md files |
| `migrations/` | Database migrations |
| `docs/` | Documentation |

---

*Last Updated: January 2026 | Version 0.3.0*
