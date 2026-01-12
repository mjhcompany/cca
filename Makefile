# CCA Makefile
# Common commands for building, testing, and installing CCA

.PHONY: all build release release-quick test clean install uninstall docker-up docker-down \
        start stop status workers diag fmt lint ci help

# Default target
all: build

# Build debug binaries
build:
	cargo build --workspace

# Build release binaries (clean first to ensure fresh build)
release: clean
	cargo build --release --workspace

# Quick release build (incremental, no clean)
release-quick:
	cargo build --release --workspace

# Run all tests
test:
	cargo test --workspace

# Clean build artifacts
clean:
	cargo clean

# Install CCA (requires release build first)
install: release
	./scripts/install.sh

# Uninstall CCA
uninstall:
	./scripts/uninstall.sh

# Start Docker services only
docker-up:
	docker-compose up -d

# Stop Docker services
docker-down:
	docker-compose down

# Start the daemon
start:
	ccad

# Stop the daemon
stop:
	cca daemon stop

# Check daemon status
status:
	@cca daemon status

# List connected workers
workers:
	@cca agent list

# Run diagnostics
diag:
	@cca agent diag

# Format code
fmt:
	cargo fmt --all

# Run clippy lints
lint:
	cargo clippy --workspace -- -D warnings

# Run CI checks (format, lint, test)
ci: fmt lint test

# Help
help:
	@echo "CCA Makefile targets:"
	@echo ""
	@echo "Build:"
	@echo "  make build         - Build debug binaries"
	@echo "  make release       - Clean + build release (always fresh)"
	@echo "  make release-quick - Build release (incremental)"
	@echo "  make test          - Run all tests"
	@echo "  make clean      - Clean build artifacts"
	@echo "  make fmt        - Format code"
	@echo "  make lint       - Run clippy lints"
	@echo "  make ci         - Run CI checks (fmt, lint, test)"
	@echo ""
	@echo "Install:"
	@echo "  make install    - Build release and install"
	@echo "  make uninstall  - Uninstall CCA"
	@echo ""
	@echo "Docker:"
	@echo "  make docker-up  - Start Redis and PostgreSQL"
	@echo "  make docker-down - Stop Docker services"
	@echo ""
	@echo "Runtime:"
	@echo "  make start      - Start the daemon (ccad)"
	@echo "  make stop       - Stop the daemon"
	@echo "  make status     - Show daemon status"
	@echo "  make workers    - List connected workers"
	@echo "  make diag       - Run diagnostics"
	@echo ""
	@echo "Start workers in separate terminals:"
	@echo "  cca agent worker coordinator"
	@echo "  cca agent worker backend"
