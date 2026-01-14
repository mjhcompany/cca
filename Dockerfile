# CCA Daemon Production Dockerfile
# Multi-stage build for minimal runtime image

# =============================================================================
# Stage 1: Builder
# =============================================================================
FROM rust:1.75-bookworm AS builder

# Install build dependencies
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

# Create dummy source files to build dependencies
RUN mkdir -p crates/cca-core/src && echo "pub fn dummy() {}" > crates/cca-core/src/lib.rs && \
    mkdir -p crates/cca-daemon/src && echo "fn main() {}" > crates/cca-daemon/src/main.rs && \
    mkdir -p crates/cca-cli/src && echo "fn main() {}" > crates/cca-cli/src/main.rs && \
    mkdir -p crates/cca-mcp/src && echo "pub fn dummy() {}" > crates/cca-mcp/src/lib.rs && \
    mkdir -p crates/cca-acp/src && echo "pub fn dummy() {}" > crates/cca-acp/src/lib.rs && \
    mkdir -p crates/cca-rl/src && echo "pub fn dummy() {}" > crates/cca-rl/src/lib.rs && \
    mkdir -p tests/chaos/src && echo "fn main() {}" > tests/chaos/src/main.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release --bin ccad 2>/dev/null || true

# Remove dummy source files
RUN rm -rf crates/*/src tests/chaos/src

# Copy actual source code
COPY crates crates/
COPY migrations migrations/

# Build the actual binary
ARG VERSION=0.0.0
RUN cargo build --release --locked --bin ccad && \
    strip target/release/ccad

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -s /bin/false -U cca

# Copy the binary from builder
COPY --from=builder /build/target/release/ccad /usr/local/bin/ccad

# Copy migrations for runtime database setup
COPY --from=builder /build/migrations /opt/cca/migrations

# Create directories for runtime data
RUN mkdir -p /var/lib/cca /var/log/cca /etc/cca && \
    chown -R cca:cca /var/lib/cca /var/log/cca /etc/cca

# Default configuration (can be overridden with environment variables)
ENV CCA__DAEMON__BIND_ADDRESS=0.0.0.0:9200 \
    CCA__DAEMON__WS_ADDRESS=0.0.0.0:9100 \
    CCA__DAEMON__LOG_LEVEL=info \
    CCA__DAEMON__DATA_DIR=/var/lib/cca \
    RUST_LOG=info

# Expose ports
# HTTP API
EXPOSE 9200
# WebSocket (ACP)
EXPOSE 9100

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9200/health || exit 1

# Run as non-root user
USER cca

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/ccad"]

# Default command (can be overridden)
CMD []
