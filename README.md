# CCA - Claude Code Agentic

A next-generation multi-agent orchestration system for Claude Code, written in Rust.

## Overview

CCA enables orchestration of multiple Claude Code instances through a single Command Center. It combines:

- **True independent Claude Code instances** (not simulated agents)
- **PostgreSQL + pgvector** for enterprise-grade persistence
- **Redis** for real-time session state and pub/sub messaging
- **MCP/ACP protocols** for standardized agent communication
- **Reinforcement learning** for task optimization

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    COMMAND CENTER (CC)                       │
│              User's Primary Claude Code Instance             │
│                                                              │
│   ┌────────────────────────────────────────────────────┐    │
│   │                    CCA Plugin (MCP)                 │    │
│   └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    COORDINATOR AGENT                         │
│              Persistent Claude Code Instance                 │
└─────────────────────────────────────────────────────────────┘
                              │
            ┌─────────────────┼─────────────────┐
            ▼                 ▼                 ▼
     ┌───────────┐     ┌───────────┐     ┌───────────┐
     │ frontend  │     │  backend  │     │ security  │
     │   agent   │     │   agent   │     │   agent   │
     └───────────┘     └───────────┘     └───────────┘
```

## Quick Start

### Prerequisites

- Rust 1.75+
- Docker & Docker Compose
- Claude Code CLI (`claude`)

### Setup

1. **Start infrastructure:**
   ```bash
   docker-compose up -d
   ```
   This starts PostgreSQL (port 5433) and Redis (port 6380).

2. **Build the project:**
   ```bash
   cargo build --release
   ```

3. **Start the daemon:**
   ```bash
   ./target/release/ccad
   ```
   The daemon runs on `http://127.0.0.1:9200`.

4. **Configure Claude Code MCP:**

   Copy the MCP configuration to your Claude Code settings:
   ```bash
   cp .claude/mcp_servers.json ~/.config/claude-code/mcp_servers.json
   ```

   Or manually add to your MCP settings:
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

5. **Use CCA from Claude Code:**

   Once configured, you can use CCA tools directly:
   - `cca_task` - Send tasks to the Coordinator agent
   - `cca_status` - Check daemon and task status
   - `cca_agents` - List running agents
   - `cca_activity` - View agent activity
   - `cca_memory` - Query the ReasoningBank

## CLI Usage

```bash
# Daemon management
cca daemon start              # Start the daemon
cca daemon stop               # Stop the daemon
cca daemon status             # Show daemon status

# Agent management
cca agent spawn frontend      # Spawn a frontend agent
cca agent list                # List all agents
cca agent attach backend      # Attach to backend agent

# Task management
cca task create "Add auth"    # Create a new task
cca task status <id>          # Check task status

# Memory operations
cca memory search "auth"      # Search patterns
cca memory stats              # Show memory statistics
```

## Project Structure

```
cca/
├── crates/
│   ├── cca-core/       # Core types and traits
│   ├── cca-daemon/     # Main daemon (ccad)
│   ├── cca-cli/        # CLI tool (cca)
│   ├── cca-mcp/        # MCP server plugin
│   ├── cca-acp/        # Agent Client Protocol
│   └── cca-rl/         # Reinforcement Learning
├── agents/             # Agent CLAUDE.md files
├── migrations/         # Database migrations
└── docker-compose.yml  # Infrastructure setup
```

## Configuration

Copy `cca.toml.example` to `cca.toml` and adjust settings:

```toml
[daemon]
bind_address = "127.0.0.1:9200"
max_agents = 10

[redis]
url = "redis://localhost:6380"

[postgres]
url = "postgres://cca:cca@localhost:5433/cca"
```

Environment variables can override config values with `CCA__` prefix:
```bash
export CCA__DAEMON__BIND_ADDRESS="0.0.0.0:9200"
export CCA__REDIS__URL="redis://custom-redis:6379"
```

## Development

See [WORKPLAN.md](./WORKPLAN.md) for the development roadmap.

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Linting

```bash
cargo clippy
cargo fmt --check
```

## License

MIT
