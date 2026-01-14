// CCA Baseline Performance Test
// Establishes performance baselines for regression testing
//
// Run: k6 run baseline.js
// With Prometheus: k6 run --out experimental-prometheus-rw baseline.js

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';
import { CONFIG, getAuthHeaders, getRandomAgentRole, getRandomTask, getRandomPriority } from './config.js';

// Custom metrics for baseline tracking
const errorRate = new Rate('errors');
const healthCheckDuration = new Trend('health_check_duration', true);
const agentSpawnDuration = new Trend('agent_spawn_duration', true);
const taskSubmitDuration = new Trend('task_submit_duration', true);
const statusCheckDuration = new Trend('status_check_duration', true);
const successfulOperations = new Counter('successful_operations');
const failedOperations = new Counter('failed_operations');

// Baseline test configuration
export const options = {
    scenarios: {
        // Warm-up phase
        warmup: {
            executor: 'constant-vus',
            vus: 1,
            duration: '30s',
            startTime: '0s',
            tags: { phase: 'warmup' },
        },
        // Baseline measurement phase
        baseline: {
            executor: 'constant-vus',
            vus: 5,
            duration: '2m',
            startTime: '30s',
            tags: { phase: 'baseline' },
        },
        // Light load phase
        light_load: {
            executor: 'constant-vus',
            vus: 10,
            duration: '2m',
            startTime: '2m30s',
            tags: { phase: 'light_load' },
        },
        // Cooldown phase
        cooldown: {
            executor: 'constant-vus',
            vus: 1,
            duration: '30s',
            startTime: '4m30s',
            tags: { phase: 'cooldown' },
        },
    },
    thresholds: {
        // HTTP request thresholds
        http_req_duration: ['p(95)<2000', 'p(99)<5000'],
        http_req_failed: ['rate<0.01'],

        // Custom metric thresholds
        health_check_duration: ['p(95)<100'],
        agent_spawn_duration: ['p(95)<3000'],
        task_submit_duration: ['p(95)<1000'],
        status_check_duration: ['p(95)<500'],
        errors: ['rate<0.05'],
    },
};

// Setup function - runs once before the test
export function setup() {
    console.log(`Starting baseline test against ${CONFIG.HTTP_BASE_URL}`);

    // Verify service is available
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    if (healthRes.status !== 200) {
        throw new Error(`CCA service is not healthy: ${healthRes.status}`);
    }

    return {
        startTime: new Date().toISOString(),
        baseUrl: CONFIG.HTTP_BASE_URL,
    };
}

// Main test function
export default function (data) {
    const headers = getAuthHeaders();

    // 1. Health check (frequent, lightweight)
    {
        const start = Date.now();
        const res = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
        const duration = Date.now() - start;
        healthCheckDuration.add(duration);

        const success = check(res, {
            'health check status is 200': (r) => r.status === 200,
            'health check has status': (r) => r.json('status') !== undefined,
        });
        errorRate.add(!success);
        if (success) successfulOperations.add(1);
        else failedOperations.add(1);
    }

    sleep(0.5);

    // 2. Status check
    {
        const start = Date.now();
        const res = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/status`, { headers });
        const duration = Date.now() - start;
        statusCheckDuration.add(duration);

        const success = check(res, {
            'status check returns 200': (r) => r.status === 200,
        });
        errorRate.add(!success);
        if (success) successfulOperations.add(1);
        else failedOperations.add(1);
    }

    sleep(0.5);

    // 3. List agents
    {
        const res = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/agents`, { headers });
        const success = check(res, {
            'list agents returns 200': (r) => r.status === 200,
            'list agents returns array': (r) => Array.isArray(r.json()),
        });
        errorRate.add(!success);
        if (success) successfulOperations.add(1);
        else failedOperations.add(1);
    }

    sleep(0.5);

    // 4. Submit a task (simulated - doesn't spawn actual agent)
    {
        const start = Date.now();
        const res = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/tasks`,
            JSON.stringify({
                description: getRandomTask(),
                priority: getRandomPriority(),
            }),
            { headers }
        );
        const duration = Date.now() - start;
        taskSubmitDuration.add(duration);

        const success = check(res, {
            'task submission returns 200 or 201': (r) => r.status === 200 || r.status === 201,
        });
        errorRate.add(!success);
        if (success) successfulOperations.add(1);
        else failedOperations.add(1);
    }

    sleep(1);

    // 5. Check ACP status
    {
        const res = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/acp/status`, { headers });
        const success = check(res, {
            'acp status returns 200': (r) => r.status === 200,
        });
        errorRate.add(!success);
        if (success) successfulOperations.add(1);
        else failedOperations.add(1);
    }

    sleep(0.5);

    // 6. Check Redis status
    {
        const res = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/redis/status`, { headers });
        const success = check(res, {
            'redis status returns 200': (r) => r.status === 200,
        });
        errorRate.add(!success);
        if (success) successfulOperations.add(1);
        else failedOperations.add(1);
    }

    sleep(0.5);

    // 7. Check PostgreSQL status
    {
        const res = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/postgres/status`, { headers });
        const success = check(res, {
            'postgres status returns 200': (r) => r.status === 200,
        });
        errorRate.add(!success);
        if (success) successfulOperations.add(1);
        else failedOperations.add(1);
    }

    sleep(0.5);
}

// Teardown function - runs once after the test
export function teardown(data) {
    console.log(`Baseline test completed. Started at ${data.startTime}`);
}

// Handle test summary
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'baseline',
        metrics: {
            http_req_duration_p95: data.metrics.http_req_duration?.values?.['p(95)'],
            http_req_duration_p99: data.metrics.http_req_duration?.values?.['p(99)'],
            http_req_failed: data.metrics.http_req_failed?.values?.rate,
            health_check_p95: data.metrics.health_check_duration?.values?.['p(95)'],
            agent_spawn_p95: data.metrics.agent_spawn_duration?.values?.['p(95)'],
            task_submit_p95: data.metrics.task_submit_duration?.values?.['p(95)'],
            error_rate: data.metrics.errors?.values?.rate,
            successful_ops: data.metrics.successful_operations?.values?.count,
            failed_ops: data.metrics.failed_operations?.values?.count,
        },
        thresholds_passed: Object.values(data.thresholds || {}).every(t => t.ok),
    };

    return {
        'results/baseline-summary.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data, { indent: '  ', enableColors: true }),
    };
}

function textSummary(data, options) {
    const { indent = '', enableColors = false } = options;
    const c = enableColors ? {
        green: '\x1b[32m',
        red: '\x1b[31m',
        yellow: '\x1b[33m',
        reset: '\x1b[0m',
    } : { green: '', red: '', yellow: '', reset: '' };

    let output = '\n';
    output += `${indent}CCA Baseline Test Summary\n`;
    output += `${indent}${'='.repeat(50)}\n\n`;

    // Key metrics
    const p95 = data.metrics.http_req_duration?.values?.['p(95)'];
    const errorRate = data.metrics.errors?.values?.rate || 0;

    output += `${indent}Response Time (p95): ${p95 ? p95.toFixed(2) + 'ms' : 'N/A'}\n`;
    output += `${indent}Error Rate: ${(errorRate * 100).toFixed(2)}%\n`;
    output += `${indent}Total Requests: ${data.metrics.http_reqs?.values?.count || 0}\n`;

    // Threshold status
    output += `\n${indent}Thresholds:\n`;
    for (const [name, threshold] of Object.entries(data.thresholds || {})) {
        const status = threshold.ok ? `${c.green}PASS${c.reset}` : `${c.red}FAIL${c.reset}`;
        output += `${indent}  ${name}: ${status}\n`;
    }

    return output;
}
