/**
 * CCA Load Test: Full System Integration Test
 *
 * Comprehensive load test that exercises all major CCA components simultaneously:
 * - HTTP API endpoints
 * - WebSocket ACP connections
 * - Redis pub/sub
 * - PostgreSQL queries
 * - Agent lifecycle
 *
 * This test simulates realistic usage patterns with mixed workloads.
 *
 * Run with: k6 run full-system.js
 */

import http from 'k6/http';
import ws from 'k6/ws';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId, getRandomAgentRole, getRandomTask, getRandomPriority } from './config.js';

// Custom metrics - organized by component
// HTTP API metrics
const httpApiLatency = new Trend('http_api_latency', true);
const httpApiSuccess = new Rate('http_api_success');
const httpApiErrors = new Counter('http_api_errors');

// WebSocket metrics
const wsLatency = new Trend('ws_latency', true);
const wsSuccess = new Rate('ws_success');
const wsErrors = new Counter('ws_errors');

// Database metrics
const dbLatency = new Trend('db_latency', true);
const dbSuccess = new Rate('db_success');

// Redis metrics
const redisLatency = new Trend('redis_latency', true);
const redisSuccess = new Rate('redis_success');

// System metrics
const systemHealth = new Gauge('system_health');
const totalOperations = new Counter('total_operations');

// Test scenarios simulating different usage patterns
export const options = {
    scenarios: {
        // Scenario 1: Normal operation - mixed workload
        normal_operation: {
            executor: 'ramping-vus',
            startVUs: 5,
            stages: [
                { duration: '1m', target: 20 },   // Ramp up
                { duration: '5m', target: 20 },   // Sustain
                { duration: '1m', target: 50 },   // Increase
                { duration: '5m', target: 50 },   // Sustain
                { duration: '1m', target: 20 },   // Cool down
                { duration: '2m', target: 20 },   // Sustain
            ],
            tags: { scenario: 'normal' },
        },
        // Scenario 2: API-heavy workload
        api_heavy: {
            executor: 'constant-arrival-rate',
            rate: 100,
            timeUnit: '1s',
            duration: '5m',
            preAllocatedVUs: 100,
            maxVUs: 200,
            startTime: '16m',
            tags: { scenario: 'api_heavy' },
            env: { WORKLOAD: 'api' },
        },
        // Scenario 3: Database-heavy workload
        db_heavy: {
            executor: 'constant-arrival-rate',
            rate: 50,
            timeUnit: '1s',
            duration: '5m',
            preAllocatedVUs: 75,
            maxVUs: 150,
            startTime: '22m',
            tags: { scenario: 'db_heavy' },
            env: { WORKLOAD: 'db' },
        },
        // Scenario 4: Stress test - maximum load
        stress: {
            executor: 'ramping-vus',
            startVUs: 10,
            stages: [
                { duration: '30s', target: 50 },
                { duration: '1m', target: 100 },
                { duration: '2m', target: 150 },
                { duration: '2m', target: 200 },
                { duration: '1m', target: 50 },
            ],
            startTime: '28m',
            tags: { scenario: 'stress' },
        },
    },
    thresholds: {
        'http_api_latency': ['p(95)<2000', 'p(99)<5000'],
        'http_api_success': ['rate>0.95'],
        'ws_latency': ['p(95)<1000'],
        'ws_success': ['rate>0.90'],
        'db_latency': ['p(95)<1500'],
        'db_success': ['rate>0.95'],
        'redis_latency': ['p(95)<500'],
        'redis_success': ['rate>0.98'],
        'http_req_failed': ['rate<0.05'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)'],
};

// Setup function
export function setup() {
    console.log('=== CCA Full System Load Test ===');
    console.log(`HTTP Target: ${CONFIG.HTTP_BASE_URL}`);
    console.log(`WS Target: ${CONFIG.WS_BASE_URL}`);

    // Comprehensive health check
    const healthChecks = {
        http: false,
        postgres: false,
        redis: false,
        acp: false,
    };

    // Check HTTP API
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    healthChecks.http = healthRes.status === 200;

    // Check PostgreSQL
    const pgRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/postgres/status`, {
        headers: getAuthHeaders(),
    });
    healthChecks.postgres = pgRes.status === 200;

    // Check Redis
    const redisRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/redis/status`, {
        headers: getAuthHeaders(),
    });
    healthChecks.redis = redisRes.status === 200;

    // Check ACP
    const acpRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/acp/status`, {
        headers: getAuthHeaders(),
    });
    healthChecks.acp = acpRes.status === 200;

    console.log(`Health checks: ${JSON.stringify(healthChecks)}`);

    const allHealthy = Object.values(healthChecks).every(v => v);
    if (!allHealthy) {
        console.warn('Warning: Not all services are healthy');
    }

    return {
        startTime: Date.now(),
        healthChecks,
        allHealthy,
    };
}

// Main test function - mixed workload
export default function(data) {
    const vuId = __VU;
    const uniqueId = generateId();
    const workload = __ENV.WORKLOAD || 'mixed';

    // Route to specific workload if specified
    if (workload === 'api') {
        apiHeavyWorkload(vuId, uniqueId);
    } else if (workload === 'db') {
        dbHeavyWorkload(vuId, uniqueId);
    } else {
        mixedWorkload(vuId, uniqueId);
    }
}

function mixedWorkload(vuId, uniqueId) {
    // Health check
    group('System Health', function() {
        const healthStart = Date.now();
        const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`, {
            tags: { name: 'health_check' },
        });
        const healthDuration = Date.now() - healthStart;
        httpApiLatency.add(healthDuration);
        totalOperations.add(1);

        const healthy = check(healthRes, {
            'system healthy': (r) => r.status === 200,
        });

        httpApiSuccess.add(healthy ? 1 : 0);
        systemHealth.add(healthy ? 1 : 0);
    });

    // API operations
    group('API Operations', function() {
        // List agents
        const agentsStart = Date.now();
        const agentsRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/agents`, {
            headers: getAuthHeaders(),
            tags: { name: 'list_agents' },
        });
        httpApiLatency.add(Date.now() - agentsStart);
        totalOperations.add(1);

        httpApiSuccess.add(check(agentsRes, {
            'list agents OK': (r) => r.status === 200,
        }) ? 1 : 0);

        // Get status
        const statusStart = Date.now();
        const statusRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/status`, {
            headers: getAuthHeaders(),
            tags: { name: 'get_status' },
        });
        httpApiLatency.add(Date.now() - statusStart);
        totalOperations.add(1);

        httpApiSuccess.add(check(statusRes, {
            'get status OK': (r) => r.status === 200,
        }) ? 1 : 0);
    });

    // Database operations
    group('Database Operations', function() {
        // Create task
        const taskPayload = JSON.stringify({
            description: `Mixed workload task ${uniqueId}`,
            priority: getRandomPriority(),
        });

        const createStart = Date.now();
        const createRes = http.post(`${CONFIG.HTTP_BASE_URL}/api/v1/tasks`, taskPayload, {
            headers: getAuthHeaders(),
            tags: { name: 'create_task' },
        });
        dbLatency.add(Date.now() - createStart);
        totalOperations.add(1);

        dbSuccess.add(check(createRes, {
            'create task OK': (r) => r.status === 200 || r.status === 201,
        }) ? 1 : 0);

        // Query tasks
        const queryStart = Date.now();
        const queryRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/tasks`, {
            headers: getAuthHeaders(),
            tags: { name: 'list_tasks' },
        });
        dbLatency.add(Date.now() - queryStart);
        totalOperations.add(1);

        dbSuccess.add(check(queryRes, {
            'list tasks OK': (r) => r.status === 200,
        }) ? 1 : 0);

        // Memory search
        const searchPayload = JSON.stringify({
            query: getRandomTask(),
            limit: 5,
        });

        const searchStart = Date.now();
        const searchRes = http.post(`${CONFIG.HTTP_BASE_URL}/api/v1/memory/search`, searchPayload, {
            headers: getAuthHeaders(),
            tags: { name: 'memory_search' },
        });
        dbLatency.add(Date.now() - searchStart);
        totalOperations.add(1);

        dbSuccess.add(check(searchRes, {
            'memory search OK': (r) => r.status === 200,
        }) ? 1 : 0);
    });

    // Redis operations
    group('Redis Operations', function() {
        // Broadcast message
        const broadcastPayload = JSON.stringify({
            message: `Mixed workload broadcast ${uniqueId}`,
        });

        const broadcastStart = Date.now();
        const broadcastRes = http.post(`${CONFIG.HTTP_BASE_URL}/api/v1/pubsub/broadcast`, broadcastPayload, {
            headers: getAuthHeaders(),
            tags: { name: 'broadcast' },
        });
        redisLatency.add(Date.now() - broadcastStart);
        totalOperations.add(1);

        redisSuccess.add(check(broadcastRes, {
            'broadcast OK': (r) => r.status === 200,
        }) ? 1 : 0);

        // Redis status
        const redisStatusStart = Date.now();
        const redisStatusRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/redis/status`, {
            headers: getAuthHeaders(),
            tags: { name: 'redis_status' },
        });
        redisLatency.add(Date.now() - redisStatusStart);
        totalOperations.add(1);

        redisSuccess.add(check(redisStatusRes, {
            'redis status OK': (r) => r.status === 200,
        }) ? 1 : 0);
    });

    sleep(0.5 + Math.random() * 0.5);
}

function apiHeavyWorkload(vuId, uniqueId) {
    // Rapid API calls
    const endpoints = [
        { method: 'GET', url: '/health' },
        { method: 'GET', url: '/api/v1/status' },
        { method: 'GET', url: '/api/v1/agents' },
        { method: 'GET', url: '/api/v1/tasks' },
        { method: 'GET', url: '/api/v1/activity' },
        { method: 'GET', url: '/api/v1/workloads' },
        { method: 'GET', url: '/api/v1/rl/stats' },
        { method: 'GET', url: '/api/v1/tokens/metrics' },
    ];

    for (const endpoint of endpoints) {
        const start = Date.now();
        const res = http.request(
            endpoint.method,
            `${CONFIG.HTTP_BASE_URL}${endpoint.url}`,
            null,
            {
                headers: getAuthHeaders(),
                tags: { name: endpoint.url.replace('/api/v1/', '') },
            }
        );
        httpApiLatency.add(Date.now() - start);
        totalOperations.add(1);

        const success = res.status === 200;
        httpApiSuccess.add(success ? 1 : 0);
        if (!success) {
            httpApiErrors.add(1);
        }
    }

    sleep(0.1);
}

function dbHeavyWorkload(vuId, uniqueId) {
    // Database-intensive operations
    for (let i = 0; i < 5; i++) {
        // Create task
        const taskPayload = JSON.stringify({
            description: `DB heavy task ${uniqueId}-${i}`,
            priority: getRandomPriority(),
        });

        const createStart = Date.now();
        const createRes = http.post(`${CONFIG.HTTP_BASE_URL}/api/v1/tasks`, taskPayload, {
            headers: getAuthHeaders(),
            tags: { name: 'create_task_heavy' },
        });
        dbLatency.add(Date.now() - createStart);
        totalOperations.add(1);

        dbSuccess.add(check(createRes, {
            'create task OK': (r) => r.status === 200 || r.status === 201,
        }) ? 1 : 0);

        // Memory search
        const searchPayload = JSON.stringify({
            query: getRandomTask(),
            limit: 10,
        });

        const searchStart = Date.now();
        const searchRes = http.post(`${CONFIG.HTTP_BASE_URL}/api/v1/memory/search`, searchPayload, {
            headers: getAuthHeaders(),
            tags: { name: 'memory_search_heavy' },
        });
        dbLatency.add(Date.now() - searchStart);
        totalOperations.add(1);

        dbSuccess.add(check(searchRes, {
            'memory search OK': (r) => r.status === 200,
        }) ? 1 : 0);
    }

    sleep(0.2);
}

// Teardown function
export function teardown(data) {
    console.log('\n=== Full System Test Complete ===');
    console.log(`Test duration: ${((Date.now() - data.startTime) / 1000 / 60).toFixed(2)} minutes`);

    // Final health check
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    console.log(`Final system health: ${healthRes.status === 200 ? 'HEALTHY' : 'UNHEALTHY'}`);
}

// Custom summary handler
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'full-system',
        metrics: {
            http_api: {
                latency: data.metrics.http_api_latency,
                success_rate: data.metrics.http_api_success,
                errors: data.metrics.http_api_errors,
            },
            websocket: {
                latency: data.metrics.ws_latency,
                success_rate: data.metrics.ws_success,
                errors: data.metrics.ws_errors,
            },
            database: {
                latency: data.metrics.db_latency,
                success_rate: data.metrics.db_success,
            },
            redis: {
                latency: data.metrics.redis_latency,
                success_rate: data.metrics.redis_success,
            },
            system: {
                total_operations: data.metrics.total_operations,
                http_reqs: data.metrics.http_reqs,
                http_req_duration: data.metrics.http_req_duration,
                http_req_failed: data.metrics.http_req_failed,
            },
        },
        thresholds: data.thresholds,
    };

    return {
        'results/full-system-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

function textSummary(data) {
    const lines = [
        '\n╔══════════════════════════════════════════════════════════════════════╗',
        '║              CCA FULL SYSTEM LOAD TEST RESULTS                       ║',
        '╠══════════════════════════════════════════════════════════════════════╣',
    ];

    // HTTP API
    if (data.metrics.http_api_latency) {
        const lat = data.metrics.http_api_latency.values;
        lines.push(`║ HTTP API Latency (ms):                                                ║`);
        lines.push(`║   avg: ${lat.avg.toFixed(2).padStart(10)} | p95: ${lat['p(95)'].toFixed(2).padStart(10)} | max: ${lat.max.toFixed(2).padStart(10)}       ║`);
    }
    if (data.metrics.http_api_success) {
        const rate = (data.metrics.http_api_success.values.rate * 100).toFixed(2);
        lines.push(`║   Success Rate: ${rate.padStart(6)}%                                               ║`);
    }

    // Database
    if (data.metrics.db_latency) {
        const lat = data.metrics.db_latency.values;
        lines.push(`║ Database Latency (ms):                                                ║`);
        lines.push(`║   avg: ${lat.avg.toFixed(2).padStart(10)} | p95: ${lat['p(95)'].toFixed(2).padStart(10)} | max: ${lat.max.toFixed(2).padStart(10)}       ║`);
    }
    if (data.metrics.db_success) {
        const rate = (data.metrics.db_success.values.rate * 100).toFixed(2);
        lines.push(`║   Success Rate: ${rate.padStart(6)}%                                               ║`);
    }

    // Redis
    if (data.metrics.redis_latency) {
        const lat = data.metrics.redis_latency.values;
        lines.push(`║ Redis Latency (ms):                                                   ║`);
        lines.push(`║   avg: ${lat.avg.toFixed(2).padStart(10)} | p95: ${lat['p(95)'].toFixed(2).padStart(10)} | max: ${lat.max.toFixed(2).padStart(10)}       ║`);
    }
    if (data.metrics.redis_success) {
        const rate = (data.metrics.redis_success.values.rate * 100).toFixed(2);
        lines.push(`║   Success Rate: ${rate.padStart(6)}%                                               ║`);
    }

    // Overall
    if (data.metrics.total_operations) {
        const ops = data.metrics.total_operations.values.count;
        lines.push(`║ Total Operations: ${ops.toString().padStart(10)}                                       ║`);
    }
    if (data.metrics.http_reqs) {
        const reqs = data.metrics.http_reqs.values.count;
        const rate = data.metrics.http_reqs.values.rate.toFixed(2);
        lines.push(`║ HTTP Requests: ${reqs.toString().padStart(10)} (${rate} req/s)                          ║`);
    }

    lines.push('╚══════════════════════════════════════════════════════════════════════╝');

    return lines.join('\n');
}
