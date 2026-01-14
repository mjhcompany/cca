# CCA Performance Testing Guide

This document provides comprehensive documentation for running performance tests, validating benchmark accuracy, and measuring the 30% token reduction target.

## Table of Contents

1. [Overview](#overview)
2. [Primary Performance Targets](#primary-performance-targets)
3. [Token Efficiency System](#token-efficiency-system)
4. [Running Performance Tests](#running-performance-tests)
5. [Benchmark Accuracy Validation](#benchmark-accuracy-validation)
6. [CI/CD Integration](#cicd-integration)
7. [Interpreting Results](#interpreting-results)
8. [Troubleshooting](#troubleshooting)

---

## Overview

CCA includes a comprehensive performance testing infrastructure built on:

- **k6**: Load testing framework for HTTP/WebSocket endpoints
- **Prometheus**: Metrics collection and monitoring
- **Grafana**: Visualization dashboards
- **Toxiproxy**: Chaos testing and fault injection

The testing suite measures:
- Agent spawn performance
- Message latency between agents
- API endpoint throughput
- Token efficiency and compression ratios
- Database query performance
- WebSocket throughput

---

## Primary Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Agent Spawn Time** | < 2s (P95) | Time to spawn and initialize an agent |
| **Message Latency** | < 50ms (P99) | Inter-agent message delivery time |
| **API Latency** | < 200ms (P95) | HTTP endpoint response time |
| **Token Reduction** | 30%+ | Compression ratio for context content |

### Target Definitions

1. **Agent Spawn Time (< 2s P95)**
   - Measures time from spawn request to agent ready state
   - Includes WebSocket connection and initial handshake
   - Test file: `tests/load/agent-spawning.js`

2. **Message Latency (< 50ms P99)**
   - Measures round-trip time for inter-agent messages
   - Critical for real-time agent coordination
   - Test file: `tests/load/message-latency.js`

3. **API Latency (< 200ms P95)**
   - Measures HTTP API response times
   - Includes task submission, status queries, agent management
   - Test file: `tests/load/api-throughput.js`

4. **Token Reduction (30%+)**
   - Measures compression efficiency for context content
   - Combines multiple strategies: code comments, deduplication, summarization
   - Test file: `tests/load/token-service.js`

---

## Token Efficiency System

### Architecture

The token efficiency system (`crates/cca-daemon/src/tokens.rs`) provides:

```
┌─────────────────────────────────────────────────────────────────┐
│                     Token Service                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │ TokenCounter │    │ Context      │    │ Context          │   │
│  │              │───▶│ Analyzer     │───▶│ Compressor       │   │
│  │ (~4 char/tok)│    │ (Redundancy) │    │ (Strategies)     │   │
│  └──────────────┘    └──────────────┘    └──────────────────┘   │
│         │                   │                    │               │
│         └───────────────────┼────────────────────┘               │
│                             ▼                                    │
│                   ┌──────────────────┐                           │
│                   │  Token Metrics   │                           │
│                   │  (Per-agent &    │                           │
│                   │   Global)        │                           │
│                   └──────────────────┘                           │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Components

#### 1. TokenCounter

Estimates token count using BPE-like approximation:
- ~4 characters per token (conservative estimate)
- Adjusts for whitespace and punctuation
- Considers code vs prose differences

```rust
// Token estimation heuristic
let word_estimate = (words as f64 * 1.3) as u32;  // ~1.3 tokens per word
let char_estimate = (chars as f64 / 4.0) as u32;  // ~4 chars per token
token_count = max(word_estimate, char_estimate);
```

#### 2. ContextAnalyzer

Detects redundancy and compression potential:
- N-gram similarity detection (Jaccard similarity)
- Repeated line detection
- Code block density analysis
- Verbosity scoring (filler words, structural overhead)

#### 3. ContextCompressor

Applies compression strategies:

| Strategy | Description | Typical Reduction |
|----------|-------------|-------------------|
| `code_comments` | Removes single-line comments from code blocks | 10-30% |
| `deduplication` | Removes duplicate lines across contexts | 5-20% |
| `history` | Prunes old conversation history | 20-50% |
| `summarize` | Truncates middle of long content | 30-50% |

### Measuring the 30% Target

The 30% token reduction target is measured as:

```
reduction_ratio = tokens_saved / original_tokens
```

Where:
- `original_tokens`: Token count before compression
- `tokens_saved`: `original_tokens - compressed_tokens`
- Target: `reduction_ratio >= 0.30`

#### API Endpoints

1. **POST /api/v1/tokens/analyze**
   ```json
   {
     "content": "code or text content",
     "agent_id": "optional-agent-id"
   }
   ```
   Response:
   ```json
   {
     "token_count": 1250,
     "redundancy": 0.15,
     "compression_potential": 0.35,
     "code_block_count": 3,
     "long_line_count": 5
   }
   ```

2. **POST /api/v1/tokens/compress**
   ```json
   {
     "content": "content to compress",
     "target_reduction": 0.3,
     "strategies": ["code_comments", "deduplication", "summarize"],
     "agent_id": "optional-agent-id"
   }
   ```
   Response:
   ```json
   {
     "compressed_content": "compressed...",
     "original_tokens": 1250,
     "compressed_tokens": 875,
     "reduction_ratio": 0.30,
     "strategies_applied": ["code_comments", "deduplication"]
   }
   ```

3. **GET /api/v1/tokens/metrics**
   ```json
   {
     "total_tokens_used": 125000,
     "total_tokens_saved": 45000,
     "compression_ratio": 0.36,
     "agents_tracked": 12
   }
   ```

### Verifying Token Reduction

To verify the 30% target is being met:

```bash
# Run the token service load test
k6 run tests/load/token-service.js

# Check the compression_ratio metric in results
# Should show avg reduction >= 30%
```

The load test:
1. Generates sample code content (500 chars to 50KB)
2. Sends analyze requests to measure token counts
3. Sends compress requests with `target_reduction: 0.3`
4. Records actual compression ratios achieved
5. Reports average reduction in summary

---

## Running Performance Tests

### Prerequisites

1. **Install k6**
   ```bash
   # macOS
   brew install k6

   # Linux
   sudo gpg -k
   sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
       --keyserver hkp://keyserver.ubuntu.com:80 \
       --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
   echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" \
       | sudo tee /etc/apt/sources.list.d/k6.list
   sudo apt-get update && sudo apt-get install k6
   ```

2. **Start CCA Services**
   ```bash
   # Start infrastructure
   docker compose up -d

   # Start CCA daemon
   cargo run --bin ccad
   ```

### Running All Tests

```bash
# Run complete test suite
./tests/load/run-all.sh

# Quick mode (30s per test)
./tests/load/run-all.sh -q

# Run primary targets only
./tests/load/run-all.sh -t primary
```

### Running Individual Tests

```bash
# Agent spawning (< 2s target)
k6 run tests/load/agent-spawning.js

# Message latency (< 50ms P99 target)
k6 run tests/load/message-latency.js

# API throughput (< 200ms P95 target)
k6 run tests/load/api-throughput.js

# Token service (30% reduction target)
k6 run tests/load/token-service.js

# Full system integration
k6 run tests/load/full-system.js
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CCA_HTTP_URL` | `http://localhost:9200` | CCA HTTP API URL |
| `CCA_WS_URL` | `ws://localhost:9100` | CCA WebSocket URL |
| `CCA_API_KEY` | `test-api-key` | API authentication key |

### Test Configuration

All tests use shared configuration from `tests/load/config.js`:

```javascript
export const CONFIG = {
    THRESHOLDS: {
        AGENT_SPAWN_P95: 2000,      // < 2s
        MESSAGE_LATENCY_P99: 50,    // < 50ms
        HTTP_REQ_DURATION_P95: 200, // < 200ms
        TOKEN_ANALYZE_P95: 500,     // < 500ms
        TOKEN_COMPRESS_P95: 2000,   // < 2s
    },
};
```

---

## Benchmark Accuracy Validation

### Token Counter Accuracy

The token counter uses a ~4 char/token estimate which is validated against:
- GPT-4 tokenization patterns
- Claude tokenization patterns
- Typical code and prose content

**Validation test:**
```bash
cargo test --test token_service_integration
```

This runs tests for:
- `test_token_counter_code`: Verifies code token estimation
- `test_token_counter_prose`: Verifies prose token estimation
- `test_compression_target`: Verifies 30% reduction achievable

### Compression Ratio Accuracy

The compression ratio is calculated as:
```
actual_reduction = 1.0 - (compressed_length / original_length)
```

Where lengths are measured in estimated tokens, not raw characters.

**Validation:**
1. `test_code_comment_removal`: Verifies comment stripping
2. `test_deduplication`: Verifies duplicate removal
3. `test_end_to_end_token_processing`: Verifies full pipeline

### Load Test Metric Accuracy

k6 uses precise timing for all metrics:
- `Trend` metrics record individual values with timestamps
- Percentiles (P95, P99) are calculated from all recorded values
- Success/failure rates use `Rate` metric type

**Threshold validation in k6:**
```javascript
thresholds: {
    'token_analyze_latency': ['p(95)<500', 'p(99)<1000'],
    'compress_success': ['rate>0.95'],
    'compression_ratio': ['avg<0.8'], // avg ratio < 0.8 means > 20% reduction
}
```

---

## CI/CD Integration

### GitHub Actions Workflow

Performance tests run automatically via `.github/workflows/performance.yml`:

**Triggers:**
- Push to `main` or `develop`
- Pull requests to `main` or `develop`
- Nightly schedule (2 AM UTC)
- Manual workflow dispatch

**Workflow Steps:**
1. Start PostgreSQL and Redis services
2. Build CCA daemon
3. Run baseline performance test
4. Parse results and check thresholds
5. Comment on PR with results (if applicable)
6. Fail if thresholds breached

### Manual Trigger

```bash
gh workflow run performance.yml \
  -f test_duration=5m \
  -f vus=50
```

### PR Comments

For pull requests, the workflow posts a comment with:
- P95 latency vs threshold
- Error rate
- Pass/fail status
- Link to detailed results

---

## Interpreting Results

### Report Generation

After running tests, generate a comprehensive report:

```bash
# Automatic (via run-all.sh)
./tests/load/run-all.sh

# Manual
node tests/load/generate-report.js tests/load/results/
```

### Report Contents

The HTML report (`results/load-test-report.html`) includes:

1. **Overall Grade**: A/B/C/F based on thresholds
2. **Latency Metrics**: avg, P95, P99, max for all operations
3. **Success Rates**: Per-test success percentages
4. **Throughput**: Requests per second
5. **Recommendations**: Actionable optimization suggestions

### Grade Thresholds

| Grade | P95 Latency | Success Rate |
|-------|-------------|--------------|
| **A** (Excellent) | < 500ms | > 99% |
| **B** (Good) | < 1000ms | > 95% |
| **C** (Acceptable) | < 2000ms | > 90% |
| **F** (Failing) | > 2000ms | < 90% |

### Token Service Metrics

The token service test reports:

| Metric | Description | Target |
|--------|-------------|--------|
| `token_analyze_latency` | Time to analyze content | P95 < 500ms |
| `token_compress_latency` | Time to compress content | P95 < 2000ms |
| `compression_ratio` | Average compression achieved | > 30% reduction |
| `tokens_analyzed_total` | Total tokens processed | - |
| `compression_savings_bytes` | Total bytes saved | - |

---

## Troubleshooting

### Common Issues

**1. High Agent Spawn Time**
- Check database connection pool
- Verify Redis is responding
- Review agent initialization code

**2. High Message Latency**
- Check WebSocket connection count
- Review Redis pub/sub performance
- Check network configuration

**3. Low Compression Ratio**
- Content may have low redundancy
- Try different compression strategies
- Check for content types (code vs prose)

**4. Test Failures**
```bash
# Check CCA daemon is running
curl http://localhost:9200/health

# Check database connectivity
curl http://localhost:9200/api/v1/postgres/status

# Check Redis connectivity
curl http://localhost:9200/api/v1/redis/status
```

### Debug Mode

```bash
# Verbose k6 output
k6 run --verbose tests/load/token-service.js

# HTTP debug
k6 run --http-debug tests/load/api-throughput.js
```

### Viewing Metrics

```bash
# Prometheus metrics
curl http://localhost:9200/metrics

# Token-specific metrics
curl http://localhost:9200/metrics | grep token

# Agent metrics
curl http://localhost:9200/metrics | grep agent
```

---

## Quick Reference

### Running Performance Tests

```bash
# All tests
./tests/load/run-all.sh

# Quick mode
./tests/load/run-all.sh -q

# Primary targets only
./tests/load/run-all.sh -t primary

# Token service only
./tests/load/run-all.sh -t token
```

### Key Files

| File | Purpose |
|------|---------|
| `tests/load/config.js` | Shared configuration and thresholds |
| `tests/load/token-service.js` | Token efficiency tests |
| `tests/load/agent-spawning.js` | Agent spawn performance |
| `tests/load/message-latency.js` | Message latency tests |
| `tests/load/run-all.sh` | Test runner script |
| `tests/load/generate-report.js` | Report generator |
| `crates/cca-daemon/src/tokens.rs` | Token service implementation |

### Performance Targets Summary

| Metric | Target | Test |
|--------|--------|------|
| Agent spawn | < 2s (P95) | `agent-spawning.js` |
| Message latency | < 50ms (P99) | `message-latency.js` |
| API latency | < 200ms (P95) | `api-throughput.js` |
| Token reduction | 30%+ | `token-service.js` |
