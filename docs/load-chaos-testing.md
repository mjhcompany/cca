# Load and Chaos Testing Guide

This document describes the infrastructure and procedures for load and chaos testing of the CCA (Claude Code Agent) system.

## Overview

CCA provides comprehensive testing infrastructure for:
- **Load Testing**: Measure system performance under various load conditions using k6
- **Chaos Testing**: Validate system resilience using Toxiproxy for fault injection
- **Monitoring**: Real-time metrics collection with Prometheus and Grafana dashboards
- **CI/CD Integration**: Automated performance regression testing in GitHub Actions

## Quick Start

### Prerequisites

1. **Docker & Docker Compose**: For running the test environment
2. **k6**: Load testing tool (https://k6.io)
3. **Rust toolchain**: For building CCA daemon

### Installation

```bash
# Install k6 on Linux
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
  --keyserver hkp://keyserver.ubuntu.com:80 \
  --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" \
  | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update && sudo apt-get install k6

# Install k6 on macOS
brew install k6
```

### Running Tests

```bash
# 1. Start the test environment
docker compose -f docker-compose.test.yml up -d

# 2. Build and start CCA daemon
cargo build --release --bin ccad
./target/release/ccad &

# 3. Run load tests
./tests/load/run-all.sh

# 4. View results in Grafana
open http://localhost:3000
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Test Infrastructure                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐     ┌──────────────┐     ┌──────────────────┐    │
│  │   k6     │────▶│  CCA Daemon  │────▶│  Redis/Postgres  │    │
│  │  Tests   │     │  (/metrics)  │     │   (Toxiproxy)    │    │
│  └──────────┘     └──────────────┘     └──────────────────┘    │
│       │                  │                      │               │
│       ▼                  ▼                      ▼               │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    Prometheus                             │  │
│  │              (metrics collection)                         │  │
│  └──────────────────────────────────────────────────────────┘  │
│                          │                                      │
│                          ▼                                      │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                     Grafana                               │  │
│  │              (visualization)                              │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Test Environment

### Docker Compose Services

The `docker-compose.test.yml` provides:

| Service | Port | Description |
|---------|------|-------------|
| redis-test | 16380 | Isolated Redis instance |
| postgres-test | 15433 | Isolated PostgreSQL with pgvector |
| prometheus | 9090 | Metrics collection |
| grafana | 3000 | Dashboards (admin/admin) |
| toxiproxy | 8474 | Network fault injection API |
| redis-exporter | 9121 | Redis metrics exporter |
| postgres-exporter | 9187 | PostgreSQL metrics exporter |

### Starting the Environment

```bash
# Start all services
docker compose -f docker-compose.test.yml up -d

# Check status
docker compose -f docker-compose.test.yml ps

# View logs
docker compose -f docker-compose.test.yml logs -f

# Stop and cleanup
docker compose -f docker-compose.test.yml down -v
```

## Load Testing

### Available Tests

| Test File | Description |
|-----------|-------------|
| `baseline.js` | Establishes performance baselines |
| `agent-spawning.js` | Tests agent lifecycle performance |
| `websocket-throughput.js` | Tests ACP WebSocket performance |
| `redis-pubsub.js` | Tests Redis pub/sub operations |
| `postgres-queries.js` | Tests database query performance |
| `full-system.js` | Comprehensive integration test |
| `task-submission.js` | Tests task queue performance |

### Running Individual Tests

```bash
# Run baseline test
k6 run tests/load/baseline.js

# Run with custom duration and VUs
k6 run --duration 5m --vus 50 tests/load/full-system.js

# Run with Prometheus output
k6 run --out experimental-prometheus-rw tests/load/baseline.js

# Run quick smoke test
QUICK_MODE=true ./tests/load/run-all.sh
```

### Test Configuration

Tests use `tests/load/config.js` for shared configuration:

```javascript
export const CONFIG = {
    HTTP_BASE_URL: __ENV.CCA_HTTP_URL || 'http://localhost:9200',
    WS_BASE_URL: __ENV.CCA_WS_URL || 'ws://localhost:9100',
    API_KEY: __ENV.CCA_API_KEY || 'test-api-key',

    THRESHOLDS: {
        HTTP_REQ_DURATION_P95: 2000,  // 95% under 2s
        HTTP_REQ_FAILED_RATE: 0.01,   // Less than 1% failures
        WS_MESSAGE_LATENCY_P95: 500,  // 95% under 500ms
    },
};
```

### Performance Thresholds

Default thresholds for passing tests:

| Metric | Threshold | Description |
|--------|-----------|-------------|
| http_req_duration (p95) | < 2000ms | 95th percentile response time |
| http_req_duration (p99) | < 5000ms | 99th percentile response time |
| http_req_failed | < 1% | Request failure rate |
| ws_connecting (p95) | < 1000ms | WebSocket connection time |
| ws_message_latency (p95) | < 500ms | WebSocket message latency |

## Chaos Testing

### Overview

Chaos testing uses Toxiproxy to inject network faults and test system resilience.

### Available Experiments

| Experiment | Description |
|------------|-------------|
| `redis-latency` | Adds 100-500ms latency to Redis |
| `redis-timeout` | Causes Redis connection timeouts |
| `redis-down` | Takes Redis completely offline |
| `postgres-latency` | Adds 100-500ms latency to PostgreSQL |
| `postgres-timeout` | Causes PostgreSQL connection timeouts |
| `postgres-down` | Takes PostgreSQL completely offline |
| `network-jitter` | Random latency on all connections |

### Running Chaos Experiments

```bash
# Run a single experiment for 60 seconds
./tests/chaos/run-chaos.sh redis-latency 60

# Run all experiments sequentially
./tests/chaos/run-chaos.sh all

# Clear all injected faults
./tests/chaos/run-chaos.sh clear
```

### Using Toxiproxy API Directly

```bash
# Add latency to Redis
curl -X POST http://localhost:8474/proxies/redis/toxics \
  -H "Content-Type: application/json" \
  -d '{"name": "latency", "type": "latency", "attributes": {"latency": 200}}'

# Remove the toxic
curl -X DELETE http://localhost:8474/proxies/redis/toxics/latency

# Disable Redis entirely
curl -X POST http://localhost:8474/proxies/redis \
  -H "Content-Type: application/json" \
  -d '{"enabled": false}'
```

## Monitoring

### Prometheus Metrics

CCA exposes metrics at `/metrics` endpoint:

```bash
curl http://localhost:9200/metrics
```

Key metrics:
- `cca_http_requests_total` - HTTP request count by endpoint/status
- `cca_http_request_duration_seconds` - Request latency histogram
- `cca_active_agents` - Current number of active agents
- `cca_tasks_in_progress` - Current task queue depth
- `cca_redis_connected` - Redis connection status
- `cca_postgres_connected` - PostgreSQL connection status

### Grafana Dashboards

Access Grafana at http://localhost:3000 (admin/admin):

1. **CCA Overview**: System-wide metrics and health status
2. **k6 Load Testing**: Real-time load test metrics

### Prometheus Queries

```promql
# Request rate by endpoint
sum(rate(cca_http_requests_total[5m])) by (endpoint)

# P95 latency
histogram_quantile(0.95, sum(rate(cca_http_request_duration_seconds_bucket[5m])) by (le))

# Error rate
sum(rate(cca_http_requests_total{status=~"5.."}[5m])) / sum(rate(cca_http_requests_total[5m]))
```

## CI/CD Integration

### GitHub Actions Workflow

The `performance.yml` workflow runs:

1. **On every PR**: Quick baseline test (2 min, 10 VUs)
2. **Nightly**: Full performance test suite
3. **On manual trigger**: Customizable test parameters

### Triggering Manual Tests

```bash
gh workflow run performance.yml \
  -f test_duration=5m \
  -f vus=50
```

### Performance Regression Detection

The CI pipeline:
1. Runs load tests against the PR branch
2. Compares results against baseline thresholds
3. Comments on PR with results
4. Fails if thresholds are breached

## Best Practices

### Before Running Tests

1. Ensure the test environment is isolated from production
2. Start with low load and gradually increase
3. Monitor system resources during tests
4. Run baseline tests first to establish benchmarks

### During Tests

1. Watch Grafana dashboards for anomalies
2. Check system logs for errors
3. Monitor resource utilization (CPU, memory, network)
4. Note any threshold breaches

### After Tests

1. Review test results and metrics
2. Compare against previous baselines
3. Document any performance regressions
4. Clean up test data and containers

## Troubleshooting

### Common Issues

**k6 can't connect to CCA daemon**
```bash
# Check if daemon is running
curl http://localhost:9200/health

# Check if ports are accessible
netstat -tlnp | grep 9200
```

**Prometheus not scraping metrics**
```bash
# Check Prometheus targets
curl http://localhost:9090/api/v1/targets

# Verify metrics endpoint
curl http://localhost:9200/metrics
```

**Toxiproxy not working**
```bash
# Check Toxiproxy status
curl http://localhost:8474/proxies

# Verify proxy configuration
curl http://localhost:8474/proxies/redis
```

### Getting Help

- Check the [CCA documentation](./README.md)
- Review [k6 documentation](https://k6.io/docs/)
- Check [Toxiproxy documentation](https://github.com/Shopify/toxiproxy)
- Review [Prometheus documentation](https://prometheus.io/docs/)
