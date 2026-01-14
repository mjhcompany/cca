/**
 * CCA Load Test: API Endpoint Throughput
 *
 * Tests HTTP API endpoint throughput and latency under various load conditions.
 * Focuses on:
 * - Endpoint response times under load
 * - Request throughput (requests/second)
 * - Error rates under heavy load
 * - Latency distribution (P50, P95, P99)
 *
 * Run with: k6 run api-throughput.js
 * Run specific scenario: k6 run -e SCENARIO=high_throughput api-throughput.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId, getRandomTask, getRandomPriority } from './config.js';

// Custom metrics for API throughput analysis
const apiLatency = new Trend('api_latency', true);
const apiThroughput = new Rate('api_throughput');
const apiSuccess = new Rate('api_success');
const apiErrors = new Counter('api_errors');
const requestsPerSecond = new Gauge('requests_per_second');

// Per-endpoint metrics
const healthLatency = new Trend('health_endpoint_latency', true);
const statusLatency = new Trend('status_endpoint_latency', true);
const agentsLatency = new Trend('agents_endpoint_latency', true);
const tasksLatency = new Trend('tasks_endpoint_latency', true);
const workloadsLatency = new Trend('workloads_endpoint_latency', true);
const tokensLatency = new Trend('tokens_endpoint_latency', true);
const memoryLatency = new Trend('memory_endpoint_latency', true);

// Test scenarios with different throughput patterns
export const options = {
    scenarios: {
        // Scenario 1: Baseline - low constant load
        baseline: {
            executor: 'constant-arrival-rate',
            rate: 50,
            timeUnit: '1s',
            duration: '1m',
            preAllocatedVUs: 20,
            maxVUs: 50,
            tags: { scenario: 'baseline' },
        },
        // Scenario 2: Medium throughput - 100 req/s
        medium_throughput: {
            executor: 'constant-arrival-rate',
            rate: 100,
            timeUnit: '1s',
            duration: '2m',
            preAllocatedVUs: 50,
            maxVUs: 100,
            startTime: '1m30s',
            tags: { scenario: 'medium' },
        },
        // Scenario 3: High throughput - 200 req/s
        high_throughput: {
            executor: 'constant-arrival-rate',
            rate: 200,
            timeUnit: '1s',
            duration: '2m',
            preAllocatedVUs: 100,
            maxVUs: 200,
            startTime: '4m',
            tags: { scenario: 'high' },
        },
        // Scenario 4: Stress - 500 req/s (find breaking point)
        stress_throughput: {
            executor: 'constant-arrival-rate',
            rate: 500,
            timeUnit: '1s',
            duration: '2m',
            preAllocatedVUs: 150,
            maxVUs: 300,
            startTime: '6m30s',
            tags: { scenario: 'stress' },
        },
        // Scenario 5: Ramping throughput
        ramping_throughput: {
            executor: 'ramping-arrival-rate',
            startRate: 10,
            timeUnit: '1s',
            stages: [
                { duration: '30s', target: 50 },
                { duration: '1m', target: 100 },
                { duration: '1m', target: 200 },
                { duration: '30s', target: 300 },
                { duration: '30s', target: 100 },
                { duration: '30s', target: 50 },
            ],
            preAllocatedVUs: 100,
            maxVUs: 200,
            startTime: '9m',
            tags: { scenario: 'ramping' },
        },
    },
    thresholds: {
        // Overall API latency targets
        'api_latency': ['p(95)<200', 'p(99)<500'],
        'api_success': ['rate>0.99'],

        // Per-endpoint latency targets (P95 < 200ms)
        'health_endpoint_latency': ['p(95)<100', 'p(99)<200'],
        'status_endpoint_latency': ['p(95)<200', 'p(99)<500'],
        'agents_endpoint_latency': ['p(95)<200', 'p(99)<500'],
        'tasks_endpoint_latency': ['p(95)<200', 'p(99)<500'],
        'workloads_endpoint_latency': ['p(95)<200', 'p(99)<500'],
        'tokens_endpoint_latency': ['p(95)<300', 'p(99)<700'],

        // HTTP metrics
        'http_req_duration': ['p(95)<200', 'p(99)<500'],
        'http_req_failed': ['rate<0.01'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)', 'count'],
};

// API endpoints to test with their weights (frequency of testing)
const ENDPOINTS = [
    { method: 'GET', path: '/health', weight: 20, metric: healthLatency, auth: false },
    { method: 'GET', path: '/api/v1/status', weight: 15, metric: statusLatency, auth: true },
    { method: 'GET', path: '/api/v1/agents', weight: 15, metric: agentsLatency, auth: true },
    { method: 'GET', path: '/api/v1/tasks', weight: 15, metric: tasksLatency, auth: true },
    { method: 'GET', path: '/api/v1/workloads', weight: 10, metric: workloadsLatency, auth: true },
    { method: 'GET', path: '/api/v1/activity', weight: 10, metric: statusLatency, auth: true },
    { method: 'GET', path: '/api/v1/tokens/metrics', weight: 10, metric: tokensLatency, auth: true },
    { method: 'GET', path: '/api/v1/rl/stats', weight: 5, metric: statusLatency, auth: true },
];

// Calculate total weight for random selection
const TOTAL_WEIGHT = ENDPOINTS.reduce((sum, e) => sum + e.weight, 0);

// Select endpoint based on weight distribution
function selectEndpoint() {
    let random = Math.random() * TOTAL_WEIGHT;
    for (const endpoint of ENDPOINTS) {
        random -= endpoint.weight;
        if (random <= 0) {
            return endpoint;
        }
    }
    return ENDPOINTS[0];
}

// Setup function
export function setup() {
    console.log('=== CCA API Throughput Load Test ===');
    console.log(`Target URL: ${CONFIG.HTTP_BASE_URL}`);
    console.log('');

    // Verify API is accessible
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    if (healthRes.status !== 200) {
        console.error('Health check failed! Aborting test.');
        return { abort: true };
    }

    console.log('[OK] API is healthy');
    console.log('');

    return {
        startTime: Date.now(),
        abort: false,
    };
}

// Main test function - high-frequency API calls
export default function(data) {
    if (data && data.abort) {
        return;
    }

    const endpoint = selectEndpoint();
    const headers = endpoint.auth ? getAuthHeaders() : { 'Content-Type': 'application/json' };
    const url = `${CONFIG.HTTP_BASE_URL}${endpoint.path}`;

    const start = Date.now();
    const res = http.request(endpoint.method, url, null, {
        headers: headers,
        tags: { name: endpoint.path.replace('/api/v1/', ''), endpoint: endpoint.path },
        timeout: '10s',
    });
    const duration = Date.now() - start;

    // Record metrics
    apiLatency.add(duration);
    endpoint.metric.add(duration);

    // Check success
    const success = check(res, {
        'status is 200': (r) => r.status === 200,
        'response time < 500ms': () => duration < 500,
    });

    if (success) {
        apiSuccess.add(1);
        apiThroughput.add(1);
    } else {
        apiSuccess.add(0);
        apiThroughput.add(0);
        apiErrors.add(1);

        // Log errors occasionally
        if (Math.random() < 0.1) {
            console.warn(`[${endpoint.path}] Status: ${res.status}, Duration: ${duration}ms`);
        }
    }
}

// Teardown function
export function teardown(data) {
    if (data && data.abort) {
        return;
    }

    const duration = (Date.now() - data.startTime) / 1000;
    console.log('');
    console.log('=== API Throughput Test Complete ===');
    console.log(`Test duration: ${duration.toFixed(2)}s`);
}

// Custom summary handler
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'api-throughput',
        description: 'HTTP API endpoint throughput and latency test',
        metrics: {
            overall: {
                latency: data.metrics.api_latency,
                success_rate: data.metrics.api_success,
                errors: data.metrics.api_errors,
                throughput: data.metrics.api_throughput,
            },
            endpoints: {
                health: data.metrics.health_endpoint_latency,
                status: data.metrics.status_endpoint_latency,
                agents: data.metrics.agents_endpoint_latency,
                tasks: data.metrics.tasks_endpoint_latency,
                workloads: data.metrics.workloads_endpoint_latency,
                tokens: data.metrics.tokens_endpoint_latency,
            },
            http: {
                requests: data.metrics.http_reqs,
                duration: data.metrics.http_req_duration,
                failed: data.metrics.http_req_failed,
            },
        },
        thresholds: data.thresholds,
    };

    return {
        'results/api-throughput-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

function textSummary(data) {
    const lines = [
        '',
        '╔══════════════════════════════════════════════════════════════════════╗',
        '║              CCA API THROUGHPUT LOAD TEST RESULTS                    ║',
        '╠══════════════════════════════════════════════════════════════════════╣',
    ];

    // Overall metrics
    lines.push('║ OVERALL API PERFORMANCE                                              ║');
    lines.push('║                                                                      ║');

    if (data.metrics.api_latency) {
        const lat = data.metrics.api_latency.values;
        lines.push(`║   Latency (ms):    avg: ${lat.avg.toFixed(0).padStart(6)} | p95: ${lat['p(95)'].toFixed(0).padStart(6)} | p99: ${lat['p(99)'].toFixed(0).padStart(6)} | max: ${lat.max.toFixed(0).padStart(6)} ║`);
    }

    if (data.metrics.api_success) {
        const rate = (data.metrics.api_success.values.rate * 100).toFixed(3);
        lines.push(`║   Success Rate:    ${rate.padStart(7)}%                                             ║`);
    }

    if (data.metrics.http_reqs) {
        const reqs = data.metrics.http_reqs.values;
        const rps = reqs.rate ? reqs.rate.toFixed(2) : '0.00';
        lines.push(`║   Throughput:      ${rps.padStart(8)} req/s (${reqs.count} total)                      ║`);
    }

    lines.push('║                                                                      ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════╣');

    // Per-endpoint metrics
    lines.push('║ ENDPOINT LATENCIES (P95 target: <200ms)                              ║');
    lines.push('║                                                                      ║');

    const endpointMetrics = [
        { name: 'Health', metric: data.metrics.health_endpoint_latency },
        { name: 'Status', metric: data.metrics.status_endpoint_latency },
        { name: 'Agents', metric: data.metrics.agents_endpoint_latency },
        { name: 'Tasks', metric: data.metrics.tasks_endpoint_latency },
        { name: 'Workloads', metric: data.metrics.workloads_endpoint_latency },
        { name: 'Tokens', metric: data.metrics.tokens_endpoint_latency },
    ];

    for (const ep of endpointMetrics) {
        if (ep.metric) {
            const lat = ep.metric.values;
            const p95Status = lat['p(95)'] < 200 ? '✓' : '✗';
            lines.push(`║   ${ep.name.padEnd(12)} p95: ${lat['p(95)'].toFixed(0).padStart(6)}ms  p99: ${lat['p(99)'].toFixed(0).padStart(6)}ms  avg: ${lat.avg.toFixed(0).padStart(6)}ms  ${p95Status} ║`);
        }
    }

    lines.push('║                                                                      ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════╣');

    // Threshold results
    lines.push('║ THRESHOLD RESULTS                                                    ║');
    lines.push('║                                                                      ║');

    if (data.thresholds) {
        const passed = Object.values(data.thresholds).filter(t => t.ok).length;
        const total = Object.keys(data.thresholds).length;
        const status = passed === total ? 'PASS' : 'FAIL';
        lines.push(`║   Thresholds: ${passed}/${total} ${status}                                                   ║`);
    }

    lines.push('║                                                                      ║');
    lines.push('╚══════════════════════════════════════════════════════════════════════╝');
    lines.push('');

    return lines.join('\n');
}
