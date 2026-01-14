/**
 * CCA Load Test: WebSocket (ACP) Message Throughput
 *
 * Tests ACP WebSocket connection performance and message throughput.
 * Measures:
 * - Connection establishment time
 * - Message latency (round-trip time)
 * - Message throughput (messages/second)
 * - Backpressure handling under load
 *
 * Run with: k6 run websocket-throughput.js
 */

import http from 'k6/http';
import ws from 'k6/ws';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getRandomAgentRole, generateId } from './config.js';

// Custom metrics
const wsConnectDuration = new Trend('ws_connect_duration', true);
const wsMessageLatency = new Trend('ws_message_latency', true);
const wsMessagesSent = new Counter('ws_messages_sent');
const wsMessagesReceived = new Counter('ws_messages_received');
const wsConnectionSuccess = new Rate('ws_connection_success');
const wsMessageSuccess = new Rate('ws_message_success');
const wsActiveConnections = new Gauge('ws_active_connections');
const wsErrors = new Counter('ws_errors');
const wsMessagesDropped = new Counter('ws_messages_dropped');

// Test scenarios
export const options = {
    scenarios: {
        // Scenario 1: Low concurrent connections (10) with high message rate
        low_connections_high_throughput: {
            executor: 'constant-vus',
            vus: 10,
            duration: '2m',
            tags: { scenario: 'low_conn_high_msg' },
            env: { MESSAGES_PER_VU: '100' },
        },
        // Scenario 2: Medium concurrent connections (50)
        medium_connections: {
            executor: 'constant-vus',
            vus: 50,
            duration: '3m',
            startTime: '2m30s',
            tags: { scenario: 'medium_conn' },
            env: { MESSAGES_PER_VU: '50' },
        },
        // Scenario 3: High concurrent connections (100)
        high_connections: {
            executor: 'constant-vus',
            vus: 100,
            duration: '5m',
            startTime: '6m',
            tags: { scenario: 'high_conn' },
            env: { MESSAGES_PER_VU: '30' },
        },
        // Scenario 4: Spike test - sudden connection surge
        spike_connections: {
            executor: 'ramping-vus',
            startVUs: 10,
            stages: [
                { duration: '30s', target: 10 },
                { duration: '10s', target: 100 },  // Spike
                { duration: '1m', target: 100 },
                { duration: '10s', target: 10 },   // Drop
                { duration: '30s', target: 10 },
            ],
            startTime: '12m',
            tags: { scenario: 'spike' },
            env: { MESSAGES_PER_VU: '20' },
        },
    },
    thresholds: {
        'ws_connect_duration': ['p(95)<2000', 'p(99)<5000'],
        'ws_message_latency': ['p(95)<500', 'p(99)<1000'],
        'ws_connection_success': ['rate>0.95'],
        'ws_message_success': ['rate>0.98'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)'],
};

// Setup function
export function setup() {
    console.log('=== CCA WebSocket Throughput Load Test ===');
    console.log(`Target URL: ${CONFIG.WS_BASE_URL}`);

    // Check ACP status via HTTP API
    const statusRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/acp/status`, {
        headers: { 'X-API-Key': CONFIG.API_KEY },
    });

    let acpActive = false;
    if (statusRes && statusRes.status === 200) {
        try {
            const status = JSON.parse(statusRes.body);
            acpActive = status.running || status.active || true;
        } catch (e) {
            acpActive = true; // Assume active if we can't parse
        }
    }

    return { startTime: Date.now(), acpActive };
}

// Main test function
export default function(data) {
    const vuId = __VU;
    const agentId = `load-test-${vuId}-${generateId()}`;
    const role = getRandomAgentRole();
    const messagesPerVu = parseInt(__ENV.MESSAGES_PER_VU || '50');

    // Track pending requests for latency measurement
    const pendingRequests = new Map();

    const connectStart = Date.now();
    let connectionEstablished = false;
    let authenticated = false;

    const wsUrl = `${CONFIG.WS_BASE_URL}`;

    const response = ws.connect(wsUrl, {
        headers: {
            'X-API-Key': CONFIG.API_KEY,
        },
    }, function(socket) {
        const connectDuration = Date.now() - connectStart;
        wsConnectDuration.add(connectDuration);
        connectionEstablished = true;
        wsConnectionSuccess.add(1);
        wsActiveConnections.add(1);

        // Handle incoming messages
        socket.on('message', function(msg) {
            wsMessagesReceived.add(1);

            try {
                const data = JSON.parse(msg);

                // Handle JSON-RPC response
                if (data.id && pendingRequests.has(data.id)) {
                    const sentTime = pendingRequests.get(data.id);
                    const latency = Date.now() - sentTime;
                    wsMessageLatency.add(latency);
                    pendingRequests.delete(data.id);
                    wsMessageSuccess.add(1);
                }

                // Handle error responses
                if (data.error) {
                    wsErrors.add(1);
                    wsMessageSuccess.add(0);
                }
            } catch (e) {
                // Non-JSON message, still count as received
            }
        });

        socket.on('error', function(e) {
            wsErrors.add(1);
            console.error(`VU ${vuId}: WebSocket error - ${e.message || e}`);
        });

        socket.on('close', function() {
            wsActiveConnections.add(-1);
        });

        // Step 1: Register agent
        const registerMsg = JSON.stringify({
            jsonrpc: '2.0',
            method: 'agent.register',
            params: {
                agent_id: agentId,
                role: role,
                api_key: CONFIG.API_KEY,
            },
            id: `register-${agentId}`,
        });

        pendingRequests.set(`register-${agentId}`, Date.now());
        socket.send(registerMsg);
        wsMessagesSent.add(1);

        // Wait for registration
        sleep(0.5);

        // Step 2: Send heartbeats and messages
        for (let i = 0; i < messagesPerVu; i++) {
            // Heartbeat message
            const heartbeatId = `heartbeat-${agentId}-${i}`;
            const heartbeatMsg = JSON.stringify({
                jsonrpc: '2.0',
                method: 'agent.heartbeat',
                params: {
                    agent_id: agentId,
                },
                id: heartbeatId,
            });

            pendingRequests.set(heartbeatId, Date.now());
            socket.send(heartbeatMsg);
            wsMessagesSent.add(1);

            // Status check (every 10 messages)
            if (i % 10 === 0) {
                const statusId = `status-${agentId}-${i}`;
                const statusMsg = JSON.stringify({
                    jsonrpc: '2.0',
                    method: 'agent.status',
                    params: {
                        agent_id: agentId,
                    },
                    id: statusId,
                });

                pendingRequests.set(statusId, Date.now());
                socket.send(statusMsg);
                wsMessagesSent.add(1);
            }

            // Small delay between messages to simulate realistic usage
            sleep(0.05 + Math.random() * 0.05);
        }

        // Wait for remaining responses
        sleep(1);

        // Check for dropped messages (pending requests that never got responses)
        const droppedCount = pendingRequests.size;
        if (droppedCount > 0) {
            wsMessagesDropped.add(droppedCount);
            for (let i = 0; i < droppedCount; i++) {
                wsMessageSuccess.add(0);
            }
        }

        socket.close();
    });

    // Check connection result
    if (!connectionEstablished) {
        wsConnectionSuccess.add(0);
        wsErrors.add(1);
        console.error(`VU ${vuId}: Failed to establish WebSocket connection`);
    }

    // Inter-iteration delay
    sleep(1);
}

// Teardown function
export function teardown(data) {
    console.log('\n=== WebSocket Throughput Test Complete ===');
    console.log(`Test duration: ${((Date.now() - data.startTime) / 1000).toFixed(2)}s`);
}

// Custom summary handler
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'websocket-throughput',
        metrics: {
            ws_connect_duration: data.metrics.ws_connect_duration,
            ws_message_latency: data.metrics.ws_message_latency,
            ws_messages_sent: data.metrics.ws_messages_sent,
            ws_messages_received: data.metrics.ws_messages_received,
            ws_connection_success: data.metrics.ws_connection_success,
            ws_message_success: data.metrics.ws_message_success,
            ws_errors: data.metrics.ws_errors,
            ws_messages_dropped: data.metrics.ws_messages_dropped,
        },
        thresholds: data.thresholds,
    };

    return {
        'results/websocket-throughput-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

function textSummary(data) {
    const lines = [
        '\n╔══════════════════════════════════════════════════════════════╗',
        '║         CCA WEBSOCKET THROUGHPUT LOAD TEST RESULTS          ║',
        '╠══════════════════════════════════════════════════════════════╣',
    ];

    if (data.metrics.ws_connect_duration) {
        const dur = data.metrics.ws_connect_duration.values;
        lines.push(`║ Connect Duration (ms):                                       ║`);
        lines.push(`║   avg: ${dur.avg.toFixed(2).padStart(10)} | p95: ${dur['p(95)'].toFixed(2).padStart(10)} | max: ${dur.max.toFixed(2).padStart(10)} ║`);
    }

    if (data.metrics.ws_message_latency) {
        const lat = data.metrics.ws_message_latency.values;
        lines.push(`║ Message Latency (ms):                                        ║`);
        lines.push(`║   avg: ${lat.avg.toFixed(2).padStart(10)} | p95: ${lat['p(95)'].toFixed(2).padStart(10)} | max: ${lat.max.toFixed(2).padStart(10)} ║`);
    }

    if (data.metrics.ws_messages_sent && data.metrics.ws_messages_received) {
        const sent = data.metrics.ws_messages_sent.values.count;
        const recv = data.metrics.ws_messages_received.values.count;
        lines.push(`║ Messages: Sent ${sent.toString().padStart(8)} | Received ${recv.toString().padStart(8)}              ║`);
    }

    if (data.metrics.ws_connection_success) {
        const rate = (data.metrics.ws_connection_success.values.rate * 100).toFixed(2);
        lines.push(`║ Connection Success Rate: ${rate.padStart(6)}%                            ║`);
    }

    if (data.metrics.ws_message_success) {
        const rate = (data.metrics.ws_message_success.values.rate * 100).toFixed(2);
        lines.push(`║ Message Success Rate: ${rate.padStart(6)}%                               ║`);
    }

    if (data.metrics.ws_errors) {
        const errors = data.metrics.ws_errors.values.count;
        lines.push(`║ Total Errors: ${errors.toString().padStart(8)}                                       ║`);
    }

    lines.push('╚══════════════════════════════════════════════════════════════╝');

    return lines.join('\n');
}
