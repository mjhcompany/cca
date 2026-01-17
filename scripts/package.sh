#!/usr/bin/env bash
#
# CCA Binary Distribution Packager
# Creates installation packages without source code
#
# Usage: ./scripts/package.sh [OPTIONS]
#
# Options:
#   --target TARGET    Cross-compile target (default: current host)
#   --version VERSION  Package version (default: from Cargo.toml)
#   --output DIR       Output directory (default: dist/)
#   --skip-build       Use existing binaries from target/release
#   -h, --help         Show this help
#
# Examples:
#   ./scripts/package.sh                           # Build for current platform
#   ./scripts/package.sh --target x86_64-unknown-linux-gnu
#   ./scripts/package.sh --skip-build --version 0.3.1
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Defaults
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="${PROJECT_ROOT}/dist"
TARGET=""
VERSION=""
SKIP_BUILD=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --version)
            VERSION="$2"
            shift 2
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        -h|--help)
            head -25 "$0" | tail -23
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Get version from Cargo.toml if not specified
if [[ -z "$VERSION" ]]; then
    VERSION=$(grep -m1 '^version' "$PROJECT_ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
fi

# Determine target triple
if [[ -z "$TARGET" ]]; then
    TARGET=$(rustc -vV | grep host | awk '{print $2}')
fi

# Determine binary directory
if [[ "$SKIP_BUILD" == "true" ]]; then
    BINARY_DIR="${PROJECT_ROOT}/target/release"
else
    if [[ "$TARGET" == "$(rustc -vV | grep host | awk '{print $2}')" ]]; then
        BINARY_DIR="${PROJECT_ROOT}/target/release"
    else
        BINARY_DIR="${PROJECT_ROOT}/target/${TARGET}/release"
    fi
fi

# Simplify architecture name for package (e.g., x86_64-unknown-linux-gnu -> x86_64)
ARCH=$(echo "$TARGET" | cut -d'-' -f1)
PACKAGE_NAME="cca-${VERSION}-${ARCH}"
PACKAGE_DIR="${OUTPUT_DIR}/${PACKAGE_NAME}"

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║           CCA Binary Distribution Packager                 ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  Version:     ${GREEN}${VERSION}${NC}"
echo -e "  Target:      ${GREEN}${TARGET}${NC}"
echo -e "  Output:      ${GREEN}${OUTPUT_DIR}${NC}"
echo -e "  Skip Build:  ${GREEN}${SKIP_BUILD}${NC}"
echo ""

# Build binaries
if [[ "$SKIP_BUILD" == "false" ]]; then
    echo -e "${YELLOW}Building release binaries...${NC}"

    if [[ "$TARGET" == "$(rustc -vV | grep host | awk '{print $2}')" ]]; then
        cargo build --release --workspace
    else
        echo -e "${YELLOW}Cross-compiling for ${TARGET}...${NC}"
        cargo build --release --workspace --target "$TARGET"
    fi

    echo -e "${GREEN}Build complete${NC}"
fi

# Verify binaries exist
BINARIES=("cca" "ccad" "cca-mcp")
for bin in "${BINARIES[@]}"; do
    if [[ ! -f "${BINARY_DIR}/${bin}" ]]; then
        echo -e "${RED}Binary not found: ${BINARY_DIR}/${bin}${NC}"
        echo -e "${RED}Run without --skip-build or build first${NC}"
        exit 1
    fi
done

echo -e "${YELLOW}Creating package directory...${NC}"

# Clean and create package directory
rm -rf "$PACKAGE_DIR"
mkdir -p "$PACKAGE_DIR"/{bin,config,migrations,agents,scripts,systemd}

# Copy binaries
echo -e "  Copying binaries..."
for bin in "${BINARIES[@]}"; do
    cp "${BINARY_DIR}/${bin}" "$PACKAGE_DIR/bin/"
    chmod +x "$PACKAGE_DIR/bin/${bin}"
done

# Copy configuration templates
echo -e "  Copying configuration templates..."
cp "$PROJECT_ROOT/cca.toml.example" "$PACKAGE_DIR/config/cca.toml.example"
cp "$PROJECT_ROOT/scripts/install.conf" "$PACKAGE_DIR/config/install.conf.example"

# Copy migrations
echo -e "  Copying database migrations..."
cp "$PROJECT_ROOT/migrations/"*.sql "$PACKAGE_DIR/migrations/"

# Copy agent definitions
echo -e "  Copying agent definitions..."
if [[ -d "$PROJECT_ROOT/agents" ]]; then
    cp -r "$PROJECT_ROOT/agents/"*.md "$PACKAGE_DIR/agents/" 2>/dev/null || true
fi

# Create systemd service file
echo -e "  Creating systemd service file..."
cat > "$PACKAGE_DIR/systemd/ccad.service" << 'EOF'
[Unit]
Description=CCA Daemon - Claude Code Agents Orchestration
# Note: Redis and PostgreSQL can be on remote hosts or non-standard ports.
# Configure their URLs in /usr/local/etc/cca/cca.toml
# If running locally, add dependencies like: After=postgresql.service redis.service
After=network.target

[Service]
Type=simple
User=cca
Group=cca
EnvironmentFile=/usr/local/etc/cca/cca.env
ExecStart=/usr/local/bin/ccad --config /usr/local/etc/cca/cca.toml
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=5
LimitNOFILE=65536

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/log/cca /usr/local/share/cca
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
EOF

# Create the installer script
echo -e "  Creating installer script..."
cat > "$PACKAGE_DIR/install.sh" << 'INSTALLER'
#!/usr/bin/env bash
#
# CCA Installation Script
# Installs CCA binaries and configuration
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Defaults (can be overridden via environment or install.conf)
PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="${BIN_DIR:-${PREFIX}/bin}"
CONFIG_DIR="${CONFIG_DIR:-${PREFIX}/etc/cca}"
DATA_DIR="${DATA_DIR:-${PREFIX}/share/cca}"
LOG_DIR="${LOG_DIR:-/var/log/cca}"

CCA_DAEMON_PORT="${CCA_DAEMON_PORT:-8580}"
CCA_ACP_PORT="${CCA_ACP_PORT:-8581}"
REDIS_PORT="${REDIS_PORT:-16379}"
POSTGRES_PORT="${POSTGRES_PORT:-15432}"
POSTGRES_USER="${POSTGRES_USER:-cca}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-cca_secure_password}"
POSTGRES_DB="${POSTGRES_DB:-cca}"

INSTALL_SYSTEMD="${INSTALL_SYSTEMD:-true}"
CREATE_USER="${CREATE_USER:-true}"
RUN_MIGRATIONS="${RUN_MIGRATIONS:-true}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Load install.conf if exists
if [[ -f "${SCRIPT_DIR}/config/install.conf" ]]; then
    source "${SCRIPT_DIR}/config/install.conf"
elif [[ -f "${SCRIPT_DIR}/install.conf" ]]; then
    source "${SCRIPT_DIR}/install.conf"
fi

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║              CCA Installation Script                       ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  Install prefix:  ${GREEN}${PREFIX}${NC}"
echo -e "  Binaries:        ${GREEN}${BIN_DIR}${NC}"
echo -e "  Configuration:   ${GREEN}${CONFIG_DIR}${NC}"
echo -e "  Data directory:  ${GREEN}${DATA_DIR}${NC}"
echo -e "  Log directory:   ${GREEN}${LOG_DIR}${NC}"
echo ""

# Check for root
if [[ $EUID -ne 0 ]]; then
    echo -e "${RED}This script must be run as root (use sudo)${NC}"
    exit 1
fi

# Create CCA user if needed
if [[ "$CREATE_USER" == "true" ]]; then
    if ! id -u cca &>/dev/null; then
        echo -e "${YELLOW}Creating cca system user...${NC}"
        if useradd --system --no-create-home --shell /usr/sbin/nologin cca 2>/dev/null; then
            echo -e "  ${GREEN}✓${NC} Created system user 'cca'"
        else
            echo -e "  ${RED}✗${NC} Failed to create user 'cca'"
            echo -e "  ${YELLOW}→${NC} You may need to create it manually or edit ccad.service to use a different user"
        fi
    else
        echo -e "  ${GREEN}✓${NC} User 'cca' already exists"
    fi
else
    echo -e "${YELLOW}Skipping user creation (CREATE_USER=false)${NC}"
    echo -e "  ${YELLOW}→${NC} Make sure to edit /etc/systemd/system/ccad.service to use an existing user"
fi

# Create directories
echo -e "${YELLOW}Creating directories...${NC}"
mkdir -p "$BIN_DIR" "$CONFIG_DIR" "$DATA_DIR" "$LOG_DIR" "$DATA_DIR/agents"

# Install binaries
echo -e "${YELLOW}Installing binaries...${NC}"
for bin in cca ccad cca-mcp; do
    if [[ -f "${SCRIPT_DIR}/bin/${bin}" ]]; then
        cp "${SCRIPT_DIR}/bin/${bin}" "${BIN_DIR}/"
        chmod +x "${BIN_DIR}/${bin}"
        echo -e "  ${GREEN}✓${NC} ${bin}"
    else
        echo -e "  ${RED}✗${NC} ${bin} (not found)"
    fi
done

# Generate API key
API_KEY=$(openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | xxd -p)

# Install configuration
echo -e "${YELLOW}Installing configuration...${NC}"
if [[ ! -f "${CONFIG_DIR}/cca.toml" ]]; then
    cat > "${CONFIG_DIR}/cca.toml" << TOMLEOF
# CCA Configuration
# Generated by installer

[daemon]
bind_address = "127.0.0.1:${CCA_DAEMON_PORT}"
log_level = "info"
max_agents = 10
log_file = "${LOG_DIR}/ccad.log"
data_dir = "${DATA_DIR}"
require_auth = true
api_keys = ["${API_KEY}"]

[redis]
url = "redis://localhost:${REDIS_PORT}"
pool_size = 10
context_ttl_seconds = 3600

[postgres]
url = "postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${POSTGRES_PORT}/${POSTGRES_DB}"
pool_size = 10
max_connections = 20

[agents]
default_timeout_seconds = 300
context_compression = true
token_budget_per_task = 50000
claude_path = "claude"

[acp]
websocket_port = ${CCA_ACP_PORT}
reconnect_interval_ms = 1000
max_reconnect_attempts = 5

[mcp]
enabled = true
bind_address = "127.0.0.1:9201"

[learning]
enabled = true
default_algorithm = "ppo"
training_batch_size = 32
update_interval_seconds = 300

[embeddings]
enabled = false
# ollama_url = "http://localhost:11434"
# model = "nomic-embed-text:latest"
# dimension = 768
TOMLEOF
    echo -e "  ${GREEN}✓${NC} cca.toml (generated)"
else
    echo -e "  ${YELLOW}⚠${NC} cca.toml (already exists, skipped)"
fi

# Create environment file
cat > "${CONFIG_DIR}/cca.env" << ENVEOF
# CCA Environment Variables
export CCA_CONFIG=${CONFIG_DIR}/cca.toml
export CCA_DATA_DIR=${DATA_DIR}
export CCA_DAEMON_URL=http://127.0.0.1:${CCA_DAEMON_PORT}
export CCA_ACP_URL=ws://127.0.0.1:${CCA_ACP_PORT}
export CCA_API_KEY=${API_KEY}
ENVEOF
echo -e "  ${GREEN}✓${NC} cca.env"

# Copy agent definitions
if [[ -d "${SCRIPT_DIR}/agents" ]]; then
    cp -r "${SCRIPT_DIR}/agents/"* "${DATA_DIR}/agents/" 2>/dev/null || true
    echo -e "  ${GREEN}✓${NC} Agent definitions"
fi

# Copy migrations
if [[ -d "${SCRIPT_DIR}/migrations" ]]; then
    mkdir -p "${DATA_DIR}/migrations"
    cp "${SCRIPT_DIR}/migrations/"*.sql "${DATA_DIR}/migrations/"
    echo -e "  ${GREEN}✓${NC} Database migrations"
fi

# Run migrations if PostgreSQL is available
if [[ "$RUN_MIGRATIONS" == "true" ]]; then
    echo -e "${YELLOW}Checking database...${NC}"
    if command -v psql &>/dev/null; then
        if PGPASSWORD="${POSTGRES_PASSWORD}" psql -h localhost -p "${POSTGRES_PORT}" -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" -c "SELECT 1" &>/dev/null; then
            echo -e "  Running migrations..."
            for migration in "${DATA_DIR}/migrations/"*.sql; do
                PGPASSWORD="${POSTGRES_PASSWORD}" psql -h localhost -p "${POSTGRES_PORT}" -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" -f "$migration" 2>&1 | grep -v "already exists" || true
            done
            echo -e "  ${GREEN}✓${NC} Migrations complete"
        else
            echo -e "  ${YELLOW}⚠${NC} PostgreSQL not reachable, skipping migrations"
        fi
    else
        echo -e "  ${YELLOW}⚠${NC} psql not found, skipping migrations"
    fi
fi

# Install systemd service
if [[ "$INSTALL_SYSTEMD" == "true" ]] && [[ -d /etc/systemd/system ]]; then
    echo -e "${YELLOW}Installing systemd service...${NC}"
    if [[ -f "${SCRIPT_DIR}/systemd/ccad.service" ]]; then
        cp "${SCRIPT_DIR}/systemd/ccad.service" /etc/systemd/system/
        systemctl daemon-reload
        echo -e "  ${GREEN}✓${NC} ccad.service installed"
        echo -e "  ${YELLOW}→${NC} Enable with: systemctl enable ccad"
        echo -e "  ${YELLOW}→${NC} Start with:  systemctl start ccad"
    fi
fi

# Set ownership
chown -R cca:cca "$DATA_DIR" "$LOG_DIR" 2>/dev/null || true
chmod 600 "${CONFIG_DIR}/cca.toml" "${CONFIG_DIR}/cca.env"

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║              Installation Complete!                        ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  API Key: ${YELLOW}${API_KEY}${NC}"
echo ""
echo -e "  ${BLUE}Quick Start:${NC}"
echo -e "    1. Start Docker services (Redis + PostgreSQL):"
echo -e "       ${YELLOW}docker-compose up -d${NC}"
echo ""
echo -e "    2. Start the daemon:"
echo -e "       ${YELLOW}sudo systemctl start ccad${NC}"
echo -e "       or: ${YELLOW}ccad --config ${CONFIG_DIR}/cca.toml${NC}"
echo ""
echo -e "    3. Configure Claude Code MCP:"
echo -e "       Add to ~/.claude/mcp_servers.json:"
cat << 'MCPEOF'
       {
         "cca": {
           "command": "/usr/local/bin/cca-mcp",
           "args": [],
           "env": {
             "CCA_DAEMON_URL": "http://127.0.0.1:8580",
MCPEOF
echo -e "             \"CCA_API_KEY\": \"${API_KEY}\""
cat << 'MCPEOF'
           }
         }
       }
MCPEOF
echo ""
echo -e "  ${BLUE}Documentation:${NC} https://github.com/anthropics/cca"
echo ""
INSTALLER

chmod +x "$PACKAGE_DIR/install.sh"

# Create uninstall script
echo -e "  Creating uninstall script..."
cat > "$PACKAGE_DIR/uninstall.sh" << 'UNINSTALLER'
#!/usr/bin/env bash
#
# CCA Uninstallation Script
#

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="${BIN_DIR:-${PREFIX}/bin}"
CONFIG_DIR="${CONFIG_DIR:-${PREFIX}/etc/cca}"
DATA_DIR="${DATA_DIR:-${PREFIX}/share/cca}"
LOG_DIR="${LOG_DIR:-/var/log/cca}"

echo -e "${YELLOW}CCA Uninstaller${NC}"
echo ""

if [[ $EUID -ne 0 ]]; then
    echo -e "${RED}This script must be run as root (use sudo)${NC}"
    exit 1
fi

# Stop services
if systemctl is-active ccad &>/dev/null; then
    echo -e "Stopping ccad service..."
    systemctl stop ccad
fi

# Remove binaries
echo -e "Removing binaries..."
rm -f "${BIN_DIR}/cca" "${BIN_DIR}/ccad" "${BIN_DIR}/cca-mcp"

# Remove systemd service
if [[ -f /etc/systemd/system/ccad.service ]]; then
    echo -e "Removing systemd service..."
    systemctl disable ccad 2>/dev/null || true
    rm -f /etc/systemd/system/ccad.service
    systemctl daemon-reload
fi

echo ""
read -p "Remove configuration (${CONFIG_DIR})? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf "$CONFIG_DIR"
    echo -e "  ${GREEN}✓${NC} Configuration removed"
fi

read -p "Remove data directory (${DATA_DIR})? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf "$DATA_DIR"
    echo -e "  ${GREEN}✓${NC} Data removed"
fi

read -p "Remove logs (${LOG_DIR})? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf "$LOG_DIR"
    echo -e "  ${GREEN}✓${NC} Logs removed"
fi

echo ""
echo -e "${GREEN}Uninstallation complete${NC}"
UNINSTALLER

chmod +x "$PACKAGE_DIR/uninstall.sh"

# Create README
echo -e "  Creating README..."
cat > "$PACKAGE_DIR/README.md" << README
# CCA - Claude Code Agents

Version: ${VERSION}
Target: ${TARGET}

## Quick Install

\`\`\`bash
sudo ./install.sh
\`\`\`

## Prerequisites

- Redis instance (local or remote)
- PostgreSQL instance with pgvector extension (local or remote)
- Optional: Docker and docker-compose (included docker-compose.yml for local setup)

## System User

The installer creates a \`cca\` system user to run the daemon securely.
If user creation fails or you prefer to use a different user:

\`\`\`bash
# Option 1: Create the user manually
sudo useradd --system --no-create-home --shell /usr/sbin/nologin cca

# Option 2: Edit the systemd service to use your user
sudo sed -i 's/User=cca/User=yourusername/' /etc/systemd/system/ccad.service
sudo sed -i 's/Group=cca/Group=yourusername/' /etc/systemd/system/ccad.service
sudo systemctl daemon-reload
\`\`\`

## Package Contents

\`\`\`
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
\`\`\`

## Configuration

Edit \`/usr/local/etc/cca/cca.toml\` to customize:
- Daemon ports and bind addresses
- Redis URL (can be remote: \`redis://host:port\`)
- PostgreSQL URL (can be remote: \`postgres://user:pass@host:port/db\`)
- Agent settings and permissions
- Embedding service (Ollama)

### Hot-Reload Configuration

Many settings can be reloaded without restarting the daemon:

\`\`\`bash
# Via CLI
cca config reload

# Via systemctl (sends SIGHUP)
sudo systemctl reload ccad

# Via signal directly
kill -HUP \$(pidof ccad)
\`\`\`

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

\`\`\`bash
# If using local Docker for Redis/PostgreSQL
docker-compose up -d

# Start CCA daemon
sudo systemctl enable ccad  # Enable on boot
sudo systemctl start ccad

# Check status
sudo systemctl status ccad
cca daemon status
\`\`\`

## MCP Integration

Add to \`~/.claude/mcp_servers.json\`:

\`\`\`json
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
\`\`\`

## Troubleshooting

### Service fails with "status=217/USER"
The \`cca\` user doesn't exist. See "System User" section above.

### Service fails to connect to Redis/PostgreSQL
Check that your database services are running and the URLs in \`cca.toml\` are correct.
Redis and PostgreSQL can be on remote hosts - just update the URLs accordingly.

### View logs
\`\`\`bash
sudo journalctl -u ccad -f
# or
tail -f /var/log/cca/ccad.log
\`\`\`

## License

MIT License
README

# Copy docker-compose template for users who need it
echo -e "  Creating docker-compose template..."
cat > "$PACKAGE_DIR/docker-compose.yml" << 'DOCKERCOMPOSE'
# CCA Docker Compose - Redis and PostgreSQL
# Customize ports in install.conf before running install.sh

services:
  redis:
    image: redis:alpine
    container_name: cca-redis
    ports:
      - "16379:6379"
    volumes:
      - cca-redis-data:/data
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

  postgres:
    image: pgvector/pgvector:pg18
    container_name: cca-postgres
    environment:
      POSTGRES_USER: cca
      POSTGRES_PASSWORD: cca_secure_password
      POSTGRES_DB: cca
    ports:
      - "15432:5432"
    volumes:
      - cca-postgres-data:/var/lib/postgresql
    restart: unless-stopped
    command:
      - "postgres"
      - "-c"
      - "shared_preload_libraries=vector"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U cca -d cca"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  cca-redis-data:
  cca-postgres-data:
DOCKERCOMPOSE

# Create the archive
echo ""
echo -e "${YELLOW}Creating distribution archive...${NC}"
cd "$OUTPUT_DIR"
tar -czvf "${PACKAGE_NAME}.tgz" "${PACKAGE_NAME}"

# Calculate checksums
echo -e "${YELLOW}Generating checksums...${NC}"
sha256sum "${PACKAGE_NAME}.tgz" > "${PACKAGE_NAME}.tgz.sha256"

# Summary
ARCHIVE_SIZE=$(du -h "${PACKAGE_NAME}.tgz" | cut -f1)

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║              Package Created Successfully!                 ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  Package:   ${BLUE}${OUTPUT_DIR}/${PACKAGE_NAME}.tgz${NC}"
echo -e "  Size:      ${BLUE}${ARCHIVE_SIZE}${NC}"
echo -e "  Checksum:  ${BLUE}${OUTPUT_DIR}/${PACKAGE_NAME}.tgz.sha256${NC}"
echo ""
echo -e "  ${YELLOW}Contents:${NC}"
tar -tzf "${PACKAGE_NAME}.tgz" | head -20
echo "  ..."
echo ""
echo -e "  ${YELLOW}Distribution:${NC}"
echo -e "    1. Copy ${PACKAGE_NAME}.tgz to target machine"
echo -e "    2. Extract: tar -xzf ${PACKAGE_NAME}.tgz"
echo -e "    3. Install: cd ${PACKAGE_NAME} && sudo ./install.sh"
echo ""
