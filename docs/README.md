# CCA Documentation

**CCA (Claude Code Agentic)** is a next-generation multi-agent orchestration system for Claude Code, written in Rust.

## Overview

CCA enables orchestration of multiple Claude Code instances through a single **Command Center** architecture. It combines:

- **True independent Claude Code instances** (not simulated agents)
- **PostgreSQL + pgvector** for enterprise-grade persistence
- **Redis** for real-time session state and pub/sub messaging
- **MCP/ACP protocols** for standardized agent communication
- **Reinforcement learning** for task optimization

## Documentation Index

### Architecture

- [**Architecture Overview**](./architecture.md) - System architecture with Mermaid diagrams
- [**Data Flow**](./data-flow.md) - How data moves through the system
- [**Communication Protocols**](./protocols.md) - ACP and MCP protocol details

### Components

- [**cca-core**](./components/cca-core.md) - Core types, traits, and shared functionality
- [**cca-daemon**](./components/cca-daemon.md) - Main daemon orchestration service
- [**cca-acp**](./components/cca-acp.md) - Agent Communication Protocol (WebSocket)
- [**cca-mcp**](./components/cca-mcp.md) - Model Context Protocol server
- [**cca-rl**](./components/cca-rl.md) - Reinforcement Learning engine
- [**cca-cli**](./components/cca-cli.md) - Command-line interface

### Guides

- [**API Reference**](./api-reference.md) - HTTP API and MCP tool documentation
- [**Deployment Guide**](./deployment.md) - Setup and deployment instructions
- [**Configuration**](./configuration.md) - Configuration options and environment variables

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

2. **Build the project:**
   ```bash
   cargo build --release
   ```

3. **Start the daemon:**
   ```bash
   ./target/release/ccad
   ```

4. **Configure Claude Code MCP:**
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

5. **Use CCA tools in Claude Code:**
   - `cca_task` - Send tasks to the Coordinator
   - `cca_status` - Check task/system status
   - `cca_agents` - List running agents
   - `cca_memory` - Query the ReasoningBank

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
├── docs/               # Documentation
└── docker-compose.yml  # Infrastructure setup
```

## Key Concepts

### Command Center Architecture

All user interaction flows through a single **Command Center (CC)** - a Claude Code instance with the CCA plugin installed. The Coordinator agent analyzes tasks and routes them to specialist agents.

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

### Communication Channels

- **ACP (Agent Communication Protocol)** - WebSocket-based real-time communication
- **MCP (Model Context Protocol)** - Tool invocation from Claude Code
- **Redis Pub/Sub** - Event broadcasting and coordination

## License

MIT
