<p align="center">
  <h1 align="center">CCA - Claude Code Agentic</h1>
  <p align="center">
    <strong>A next-generation multi-agent orchestration system for Claude Code</strong>
  </p>
  <p align="center">
    <a href="#features">Features</a> •
    <a href="#quick-start">Quick Start</a> •
    <a href="#documentation">Documentation</a> •
    <a href="#architecture">Architecture</a> •
    <a href="#contributing">Contributing</a>
  </p>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square&logo=rust" alt="Rust 1.75+">
  <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License: MIT">
  <img src="https://img.shields.io/badge/status-alpha-yellow?style=flat-square" alt="Status: Alpha">
  <img src="https://img.shields.io/badge/PRs-welcome-brightgreen?style=flat-square" alt="PRs Welcome">
</p>

---

## Overview

CCA enables orchestration of multiple **independent Claude Code instances** through a single **Command Center** architecture. Unlike simulated agents, CCA spawns real Claude Code processes that collaborate on complex tasks.

```mermaid
graph TB
    subgraph CC["Command Center (CC)"]
        direction TB
        User[User's Primary Claude Code Instance]
        MCP[CCA Plugin - MCP]
        User --> MCP
    end

    subgraph Coord["Coordinator Agent"]
        CoordInstance[Persistent Claude Code Instance]
    end

    subgraph Specialists["Specialist Agents"]
        FE[Frontend Agent]
        BE[Backend Agent]
        SEC[Security Agent]
    end

    CC -->|Tasks| Coord
    Coord -->|Delegate| FE
    Coord -->|Delegate| BE
    Coord -->|Delegate| SEC
```

## Features

| Feature | Description |
|---------|-------------|
| **Multi-Agent Orchestration** | Spawn and coordinate multiple Claude Code instances |
| **PostgreSQL + pgvector** | Enterprise-grade persistence with vector similarity search |
| **Redis Integration** | Real-time session state and pub/sub messaging |
| **MCP Protocol** | Standard Claude Code plugin integration |
| **ACP WebSocket** | Real-time agent-to-agent communication |
| **Reinforcement Learning** | Intelligent task routing that learns from experience |
| **Token Efficiency** | Context compression and token usage optimization |
| **ReasoningBank** | Pattern storage and retrieval for learned solutions |

## Quick Start

### Prerequisites

- **Rust 1.75+** - [Install Rust](https://rustup.rs/)
- **Docker & Docker Compose** - [Install Docker](https://docs.docker.com/get-docker/)
- **Claude Code CLI** - The `claude` command must be available

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/cca.git
cd cca

# Start infrastructure (PostgreSQL + Redis)
docker-compose up -d

# Build the project
cargo build --release
```

### Running

```bash
# Start the CCA daemon
./target/release/ccad
```

The daemon starts on `http://127.0.0.1:9200` with ACP WebSocket on port `9100`.

### Claude Code Integration

Add CCA to your Claude Code MCP configuration:

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

### Usage

Once configured, use CCA tools directly in Claude Code:

| Tool | Description |
|------|-------------|
| `cca_task` | Send tasks to the Coordinator agent |
| `cca_status` | Check daemon and task status |
| `cca_agents` | List running agents |
| `cca_activity` | View agent activity |
| `cca_memory` | Query the ReasoningBank |
| `cca_broadcast` | Send messages to all agents |
| `cca_rl_status` | View RL engine statistics |
| `cca_tokens_analyze` | Analyze token usage |

## Documentation

Comprehensive documentation is available in the [`docs/`](./docs/) folder:

| Document | Description |
|----------|-------------|
| [**README**](./docs/README.md) | Documentation index and quick start |
| [**Architecture**](./docs/architecture.md) | System architecture with Mermaid diagrams |
| [**API Reference**](./docs/api-reference.md) | Complete HTTP API documentation |
| [**Deployment**](./docs/deployment.md) | Development and production deployment guide |
| [**Configuration**](./docs/configuration.md) | All configuration options |
| [**Data Flow**](./docs/data-flow.md) | Data flow diagrams |
| [**Protocols**](./docs/protocols.md) | MCP, ACP, and pub/sub protocols |

### Component Documentation

| Crate | Description |
|-------|-------------|
| [cca-core](./docs/components/cca-core.md) | Core types, traits, and shared functionality |
| [cca-daemon](./docs/components/cca-daemon.md) | Main daemon orchestration service |
| [cca-acp](./docs/components/cca-acp.md) | Agent Communication Protocol (WebSocket) |
| [cca-mcp](./docs/components/cca-mcp.md) | Model Context Protocol server |
| [cca-rl](./docs/components/cca-rl.md) | Reinforcement Learning engine |
| [cca-cli](./docs/components/cca-cli.md) | Command-line interface |

## Architecture

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
| `docker-compose.yml` | Infrastructure setup |

### Agent Roles

| Role | Description |
|------|-------------|
| **Coordinator** | Routes tasks to specialists, aggregates results |
| **Frontend** | Frontend/UI development specialist |
| **Backend** | Backend/API development specialist |
| **DBA** | Database administration specialist |
| **DevOps** | Infrastructure/deployment specialist |
| **Security** | Security review specialist |
| **QA** | Testing and quality assurance |

### Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 1.75+ |
| HTTP Server | Axum |
| WebSocket | Tokio-Tungstenite |
| Database | PostgreSQL 16 + pgvector |
| Cache/Pub-Sub | Redis 7 |
| PTY Management | portable-pty |
| Serialization | Serde |

## CLI Reference

```bash
# Daemon management
cca daemon start              # Start the daemon
cca daemon stop               # Stop the daemon
cca daemon status             # Show daemon status

# Agent management
cca agent spawn frontend      # Spawn a frontend agent
cca agent list                # List all agents
cca agent stop <id>           # Stop an agent

# Task management
cca task create "Add auth"    # Create a new task
cca task status <id>          # Check task status
cca task list                 # List recent tasks

# Memory operations
cca memory search "auth"      # Search patterns
cca memory stats              # Show memory statistics

# Configuration
cca config show               # Show current configuration
cca config init               # Initialize config file
```

## Configuration

Create a `cca.toml` file or use environment variables:

```toml
[daemon]
bind_address = "127.0.0.1:9200"
max_agents = 10
require_auth = false

[redis]
url = "redis://localhost:6380"

[postgres]
url = "postgres://cca:cca@localhost:5433/cca"

[learning]
enabled = true
default_algorithm = "q_learning"
```

Environment variables override config with `CCA__` prefix:

```bash
export CCA__DAEMON__BIND_ADDRESS="0.0.0.0:9200"
export CCA__REDIS__URL="redis://redis-host:6379"
export CCA__DAEMON__API_KEYS="key1,key2"
```

See [Configuration Guide](./docs/configuration.md) for all options.

## Development

### Building

```bash
cargo build           # Debug build
cargo build --release # Release build
```

### Testing

```bash
cargo test            # Run all tests
cargo test -p cca-core # Test specific crate
```

### Linting

```bash
cargo clippy          # Run linter
cargo fmt --check     # Check formatting
cargo fmt             # Format code
```

### Development Roadmap

See [WORKPLAN.md](./WORKPLAN.md) for the development roadmap and progress.

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Security

CCA uses granular permission control for Claude Code invocations:

**Permission Modes:**
- `allowlist` (default): Uses `--allowedTools` and `--disallowedTools` for granular control
- `sandbox`: Minimal read-only permissions, expects external sandboxing (container/VM)
- `dangerous`: Legacy mode using `--dangerously-skip-permissions` (NOT recommended)

**Configuration:**
```toml
# cca.toml
[agents.permissions]
mode = "allowlist"
allowed_tools = ["Read", "Glob", "Grep", "Write(src/**)", "Bash(git *)"]
denied_tools = ["Bash(rm -rf *)", "Bash(sudo *)", "Read(.env*)"]
allow_network = false
```

**Environment Variables (CLI workers):**
```bash
export CCA_PERMISSION_MODE=allowlist  # or "sandbox" or "dangerous"
export CCA_ALLOWED_TOOLS="Read,Glob,Grep,Write(src/**)"
export CCA_DENIED_TOOLS="Bash(rm *),Bash(sudo *)"
```

See [SECURITY_REVIEW.md](./SECURITY_REVIEW.md) for security considerations.

## License

This project is licensed under the MIT License - see the [LICENSE](./LICENSE) file for details.

---

<p align="center">
  Built with Rust and Claude Code
</p>
