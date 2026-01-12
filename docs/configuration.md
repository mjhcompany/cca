# Configuration

Complete guide to CCA configuration options.

## Configuration File

CCA uses TOML configuration files. The configuration is loaded in the following order:

1. `CCA_CONFIG` environment variable (if set)
2. `./cca.toml` (current directory)
3. `~/.config/cca/cca.toml` (user config)

Environment variables can override any setting using the `CCA__` prefix with double underscores as separators.

## Full Configuration Example

```toml
# CCA Configuration File

[daemon]
# Address to bind the HTTP API server
bind_address = "127.0.0.1:9200"

# Logging level: debug, info, warn, error
log_level = "info"

# Maximum number of concurrent agents
max_agents = 10

# API authentication (set keys via environment for security)
# api_keys = ["key1", "key2"]  # Or use CCA__DAEMON__API_KEYS env var
require_auth = false

[redis]
# Redis connection URL
url = "redis://localhost:6380"

# Connection pool size
pool_size = 10

# Context cache TTL in seconds
context_ttl_seconds = 3600

[postgres]
# PostgreSQL connection URL
url = "postgres://cca:cca@localhost:5433/cca"

# Connection pool size
pool_size = 10

# Maximum connections
max_connections = 20

[agents]
# Default task timeout in seconds
default_timeout_seconds = 300

# Enable context compression
context_compression = true

# Token budget per task
token_budget_per_task = 50000

# Path to Claude Code binary (default: "claude" in PATH)
claude_path = "claude"

[acp]
# WebSocket server port for agent communication
websocket_port = 9100

# Reconnection interval in milliseconds
reconnect_interval_ms = 1000

# Maximum reconnection attempts
max_reconnect_attempts = 5

[mcp]
# Enable MCP server
enabled = true

# MCP server bind address (not currently used - MCP uses stdio)
bind_address = "127.0.0.1:9201"

[learning]
# Enable RL learning
enabled = true

# Default RL algorithm: q_learning, ppo, dqn
default_algorithm = "q_learning"

# Training batch size
training_batch_size = 32

# Training update interval in seconds
update_interval_seconds = 300
```

## Configuration Sections

### [daemon]

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `bind_address` | string | `"127.0.0.1:9200"` | HTTP API bind address |
| `log_level` | string | `"info"` | Logging level |
| `max_agents` | integer | `10` | Max concurrent agents |
| `api_keys` | array | `[]` | API keys for authentication |
| `require_auth` | boolean | `false` | Require authentication |

### [redis]

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `url` | string | `""` | Redis connection URL |
| `pool_size` | integer | `10` | Connection pool size |
| `context_ttl_seconds` | integer | `3600` | Context cache TTL |

**Note:** If `url` is empty, Redis features are disabled.

### [postgres]

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `url` | string | `""` | PostgreSQL connection URL |
| `pool_size` | integer | `10` | Connection pool size |
| `max_connections` | integer | `20` | Maximum connections |

**Note:** If `url` is empty, PostgreSQL features are disabled.

### [agents]

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `default_timeout_seconds` | integer | `300` | Task timeout |
| `context_compression` | boolean | `true` | Enable compression |
| `token_budget_per_task` | integer | `50000` | Token limit per task |
| `claude_path` | string | `"claude"` | Claude Code binary path |

### [acp]

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `websocket_port` | integer | `9100` | WebSocket server port |
| `reconnect_interval_ms` | integer | `1000` | Reconnection interval |
| `max_reconnect_attempts` | integer | `5` | Max reconnection attempts |

### [mcp]

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable MCP server |
| `bind_address` | string | `"127.0.0.1:9201"` | MCP bind address |

### [learning]

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable RL learning |
| `default_algorithm` | string | `"ppo"` | Default algorithm |
| `training_batch_size` | integer | `32` | Batch size |
| `update_interval_seconds` | integer | `300` | Update interval |

## Environment Variables

All configuration options can be set via environment variables using the `CCA__` prefix:

```bash
# Daemon settings
export CCA__DAEMON__BIND_ADDRESS="0.0.0.0:9200"
export CCA__DAEMON__LOG_LEVEL="debug"
export CCA__DAEMON__MAX_AGENTS="20"
export CCA__DAEMON__REQUIRE_AUTH="true"
export CCA__DAEMON__API_KEYS="key1,key2,key3"

# Redis settings
export CCA__REDIS__URL="redis://redis-host:6379"
export CCA__REDIS__POOL_SIZE="20"
export CCA__REDIS__CONTEXT_TTL_SECONDS="7200"

# PostgreSQL settings
export CCA__POSTGRES__URL="postgres://user:pass@pg-host:5432/cca"
export CCA__POSTGRES__POOL_SIZE="20"
export CCA__POSTGRES__MAX_CONNECTIONS="50"

# Agent settings
export CCA__AGENTS__DEFAULT_TIMEOUT_SECONDS="600"
export CCA__AGENTS__CONTEXT_COMPRESSION="true"
export CCA__AGENTS__TOKEN_BUDGET_PER_TASK="100000"
export CCA__AGENTS__CLAUDE_PATH="/usr/local/bin/claude"

# ACP settings
export CCA__ACP__WEBSOCKET_PORT="9100"
export CCA__ACP__RECONNECT_INTERVAL_MS="2000"
export CCA__ACP__MAX_RECONNECT_ATTEMPTS="10"

# Learning settings
export CCA__LEARNING__ENABLED="true"
export CCA__LEARNING__DEFAULT_ALGORITHM="q_learning"
export CCA__LEARNING__TRAINING_BATCH_SIZE="64"
```

## Configuration Precedence

1. **Environment variables** (highest priority)
2. **Config file**
3. **Default values** (lowest priority)

## Common Configurations

### Development

```toml
[daemon]
bind_address = "127.0.0.1:9200"
log_level = "debug"
max_agents = 5
require_auth = false

[redis]
url = "redis://localhost:6380"

[postgres]
url = "postgres://cca:cca@localhost:5433/cca"
```

### Production

```toml
[daemon]
bind_address = "0.0.0.0:9200"
log_level = "info"
max_agents = 20
require_auth = true
# api_keys set via environment

[redis]
url = "redis://redis-cluster:6379"
pool_size = 20

[postgres]
url = "postgres://cca:${POSTGRES_PASSWORD}@pg-primary:5432/cca"
pool_size = 20
max_connections = 50
```

### Minimal (No External Services)

```toml
[daemon]
bind_address = "127.0.0.1:9200"
log_level = "info"
max_agents = 5

# Leave redis and postgres urls empty to disable
[redis]
url = ""

[postgres]
url = ""
```

## Claude Code MCP Configuration

Add to Claude Code's MCP settings (`~/.config/claude-code/mcp_servers.json`):

```json
{
    "mcpServers": {
        "cca": {
            "command": "/path/to/cca-mcp",
            "args": ["--daemon-url", "http://127.0.0.1:9200"],
            "env": {
                "CCA_API_KEY": "your-api-key"
            }
        }
    }
}
```

## Security Best Practices

### API Keys

1. **Never commit API keys to version control**
2. **Use environment variables:**
   ```bash
   export CCA__DAEMON__API_KEYS="$(cat /secrets/api-keys)"
   ```
3. **Rotate keys regularly**

### Database Credentials

1. **Use environment variables for URLs:**
   ```bash
   export CCA__POSTGRES__URL="postgres://user:${DB_PASSWORD}@host:5432/cca"
   ```
2. **Use separate credentials for read/write operations**
3. **Enable SSL for database connections**

### Network

1. **Bind to localhost in development:**
   ```toml
   bind_address = "127.0.0.1:9200"
   ```
2. **Use reverse proxy in production for TLS**
3. **Configure firewall rules**

## Validation

The daemon validates configuration on startup:

```
2024-01-10T12:00:00Z  INFO Loading config from: /path/to/cca.toml
2024-01-10T12:00:00Z  WARN Redis URL not configured. Redis features will be disabled.
2024-01-10T12:00:00Z  WARN PostgreSQL URL not configured. PostgreSQL features will be disabled.
2024-01-10T12:00:00Z  WARN Authentication is required but no API keys configured.
```

## CLI Commands

```bash
# Show current configuration
cca config show

# Initialize new configuration file
cca config init
cca config init --path /custom/path/cca.toml

# Set configuration values
cca config set daemon.max_agents 20
cca config set redis.url "redis://localhost:6379"
```
