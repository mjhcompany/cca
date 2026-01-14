# Multi-stage Dockerfile for CCA (Claude Code Agent)
# Build stage: Compile all Rust binaries in release mode
# Runtime stage: Minimal Debian image with compiled binaries

# =============================================================================
# Stage 1: Builder
# =============================================================================
FROM rust:1.81 AS builder

# Install build dependencies for tree-sitter and other native libs
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace configuration first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates/cca-core/Cargo.toml crates/cca-core/
COPY crates/cca-daemon/Cargo.toml crates/cca-daemon/
COPY crates/cca-cli/Cargo.toml crates/cca-cli/
COPY crates/cca-mcp/Cargo.toml crates/cca-mcp/
COPY crates/cca-acp/Cargo.toml crates/cca-acp/
COPY crates/cca-rl/Cargo.toml crates/cca-rl/
COPY tests/chaos/Cargo.toml tests/chaos/

# Create dummy source files to build dependencies (for better layer caching)
RUN mkdir -p crates/cca-core/src && echo "pub fn dummy() {}" > crates/cca-core/src/lib.rs && \
    mkdir -p crates/cca-daemon/src && echo "fn main() {}" > crates/cca-daemon/src/main.rs && \
    mkdir -p crates/cca-cli/src && echo "fn main() {}" > crates/cca-cli/src/main.rs && \
    mkdir -p crates/cca-mcp/src && echo "fn main() {}" > crates/cca-mcp/src/main.rs && \
    mkdir -p crates/cca-acp/src && echo "pub fn dummy() {}" > crates/cca-acp/src/lib.rs && \
    mkdir -p crates/cca-rl/src && echo "pub fn dummy() {}" > crates/cca-rl/src/lib.rs && \
    mkdir -p tests/chaos/src && echo "fn main() {}" > tests/chaos/src/main.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release --bin ccad --bin cca --bin cca-mcp 2>/dev/null || true

# Remove dummy source files
RUN rm -rf crates/*/src tests/chaos/src

# Copy actual source code
COPY crates crates/
COPY migrations migrations/

# Build all binaries in release mode
RUN cargo build --release --locked \
    --bin ccad \
    --bin cca \
    --bin cca-mcp

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd --create-home --shell /bin/bash cca

# Copy binaries from builder stage
COPY --from=builder /build/target/release/ccad /usr/local/bin/ccad
COPY --from=builder /build/target/release/cca /usr/local/bin/cca
COPY --from=builder /build/target/release/cca-mcp /usr/local/bin/cca-mcp

# Ensure binaries are executable
RUN chmod +x /usr/local/bin/ccad /usr/local/bin/cca /usr/local/bin/cca-mcp

# Copy migrations for runtime database setup
COPY --from=builder /build/migrations /opt/cca/migrations

# Create directories for runtime data
RUN mkdir -p /var/lib/cca /var/log/cca /etc/cca \
    && chown -R cca:cca /var/lib/cca /var/log/cca /etc/cca /opt/cca

# Switch to non-root user
USER cca
WORKDIR /home/cca

# Expose ports
# 8580: HTTP API
# 8581: ACP WebSocket
EXPOSE 8580 8581

# Health check - verify daemon is responding
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8580/health || exit 1

# Set environment defaults
ENV RUST_LOG=info \
    CCA_HTTP_PORT=8580 \
    CCA_ACP_PORT=8581 \
    CCA_DATA_DIR=/var/lib/cca

# Run the daemon
ENTRYPOINT ["ccad"]
