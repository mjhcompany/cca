# CCA Chaos Testing Infrastructure

This directory contains chaos testing infrastructure to verify system resilience and recovery capabilities.

## Test Categories

1. **Agent Crash Recovery** (`agent_crash_tests.rs`) - Tests agent process termination and recovery
2. **Redis Disconnection** (`redis_chaos_tests.rs`) - Tests Redis connection failures and reconnection
3. **PostgreSQL Failover** (`postgres_chaos_tests.rs`) - Tests database failover and query timeouts
4. **Graceful Degradation** (`degradation_tests.rs`) - Tests system behavior when services are unavailable

## Running Tests

```bash
# Run all chaos tests
cargo test --test chaos_tests

# Run specific test category
cargo test --test chaos_tests agent_crash
cargo test --test chaos_tests redis_chaos
cargo test --test chaos_tests postgres_chaos
cargo test --test chaos_tests graceful_degradation

# Run with verbose output
RUST_LOG=debug cargo test --test chaos_tests -- --nocapture
```

## Configuration

Set environment variables to configure test behavior:

```bash
# Test timeouts
CHAOS_TEST_TIMEOUT_SECS=60

# Service endpoints (for integration tests)
CCA__POSTGRES__URL=postgres://localhost/cca_test
CCA__REDIS__URL=redis://localhost:6379

# Chaos injection settings
CHAOS_AGENT_KILL_DELAY_MS=100
CHAOS_RECONNECT_ATTEMPTS=5
```

## Test Architecture

The chaos tests use a layered approach:

1. **Mock Layer** - Unit tests with mocked services for fast feedback
2. **Integration Layer** - Tests against real services in controlled environments
3. **Chaos Injection** - Uses process signals, connection drops, and timeouts

## Requirements

- Rust 1.70+
- Docker (for integration tests with real services)
- k6 (for load-based chaos scenarios)
