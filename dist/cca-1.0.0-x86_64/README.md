# CCA - Claude Code Agents

Version: 1.0.0
Target: x86_64-unknown-linux-gnu

## Quick Install

```bash
sudo ./install.sh
```

## Prerequisites

- Docker and docker-compose (for Redis and PostgreSQL)
- OR existing Redis and PostgreSQL instances

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
├── install.sh
├── uninstall.sh
└── README.md
```

## Configuration

After installation, edit `/usr/local/etc/cca/cca.toml` to customize:
- Daemon ports
- Database connections
- Agent settings
- Embedding service (Ollama)

## Starting Services

```bash
# Start Redis and PostgreSQL (if using Docker)
docker-compose up -d

# Start CCA daemon
sudo systemctl start ccad

# Check status
cca status
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

## License

Proprietary - All rights reserved
