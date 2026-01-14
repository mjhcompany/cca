# CCA Load Test Suite

Comprehensive load testing suite for the CCA (Claude Code Agentic) system using [k6](https://k6.io/).

## Primary Performance Targets

| Metric | Target | Test File |
|--------|--------|-----------|
| **Agent Spawn Time** | **<2s (P95)** | `agent-spawning.js` |
| **Message Latency** | **<50ms (P99)** | `message-latency.js` |
| **API Endpoint Latency** | **<200ms (P95)** | `api-throughput.js` |

## Overview

This test suite measures system performance under various load conditions:

| Test | Description | Key Metrics |
|------|-------------|-------------|
| `api-throughput.js` | API endpoint throughput testing | P95/P99 latency, req/s |
| `agent-spawning.js` | Concurrent agent spawning (10, 50, 100 agents) | Spawn time (<2s target), success rate |
| `message-latency.js` | Message latency under load | P99 latency (<50ms target) |
| `token-service.js` | Token analysis and compression | Analysis latency, compression ratio |
| `websocket-throughput.js` | ACP WebSocket message throughput | Connection time, message latency |
| `redis-pubsub.js` | Redis pub/sub performance | Broadcast latency, throughput |
| `postgres-queries.js` | PostgreSQL query performance | Query duration, connection pool |
| `task-submission.js` | Task queue and coordinator routing | Queue latency, routing time |
| `full-system.js` | Full system integration test | All metrics combined |

## Prerequisites

### Required
- [k6](https://k6.io/docs/getting-started/installation/) - Load testing tool
- Running CCA daemon (`ccad`)
- PostgreSQL and Redis (via docker-compose)

### Optional
- Node.js (for report generation)

### Installing k6

```bash
# macOS
brew install k6

# Debian/Ubuntu
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
    --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | \
    sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update && sudo apt-get install k6

# Docker
docker pull grafana/k6
```

## Quick Start

```bash
# Start CCA infrastructure
cd /path/to/cca
docker-compose up -d
cargo run --bin ccad &

# Run all load tests
./run-all.sh

# Run specific test
./run-all.sh -t agent

# Quick mode (shorter durations)
./run-all.sh -q
```

## Running Individual Tests

```bash
# Agent spawning test (10, 50, 100 concurrent agents)
k6 run agent-spawning.js

# WebSocket throughput test
k6 run websocket-throughput.js

# Redis pub/sub test
k6 run redis-pubsub.js

# PostgreSQL query test
k6 run postgres-queries.js

# Full system integration test
k6 run full-system.js
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CCA_HTTP_URL` | `http://localhost:9200` | CCA HTTP API URL |
| `CCA_WS_URL` | `ws://localhost:9100` | CCA WebSocket URL |
| `CCA_API_KEY` | `test-api-key` | API key for authentication |

### Example with custom configuration

```bash
CCA_HTTP_URL=http://prod:9200 \
CCA_WS_URL=ws://prod:9100 \
CCA_API_KEY=my-api-key \
k6 run agent-spawning.js
```

## Test Scenarios

### Agent Spawning (`agent-spawning.js`)

Tests concurrent agent spawning with three scenarios:

| Scenario | VUs | Duration | Description |
|----------|-----|----------|-------------|
| ten_agents | 10 | 2m | Low concurrency baseline |
| fifty_agents | 50 | 5m | Medium concurrency |
| hundred_agents | 100 | 10m | High concurrency stress |

**Thresholds:**
- **Agent spawn p95 < 2000ms** (PRIMARY TARGET: <2s)
- Agent spawn p99 < 3000ms
- Success rate > 95%

### API Throughput (`api-throughput.js`)

Tests HTTP API endpoint throughput and latency:

| Scenario | Rate (req/s) | Duration | Description |
|----------|--------------|----------|-------------|
| baseline | 50 | 1m | Low constant load |
| medium_throughput | 100 | 2m | Medium throughput |
| high_throughput | 200 | 2m | High throughput |
| stress_throughput | 500 | 2m | Stress test |
| ramping_throughput | 10→300→50 | ~4m | Ramping load |

**Thresholds:**
- API latency p95 < 200ms
- API latency p99 < 500ms
- Per-endpoint p95 < 200ms

### Message Latency (`message-latency.js`)

**PRIMARY TARGET: P99 < 50ms**

Tests inter-agent message latency:

| Scenario | VUs | Duration | Description |
|----------|-----|----------|-------------|
| low_load_latency | 5 | 1m | Baseline latency |
| medium_load_latency | 20 | 2m | Medium load |
| high_load_latency | 50 | 2m | High load |
| stress_latency | 100 | 2m | Stress test |
| high_frequency | 500 msg/s | 1m | High message rate |
| sustained | 30 | 3m | Sustained load |

**Thresholds:**
- Message latency p50 < 20ms
- Message latency p95 < 40ms
- **Message latency p99 < 50ms** (PRIMARY TARGET)

### Token Service (`token-service.js`)

Tests token analysis and compression performance:

| Scenario | Load | Duration | Description |
|----------|------|----------|-------------|
| baseline | 5 VUs | 1m | Light load |
| medium_load | 20 VUs | 2m | Mixed operations |
| high_load | 50 VUs | 2m | Concurrent analysis |
| compression_focus | 50 req/s | 2m | Heavy compression |
| analysis_focus | 100 req/s | 2m | High-frequency analysis |
| large_content | 20 VUs × 5 | 3m | Large content stress |

**Thresholds:**
- Token analysis p95 < 500ms
- Token compression p95 < 2000ms
- Metrics retrieval p95 < 200ms

### WebSocket Throughput (`websocket-throughput.js`)

Tests ACP WebSocket message handling:

| Scenario | VUs | Messages/VU | Description |
|----------|-----|-------------|-------------|
| low_connections | 10 | 100 | High message rate, few connections |
| medium_connections | 50 | 50 | Balanced workload |
| high_connections | 100 | 30 | Many connections, moderate messages |
| spike | 10→100→10 | 20 | Sudden connection surge |

**Thresholds:**
- Connection time p95 < 2000ms
- Message latency p95 < 500ms

### Redis Pub/Sub (`redis-pubsub.js`)

Tests Redis messaging performance:

| Scenario | Rate (msg/s) | Duration | Description |
|----------|--------------|----------|-------------|
| low_rate | 10 | 1m | Baseline |
| medium_rate | 50 | 2m | Normal load |
| high_rate | 100 | 3m | Heavy load |
| burst | 10→200→10 | ~2m | Message flood |

**Thresholds:**
- Broadcast p95 < 1000ms
- Success rate > 95%

### PostgreSQL Queries (`postgres-queries.js`)

Tests database operations:

| Scenario | VUs | Duration | Description |
|----------|-----|----------|-------------|
| low_concurrency | 10 | 2m | Baseline |
| medium_concurrency | 50 | 3m | Normal load |
| high_concurrency | 100 | 5m | Heavy load |
| rate_limited | 200 req/s | 2m | Rate limiting |

**Operations tested:**
- Task creation
- Task queries
- Memory/pattern search (vector similarity)
- Agent state queries
- RL experience queries

**Thresholds:**
- Task create p95 < 2000ms
- Query p95 < 1000ms
- Memory search p95 < 3000ms

### Full System (`full-system.js`)

Comprehensive integration test with mixed workloads:

| Scenario | Duration | Description |
|----------|----------|-------------|
| normal_operation | 15m | Mixed workload, realistic usage |
| api_heavy | 5m | High API request rate |
| db_heavy | 5m | Database-intensive operations |
| stress | 6.5m | Maximum load (200 VUs) |

## Generating Reports

After running tests, generate a comprehensive HTML report:

```bash
# Using the runner script
./run-all.sh  # Automatically generates report

# Manual generation
node generate-report.js results/
```

### Report Contents

The generated report includes:

- **Overall Grade**: A/B/C/F based on thresholds
- **Latency Metrics**: avg, p95, p99, max for all operations
- **Success Rates**: Request success percentages
- **Throughput**: Requests per second
- **Recommendations**: Actionable optimization suggestions

### Report Files

| File | Format | Description |
|------|--------|-------------|
| `results/load-test-report.html` | HTML | Visual report with charts |
| `results/load-test-report.json` | JSON | Machine-readable metrics |
| `results/*-results.json` | JSON | Individual test results |

## Thresholds and Grading

### Grade Thresholds

| Grade | Latency p95 | Success Rate |
|-------|-------------|--------------|
| A (Excellent) | < 500ms | > 99% |
| B (Good) | < 1000ms | > 95% |
| C (Acceptable) | < 2000ms | > 90% |
| F (Failing) | > 2000ms | < 90% |

### Default k6 Thresholds

```javascript
thresholds: {
    'http_req_duration': ['p(95)<2000', 'p(99)<5000'],
    'http_req_failed': ['rate<0.05'],
}
```

## Extending Tests

### Adding Custom Scenarios

```javascript
// In any test file
export const options = {
    scenarios: {
        custom_scenario: {
            executor: 'constant-vus',
            vus: 25,
            duration: '3m',
            tags: { scenario: 'custom' },
        },
    },
};
```

### Adding Custom Metrics

```javascript
import { Trend, Counter, Rate } from 'k6/metrics';

const customLatency = new Trend('custom_latency', true);
const customErrors = new Counter('custom_errors');
const customSuccess = new Rate('custom_success');

export default function() {
    const start = Date.now();
    // ... test code ...
    customLatency.add(Date.now() - start);
}
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Load Tests
on:
  schedule:
    - cron: '0 2 * * *'  # Daily at 2 AM
  workflow_dispatch:

jobs:
  load-test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: pgvector/pgvector:pg16
        ports: ['5432:5432']
      redis:
        image: redis:7
        ports: ['6379:6379']

    steps:
      - uses: actions/checkout@v4

      - name: Install k6
        run: |
          sudo gpg -k
          sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
              --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
          echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | \
              sudo tee /etc/apt/sources.list.d/k6.list
          sudo apt-get update && sudo apt-get install k6

      - name: Start CCA
        run: cargo run --bin ccad &

      - name: Run Load Tests
        run: ./tests/load/run-all.sh -q

      - name: Upload Results
        uses: actions/upload-artifact@v4
        with:
          name: load-test-results
          path: tests/load/results/
```

## Troubleshooting

### Common Issues

**k6 not found**
```bash
# Verify installation
k6 version

# Check PATH
which k6
```

**Connection refused**
```bash
# Check CCA is running
curl http://localhost:9200/health

# Start CCA daemon
cargo run --bin ccad
```

**High error rates**
- Check rate limiting configuration
- Verify database connections
- Review CCA daemon logs

**Slow tests**
- Use quick mode: `./run-all.sh -q`
- Run individual tests: `k6 run agent-spawning.js`
- Adjust VU count: `k6 run --vus 10 test.js`

### Debug Mode

```bash
# Verbose output
k6 run --verbose agent-spawning.js

# HTTP debug
k6 run --http-debug agent-spawning.js
```

## Architecture

```
tests/load/
├── config.js              # Shared configuration
├── agent-spawning.js      # Agent spawn tests
├── websocket-throughput.js # WebSocket tests
├── redis-pubsub.js        # Redis pub/sub tests
├── postgres-queries.js    # PostgreSQL tests
├── full-system.js         # Integration tests
├── generate-report.js     # Report generator
├── run-all.sh            # Test runner script
├── README.md             # This file
└── results/              # Test results (generated)
    ├── *-results.json    # Individual test results
    ├── load-test-report.html  # HTML report
    └── load-test-report.json  # JSON report
```

## License

Part of the CCA project.
