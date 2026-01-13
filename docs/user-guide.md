# CCA User Guide

**Claude Code Agentic (CCA)** - Multi-Agent Orchestration System for Claude Code

Version: 0.3.0 | Rust 1.75+ Required | License: MIT

---

## Table of Contents

1. [Overview](#overview)
2. [Installation](#installation)
3. [Quick Start](#quick-start)
4. [CLI Commands](#cli-commands)
5. [MCP Tools for Claude Code](#mcp-tools-for-claude-code)
6. [Configuration](#configuration)
7. [Agent Roles](#agent-roles)
8. [Security](#security)
9. [Troubleshooting](#troubleshooting)

---

## Overview

CCA is a next-generation multi-agent orchestration system that enables coordination of multiple independent Claude Code instances through a Command Center architecture. Key features include:

- **Multi-Agent Orchestration** - Spawn and coordinate specialized Claude Code workers
- **Reinforcement Learning** - Intelligent task routing that learns from experience
- **ReasoningBank** - Pattern storage and retrieval for learned solutions
- **Real-time Communication** - WebSocket-based agent-to-agent messaging (ACP)
- **Token Efficiency** - Context compression and usage optimization
- **MCP Integration** - Standard Claude Code plugin interface

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

---

## Installation

### Prerequisites

Before installing CCA, ensure you have:

```bash
# Rust toolchain (1.75 or later)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version  # Verify: rustc 1.75.0 or higher

# Docker and Docker Compose (for infrastructure)
docker --version
docker-compose --version

# Claude Code CLI
claude --version
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

This starts PostgreSQL (with pgvector) and Redis.

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
```

### Binary Installation

If pre-built binaries are available:

```bash
# Download the latest release
curl -L https://github.com/your-org/cca/releases/latest/download/cca-linux-x86_64.tar.gz | tar xz

# Move to your PATH
sudo mv cca cca-mcp /usr/local/bin/

# Verify
cca --version
```

### Post-Installation Setup

1. **Initialize configuration:**

```bash
cca config init
```

2. **Configure Claude Code MCP integration** (see [MCP Tools](#mcp-tools-for-claude-code))

---

## Quick Start

### 1. Start Infrastructure

```bash
cd /path/to/cca
docker-compose up -d
```

### 2. Start the Daemon

```bash
# Start in background
cca daemon start

# Or start in foreground for debugging
cca daemon start --foreground
```

### 3. Verify Daemon Status

```bash
cca daemon status
```

### 4. Start Worker Agents

Open separate terminal windows for each worker:

```bash
# Terminal 1 - Coordinator
cca agent worker coordinator

# Terminal 2 - Backend developer
cca agent worker backend

# Terminal 3 - Frontend developer
cca agent worker frontend
```

### 5. Check Connected Agents

```bash
cca agent list
```

### 6. Create Your First Task

```bash
cca task create "Analyze the codebase structure and suggest improvements"
```

### 7. Monitor Progress

```bash
cca task list
cca task status <task-id>
```

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
| `cca agent stop <id>` | Disconnect a worker by ID or role |
| `cca agent send <id> "message"` | Send a message to a specific agent |
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
```

### Task Management

| Command | Description |
|---------|-------------|
| `cca task create "description"` | Create a new task |
| `cca task create "desc" -a <role>` | Create task for specific agent |
| `cca task list` | List recent tasks (default: 10) |
| `cca task list --limit <n>` | List last N tasks |
| `cca task status <id>` | Check task status |
| `cca task cancel <id>` | Cancel a pending task |

**Examples:**

```bash
# Create a general task
cca task create "Implement user authentication API"

# Create a task for the frontend agent
cca task create "Build a login form component" -a frontend

# Check task status
cca task status abc123

# List recent tasks
cca task list --limit 20
```

### Memory Management

| Command | Description |
|---------|-------------|
| `cca memory search "query"` | Search the ReasoningBank |
| `cca memory search "query" -l <n>` | Search with custom limit |
| `cca memory store "pattern"` | Store a new pattern |
| `cca memory store "pattern" -t <type>` | Store with pattern type |
| `cca memory stats` | Show memory statistics |
| `cca memory export <file>` | Export patterns to JSON file |
| `cca memory import <file>` | Import patterns from JSON file |

**Examples:**

```bash
# Search for authentication patterns
cca memory search "authentication"

# Store a useful pattern
cca memory store "Use JWT tokens with refresh rotation for secure auth"

# Export all patterns for backup
cca memory export backup-patterns.json
```

### Configuration

| Command | Description |
|---------|-------------|
| `cca config show` | Display current configuration |
| `cca config init` | Create default cca.toml |
| `cca config init --force` | Overwrite existing config |
| `cca config set <key> <value>` | Set individual config values |

**Examples:**

```bash
# View current config
cca config show

# Create initial config
cca config init

# Update a setting
cca config set daemon.max_agents 20
```

### Status

```bash
# Quick system status check
cca status
```

### Global Options

```bash
# Enable verbose/debug logging for any command
cca --verbose daemon status
cca --verbose agent list
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
| `cca_workloads` | Get workload distribution | none |

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

---

## Security

### Permission Modes

CCA supports three permission modes for agent security:

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

### Tool Pattern Syntax

```
Simple:       "Read", "Glob", "Grep"
Patterns:     "Write(src/**)", "Bash(git *)"
Exclusions:   "Bash(rm -rf *)", "Read(.env*)"
```

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

# Verify infrastructure is running
docker-compose ps

# Check logs for errors
cca daemon logs -n 50
```

#### Workers Can't Connect

```bash
# Verify daemon is running
cca daemon status

# Check ACP WebSocket port
lsof -i :9100

# Check daemon logs
cca daemon logs -f
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
```

#### Database Connection Issues

```bash
# Check PostgreSQL
docker-compose ps postgres
docker-compose logs postgres

# Check Redis
docker-compose ps redis
docker-compose logs redis
```

### Log Locations

- **Daemon logs:** `~/.local/share/cca/ccad.log`
- **PID file:** `~/.local/run/cca/ccad.pid`

### Getting Help

```bash
# CLI help
cca --help
cca <command> --help

# Example
cca daemon --help
cca agent --help
cca task --help
```

---

## HTTP API Reference

CCA exposes a REST API for programmatic access:

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (no auth) |
| `/api/v1/status` | GET | System status |
| `/api/v1/agents` | GET | List agents |
| `/api/v1/activity` | GET | Agent activity |
| `/api/v1/workloads` | GET | Workload distribution |
| `/api/v1/tasks` | POST | Create task |
| `/api/v1/tasks` | GET | List tasks |
| `/api/v1/tasks/<id>` | GET | Get task details |
| `/api/v1/tasks/<id>/cancel` | POST | Cancel task |
| `/api/v1/memory/search` | GET | Search patterns |
| `/api/v1/memory/store` | POST | Store pattern |
| `/api/v1/memory/stats` | GET | Memory statistics |
| `/api/v1/rl/stats` | GET | RL engine status |
| `/api/v1/rl/train` | POST | Trigger training |
| `/api/v1/acp/status` | GET | ACP server status |

---

## Resources

- **Source Code:** Check the `crates/` directory for implementation details
- **Agent Configs:** Role-specific configurations in `agents/` directory
- **Database Migrations:** Schema files in `migrations/` directory

---

*Last Updated: January 2026 | Version 0.3.0*
