# CCA - Claude Code Agents

Version: 1.0.0
Target: x86_64-unknown-linux-gnu

## Quick Install

```bash
sudo ./install.sh
```

## Prerequisites

- Redis instance (local or remote)
- PostgreSQL instance with pgvector extension (local or remote)
- Optional: Docker and docker-compose (included docker-compose.yml for local setup)

## System User

The installer creates a `cca` system user to run the daemon securely.
If user creation fails or you prefer to use a different user:

```bash
# Option 1: Create the user manually
sudo useradd --system --no-create-home --shell /usr/sbin/nologin cca

# Option 2: Edit the systemd service to use your user
sudo sed -i 's/User=cca/User=yourusername/' /etc/systemd/system/ccad.service
sudo sed -i 's/Group=cca/Group=yourusername/' /etc/systemd/system/ccad.service
sudo systemctl daemon-reload
```

## Package Contents

```
├── bin/
│   ├── cca          # CLI client
│   ├── ccad         # Daemon service
│   └── cca-mcp      # MCP server for Claude Code
├── config/
│   ├── cca.toml.example
│   └── install.conf.example
├── migrations/
│   └── init.sql     # Database schema
├── agents/
│   └── *.md         # Agent role definitions
├── systemd/
│   └── ccad.service
├── docker-compose.yml
├── install.sh
├── uninstall.sh
└── README.md
```

## Configuration

Edit `/usr/local/etc/cca/cca.toml` to customize:
- Daemon ports and bind addresses
- Redis URL (can be remote: `redis://host:port`)
- PostgreSQL URL (can be remote: `postgres://user:pass@host:port/db`)
- Agent settings and permissions
- Embedding service (Ollama)

### Hot-Reload Configuration

Many settings can be reloaded without restarting the daemon:

```bash
# Via CLI
cca config reload

# Via systemctl (sends SIGHUP)
sudo systemctl reload ccad

# Via signal directly
kill -HUP $(pidof ccad)
```

**Hot-reloadable settings:**
- API keys
- Rate limits
- Agent timeouts and permissions
- Learning settings

**Requires restart:**
- Bind addresses and ports
- Database URLs
- ACP WebSocket port

## Starting Services

```bash
# If using local Docker for Redis/PostgreSQL
docker-compose up -d

# Start CCA daemon
sudo systemctl enable ccad  # Enable on boot
sudo systemctl start ccad

# Check status
sudo systemctl status ccad
cca daemon status
```

## MCP Integration

Add to `~/.claude/mcp_servers.json`:

```json
{
  "cca": {
    "command": "/usr/local/bin/cca-mcp",
    "args": [],
    "env": {
      "CCA_DAEMON_URL": "http://127.0.0.1:8580",
      "CCA_API_KEY": "<your-api-key>"
    }
  }
}
```

## Troubleshooting

### Service fails with "status=217/USER"
The `cca` user doesn't exist. See "System User" section above.

### Service fails to connect to Redis/PostgreSQL
Check that your database services are running and the URLs in `cca.toml` are correct.
Redis and PostgreSQL can be on remote hosts - just update the URLs accordingly.

### View logs
```bash
sudo journalctl -u ccad -f
# or
tail -f /var/log/cca/ccad.log
```

## License

MIT License
