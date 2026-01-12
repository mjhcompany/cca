# CCA Makefile
# Common commands for building, testing, and installing CCA

.PHONY: all build release test clean install uninstall docker-up docker-down help

# Default target
all: build

# Build debug binaries
build:
	cargo build --workspace

# Build release binaries
release:
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

# Start the daemon (must be installed first)
start:
	ccad

# Check daemon status
status:
	@curl -s http://127.0.0.1:8580/api/v1/health 2>/dev/null || echo "Daemon not running"

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
	@echo "  make build     - Build debug binaries"
	@echo "  make release   - Build release binaries"
	@echo "  make test      - Run all tests"
	@echo "  make clean     - Clean build artifacts"
	@echo "  make install   - Build release and install (runs install.sh)"
	@echo "  make uninstall - Uninstall CCA (runs uninstall.sh)"
	@echo "  make docker-up - Start Docker services"
	@echo "  make docker-down - Stop Docker services"
	@echo "  make start     - Start the daemon"
	@echo "  make status    - Check daemon status"
	@echo "  make fmt       - Format code"
	@echo "  make lint      - Run clippy lints"
	@echo "  make ci        - Run CI checks (fmt, lint, test)"
