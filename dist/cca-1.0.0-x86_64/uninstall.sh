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
