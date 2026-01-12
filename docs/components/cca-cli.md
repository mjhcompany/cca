# cca-cli

Command-line interface for CCA debugging and testing.

## Overview

The `cca-cli` crate provides a command-line interface for CCA. Primary usage is through the Command Center (Claude Code with CCA plugin), but the CLI is useful for debugging and testing.

## Commands

```
cca - Claude Code Agentic CLI

USAGE:
    cca [OPTIONS] <COMMAND>

OPTIONS:
    -v, --verbose    Enable verbose output
    -h, --help       Print help
    -V, --version    Print version

COMMANDS:
    daemon    Manage the CCA daemon
    agent     Manage agents
    task      Task operations
    memory    Memory operations
    config    Configuration management
    status    Show system status
```

## Daemon Commands

```
cca daemon <COMMAND>

COMMANDS:
    start     Start the CCA daemon
    stop      Stop the CCA daemon
    status    Show daemon status
    logs      View daemon logs
```

### Examples

```bash
# Start daemon in foreground
cca daemon start

# Start daemon in background
cca daemon start --background

# Check daemon status
cca daemon status

# View logs
cca daemon logs
cca daemon logs --follow
cca daemon logs --lines 100
```

## Agent Commands

```
cca agent <COMMAND>

COMMANDS:
    spawn     Spawn a new agent
    stop      Stop an agent
    list      List all agents
    attach    Attach to agent PTY
    send      Send message to agent
    logs      View agent logs
```

### Examples

```bash
# List all agents
cca agent list

# Spawn agents
cca agent spawn coordinator
cca agent spawn frontend
cca agent spawn backend

# Send message to agent
cca agent send <agent-id> "Implement authentication"

# Attach to agent (manual intervention)
cca agent attach <agent-id>

# Stop agent
cca agent stop <agent-id>
```

### Agent Roles

| Role | Description |
|------|-------------|
| `coordinator` | Routes tasks to specialists |
| `frontend` | Frontend development |
| `backend` | Backend development |
| `dba` | Database administration |
| `devops` | Infrastructure/deployment |
| `security` | Security review |
| `qa` | Quality assurance |

## Task Commands

```
cca task <COMMAND>

COMMANDS:
    create    Create a new task
    status    Check task status
    list      List recent tasks
    cancel    Cancel a task
```

### Examples

```bash
# Create task
cca task create "Add user authentication"
cca task create "Fix login bug" --priority high

# Check status
cca task status <task-id>

# List tasks
cca task list
cca task list --limit 20
cca task list --status pending
```

### Task Priorities

- `low`
- `normal` (default)
- `high`
- `critical`

## Memory Commands

```
cca memory <COMMAND>

COMMANDS:
    store     Store a pattern
    search    Search patterns
    stats     Show memory statistics
    export    Export patterns to file
    import    Import patterns from file
```

### Examples

```bash
# Search patterns
cca memory search "authentication"
cca memory search "error handling" --limit 5

# Store pattern
cca memory store "Pattern content..."

# View statistics
cca memory stats

# Export/import
cca memory export patterns.json
cca memory import patterns.json
```

## Config Commands

```
cca config <COMMAND>

COMMANDS:
    show      Show current configuration
    set       Set a configuration value
    init      Initialize configuration file
```

### Examples

```bash
# Show config
cca config show

# Set values
cca config set daemon.max_agents 20
cca config set redis.url "redis://localhost:6380"

# Initialize new config
cca config init
cca config init --path /custom/path/cca.toml
```

## Status Command

```bash
# Show system status
cca status
```

Output:
```
CCA Status
==========
Daemon: running
  Version: 0.1.0
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

## Global Options

### Verbose Mode

```bash
cca -v daemon start
cca --verbose agent list
```

Enables debug logging for troubleshooting.

### Environment Variables

```bash
# Override daemon URL
CCA_DAEMON_URL=http://localhost:9200 cca status

# Custom config file
CCA_CONFIG=/path/to/cca.toml cca daemon start
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |
| 3 | Connection error |
| 4 | Not found |

## Configuration

The CLI reads configuration from:
1. `CCA_CONFIG` environment variable
2. `./cca.toml`
3. `~/.config/cca/cca.toml`

## Usage with Claude Code

The CLI is primarily for debugging. In normal operation, use Claude Code with the CCA plugin:

```
User (in Claude Code): "Add authentication to the API"

→ CCA plugin calls cca_task tool
→ Task routed to Coordinator
→ Coordinator delegates to specialists
→ Results returned to user
```

The CLI provides equivalent functionality for testing:

```bash
# Equivalent to cca_task tool
cca task create "Add authentication to the API"

# Equivalent to cca_agents tool
cca agent list

# Equivalent to cca_status tool
cca status
```

## Dependencies

- `clap` - Argument parsing
- `reqwest` - HTTP client
- `tokio` - Async runtime
- `tracing` - Logging
