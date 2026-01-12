#!/bin/bash
# CCA Uninstallation Script
# Stops services and removes CCA from the system

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

echo -e "${RED}CCA Uninstallation Script${NC}"
echo "=========================="
echo ""

# =============================================================================
# Load Configuration (if exists)
# =============================================================================
if [[ -f "$CONFIG_FILE" ]]; then
    echo -e "${BLUE}Loading configuration from: $CONFIG_FILE${NC}"
    source "$CONFIG_FILE"
fi

# Set defaults
PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="${BIN_DIR:-$PREFIX/bin}"
DATA_DIR="${DATA_DIR:-$PREFIX/share/cca}"
CONFIG_DIR="${CONFIG_DIR:-$PREFIX/etc/cca}"
LOG_DIR="${LOG_DIR:-/var/log/cca}"
DOCKER_COMPOSE_PROJECT="${DOCKER_COMPOSE_PROJECT:-cca}"

echo ""
echo "This will:"
echo "  1. Stop and remove Docker containers (cca-redis, cca-postgres)"
echo "  2. Remove CCA binaries from $BIN_DIR"
echo "  3. Optionally remove configuration, data, and logs"
echo ""

# Confirm
read -p "Continue with uninstallation? [y/N] " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

# =============================================================================
# Check for sudo if needed
# =============================================================================
SUDO=""
if [[ $EUID -ne 0 ]] && [[ -f "$BIN_DIR/cca" ]] && [[ ! -w "$BIN_DIR" ]]; then
    SUDO="sudo"
    echo -e "${YELLOW}Note: sudo required for removal from $PREFIX${NC}"
fi

# =============================================================================
# Stop CCA Daemon
# =============================================================================
echo ""
echo -e "${BLUE}Stopping CCA daemon...${NC}"
pkill ccad 2>/dev/null && echo -e "  ${GREEN}✓${NC} Daemon stopped" || echo -e "  ${YELLOW}~${NC} Daemon not running"
sleep 1

# =============================================================================
# Stop Docker Services
# =============================================================================
echo -e "${BLUE}Stopping Docker services...${NC}"

# Check for docker-compose
if command -v docker-compose &> /dev/null; then
    DOCKER_COMPOSE="docker-compose"
elif docker compose version &> /dev/null 2>&1; then
    DOCKER_COMPOSE="docker compose"
else
    DOCKER_COMPOSE=""
fi

if [[ -n "$DOCKER_COMPOSE" ]] && [[ -f "$PROJECT_DIR/docker-compose.yml" ]]; then
    cd "$PROJECT_DIR"
    $DOCKER_COMPOSE -p "$DOCKER_COMPOSE_PROJECT" down 2>/dev/null || true
    echo -e "  ${GREEN}✓${NC} Docker services stopped"
else
    # Try to stop containers directly
    docker stop cca-redis cca-postgres 2>/dev/null || true
    docker rm cca-redis cca-postgres 2>/dev/null || true
    echo -e "  ${GREEN}✓${NC} Containers removed"
fi

# Ask about Docker volumes
echo ""
read -p "Remove Docker volumes (Redis and PostgreSQL data)? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    docker volume rm cca-redis-data cca-postgres-data 2>/dev/null || true
    # Also try with project prefix
    docker volume rm ${DOCKER_COMPOSE_PROJECT}_cca-redis-data ${DOCKER_COMPOSE_PROJECT}_cca-postgres-data 2>/dev/null || true
    echo -e "  ${RED}-${NC} Docker volumes removed"
else
    echo -e "  ${YELLOW}~${NC} Docker volumes kept"
fi

# =============================================================================
# Remove Binaries
# =============================================================================
echo ""
echo -e "${BLUE}Removing binaries...${NC}"
for bin in cca ccad cca-mcp; do
    if [[ -f "$BIN_DIR/$bin" ]]; then
        $SUDO rm -f "$BIN_DIR/$bin"
        echo -e "  ${RED}-${NC} $bin"
    fi
done

# =============================================================================
# Remove Data Directory
# =============================================================================
echo ""
if [[ -d "$DATA_DIR" ]]; then
    read -p "Remove data directory ($DATA_DIR)? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        $SUDO rm -rf "$DATA_DIR"
        echo -e "  ${RED}-${NC} $DATA_DIR"
    else
        echo -e "  ${YELLOW}~${NC} $DATA_DIR (kept)"
    fi
fi

# =============================================================================
# Remove Configuration
# =============================================================================
if [[ -d "$CONFIG_DIR" ]]; then
    read -p "Remove configuration directory ($CONFIG_DIR)? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        $SUDO rm -rf "$CONFIG_DIR"
        echo -e "  ${RED}-${NC} $CONFIG_DIR"
    else
        echo -e "  ${YELLOW}~${NC} $CONFIG_DIR (kept)"
    fi
fi

# =============================================================================
# Remove Logs
# =============================================================================
if [[ -d "$LOG_DIR" ]]; then
    read -p "Remove log directory ($LOG_DIR)? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        $SUDO rm -rf "$LOG_DIR"
        echo -e "  ${RED}-${NC} $LOG_DIR"
    else
        echo -e "  ${YELLOW}~${NC} $LOG_DIR (kept)"
    fi
fi

# =============================================================================
# Remove docker-compose.yml
# =============================================================================
if [[ -f "$PROJECT_DIR/docker-compose.yml" ]]; then
    read -p "Remove generated docker-compose.yml? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -f "$PROJECT_DIR/docker-compose.yml"
        echo -e "  ${RED}-${NC} docker-compose.yml"
    else
        echo -e "  ${YELLOW}~${NC} docker-compose.yml (kept)"
    fi
fi

# =============================================================================
# Summary
# =============================================================================
echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}Uninstallation complete!${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════════${NC}"
echo ""
echo "Notes:"
echo "  - Remove 'source ${CONFIG_DIR}/cca.env' from your shell profile if added"
echo "  - The install.conf configuration file was preserved in $SCRIPT_DIR"
