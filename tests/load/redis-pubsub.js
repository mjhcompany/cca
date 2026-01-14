/**
 * CCA Load Test: Redis Pub/Sub Performance
 *
 * Tests Redis pub/sub performance through the CCA API.
 * Since k6 doesn't have native Redis support, we test through the HTTP API
 * endpoints that use Redis pub/sub internally.
 *
 * Measures:
 * - Broadcast message delivery time
 * - Pub/sub throughput via API
 * - Message propagation latency
 * - Redis connection pool performance
 *
 * Run with: k6 run redis-pubsub.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId, getRandomTask } from './config.js';

// Custom metrics
const broadcastDuration = new Trend('broadcast_duration', true);
const broadcastSuccess = new Rate('broadcast_success');
const broadcastErrors = new Counter('broadcast_errors');
const messagesPublished = new Counter('messages_published');
const redisStatusDuration = new Trend('redis_status_duration', true);
const pubsubThroughput = new Trend('pubsub_throughput', true);
const redisPoolActive = new Gauge('redis_pool_active');

// Test scenarios
export const options = {
    scenarios: {
        // Scenario 1: Low rate broadcast (baseline)
        low_rate: {
            executor: 'constant-arrival-rate',
            rate: 10,
            timeUnit: '1s',
            duration: '1m',
            preAllocatedVUs: 20,
            maxVUs: 50,
            tags: { scenario: 'low_rate' },
        },
        // Scenario 2: Medium rate broadcast
        medium_rate: {
            executor: 'constant-arrival-rate',
            rate: 50,
            timeUnit: '1s',
            duration: '2m',
            preAllocatedVUs: 100,
            maxVUs: 150,
            startTime: '1m30s',
            tags: { scenario: 'medium_rate' },
        },
        // Scenario 3: High rate broadcast stress test
        high_rate: {
            executor: 'constant-arrival-rate',
            rate: 100,
            timeUnit: '1s',
            duration: '3m',
            preAllocatedVUs: 150,
            maxVUs: 250,
            startTime: '4m',
            tags: { scenario: 'high_rate' },
        },
        // Scenario 4: Burst test - simulate sudden message flood
        burst: {
            executor: 'ramping-arrival-rate',
            startRate: 10,
            timeUnit: '1s',
            stages: [
                { duration: '30s', target: 10 },
                { duration: '10s', target: 200 },  // Burst
                { duration: '30s', target: 200 },
                { duration: '10s', target: 10 },
                { duration: '20s', target: 10 },
            ],
            preAllocatedVUs: 200,
            maxVUs: 300,
            startTime: '7m30s',
            tags: { scenario: 'burst' },
        },
    },
    thresholds: {
        'broadcast_duration': ['p(95)<1000', 'p(99)<2000'],
        'broadcast_success': ['rate>0.95'],
        'redis_status_duration': ['p(95)<500'],
        'http_req_duration': ['p(95)<2000'],
        'http_req_failed': ['rate<0.05'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)'],
};

// Setup function
export function setup() {
    console.log('=== CCA Redis Pub/Sub Load Test ===');
    console.log(`Target URL: ${CONFIG.HTTP_BASE_URL}`);

    // Check Redis status
    const redisRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/redis/status`, {
        headers: getAuthHeaders(),
    });

    let redisConnected = false;
    if (redisRes.status === 200) {
        try {
            const status = JSON.parse(redisRes.body);
            redisConnected = status.connected !== false;
            console.log(`Redis status: ${JSON.stringify(status)}`);
        } catch (e) {
            redisConnected = true; // Assume connected if parsing fails
        }
    }

    if (!redisConnected) {
        console.warn('Warning: Redis may not be connected');
    }

    return { startTime: Date.now(), redisConnected };
}

// Main test function
export default function(data) {
    const vuId = __VU;
    const iterationId = __ITER;
    const messageId = generateId();

    group('Redis Status Check', function() {
        // Check Redis connection pool status
        const statusStart = Date.now();
        const statusRes = http.get(
            `${CONFIG.HTTP_BASE_URL}/api/v1/redis/status`,
            {
                headers: getAuthHeaders(),
                tags: { name: 'redis_status' },
            }
        );
        const statusDuration = Date.now() - statusStart;
        redisStatusDuration.add(statusDuration);

        check(statusRes, {
            'redis status 200': (r) => r.status === 200,
            'redis connected': (r) => {
                try {
                    const data = JSON.parse(r.body);
                    return data.connected !== false;
                } catch (e) {
                    return true;
                }
            },
        });

        // Extract pool metrics if available
        try {
            const statusData = JSON.parse(statusRes.body);
            if (statusData.pool_size !== undefined) {
                redisPoolActive.add(statusData.active_connections || statusData.pool_size);
            }
        } catch (e) {
            // Ignore parse errors
        }
    });

    group('Broadcast Message', function() {
        // Broadcast a message via pub/sub
        const broadcastPayload = JSON.stringify({
            message: `Load test message ${messageId}`,
            channel: 'cca:pubsub:broadcast',
            metadata: {
                vu_id: vuId,
                iteration: iterationId,
                timestamp: Date.now(),
                test_type: 'load_test',
            },
        });

        const broadcastStart = Date.now();

        const broadcastRes = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/pubsub/broadcast`,
            broadcastPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'pubsub_broadcast' },
                timeout: '10s',
            }
        );

        const duration = Date.now() - broadcastStart;
        broadcastDuration.add(duration);

        const success = check(broadcastRes, {
            'broadcast status 200': (r) => r.status === 200,
            'broadcast successful': (r) => {
                try {
                    const body = JSON.parse(r.body);
                    return body.success !== false && body.error === undefined;
                } catch (e) {
                    return r.status === 200;
                }
            },
            'broadcast under 1s': () => duration < 1000,
        });

        if (success) {
            broadcastSuccess.add(1);
            messagesPublished.add(1);
            pubsubThroughput.add(1000 / duration); // Messages per second potential
        } else {
            broadcastSuccess.add(0);
            broadcastErrors.add(1);
            console.error(`VU ${vuId}: Broadcast failed - Status: ${broadcastRes.status}`);
        }
    });

    group('Agent Broadcast', function() {
        // Use the /broadcast endpoint to broadcast to all agents
        const agentBroadcastPayload = JSON.stringify({
            message: `Agent notification ${messageId}: ${getRandomTask()}`,
        });

        const agentBroadcastStart = Date.now();

        const agentBroadcastRes = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/broadcast`,
            agentBroadcastPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'agent_broadcast' },
                timeout: '10s',
            }
        );

        const duration = Date.now() - agentBroadcastStart;

        check(agentBroadcastRes, {
            'agent broadcast status 200': (r) => r.status === 200,
            'agent broadcast under 2s': () => duration < 2000,
        });

        if (agentBroadcastRes.status === 200) {
            messagesPublished.add(1);
        }
    });

    // Small delay between iterations
    sleep(0.1);
}

// Teardown function
export function teardown(data) {
    console.log('\n=== Redis Pub/Sub Test Complete ===');
    console.log(`Test duration: ${((Date.now() - data.startTime) / 1000).toFixed(2)}s`);

    // Final Redis status
    const statusRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/redis/status`, {
        headers: getAuthHeaders(),
    });

    if (statusRes.status === 200) {
        try {
            const status = JSON.parse(statusRes.body);
            console.log(`Final Redis status: ${JSON.stringify(status)}`);
        } catch (e) {
            console.log('Final Redis status: OK');
        }
    }
}

// Custom summary handler
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'redis-pubsub',
        metrics: {
            broadcast_duration: data.metrics.broadcast_duration,
            broadcast_success: data.metrics.broadcast_success,
            broadcast_errors: data.metrics.broadcast_errors,
            messages_published: data.metrics.messages_published,
            redis_status_duration: data.metrics.redis_status_duration,
            pubsub_throughput: data.metrics.pubsub_throughput,
            http_reqs: data.metrics.http_reqs,
            http_req_duration: data.metrics.http_req_duration,
            http_req_failed: data.metrics.http_req_failed,
        },
        thresholds: data.thresholds,
    };

    return {
        'results/redis-pubsub-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

function textSummary(data) {
    const lines = [
        '\n╔══════════════════════════════════════════════════════════════╗',
        '║           CCA REDIS PUB/SUB LOAD TEST RESULTS               ║',
        '╠══════════════════════════════════════════════════════════════╣',
    ];

    if (data.metrics.broadcast_duration) {
        const dur = data.metrics.broadcast_duration.values;
        lines.push(`║ Broadcast Duration (ms):                                     ║`);
        lines.push(`║   avg: ${dur.avg.toFixed(2).padStart(10)} | p95: ${dur['p(95)'].toFixed(2).padStart(10)} | max: ${dur.max.toFixed(2).padStart(10)} ║`);
    }

    if (data.metrics.broadcast_success) {
        const rate = (data.metrics.broadcast_success.values.rate * 100).toFixed(2);
        lines.push(`║ Broadcast Success Rate: ${rate.padStart(6)}%                             ║`);
    }

    if (data.metrics.messages_published) {
        const count = data.metrics.messages_published.values.count;
        lines.push(`║ Messages Published: ${count.toString().padStart(8)}                                 ║`);
    }

    if (data.metrics.pubsub_throughput) {
        const throughput = data.metrics.pubsub_throughput.values;
        lines.push(`║ Throughput (msg/s potential):                                ║`);
        lines.push(`║   avg: ${throughput.avg.toFixed(2).padStart(10)} | max: ${throughput.max.toFixed(2).padStart(10)}                  ║`);
    }

    if (data.metrics.redis_status_duration) {
        const status = data.metrics.redis_status_duration.values;
        lines.push(`║ Redis Status Check (ms): avg: ${status.avg.toFixed(2).padStart(8)} | p95: ${status['p(95)'].toFixed(2).padStart(8)}    ║`);
    }

    lines.push('╚══════════════════════════════════════════════════════════════╝');

    return lines.join('\n');
}
