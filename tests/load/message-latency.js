/**
 * CCA Load Test: Message Latency Under Load
 *
 * Tests inter-agent message latency with strict P99 < 50ms target.
 * Measures:
 * - Message round-trip latency via WebSocket ACP
 * - Message delivery time via HTTP broadcast endpoint
 * - P50, P95, P99 latency distributions
 * - Latency under varying load conditions
 *
 * PRIMARY TARGET: P99 Message Latency < 50ms
 *
 * Run with: k6 run message-latency.js
 */

import http from 'k6/http';
import ws from 'k6/ws';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId, getRandomAgentRole } from './config.js';

// Custom metrics for message latency - PRIMARY FOCUS
const messageLatency = new Trend('message_latency', true);
const messageLatencyP50 = new Gauge('message_latency_p50');
const messageLatencyP99 = new Gauge('message_latency_p99');

// WebSocket message metrics
const wsRoundTrip = new Trend('ws_roundtrip_latency', true);
const wsHeartbeatLatency = new Trend('ws_heartbeat_latency', true);
const wsStatusLatency = new Trend('ws_status_latency', true);
const wsMessageSuccess = new Rate('ws_message_success');
const wsMessagesTotal = new Counter('ws_messages_total');

// HTTP broadcast metrics
const httpBroadcastLatency = new Trend('http_broadcast_latency', true);
const httpSendLatency = new Trend('http_send_latency', true);
const httpMessageSuccess = new Rate('http_message_success');

// Connection metrics
const connectionTime = new Trend('ws_connection_time', true);
const connectionSuccess = new Rate('ws_connection_success');
const activeConnections = new Gauge('active_connections');

// Error tracking
const latencyViolations = new Counter('latency_violations_50ms');
const errors = new Counter('message_errors');

// Test scenarios focused on message latency
export const options = {
    scenarios: {
        // Scenario 1: Low load - baseline latency
        low_load_latency: {
            executor: 'constant-vus',
            vus: 5,
            duration: '1m',
            tags: { scenario: 'low_load' },
            env: { MESSAGES_PER_ITERATION: '50' },
        },
        // Scenario 2: Medium load - 20 connections
        medium_load_latency: {
            executor: 'constant-vus',
            vus: 20,
            duration: '2m',
            startTime: '1m30s',
            tags: { scenario: 'medium_load' },
            env: { MESSAGES_PER_ITERATION: '30' },
        },
        // Scenario 3: High load - 50 connections
        high_load_latency: {
            executor: 'constant-vus',
            vus: 50,
            duration: '2m',
            startTime: '4m',
            tags: { scenario: 'high_load' },
            env: { MESSAGES_PER_ITERATION: '20' },
        },
        // Scenario 4: Stress test - 100 connections
        stress_latency: {
            executor: 'constant-vus',
            vus: 100,
            duration: '2m',
            startTime: '6m30s',
            tags: { scenario: 'stress' },
            env: { MESSAGES_PER_ITERATION: '15' },
        },
        // Scenario 5: High-frequency messaging
        high_frequency: {
            executor: 'constant-arrival-rate',
            rate: 500,
            timeUnit: '1s',
            duration: '1m',
            preAllocatedVUs: 50,
            maxVUs: 100,
            startTime: '9m',
            tags: { scenario: 'high_frequency' },
            env: { MESSAGES_PER_ITERATION: '1' },
        },
        // Scenario 6: Sustained load for latency consistency
        sustained: {
            executor: 'constant-vus',
            vus: 30,
            duration: '3m',
            startTime: '10m30s',
            tags: { scenario: 'sustained' },
            env: { MESSAGES_PER_ITERATION: '40' },
        },
    },
    thresholds: {
        // PRIMARY TARGET: P99 < 50ms for message latency
        'message_latency': ['p(50)<20', 'p(95)<40', 'p(99)<50', 'max<100'],

        // WebSocket specific latency targets
        'ws_roundtrip_latency': ['p(99)<50', 'avg<25'],
        'ws_heartbeat_latency': ['p(99)<30'],
        'ws_status_latency': ['p(99)<50'],

        // HTTP message latency targets
        'http_broadcast_latency': ['p(99)<100'],
        'http_send_latency': ['p(99)<100'],

        // Success rates
        'ws_message_success': ['rate>0.99'],
        'http_message_success': ['rate>0.99'],
        'ws_connection_success': ['rate>0.95'],

        // Latency violation tracking
        'latency_violations_50ms': ['count<100'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(50)', 'p(90)', 'p(95)', 'p(99)', 'count'],
};

// Latency tracking for P99 calculation
let latencyValues = [];

// Setup function
export function setup() {
    console.log('=== CCA Message Latency Load Test ===');
    console.log(`Target: P99 Message Latency < 50ms`);
    console.log('');
    console.log(`HTTP Target: ${CONFIG.HTTP_BASE_URL}`);
    console.log(`WebSocket Target: ${CONFIG.WS_BASE_URL}`);
    console.log('');

    // Verify services are available
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    if (healthRes.status !== 200) {
        console.error('Health check failed!');
        return { abort: true };
    }

    // Check ACP WebSocket server
    const acpRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/acp/status`, {
        headers: getAuthHeaders(),
    });

    if (acpRes.status === 200) {
        console.log('[OK] ACP server is running');
    } else {
        console.warn('[WARN] Could not verify ACP server status');
    }

    return {
        startTime: Date.now(),
        abort: false,
    };
}

// WebSocket-based message latency test
function testWebSocketLatency(vuId, messagesPerIteration) {
    const agentId = `latency-test-${vuId}-${generateId()}`;
    const role = getRandomAgentRole();
    const pendingRequests = new Map();

    const connectStart = Date.now();
    let connectionEstablished = false;

    const response = ws.connect(CONFIG.WS_BASE_URL, {
        headers: { 'X-API-Key': CONFIG.API_KEY },
    }, function(socket) {
        const connectDuration = Date.now() - connectStart;
        connectionTime.add(connectDuration);
        connectionEstablished = true;
        connectionSuccess.add(1);
        activeConnections.add(1);

        // Handle messages
        socket.on('message', function(msg) {
            const receiveTime = Date.now();

            try {
                const data = JSON.parse(msg);

                if (data.id && pendingRequests.has(data.id)) {
                    const { sentTime, type } = pendingRequests.get(data.id);
                    const latency = receiveTime - sentTime;

                    // Record to primary metric
                    messageLatency.add(latency);
                    wsMessagesTotal.add(1);

                    // Record to specific metric based on type
                    if (type === 'heartbeat') {
                        wsHeartbeatLatency.add(latency);
                    } else if (type === 'status') {
                        wsStatusLatency.add(latency);
                    } else {
                        wsRoundTrip.add(latency);
                    }

                    // Track latency violations (>50ms)
                    if (latency > 50) {
                        latencyViolations.add(1);
                    }

                    wsMessageSuccess.add(1);
                    pendingRequests.delete(data.id);
                }

                if (data.error) {
                    wsMessageSuccess.add(0);
                    errors.add(1);
                }
            } catch (e) {
                // Non-JSON message
            }
        });

        socket.on('error', function(e) {
            errors.add(1);
        });

        socket.on('close', function() {
            activeConnections.add(-1);
        });

        // Register agent
        const registerId = `register-${agentId}`;
        const registerStart = Date.now();
        pendingRequests.set(registerId, { sentTime: registerStart, type: 'register' });

        socket.send(JSON.stringify({
            jsonrpc: '2.0',
            method: 'agent.register',
            params: { agent_id: agentId, role: role, api_key: CONFIG.API_KEY },
            id: registerId,
        }));

        sleep(0.2); // Wait for registration

        // Send messages and measure latency
        for (let i = 0; i < messagesPerIteration; i++) {
            // Heartbeat (most frequent, lowest latency expected)
            const heartbeatId = `heartbeat-${agentId}-${i}`;
            const heartbeatStart = Date.now();
            pendingRequests.set(heartbeatId, { sentTime: heartbeatStart, type: 'heartbeat' });

            socket.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'agent.heartbeat',
                params: { agent_id: agentId },
                id: heartbeatId,
            }));

            // Small delay to measure individual latencies accurately
            sleep(0.01);

            // Status check every 5 messages
            if (i % 5 === 0) {
                const statusId = `status-${agentId}-${i}`;
                const statusStart = Date.now();
                pendingRequests.set(statusId, { sentTime: statusStart, type: 'status' });

                socket.send(JSON.stringify({
                    jsonrpc: '2.0',
                    method: 'agent.status',
                    params: { agent_id: agentId },
                    id: statusId,
                }));

                sleep(0.01);
            }
        }

        // Wait for remaining responses
        sleep(0.5);

        // Count unacknowledged messages as failures
        const dropped = pendingRequests.size;
        if (dropped > 0) {
            for (let i = 0; i < dropped; i++) {
                wsMessageSuccess.add(0);
            }
        }

        socket.close();
    });

    if (!connectionEstablished) {
        connectionSuccess.add(0);
        errors.add(1);
    }
}

// HTTP-based message latency test
function testHttpMessageLatency(vuId) {
    // Test broadcast endpoint latency
    const broadcastPayload = JSON.stringify({
        message: `Latency test from VU ${vuId} at ${Date.now()}`,
    });

    const broadcastStart = Date.now();
    const broadcastRes = http.post(
        `${CONFIG.HTTP_BASE_URL}/api/v1/broadcast`,
        broadcastPayload,
        {
            headers: getAuthHeaders(),
            tags: { name: 'broadcast' },
            timeout: '5s',
        }
    );
    const broadcastLatency = Date.now() - broadcastStart;

    httpBroadcastLatency.add(broadcastLatency);
    messageLatency.add(broadcastLatency);

    const broadcastSuccess = check(broadcastRes, {
        'broadcast status 200': (r) => r.status === 200,
        'broadcast latency < 100ms': () => broadcastLatency < 100,
    });

    httpMessageSuccess.add(broadcastSuccess ? 1 : 0);

    if (broadcastLatency > 50) {
        latencyViolations.add(1);
    }

    // Test pubsub broadcast latency
    const pubsubPayload = JSON.stringify({
        message: `PubSub test from VU ${vuId}`,
    });

    const pubsubStart = Date.now();
    const pubsubRes = http.post(
        `${CONFIG.HTTP_BASE_URL}/api/v1/pubsub/broadcast`,
        pubsubPayload,
        {
            headers: getAuthHeaders(),
            tags: { name: 'pubsub_broadcast' },
            timeout: '5s',
        }
    );
    const pubsubLatency = Date.now() - pubsubStart;

    httpSendLatency.add(pubsubLatency);
    messageLatency.add(pubsubLatency);

    const pubsubSuccess = check(pubsubRes, {
        'pubsub status 200': (r) => r.status === 200,
        'pubsub latency < 100ms': () => pubsubLatency < 100,
    });

    httpMessageSuccess.add(pubsubSuccess ? 1 : 0);
}

// Main test function
export default function(data) {
    if (data && data.abort) {
        return;
    }

    const vuId = __VU;
    const messagesPerIteration = parseInt(__ENV.MESSAGES_PER_ITERATION || '20');
    const scenario = __ENV.K6_SCENARIO || 'default';

    // For high-frequency scenario, just do HTTP tests
    if (scenario === 'high_frequency') {
        testHttpMessageLatency(vuId);
        return;
    }

    // Mixed WebSocket and HTTP testing
    if (Math.random() < 0.7) {
        // 70% WebSocket tests (primary latency measurement)
        testWebSocketLatency(vuId, messagesPerIteration);
    } else {
        // 30% HTTP tests
        testHttpMessageLatency(vuId);
    }

    sleep(0.5);
}

// Teardown function
export function teardown(data) {
    if (data && data.abort) {
        return;
    }

    const duration = (Date.now() - data.startTime) / 1000;
    console.log('');
    console.log('=== Message Latency Test Complete ===');
    console.log(`Test duration: ${duration.toFixed(2)}s`);
}

// Custom summary handler with detailed latency analysis
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'message-latency',
        target: 'P99 < 50ms',
        description: 'Message latency under load test',
        metrics: {
            primary: {
                message_latency: data.metrics.message_latency,
            },
            websocket: {
                roundtrip: data.metrics.ws_roundtrip_latency,
                heartbeat: data.metrics.ws_heartbeat_latency,
                status: data.metrics.ws_status_latency,
                success_rate: data.metrics.ws_message_success,
                connection_time: data.metrics.ws_connection_time,
                total_messages: data.metrics.ws_messages_total,
            },
            http: {
                broadcast: data.metrics.http_broadcast_latency,
                send: data.metrics.http_send_latency,
                success_rate: data.metrics.http_message_success,
            },
            violations: {
                over_50ms: data.metrics.latency_violations_50ms,
                errors: data.metrics.message_errors,
            },
        },
        thresholds: data.thresholds,
    };

    return {
        'results/message-latency-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

function textSummary(data) {
    const lines = [
        '',
        '╔══════════════════════════════════════════════════════════════════════════╗',
        '║           CCA MESSAGE LATENCY LOAD TEST RESULTS                          ║',
        '║                   TARGET: P99 < 50ms                                     ║',
        '╠══════════════════════════════════════════════════════════════════════════╣',
    ];

    // Primary message latency metrics
    lines.push('║ PRIMARY MESSAGE LATENCY (all sources)                                    ║');
    lines.push('║                                                                          ║');

    if (data.metrics.message_latency) {
        const lat = data.metrics.message_latency.values;
        const p99Status = lat['p(99)'] < 50 ? '✓ PASS' : '✗ FAIL';
        lines.push(`║   P50:  ${lat['p(50)'].toFixed(2).padStart(8)}ms                                                     ║`);
        lines.push(`║   P95:  ${lat['p(95)'].toFixed(2).padStart(8)}ms                                                     ║`);
        lines.push(`║   P99:  ${lat['p(99)'].toFixed(2).padStart(8)}ms  [Target: <50ms] ${p99Status.padStart(10)}                     ║`);
        lines.push(`║   Max:  ${lat.max.toFixed(2).padStart(8)}ms                                                     ║`);
        lines.push(`║   Avg:  ${lat.avg.toFixed(2).padStart(8)}ms                                                     ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════╣');

    // WebSocket metrics
    lines.push('║ WEBSOCKET MESSAGE LATENCY                                                ║');
    lines.push('║                                                                          ║');

    if (data.metrics.ws_roundtrip_latency) {
        const lat = data.metrics.ws_roundtrip_latency.values;
        lines.push(`║   Round-trip P99: ${lat['p(99)'].toFixed(2).padStart(8)}ms | Avg: ${lat.avg.toFixed(2).padStart(8)}ms                        ║`);
    }

    if (data.metrics.ws_heartbeat_latency) {
        const lat = data.metrics.ws_heartbeat_latency.values;
        lines.push(`║   Heartbeat P99:  ${lat['p(99)'].toFixed(2).padStart(8)}ms | Avg: ${lat.avg.toFixed(2).padStart(8)}ms                        ║`);
    }

    if (data.metrics.ws_message_success) {
        const rate = (data.metrics.ws_message_success.values.rate * 100).toFixed(2);
        lines.push(`║   Success Rate:   ${rate.padStart(8)}%                                              ║`);
    }

    if (data.metrics.ws_messages_total) {
        const count = data.metrics.ws_messages_total.values.count;
        lines.push(`║   Total Messages: ${count.toString().padStart(8)}                                                ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════╣');

    // HTTP metrics
    lines.push('║ HTTP MESSAGE LATENCY                                                     ║');
    lines.push('║                                                                          ║');

    if (data.metrics.http_broadcast_latency) {
        const lat = data.metrics.http_broadcast_latency.values;
        lines.push(`║   Broadcast P99:  ${lat['p(99)'].toFixed(2).padStart(8)}ms | Avg: ${lat.avg.toFixed(2).padStart(8)}ms                        ║`);
    }

    if (data.metrics.http_send_latency) {
        const lat = data.metrics.http_send_latency.values;
        lines.push(`║   PubSub P99:     ${lat['p(99)'].toFixed(2).padStart(8)}ms | Avg: ${lat.avg.toFixed(2).padStart(8)}ms                        ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════╣');

    // Violations and errors
    lines.push('║ LATENCY VIOLATIONS & ERRORS                                              ║');
    lines.push('║                                                                          ║');

    if (data.metrics.latency_violations_50ms) {
        const violations = data.metrics.latency_violations_50ms.values.count;
        const status = violations < 100 ? '✓' : '✗';
        lines.push(`║   Messages >50ms: ${violations.toString().padStart(8)}  [Threshold: <100] ${status}                       ║`);
    }

    if (data.metrics.message_errors) {
        const errorCount = data.metrics.message_errors.values.count;
        lines.push(`║   Total Errors:   ${errorCount.toString().padStart(8)}                                                ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════╣');

    // Threshold summary
    lines.push('║ THRESHOLD RESULTS                                                        ║');
    lines.push('║                                                                          ║');

    if (data.thresholds) {
        const passed = Object.values(data.thresholds).filter(t => t.ok).length;
        const total = Object.keys(data.thresholds).length;
        const status = passed === total ? 'ALL PASS' : `${total - passed} FAILED`;
        lines.push(`║   Thresholds: ${passed}/${total} ${status.padEnd(12)}                                        ║`);

        // List failed thresholds
        const failed = Object.entries(data.thresholds).filter(([_, t]) => !t.ok);
        if (failed.length > 0) {
            lines.push('║                                                                          ║');
            lines.push('║   Failed Thresholds:                                                     ║');
            for (const [name, _] of failed.slice(0, 3)) {
                lines.push(`║     - ${name.padEnd(60)} ║`);
            }
            if (failed.length > 3) {
                lines.push(`║     ... and ${failed.length - 3} more                                                  ║`);
            }
        }
    }

    lines.push('║                                                                          ║');
    lines.push('╚══════════════════════════════════════════════════════════════════════════╝');
    lines.push('');

    return lines.join('\n');
}
