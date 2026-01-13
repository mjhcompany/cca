#!/bin/bash
# CCA Installation Script
# Installs CCA binaries, configures services, and starts Docker containers

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Source directory (where this script is located)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG_FILE="${CONFIG_FILE:-$SCRIPT_DIR/install.conf}"

# Parse command line arguments
SKIP_BUILD=false
SKIP_DOCKER=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build|--no-build)
            SKIP_BUILD=true
            shift
            ;;
        --skip-docker|--no-docker)
            SKIP_DOCKER=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-build, --no-build    Skip building (use existing binaries)"
            echo "  --skip-docker, --no-docker  Skip Docker container startup"
            echo "  -h, --help                  Show this help"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

echo -e "${GREEN}CCA Installation Script${NC}"
echo "========================"
echo ""

# =============================================================================
# Load Configuration
# =============================================================================
if [[ ! -f "$CONFIG_FILE" ]]; then
    echo -e "${RED}Error: Configuration file not found: $CONFIG_FILE${NC}"
    echo "Please create install.conf from install.conf.example or specify CONFIG_FILE"
    exit 1
fi

echo -e "${BLUE}Loading configuration from: $CONFIG_FILE${NC}"
source "$CONFIG_FILE"

# Set defaults and derived paths
PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="${BIN_DIR:-$PREFIX/bin}"
DATA_DIR="${DATA_DIR:-$PREFIX/share/cca}"
CONFIG_DIR="${CONFIG_DIR:-$PREFIX/etc/cca}"
LOG_DIR="${LOG_DIR:-/var/log/cca}"

CCA_DAEMON_PORT="${CCA_DAEMON_PORT:-9280}"
CCA_ACP_PORT="${CCA_ACP_PORT:-9180}"
REDIS_PORT="${REDIS_PORT:-6380}"
POSTGRES_PORT="${POSTGRES_PORT:-5433}"
POSTGRES_USER="${POSTGRES_USER:-cca}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-cca_secure_password}"
POSTGRES_DB="${POSTGRES_DB:-cca}"
DOCKER_COMPOSE_PROJECT="${DOCKER_COMPOSE_PROJECT:-cca}"
START_SERVICES="${START_SERVICES:-yes}"
LOG_LEVEL="${LOG_LEVEL:-info}"

echo ""
echo "Configuration:"
echo "  CCA Daemon Port:    $CCA_DAEMON_PORT"
echo "  CCA ACP Port:       $CCA_ACP_PORT"
echo "  Redis Port:         $REDIS_PORT"
echo "  PostgreSQL Port:    $POSTGRES_PORT"
echo "  Install Prefix:     $PREFIX"
echo ""

# =============================================================================
# Check Prerequisites
# =============================================================================
echo -e "${BLUE}Checking prerequisites...${NC}"

# Check for Docker
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed.${NC}"
    echo "Please install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi
echo -e "  ${GREEN}✓${NC} Docker found"

# Check for docker-compose or docker compose
if command -v docker-compose &> /dev/null; then
    DOCKER_COMPOSE="docker-compose"
elif docker compose version &> /dev/null 2>&1; then
    DOCKER_COMPOSE="docker compose"
else
    echo -e "${RED}Error: docker-compose is not installed.${NC}"
    exit 1
fi
echo -e "  ${GREEN}✓${NC} Docker Compose found ($DOCKER_COMPOSE)"

RELEASE_DIR="$PROJECT_DIR/target/release"

if [[ "$SKIP_BUILD" == "true" ]]; then
    echo -e "${BLUE}Skipping build (--skip-build)${NC}"
    if [[ ! -f "$RELEASE_DIR/cca" ]] || [[ ! -f "$RELEASE_DIR/ccad" ]] || [[ ! -f "$RELEASE_DIR/cca-mcp" ]]; then
        echo -e "${RED}Error: Release binaries not found. Run without --skip-build first.${NC}"
        exit 1
    fi
    echo -e "  ${GREEN}✓${NC} Using existing release binaries"
else
    # Build release binaries (clean first to ensure fresh build)
    echo -e "${BLUE}Building release binaries...${NC}"
    cd "$PROJECT_DIR"
    cargo clean
    cargo build --release --workspace
    if [[ $? -ne 0 ]]; then
        echo -e "${RED}Error: Build failed.${NC}"
        exit 1
    fi

    if [[ ! -f "$RELEASE_DIR/cca" ]] || [[ ! -f "$RELEASE_DIR/ccad" ]] || [[ ! -f "$RELEASE_DIR/cca-mcp" ]]; then
        echo -e "${RED}Error: Release binaries not found after build.${NC}"
        exit 1
    fi
    echo -e "  ${GREEN}✓${NC} Release binaries built"
fi

# =============================================================================
# Check for Port Conflicts
# =============================================================================
echo ""
echo -e "${BLUE}Checking for port conflicts...${NC}"

# Check if our Docker containers are already running
REDIS_RUNNING=false
POSTGRES_RUNNING=false
if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^cca-redis$"; then
    REDIS_RUNNING=true
fi
if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^cca-postgres$"; then
    POSTGRES_RUNNING=true
fi

check_port() {
    local port=$1
    local service=$2
    local our_container=$3

    if ss -tuln 2>/dev/null | grep -q ":$port " || netstat -tuln 2>/dev/null | grep -q ":$port "; then
        # Port in use - check if it's our container
        if [[ "$our_container" == "true" ]]; then
            echo -e "  ${GREEN}✓${NC} Port $port ($service) - our container already running"
            return 0
        fi
        echo -e "  ${RED}✗${NC} Port $port ($service) is already in use!"
        return 1
    fi
    echo -e "  ${GREEN}✓${NC} Port $port ($service) is available"
    return 0
}

PORTS_OK=true
check_port "$CCA_DAEMON_PORT" "CCA Daemon" "false" || PORTS_OK=false
check_port "$CCA_ACP_PORT" "CCA ACP" "false" || PORTS_OK=false
check_port "$REDIS_PORT" "Redis" "$REDIS_RUNNING" || PORTS_OK=false
check_port "$POSTGRES_PORT" "PostgreSQL" "$POSTGRES_RUNNING" || PORTS_OK=false

# Skip Docker startup if containers are already running
if [[ "$REDIS_RUNNING" == "true" ]] && [[ "$POSTGRES_RUNNING" == "true" ]]; then
    echo -e "  ${GREEN}✓${NC} Docker services already running - will skip Docker startup"
    CONTAINERS_ALREADY_RUNNING=true
else
    CONTAINERS_ALREADY_RUNNING=false
fi

if [[ "$PORTS_OK" != "true" ]]; then
    echo ""
    echo -e "${YELLOW}Warning: Some ports are in use by other processes.${NC}"
    read -p "Continue anyway? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# =============================================================================
# Check for sudo if needed
# =============================================================================
SUDO=""
if [[ $EUID -ne 0 ]] && [[ ! -w "$BIN_DIR" ]]; then
    SUDO="sudo"
    echo ""
    echo -e "${YELLOW}Note: sudo required for installation to $PREFIX${NC}"
fi

# =============================================================================
# Create Directories
# =============================================================================
echo ""
echo -e "${BLUE}Creating directories...${NC}"
$SUDO mkdir -p "$BIN_DIR"
$SUDO mkdir -p "$DATA_DIR/agents"
$SUDO mkdir -p "$CONFIG_DIR"
$SUDO mkdir -p "$LOG_DIR"
$SUDO chmod 755 "$LOG_DIR"

# =============================================================================
# Generate docker-compose.yml
# =============================================================================
echo -e "${BLUE}Generating docker-compose.yml...${NC}"

REDIS_CMD=""
if [[ -n "$REDIS_PASSWORD" ]]; then
    REDIS_CMD="command: redis-server --requirepass $REDIS_PASSWORD"
fi

cat > "$PROJECT_DIR/docker-compose.yml" << EOF
# CCA Docker Compose Configuration
# Generated by install.sh - DO NOT EDIT MANUALLY
# Edit install.conf and re-run install.sh to update

services:
  redis:
    image: redis:alpine
    container_name: cca-redis
    ports:
      - "${REDIS_PORT}:6379"
    volumes:
      - cca-redis-data:/data
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5
    $REDIS_CMD

  postgres:
    image: pgvector/pgvector:pg18
    container_name: cca-postgres
    environment:
      POSTGRES_USER: ${POSTGRES_USER}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: ${POSTGRES_DB}
    ports:
      - "${POSTGRES_PORT}:5432"
    volumes:
      - cca-postgres-data:/var/lib/postgresql
    restart: unless-stopped
    command:
      - "postgres"
      - "-c"
      - "shared_preload_libraries=vector"
      - "-c"
      - "io_method=worker"
      - "-c"
      - "shared_buffers=256MB"
      - "-c"
      - "effective_cache_size=1GB"
      - "-c"
      - "maintenance_work_mem=128MB"
      - "-c"
      - "max_parallel_workers_per_gather=2"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB}"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  cca-redis-data:
  cca-postgres-data:
EOF

echo -e "  ${GREEN}+${NC} docker-compose.yml"

# =============================================================================
# Generate cca.toml Configuration
# =============================================================================
echo -e "${BLUE}Generating CCA configuration...${NC}"

# Build Redis URL
if [[ -n "$REDIS_PASSWORD" ]]; then
    REDIS_URL="redis://:${REDIS_PASSWORD}@localhost:${REDIS_PORT}"
else
    REDIS_URL="redis://localhost:${REDIS_PORT}"
fi

# Build Postgres URL
POSTGRES_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${POSTGRES_PORT}/${POSTGRES_DB}"

# Generate API key if not provided in config
if [[ -z "$CCA_API_KEY" ]]; then
    CCA_API_KEY=$(openssl rand -hex 32)
    echo -e "  ${GREEN}Generated API key:${NC} $CCA_API_KEY"
    echo -e "  ${YELLOW}Save this key - it's required for CLI and MCP access${NC}"
fi

$SUDO tee "$CONFIG_DIR/cca.toml" > /dev/null << EOF
# CCA Configuration
# Generated by install.sh
#
# To regenerate, edit install.conf and re-run install.sh
# Or edit this file directly for manual configuration

[daemon]
bind_address = "127.0.0.1:${CCA_DAEMON_PORT}"
log_level = "${LOG_LEVEL}"
max_agents = 10
log_file = "${LOG_DIR}/ccad.log"
data_dir = "${DATA_DIR}"
require_auth = true
api_keys = ["${CCA_API_KEY}"]

[redis]
url = "${REDIS_URL}"
pool_size = 10
context_ttl_seconds = 3600

[postgres]
url = "${POSTGRES_URL}"
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
enabled = ${EMBEDDINGS_ENABLED:-false}
ollama_url = "${EMBEDDINGS_OLLAMA_URL:-http://localhost:11434}"
model = "${EMBEDDINGS_MODEL:-nomic-embed-text:latest}"
dimension = ${EMBEDDINGS_DIMENSION:-768}
EOF

echo -e "  ${GREEN}+${NC} cca.toml"

# =============================================================================
# Install Binaries
# =============================================================================
echo -e "${BLUE}Installing binaries...${NC}"
$SUDO install -m 755 "$RELEASE_DIR/cca" "$BIN_DIR/cca"
$SUDO install -m 755 "$RELEASE_DIR/ccad" "$BIN_DIR/ccad"
$SUDO install -m 755 "$RELEASE_DIR/cca-mcp" "$BIN_DIR/cca-mcp"
echo -e "  ${GREEN}+${NC} cca"
echo -e "  ${GREEN}+${NC} ccad"
echo -e "  ${GREEN}+${NC} cca-mcp"

# =============================================================================
# Install Agent Definitions
# =============================================================================
echo -e "${BLUE}Installing agent definitions...${NC}"
if [[ -d "$PROJECT_DIR/agents" ]]; then
    for f in "$PROJECT_DIR/agents"/*.md; do
        if [[ -f "$f" ]]; then
            name=$(basename "$f")
            $SUDO install -m 644 "$f" "$DATA_DIR/agents/$name"
            echo -e "  ${GREEN}+${NC} agents/$name"
        fi
    done
else
    echo -e "  ${YELLOW}Warning: No agents directory found${NC}"
fi

# =============================================================================
# Start Docker Services
# =============================================================================
if [[ "$START_SERVICES" == "yes" ]] && [[ "$SKIP_DOCKER" != "true" ]] && [[ "$CONTAINERS_ALREADY_RUNNING" != "true" ]]; then
    echo ""
    echo -e "${BLUE}Starting Docker services...${NC}"
    cd "$PROJECT_DIR"
    $DOCKER_COMPOSE -p "$DOCKER_COMPOSE_PROJECT" up -d

    echo ""
    echo "Waiting for services to be healthy..."
    sleep 3
elif [[ "$SKIP_DOCKER" == "true" ]]; then
    echo ""
    echo -e "${BLUE}Skipping Docker startup (--skip-docker)...${NC}"
elif [[ "$CONTAINERS_ALREADY_RUNNING" == "true" ]]; then
    echo ""
    echo -e "${BLUE}Docker containers already running, skipping startup...${NC}"
fi

# Check service health regardless of whether we started them
if docker exec cca-redis redis-cli ping &>/dev/null; then
    echo -e "  ${GREEN}✓${NC} Redis is running on port $REDIS_PORT"
else
    echo -e "  ${YELLOW}!${NC} Redis may still be starting..."
fi

if docker exec cca-postgres pg_isready -U "$POSTGRES_USER" &>/dev/null; then
    echo -e "  ${GREEN}✓${NC} PostgreSQL is running on port $POSTGRES_PORT"
else
    echo -e "  ${YELLOW}!${NC} PostgreSQL may still be starting..."
fi

# =============================================================================
# Create Environment File
# =============================================================================
echo -e "${BLUE}Creating environment file...${NC}"
$SUDO tee "$CONFIG_DIR/cca.env" > /dev/null << EOF
# CCA Environment Variables
# Auto-loaded by cca, ccad, and cca-mcp binaries
# You can override these by setting them in your shell before running commands

export CCA_CONFIG=${CONFIG_DIR}/cca.toml
export CCA_DATA_DIR=${DATA_DIR}
export CCA_DAEMON_URL=http://127.0.0.1:${CCA_DAEMON_PORT}
export CCA_ACP_URL=ws://127.0.0.1:${CCA_ACP_PORT}
EOF
echo -e "  ${GREEN}+${NC} cca.env"

# =============================================================================
# Summary
# =============================================================================
echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}Installation complete!${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════════${NC}"
echo ""
echo "Service Ports:"
echo "  CCA Daemon API:  http://127.0.0.1:${CCA_DAEMON_PORT}"
echo "  CCA ACP Socket:  ws://127.0.0.1:${CCA_ACP_PORT}"
echo "  Redis:           localhost:${REDIS_PORT}"
echo "  PostgreSQL:      localhost:${POSTGRES_PORT}"
echo ""
echo "Files:"
echo "  Config:          ${CONFIG_DIR}/cca.toml"
echo "  Environment:     ${CONFIG_DIR}/cca.env"
echo "  Logs:            ${LOG_DIR}/ccad.log"
echo "  Data:            ${DATA_DIR}"
echo ""
echo "Next steps:"
echo "  1. Start the CCA daemon:"
echo "     ccad"
echo ""
echo "  2. Start agent workers (in separate terminals):"
echo "     cca agent worker coordinator"
echo "     cca agent worker backend"
echo ""
echo "  3. Check status:"
echo "     cca agent list"
echo "     cca agent diag"
echo ""
echo "Note: Environment is auto-loaded from ${CONFIG_DIR}/cca.env"
echo ""
echo "Docker commands:"
echo "  View logs:       docker logs cca-redis"
echo "  Stop services:   cd $PROJECT_DIR && $DOCKER_COMPOSE down"
echo "  Start services:  cd $PROJECT_DIR && $DOCKER_COMPOSE up -d"
