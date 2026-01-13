# Deployment Guide

Complete guide for deploying CCA in development and production environments.

## Prerequisites

- **Rust 1.75+** - For building from source
- **Docker & Docker Compose** - For infrastructure services
- **Claude Code CLI** - The `claude` command must be available in PATH

## Quick Start (Development)

### 1. Clone and Build

```bash
git clone https://github.com/your-org/cca.git
cd cca
cargo build --release
```

### 2. Start Infrastructure

```bash
docker-compose up -d
```

This starts:
- PostgreSQL on port `5433`
- Redis on port `6380`

### 3. Configure

Copy the example configuration:

```bash
cp cca.toml.example cca.toml
```

Edit `cca.toml` as needed:

```toml
[daemon]
bind_address = "127.0.0.1:9200"
log_level = "info"
max_agents = 10

[redis]
url = "redis://localhost:6380"

[postgres]
url = "postgres://cca:cca@localhost:5433/cca"
```

### 4. Start the Daemon

```bash
./target/release/ccad
```

### 5. Configure Claude Code

Add to Claude Code MCP settings:

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

## Docker Compose Details

### docker-compose.yml

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_USER: cca
      POSTGRES_PASSWORD: cca
      POSTGRES_DB: cca
    ports:
      - "5433:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d

  redis:
    image: redis:7-alpine
    ports:
      - "6380:6379"
    volumes:
      - redis_data:/data

volumes:
  postgres_data:
  redis_data:
```

### Database Initialization

PostgreSQL initializes automatically with migrations in `/docker-entrypoint-initdb.d`.

To run migrations manually:

```bash
# Using sqlx-cli
cargo install sqlx-cli
sqlx migrate run
```

## Production Deployment

### Architecture

```mermaid
graph TB
    subgraph "Load Balancer"
        LB[HAProxy/Nginx]
    end

    subgraph "Application Tier"
        D1[CCA Daemon 1]
        D2[CCA Daemon 2]
    end

    subgraph "Data Tier"
        PG[(PostgreSQL<br/>Primary)]
        PGR[(PostgreSQL<br/>Replica)]
        RD[(Redis<br/>Cluster)]
    end

    LB --> D1
    LB --> D2
    D1 --> PG
    D2 --> PG
    PG --> PGR
    D1 --> RD
    D2 --> RD
```

### Production Configuration

```toml
[daemon]
bind_address = "0.0.0.0:9200"
log_level = "info"
max_agents = 20
require_auth = true
# API keys loaded from environment

[redis]
url = "${REDIS_URL}"
pool_size = 20
context_ttl_seconds = 7200

[postgres]
url = "${DATABASE_URL}"
pool_size = 20
max_connections = 50

[agents]
default_timeout_seconds = 600
context_compression = true
token_budget_per_task = 100000

[acp]
websocket_port = 9100
reconnect_interval_ms = 2000
max_reconnect_attempts = 10
```

### Environment Variables

```bash
# Required
export CCA__REDIS__URL="redis://redis-host:6379"
export CCA__POSTGRES__URL="postgres://user:pass@pg-host:5432/cca"

# Authentication
export CCA__DAEMON__API_KEYS="key1,key2,key3"
export CCA__DAEMON__REQUIRE_AUTH="true"

# Claude Code path (if not in PATH)
export CCA__AGENTS__CLAUDE_PATH="/usr/local/bin/claude"

# Optional overrides
export CCA__DAEMON__BIND_ADDRESS="0.0.0.0:9200"
export CCA__DAEMON__MAX_AGENTS="20"
```

### Systemd Service

Create `/etc/systemd/system/cca-daemon.service`:

```ini
[Unit]
Description=CCA Daemon
After=network.target postgresql.service redis.service

[Service]
Type=simple
User=cca
Group=cca
WorkingDirectory=/opt/cca
ExecStart=/opt/cca/bin/ccad
Restart=always
RestartSec=5
Environment="CCA_CONFIG=/etc/cca/cca.toml"
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl enable cca-daemon
sudo systemctl start cca-daemon
sudo systemctl status cca-daemon
```

### Docker Deployment

#### Dockerfile

```dockerfile
FROM rust:1.75-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/ccad /usr/local/bin/
COPY --from=builder /app/target/release/cca-mcp /usr/local/bin/
EXPOSE 9200 9100
CMD ["ccad"]
```

#### docker-compose.prod.yml

```yaml
version: '3.8'

services:
  cca-daemon:
    build: .
    ports:
      - "9200:9200"
      - "9100:9100"
    environment:
      - CCA__REDIS__URL=redis://redis:6379
      - CCA__POSTGRES__URL=postgres://cca:${POSTGRES_PASSWORD}@postgres:5432/cca
      - CCA__DAEMON__REQUIRE_AUTH=true
      - CCA__DAEMON__API_KEYS=${API_KEYS}
    depends_on:
      - postgres
      - redis
    restart: always

  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_USER: cca
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: cca
    volumes:
      - postgres_data:/var/lib/postgresql/data

  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes
    volumes:
      - redis_data:/data

volumes:
  postgres_data:
  redis_data:
```

## Security Considerations

### Authentication

1. **Enable authentication in production:**
   ```toml
   [daemon]
   require_auth = true
   ```

2. **Set API keys via environment:**
   ```bash
   export CCA__DAEMON__API_KEYS="strong-random-key-1,strong-random-key-2"
   ```

3. **Never commit API keys to version control**

### Network Security

1. **Bind to localhost in development:**
   ```toml
   bind_address = "127.0.0.1:9200"
   ```

2. **Use reverse proxy in production:**
   - TLS termination
   - Rate limiting
   - Request filtering

3. **Firewall rules:**
   - Only expose necessary ports
   - Restrict database access to application tier

### Agent Security

CCA uses granular permission control for Claude Code invocations (SEC-007).

> **See Also:** [Security Hardening Guide](./security-hardening.md) for comprehensive agent permission documentation.

**Permission Modes:**
- `allowlist` (default, **RECOMMENDED**): Granular control via `--allowedTools` and `--disallowedTools`
- `sandbox`: Minimal read-only permissions, for use with external sandboxing
- `dangerous`: **DO NOT USE** - Legacy mode that disables ALL permission checks

**Why Dangerous Mode is Dangerous:**

The `dangerous` mode passes `--dangerously-skip-permissions` to Claude Code, which completely disables all built-in safety features. This allows agents to:
- Read/write any file (including .env, credentials, secrets)
- Execute any command (including sudo, rm -rf, etc.)
- Access network without restrictions
- Modify system configurations

**Default Security Configuration:**
```toml
[agents.permissions]
mode = "allowlist"
allowed_tools = ["Read", "Glob", "Grep", "Write(src/**)", "Write(tests/**)", "Bash(git *)"]
denied_tools = ["Bash(rm -rf *)", "Bash(sudo *)", "Read(.env*)", "Write(.env*)"]
allow_network = false  # Blocks curl, wget, nc by default
```

**Additional Mitigations:**
- Run agents in sandboxed containers (defense-in-depth)
- Use least-privilege user accounts
- Monitor agent activity via logs
- Limit agent resource usage
- Enable authentication (`require_auth = true`)
- Use role-based permission overrides for least privilege

### Database Security

1. **Use strong passwords**
2. **Enable SSL for connections**
3. **Regular backups**
4. **Principle of least privilege for DB users**

## Monitoring

### Health Checks

```bash
# HTTP health check
curl http://localhost:9200/health

# Check all services
curl http://localhost:9200/api/v1/status
curl http://localhost:9200/api/v1/redis/status
curl http://localhost:9200/api/v1/postgres/status
```

### Prometheus Metrics

(Future feature - not yet implemented)

### Logging

Configure log level:

```toml
[daemon]
log_level = "info"  # debug, info, warn, error
```

View logs:

```bash
# Systemd
journalctl -u cca-daemon -f

# Docker
docker logs -f cca-daemon
```

## Scaling

### Horizontal Scaling

Multiple daemon instances can share Redis and PostgreSQL:

1. Deploy multiple daemon instances behind load balancer
2. Each daemon manages its own agents
3. Redis provides shared state and coordination
4. PostgreSQL stores persistent data

### Vertical Scaling

Increase resources per daemon:

```toml
[daemon]
max_agents = 50

[redis]
pool_size = 50

[postgres]
max_connections = 100
```

## Backup and Recovery

### PostgreSQL Backup

```bash
# Backup
pg_dump -h localhost -p 5433 -U cca cca > backup.sql

# Restore
psql -h localhost -p 5433 -U cca cca < backup.sql
```

### Redis Backup

Redis uses append-only file (AOF) persistence. The data directory is at `redis_data:/data`.

## Troubleshooting

### Daemon won't start

1. Check configuration: `cca config show`
2. Verify Redis/PostgreSQL connectivity
3. Check port availability
4. Review logs

### Agents failing to spawn

1. Verify `claude` is in PATH
2. Check `claude_path` configuration
3. Review agent manager logs
4. Check system resources

### Connection timeouts

1. Increase timeout settings
2. Check network connectivity
3. Verify firewall rules
4. Monitor system resources

### Memory issues

1. Reduce `max_agents`
2. Enable context compression
3. Increase system memory
4. Implement connection pooling limits
