/**
 * CCA Load Test: Message Latency Under Load
 *
 * Tests inter-agent message latency with strict P99 < 50ms target.
 * Measures:
 * - Message round-trip latency via WebSocket ACP
 * - Message delivery time via HTTP broadcast endpoint
 * - P50, P95, P99 latency distributions with histogram tracking
 * - Latency under varying load conditions
 * - Inter-agent message passing latency
 *
 * PRIMARY TARGET: P99 Message Latency < 50ms
 *
 * Run with: k6 run message-latency.js
 *
 * Environment Variables:
 *   CCA_HTTP_URL - HTTP API endpoint (default: http://localhost:9200)
 *   CCA_WS_URL   - WebSocket endpoint (default: ws://localhost:9100)
 *   CCA_API_KEY  - API authentication key
 */

import http from 'k6/http';
import ws from 'k6/ws';
import { check, sleep, fail } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId, getRandomAgentRole } from './config.js';

// =============================================================================
// HISTOGRAM CONFIGURATION
// =============================================================================
// Latency buckets for histogram analysis (in milliseconds)
const LATENCY_BUCKETS = [1, 2, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 75, 100, 150, 200, 500, 1000];

// =============================================================================
// CUSTOM METRICS - PRIMARY FOCUS
// =============================================================================

// Combined message latency (all sources)
const messageLatency = new Trend('message_latency', true);
const messageLatencyP50 = new Gauge('message_latency_p50');
const messageLatencyP95 = new Gauge('message_latency_p95');
const messageLatencyP99 = new Gauge('message_latency_p99');

// Inter-agent message latency (agent-to-agent via send_message)
const interAgentLatency = new Trend('inter_agent_latency', true);
const interAgentLatencyP50 = new Gauge('inter_agent_latency_p50');
const interAgentLatencyP95 = new Gauge('inter_agent_latency_p95');
const interAgentLatencyP99 = new Gauge('inter_agent_latency_p99');

// Histogram bucket counters for detailed distribution analysis
const histogramBuckets = {};
LATENCY_BUCKETS.forEach(bucket => {
    histogramBuckets[bucket] = new Counter(`latency_histogram_le_${bucket}ms`);
});
const histogramInfinity = new Counter('latency_histogram_le_inf');

// WebSocket message metrics
const wsRoundTrip = new Trend('ws_roundtrip_latency', true);
const wsHeartbeatLatency = new Trend('ws_heartbeat_latency', true);
const wsStatusLatency = new Trend('ws_status_latency', true);
const wsSendMessageLatency = new Trend('ws_send_message_latency', true);
const wsMessageSuccess = new Rate('ws_message_success');
const wsMessagesTotal = new Counter('ws_messages_total');

// HTTP broadcast metrics
const httpBroadcastLatency = new Trend('http_broadcast_latency', true);
const httpSendLatency = new Trend('http_send_latency', true);
const httpAcpSendLatency = new Trend('http_acp_send_latency', true);
const httpMessageSuccess = new Rate('http_message_success');

// Connection metrics
const connectionTime = new Trend('ws_connection_time', true);
const connectionSuccess = new Rate('ws_connection_success');
const activeConnections = new Gauge('active_connections');

// Error tracking with granularity
const latencyViolationsP50 = new Counter('latency_violations_20ms');
const latencyViolationsP95 = new Counter('latency_violations_40ms');
const latencyViolationsP99 = new Counter('latency_violations_50ms');
const latencyViolationsMax = new Counter('latency_violations_100ms');
const errors = new Counter('message_errors');
const timeoutErrors = new Counter('timeout_errors');
const connectionErrors = new Counter('connection_errors');

// =============================================================================
// TEST SCENARIOS
// =============================================================================
// Progressive load scenarios to measure latency under different conditions
export const options = {
    scenarios: {
        // Scenario 1: Warm-up / baseline - establish baseline latency
        warmup: {
            executor: 'constant-vus',
            vus: 2,
            duration: '30s',
            tags: { scenario: 'warmup' },
            env: { MESSAGES_PER_ITERATION: '20', TEST_INTER_AGENT: 'true' },
        },
        // Scenario 2: Low load - baseline latency measurement
        low_load_latency: {
            executor: 'constant-vus',
            vus: 5,
            duration: '1m',
            startTime: '30s',
            tags: { scenario: 'low_load' },
            env: { MESSAGES_PER_ITERATION: '50', TEST_INTER_AGENT: 'true' },
        },
        // Scenario 3: Medium load - 20 concurrent connections
        medium_load_latency: {
            executor: 'constant-vus',
            vus: 20,
            duration: '2m',
            startTime: '1m45s',
            tags: { scenario: 'medium_load' },
            env: { MESSAGES_PER_ITERATION: '30', TEST_INTER_AGENT: 'true' },
        },
        // Scenario 4: High load - 50 concurrent connections
        high_load_latency: {
            executor: 'constant-vus',
            vus: 50,
            duration: '2m',
            startTime: '4m',
            tags: { scenario: 'high_load' },
            env: { MESSAGES_PER_ITERATION: '20', TEST_INTER_AGENT: 'true' },
        },
        // Scenario 5: Stress test - 100 concurrent connections
        stress_latency: {
            executor: 'constant-vus',
            vus: 100,
            duration: '2m',
            startTime: '6m30s',
            tags: { scenario: 'stress' },
            env: { MESSAGES_PER_ITERATION: '15', TEST_INTER_AGENT: 'false' },
        },
        // Scenario 6: High-frequency messaging (burst traffic)
        high_frequency: {
            executor: 'constant-arrival-rate',
            rate: 500,
            timeUnit: '1s',
            duration: '1m',
            preAllocatedVUs: 50,
            maxVUs: 100,
            startTime: '9m',
            tags: { scenario: 'high_frequency' },
            env: { MESSAGES_PER_ITERATION: '1', TEST_INTER_AGENT: 'false' },
        },
        // Scenario 7: Sustained load - latency consistency over time
        sustained: {
            executor: 'constant-vus',
            vus: 30,
            duration: '3m',
            startTime: '10m30s',
            tags: { scenario: 'sustained' },
            env: { MESSAGES_PER_ITERATION: '40', TEST_INTER_AGENT: 'true' },
        },
        // Scenario 8: Ramp-up/down - latency under changing load
        ramp: {
            executor: 'ramping-vus',
            startVUs: 5,
            stages: [
                { duration: '30s', target: 25 },
                { duration: '1m', target: 50 },
                { duration: '30s', target: 75 },
                { duration: '1m', target: 50 },
                { duration: '30s', target: 10 },
            ],
            startTime: '14m',
            tags: { scenario: 'ramp' },
            env: { MESSAGES_PER_ITERATION: '25', TEST_INTER_AGENT: 'true' },
        },
    },

    // ==========================================================================
    // THRESHOLDS - PRIMARY TARGETS
    // ==========================================================================
    thresholds: {
        // ======================================================================
        // PRIMARY TARGET: P99 < 50ms for message latency
        // ======================================================================
        'message_latency': [
            'p(50)<20',   // P50: 50% of messages under 20ms
            'p(95)<40',   // P95: 95% of messages under 40ms
            'p(99)<50',   // P99: 99% of messages under 50ms (PRIMARY)
            'max<100',    // Max: No message over 100ms
            'avg<25',     // Average should be under 25ms
        ],

        // Inter-agent message latency (agent-to-agent)
        'inter_agent_latency': [
            'p(50)<25',
            'p(95)<45',
            'p(99)<50',   // PRIMARY: P99 < 50ms
            'max<150',
        ],

        // WebSocket specific latency targets
        'ws_roundtrip_latency': ['p(50)<15', 'p(95)<35', 'p(99)<50', 'avg<20'],
        'ws_heartbeat_latency': ['p(50)<10', 'p(95)<25', 'p(99)<30'],
        'ws_status_latency': ['p(50)<15', 'p(95)<40', 'p(99)<50'],
        'ws_send_message_latency': ['p(50)<20', 'p(95)<40', 'p(99)<50'],

        // HTTP message latency targets
        'http_broadcast_latency': ['p(50)<30', 'p(95)<75', 'p(99)<100'],
        'http_send_latency': ['p(50)<30', 'p(95)<75', 'p(99)<100'],
        'http_acp_send_latency': ['p(50)<40', 'p(95)<80', 'p(99)<120'],

        // Connection metrics
        'ws_connection_time': ['p(95)<500', 'p(99)<1000'],

        // Success rates (must maintain high reliability)
        'ws_message_success': ['rate>0.99'],
        'http_message_success': ['rate>0.99'],
        'ws_connection_success': ['rate>0.95'],

        // Latency violation tracking (cumulative count thresholds)
        'latency_violations_20ms': ['count<1000'],   // P50 violations
        'latency_violations_40ms': ['count<500'],    // P95 violations
        'latency_violations_50ms': ['count<100'],    // P99 violations (PRIMARY)
        'latency_violations_100ms': ['count<10'],    // Max violations

        // Error thresholds
        'message_errors': ['count<50'],
        'timeout_errors': ['count<25'],
        'connection_errors': ['count<10'],
    },

    // Extended summary stats for detailed analysis
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(50)', 'p(75)', 'p(90)', 'p(95)', 'p(99)', 'p(99.9)', 'count'],
};

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/**
 * Record latency to histogram buckets (Prometheus-style cumulative histogram)
 * Each bucket contains count of observations <= bucket boundary
 */
function recordToHistogram(latency) {
    let recorded = false;
    for (const bucket of LATENCY_BUCKETS) {
        if (latency <= bucket) {
            histogramBuckets[bucket].add(1);
            recorded = true;
            break;
        }
    }
    if (!recorded) {
        histogramInfinity.add(1);
    }
}

/**
 * Record latency with full metric tracking
 * - Primary metric (message_latency)
 * - Histogram buckets
 * - Violation counters for P50/P95/P99/Max thresholds
 */
function recordLatency(latency, metricTrend, messageType = 'generic') {
    // Record to primary combined metric
    messageLatency.add(latency);

    // Record to specific metric if provided
    if (metricTrend) {
        metricTrend.add(latency);
    }

    // Record to histogram
    recordToHistogram(latency);

    // Track violations at each percentile threshold
    if (latency > 20) latencyViolationsP50.add(1);
    if (latency > 40) latencyViolationsP95.add(1);
    if (latency > 50) latencyViolationsP99.add(1);
    if (latency > 100) latencyViolationsMax.add(1);

    // Increment total message counter
    wsMessagesTotal.add(1);
}

/**
 * Record inter-agent message latency with dedicated tracking
 */
function recordInterAgentLatency(latency) {
    interAgentLatency.add(latency);
    recordLatency(latency, wsSendMessageLatency, 'inter_agent');
}

// =============================================================================
// SETUP FUNCTION
// =============================================================================
export function setup() {
    console.log('');
    console.log('╔══════════════════════════════════════════════════════════════════════════╗');
    console.log('║           CCA MESSAGE LATENCY LOAD TEST                                  ║');
    console.log('║                                                                          ║');
    console.log('║   PRIMARY TARGET: P99 Message Latency < 50ms                             ║');
    console.log('║                                                                          ║');
    console.log('║   Measures:                                                              ║');
    console.log('║   - P50, P95, P99 latency distributions                                  ║');
    console.log('║   - Inter-agent message passing latency                                  ║');
    console.log('║   - Histogram distribution across latency buckets                        ║');
    console.log('╚══════════════════════════════════════════════════════════════════════════╝');
    console.log('');
    console.log(`HTTP Target:      ${CONFIG.HTTP_BASE_URL}`);
    console.log(`WebSocket Target: ${CONFIG.WS_BASE_URL}`);
    console.log(`Histogram Buckets: ${LATENCY_BUCKETS.join(', ')}ms`);
    console.log('');

    // Verify services are available
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    if (healthRes.status !== 200) {
        console.error('✗ Health check failed!');
        return { abort: true, reason: 'health_check_failed' };
    }
    console.log('✓ Health check passed');

    // Check ACP WebSocket server
    const acpRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/acp/status`, {
        headers: getAuthHeaders(),
    });

    let acpConnections = 0;
    if (acpRes.status === 200) {
        try {
            const acpData = JSON.parse(acpRes.body);
            acpConnections = acpData.active_connections || 0;
            console.log(`✓ ACP server is running (${acpConnections} active connections)`);
        } catch (e) {
            console.log('✓ ACP server is running');
        }
    } else {
        console.warn('⚠ Could not verify ACP server status');
    }

    // Get list of registered agents for inter-agent testing
    const agentsRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/agents`, {
        headers: getAuthHeaders(),
    });

    let registeredAgents = [];
    if (agentsRes.status === 200) {
        try {
            const agentsData = JSON.parse(agentsRes.body);
            registeredAgents = agentsData.agents || [];
            console.log(`✓ Found ${registeredAgents.length} registered agents`);
        } catch (e) {
            // Ignore parse errors
        }
    }

    console.log('');
    console.log('Starting load test...');
    console.log('');

    return {
        startTime: Date.now(),
        abort: false,
        registeredAgents: registeredAgents,
        initialAcpConnections: acpConnections,
    };
}

// =============================================================================
// WEBSOCKET-BASED MESSAGE LATENCY TEST
// =============================================================================
// Tests WebSocket/ACP message latency including:
// - Heartbeat messages (lowest latency expected)
// - Status check messages
// - Inter-agent message passing (send_message)
function testWebSocketLatency(vuId, messagesPerIteration, testInterAgent = false, targetAgents = []) {
    const agentId = `latency-test-${vuId}-${generateId()}`;
    const role = getRandomAgentRole();
    const pendingRequests = new Map();

    const connectStart = Date.now();
    let connectionEstablished = false;
    let registrationComplete = false;

    const response = ws.connect(CONFIG.WS_BASE_URL, {
        headers: { 'X-API-Key': CONFIG.API_KEY },
        tags: { name: 'ws_latency_test' },
    }, function(socket) {
        const connectDuration = Date.now() - connectStart;
        connectionTime.add(connectDuration);
        connectionEstablished = true;
        connectionSuccess.add(1);
        activeConnections.add(1);

        // Handle incoming messages
        socket.on('message', function(msg) {
            const receiveTime = Date.now();

            try {
                const data = JSON.parse(msg);

                // Handle response to our request
                if (data.id && pendingRequests.has(data.id)) {
                    const { sentTime, type } = pendingRequests.get(data.id);
                    const latency = receiveTime - sentTime;

                    // Record latency based on message type
                    switch (type) {
                        case 'heartbeat':
                            recordLatency(latency, wsHeartbeatLatency, 'heartbeat');
                            break;
                        case 'status':
                            recordLatency(latency, wsStatusLatency, 'status');
                            break;
                        case 'send_message':
                        case 'inter_agent':
                            recordInterAgentLatency(latency);
                            break;
                        case 'register':
                            // Registration latency (not counted in primary metrics)
                            wsRoundTrip.add(latency);
                            registrationComplete = true;
                            break;
                        default:
                            recordLatency(latency, wsRoundTrip, 'generic');
                    }

                    wsMessageSuccess.add(1);
                    pendingRequests.delete(data.id);
                }

                // Handle errors
                if (data.error) {
                    wsMessageSuccess.add(0);
                    errors.add(1);
                    if (data.error.code === -32000 || data.error.message?.includes('timeout')) {
                        timeoutErrors.add(1);
                    }
                }
            } catch (e) {
                // Non-JSON message or parse error - ignore
            }
        });

        socket.on('error', function(e) {
            errors.add(1);
            connectionErrors.add(1);
        });

        socket.on('close', function() {
            activeConnections.add(-1);
        });

        // Register agent first
        const registerId = `register-${agentId}`;
        const registerStart = Date.now();
        pendingRequests.set(registerId, { sentTime: registerStart, type: 'register' });

        socket.send(JSON.stringify({
            jsonrpc: '2.0',
            method: 'agent.register',
            params: {
                agent_id: agentId,
                role: role,
                api_key: CONFIG.API_KEY,
                capabilities: ['latency_test'],
            },
            id: registerId,
        }));

        // Wait for registration to complete
        sleep(0.2);

        // =================================================================
        // MAIN MESSAGE LATENCY TEST LOOP
        // =================================================================
        for (let i = 0; i < messagesPerIteration; i++) {
            // -----------------------------------------------------------
            // Heartbeat message (most frequent, lowest latency expected)
            // -----------------------------------------------------------
            const heartbeatId = `hb-${agentId}-${i}`;
            const heartbeatStart = Date.now();
            pendingRequests.set(heartbeatId, { sentTime: heartbeatStart, type: 'heartbeat' });

            socket.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'agent.heartbeat',
                params: { agent_id: agentId, timestamp: heartbeatStart },
                id: heartbeatId,
            }));

            // Small delay to measure individual latencies accurately
            sleep(0.005);

            // -----------------------------------------------------------
            // Status check every 5 messages
            // -----------------------------------------------------------
            if (i % 5 === 0) {
                const statusId = `st-${agentId}-${i}`;
                const statusStart = Date.now();
                pendingRequests.set(statusId, { sentTime: statusStart, type: 'status' });

                socket.send(JSON.stringify({
                    jsonrpc: '2.0',
                    method: 'agent.status',
                    params: { agent_id: agentId },
                    id: statusId,
                }));

                sleep(0.005);
            }

            // -----------------------------------------------------------
            // Inter-agent message every 10 messages (if enabled)
            // -----------------------------------------------------------
            if (testInterAgent && i % 10 === 0) {
                // Send message to another agent (or broadcast if no targets)
                const sendMsgId = `msg-${agentId}-${i}`;
                const sendMsgStart = Date.now();
                pendingRequests.set(sendMsgId, { sentTime: sendMsgStart, type: 'send_message' });

                // Pick a target agent or use broadcast
                const targetAgent = targetAgents.length > 0
                    ? targetAgents[Math.floor(Math.random() * targetAgents.length)]
                    : null;

                const messagePayload = {
                    jsonrpc: '2.0',
                    method: 'send_message',
                    params: {
                        from_agent: agentId,
                        to_agent: targetAgent || '*',  // '*' for broadcast
                        message_type: 'latency_probe',
                        content: {
                            probe_id: sendMsgId,
                            sent_at: sendMsgStart,
                            iteration: i,
                        },
                    },
                    id: sendMsgId,
                };

                socket.send(JSON.stringify(messagePayload));
                sleep(0.01);
            }

            // -----------------------------------------------------------
            // Generic message every 7 messages (for variety)
            // -----------------------------------------------------------
            if (i % 7 === 0) {
                const genericId = `gen-${agentId}-${i}`;
                const genericStart = Date.now();
                pendingRequests.set(genericId, { sentTime: genericStart, type: 'generic' });

                socket.send(JSON.stringify({
                    jsonrpc: '2.0',
                    method: 'query_agent',
                    params: {
                        agent_id: agentId,
                        query: 'workload',
                    },
                    id: genericId,
                }));

                sleep(0.005);
            }
        }

        // Wait for remaining responses (with timeout tracking)
        const waitStart = Date.now();
        const maxWait = 2000; // 2 second max wait

        while (pendingRequests.size > 0 && (Date.now() - waitStart) < maxWait) {
            sleep(0.1);
        }

        // Count remaining unacknowledged messages as timeouts
        const dropped = pendingRequests.size;
        if (dropped > 0) {
            for (let i = 0; i < dropped; i++) {
                wsMessageSuccess.add(0);
                timeoutErrors.add(1);
            }
        }

        socket.close();
    });

    if (!connectionEstablished) {
        connectionSuccess.add(0);
        connectionErrors.add(1);
    }

    return { agentId, connectionEstablished };
}

// =============================================================================
// HTTP-BASED MESSAGE LATENCY TEST
// =============================================================================
// Tests HTTP endpoint latency for message broadcasting:
// - /api/v1/broadcast (ACP + Redis)
// - /api/v1/pubsub/broadcast (Redis only)
// - /api/v1/acp/send (direct ACP send to agent)
function testHttpMessageLatency(vuId, targetAgentId = null) {
    // -----------------------------------------------------------
    // Test 1: Broadcast endpoint latency (ACP + Redis)
    // -----------------------------------------------------------
    const broadcastPayload = JSON.stringify({
        message: `Latency test from VU ${vuId} at ${Date.now()}`,
        probe_type: 'latency_measurement',
        timestamp: Date.now(),
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
    recordToHistogram(broadcastLatency);

    // Track violations
    if (broadcastLatency > 20) latencyViolationsP50.add(1);
    if (broadcastLatency > 40) latencyViolationsP95.add(1);
    if (broadcastLatency > 50) latencyViolationsP99.add(1);
    if (broadcastLatency > 100) latencyViolationsMax.add(1);

    const broadcastSuccess = check(broadcastRes, {
        'broadcast status 200': (r) => r.status === 200,
        'broadcast latency < 100ms': () => broadcastLatency < 100,
    });

    httpMessageSuccess.add(broadcastSuccess ? 1 : 0);
    if (!broadcastSuccess) errors.add(1);

    // -----------------------------------------------------------
    // Test 2: PubSub broadcast latency (Redis only)
    // -----------------------------------------------------------
    const pubsubPayload = JSON.stringify({
        message: `PubSub test from VU ${vuId}`,
        timestamp: Date.now(),
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
    recordToHistogram(pubsubLatency);

    const pubsubSuccess = check(pubsubRes, {
        'pubsub status 200': (r) => r.status === 200,
        'pubsub latency < 100ms': () => pubsubLatency < 100,
    });

    httpMessageSuccess.add(pubsubSuccess ? 1 : 0);
    if (!pubsubSuccess) errors.add(1);

    // -----------------------------------------------------------
    // Test 3: Direct ACP send to agent (if target provided)
    // -----------------------------------------------------------
    if (targetAgentId) {
        const acpSendPayload = JSON.stringify({
            description: `Latency probe from VU ${vuId}`,
            priority: 'normal',
        });

        const acpSendStart = Date.now();
        const acpSendRes = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/acp/send`,
            acpSendPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'acp_send' },
                timeout: '5s',
            }
        );
        const acpSendLatency = Date.now() - acpSendStart;

        httpAcpSendLatency.add(acpSendLatency);
        recordToHistogram(acpSendLatency);

        // This is inter-agent communication via HTTP
        interAgentLatency.add(acpSendLatency);

        const acpSendSuccess = check(acpSendRes, {
            'acp_send status 200 or 404': (r) => r.status === 200 || r.status === 404,
            'acp_send latency < 150ms': () => acpSendLatency < 150,
        });

        httpMessageSuccess.add(acpSendSuccess ? 1 : 0);
        if (!acpSendSuccess) errors.add(1);
    }

    return { broadcastLatency, pubsubLatency };
}

// =============================================================================
// INTER-AGENT MESSAGE TEST (Simulates agent-to-agent communication)
// =============================================================================
// Spawns two WebSocket connections and measures message passing between them
function testInterAgentMessagePassing(vuId) {
    const senderAgentId = `sender-${vuId}-${generateId()}`;
    const receiverAgentId = `receiver-${vuId}-${generateId()}`;

    let messageReceivedLatency = null;
    let senderConnected = false;
    let receiverConnected = false;

    // First, connect the receiver agent
    const receiverResponse = ws.connect(CONFIG.WS_BASE_URL, {
        headers: { 'X-API-Key': CONFIG.API_KEY },
        tags: { name: 'ws_receiver' },
    }, function(receiverSocket) {
        receiverConnected = true;

        receiverSocket.on('message', function(msg) {
            const receiveTime = Date.now();
            try {
                const data = JSON.parse(msg);
                // Check if this is our latency probe message
                if (data.method === 'message_received' &&
                    data.params?.message_type === 'latency_probe') {
                    const sentTime = data.params?.content?.sent_at;
                    if (sentTime) {
                        messageReceivedLatency = receiveTime - sentTime;
                        recordInterAgentLatency(messageReceivedLatency);
                    }
                }
            } catch (e) {
                // Ignore parse errors
            }
        });

        // Register receiver
        receiverSocket.send(JSON.stringify({
            jsonrpc: '2.0',
            method: 'agent.register',
            params: {
                agent_id: receiverAgentId,
                role: 'receiver',
                api_key: CONFIG.API_KEY,
            },
            id: `register-${receiverAgentId}`,
        }));

        // Keep receiver alive while sender sends
        sleep(3);

        receiverSocket.close();
    });

    // Then connect the sender agent and send message
    if (receiverConnected) {
        sleep(0.2); // Give receiver time to register

        const senderResponse = ws.connect(CONFIG.WS_BASE_URL, {
            headers: { 'X-API-Key': CONFIG.API_KEY },
            tags: { name: 'ws_sender' },
        }, function(senderSocket) {
            senderConnected = true;

            // Register sender
            senderSocket.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'agent.register',
                params: {
                    agent_id: senderAgentId,
                    role: 'sender',
                    api_key: CONFIG.API_KEY,
                },
                id: `register-${senderAgentId}`,
            }));

            sleep(0.2);

            // Send message to receiver
            const sendTime = Date.now();
            senderSocket.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'send_message',
                params: {
                    from_agent: senderAgentId,
                    to_agent: receiverAgentId,
                    message_type: 'latency_probe',
                    content: {
                        probe_id: `probe-${vuId}-${Date.now()}`,
                        sent_at: sendTime,
                    },
                },
                id: `send-${senderAgentId}`,
            }));

            sleep(1);
            senderSocket.close();
        });
    }

    return { senderConnected, receiverConnected, messageReceivedLatency };
}

// =============================================================================
// MAIN TEST FUNCTION
// =============================================================================
export default function(data) {
    if (data && data.abort) {
        console.log(`Test aborted: ${data.reason || 'unknown reason'}`);
        return;
    }

    const vuId = __VU;
    const iteration = __ITER;
    const messagesPerIteration = parseInt(__ENV.MESSAGES_PER_ITERATION || '20');
    const testInterAgent = __ENV.TEST_INTER_AGENT === 'true';
    const scenario = __ENV.K6_SCENARIO || 'default';

    // Get target agents from setup data
    const targetAgents = data?.registeredAgents?.map(a => a.agent_id || a.id) || [];

    // -----------------------------------------------------------
    // High-frequency scenario: HTTP-only for maximum throughput
    // -----------------------------------------------------------
    if (scenario === 'high_frequency') {
        testHttpMessageLatency(vuId);
        return;
    }

    // -----------------------------------------------------------
    // Warmup scenario: Simple WebSocket test
    // -----------------------------------------------------------
    if (scenario === 'warmup') {
        testWebSocketLatency(vuId, Math.min(messagesPerIteration, 10), false, []);
        sleep(0.5);
        return;
    }

    // -----------------------------------------------------------
    // Standard scenario: Mixed WebSocket and HTTP testing
    // -----------------------------------------------------------
    const testChoice = Math.random();

    if (testChoice < 0.6) {
        // 60%: WebSocket latency tests (primary measurement)
        testWebSocketLatency(vuId, messagesPerIteration, testInterAgent, targetAgents);
    } else if (testChoice < 0.85) {
        // 25%: HTTP endpoint latency tests
        const targetAgent = targetAgents.length > 0
            ? targetAgents[Math.floor(Math.random() * targetAgents.length)]
            : null;
        testHttpMessageLatency(vuId, targetAgent);
    } else {
        // 15%: Full inter-agent message passing test (most complex)
        if (testInterAgent) {
            testInterAgentMessagePassing(vuId);
        } else {
            testWebSocketLatency(vuId, messagesPerIteration, false, []);
        }
    }

    // Small delay between iterations to prevent overwhelming
    sleep(0.3);
}

// =============================================================================
// TEARDOWN FUNCTION
// =============================================================================
export function teardown(data) {
    if (data && data.abort) {
        console.log(`Test was aborted: ${data.reason || 'unknown reason'}`);
        return;
    }

    const duration = (Date.now() - data.startTime) / 1000;
    console.log('');
    console.log('╔══════════════════════════════════════════════════════════════════════════╗');
    console.log('║                    MESSAGE LATENCY TEST COMPLETE                         ║');
    console.log('╚══════════════════════════════════════════════════════════════════════════╝');
    console.log(`  Duration: ${duration.toFixed(2)}s`);
    console.log(`  Initial ACP Connections: ${data.initialAcpConnections}`);
    console.log(`  Registered Agents Found: ${data.registeredAgents?.length || 0}`);
    console.log('');
}

// =============================================================================
// CUSTOM SUMMARY HANDLER WITH DETAILED LATENCY & HISTOGRAM ANALYSIS
// =============================================================================
export function handleSummary(data) {
    // Build histogram data from bucket counters
    const histogram = {};
    for (const bucket of LATENCY_BUCKETS) {
        const metricName = `latency_histogram_le_${bucket}ms`;
        if (data.metrics[metricName]) {
            histogram[`le_${bucket}ms`] = data.metrics[metricName].values.count;
        }
    }
    if (data.metrics.latency_histogram_le_inf) {
        histogram['le_inf'] = data.metrics.latency_histogram_le_inf.values.count;
    }

    // Calculate total for percentage computation
    const totalHistogramSamples = Object.values(histogram).reduce((a, b) => a + b, 0);

    // Build comprehensive summary object
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'message-latency',
        target: 'P99 < 50ms',
        description: 'Message latency under load test with histogram tracking',
        version: '2.0',

        // Primary metrics with full percentile breakdown
        primary_metrics: {
            message_latency: extractLatencyMetrics(data.metrics.message_latency),
            inter_agent_latency: extractLatencyMetrics(data.metrics.inter_agent_latency),
        },

        // WebSocket metrics
        websocket_metrics: {
            roundtrip: extractLatencyMetrics(data.metrics.ws_roundtrip_latency),
            heartbeat: extractLatencyMetrics(data.metrics.ws_heartbeat_latency),
            status: extractLatencyMetrics(data.metrics.ws_status_latency),
            send_message: extractLatencyMetrics(data.metrics.ws_send_message_latency),
            connection_time: extractLatencyMetrics(data.metrics.ws_connection_time),
            success_rate: data.metrics.ws_message_success?.values?.rate,
            total_messages: data.metrics.ws_messages_total?.values?.count,
        },

        // HTTP metrics
        http_metrics: {
            broadcast: extractLatencyMetrics(data.metrics.http_broadcast_latency),
            pubsub: extractLatencyMetrics(data.metrics.http_send_latency),
            acp_send: extractLatencyMetrics(data.metrics.http_acp_send_latency),
            success_rate: data.metrics.http_message_success?.values?.rate,
        },

        // Histogram data for detailed distribution analysis
        histogram: {
            buckets: histogram,
            total_samples: totalHistogramSamples,
            bucket_boundaries_ms: LATENCY_BUCKETS,
        },

        // Violation counts at each percentile threshold
        violations: {
            over_20ms_p50: data.metrics.latency_violations_20ms?.values?.count || 0,
            over_40ms_p95: data.metrics.latency_violations_40ms?.values?.count || 0,
            over_50ms_p99: data.metrics.latency_violations_50ms?.values?.count || 0,
            over_100ms_max: data.metrics.latency_violations_100ms?.values?.count || 0,
        },

        // Error breakdown
        errors: {
            total: data.metrics.message_errors?.values?.count || 0,
            timeouts: data.metrics.timeout_errors?.values?.count || 0,
            connections: data.metrics.connection_errors?.values?.count || 0,
        },

        // Threshold results
        thresholds: data.thresholds,
        thresholds_passed: Object.values(data.thresholds || {}).filter(t => t.ok).length,
        thresholds_total: Object.keys(data.thresholds || {}).length,
    };

    return {
        'results/message-latency-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data, histogram, totalHistogramSamples),
    };
}

// Helper to extract latency metrics from a Trend metric
function extractLatencyMetrics(metric) {
    if (!metric || !metric.values) return null;
    const v = metric.values;
    return {
        p50: v['p(50)'],
        p75: v['p(75)'],
        p90: v['p(90)'],
        p95: v['p(95)'],
        p99: v['p(99)'],
        p999: v['p(99.9)'],
        min: v.min,
        max: v.max,
        avg: v.avg,
        med: v.med,
        count: v.count,
    };
}

// Generate text summary for console output
function textSummary(data, histogram, totalHistogramSamples) {
    const lines = [
        '',
        '╔══════════════════════════════════════════════════════════════════════════════╗',
        '║              CCA MESSAGE LATENCY LOAD TEST RESULTS                           ║',
        '║                      TARGET: P99 < 50ms                                      ║',
        '╠══════════════════════════════════════════════════════════════════════════════╣',
    ];

    // ==========================================================================
    // PRIMARY MESSAGE LATENCY METRICS
    // ==========================================================================
    lines.push('║ PRIMARY MESSAGE LATENCY (all sources)                                      ║');
    lines.push('║                                                                            ║');

    if (data.metrics.message_latency) {
        const lat = data.metrics.message_latency.values;
        const p50Status = lat['p(50)'] < 20 ? '✓' : '✗';
        const p95Status = lat['p(95)'] < 40 ? '✓' : '✗';
        const p99Status = lat['p(99)'] < 50 ? '✓ PASS' : '✗ FAIL';
        const maxStatus = lat.max < 100 ? '✓' : '✗';

        lines.push(`║   P50:  ${lat['p(50)'].toFixed(2).padStart(8)}ms  [Target: <20ms]  ${p50Status}                                   ║`);
        lines.push(`║   P95:  ${lat['p(95)'].toFixed(2).padStart(8)}ms  [Target: <40ms]  ${p95Status}                                   ║`);
        lines.push(`║   P99:  ${lat['p(99)'].toFixed(2).padStart(8)}ms  [Target: <50ms]  ${p99Status}                              ║`);
        lines.push(`║   Max:  ${lat.max.toFixed(2).padStart(8)}ms  [Target: <100ms] ${maxStatus}                                   ║`);
        lines.push(`║   Avg:  ${lat.avg.toFixed(2).padStart(8)}ms                                                       ║`);
        lines.push(`║   Count: ${lat.count.toString().padStart(7)} messages                                              ║`);
    }

    lines.push('║                                                                            ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════════╣');

    // ==========================================================================
    // INTER-AGENT MESSAGE LATENCY
    // ==========================================================================
    lines.push('║ INTER-AGENT MESSAGE LATENCY                                                ║');
    lines.push('║                                                                            ║');

    if (data.metrics.inter_agent_latency && data.metrics.inter_agent_latency.values.count > 0) {
        const lat = data.metrics.inter_agent_latency.values;
        const p99Status = lat['p(99)'] < 50 ? '✓ PASS' : '✗ FAIL';
        lines.push(`║   P50:  ${lat['p(50)'].toFixed(2).padStart(8)}ms                                                       ║`);
        lines.push(`║   P95:  ${lat['p(95)'].toFixed(2).padStart(8)}ms                                                       ║`);
        lines.push(`║   P99:  ${lat['p(99)'].toFixed(2).padStart(8)}ms  [Target: <50ms]  ${p99Status}                              ║`);
        lines.push(`║   Avg:  ${lat.avg.toFixed(2).padStart(8)}ms | Count: ${lat.count.toString().padStart(6)}                                  ║`);
    } else {
        lines.push('║   (No inter-agent messages recorded)                                       ║');
    }

    lines.push('║                                                                            ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════════╣');

    // ==========================================================================
    // LATENCY HISTOGRAM
    // ==========================================================================
    lines.push('║ LATENCY HISTOGRAM DISTRIBUTION                                             ║');
    lines.push('║                                                                            ║');

    if (totalHistogramSamples > 0) {
        // Show key buckets with bar visualization
        const keyBuckets = [5, 10, 20, 30, 40, 50, 75, 100];
        let cumulative = 0;

        for (const bucket of keyBuckets) {
            const count = histogram[`le_${bucket}ms`] || 0;
            cumulative += count;
            const pct = ((cumulative / totalHistogramSamples) * 100).toFixed(1);
            const barLen = Math.round((cumulative / totalHistogramSamples) * 30);
            const bar = '█'.repeat(barLen) + '░'.repeat(30 - barLen);
            const marker = bucket === 50 ? ' ← P99 TARGET' : '';
            lines.push(`║   ≤${bucket.toString().padStart(3)}ms: ${bar} ${pct.padStart(5)}%${marker.padEnd(14)}║`);
        }

        // Show overflow count
        const overflow = histogram['le_inf'] || 0;
        if (overflow > 0) {
            const overflowPct = ((overflow / totalHistogramSamples) * 100).toFixed(2);
            lines.push(`║   >1000ms: ${overflow} samples (${overflowPct}%)                                         ║`);
        }
    } else {
        lines.push('║   (No histogram data recorded)                                             ║');
    }

    lines.push('║                                                                            ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════════╣');

    // ==========================================================================
    // WEBSOCKET METRICS
    // ==========================================================================
    lines.push('║ WEBSOCKET MESSAGE LATENCY                                                  ║');
    lines.push('║                                                                            ║');

    if (data.metrics.ws_roundtrip_latency) {
        const lat = data.metrics.ws_roundtrip_latency.values;
        lines.push(`║   Round-trip:  P99: ${lat['p(99)'].toFixed(2).padStart(7)}ms | P95: ${lat['p(95)'].toFixed(2).padStart(7)}ms | Avg: ${lat.avg.toFixed(2).padStart(7)}ms   ║`);
    }

    if (data.metrics.ws_heartbeat_latency) {
        const lat = data.metrics.ws_heartbeat_latency.values;
        lines.push(`║   Heartbeat:   P99: ${lat['p(99)'].toFixed(2).padStart(7)}ms | P95: ${lat['p(95)'].toFixed(2).padStart(7)}ms | Avg: ${lat.avg.toFixed(2).padStart(7)}ms   ║`);
    }

    if (data.metrics.ws_status_latency) {
        const lat = data.metrics.ws_status_latency.values;
        lines.push(`║   Status:      P99: ${lat['p(99)'].toFixed(2).padStart(7)}ms | P95: ${lat['p(95)'].toFixed(2).padStart(7)}ms | Avg: ${lat.avg.toFixed(2).padStart(7)}ms   ║`);
    }

    if (data.metrics.ws_message_success) {
        const rate = (data.metrics.ws_message_success.values.rate * 100).toFixed(2);
        const status = parseFloat(rate) >= 99 ? '✓' : '✗';
        lines.push(`║   Success Rate: ${rate}%  [Target: >99%]  ${status}                                  ║`);
    }

    if (data.metrics.ws_messages_total) {
        const count = data.metrics.ws_messages_total.values.count;
        lines.push(`║   Total WS Messages: ${count.toString().padStart(8)}                                          ║`);
    }

    lines.push('║                                                                            ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════════╣');

    // ==========================================================================
    // HTTP METRICS
    // ==========================================================================
    lines.push('║ HTTP MESSAGE LATENCY                                                       ║');
    lines.push('║                                                                            ║');

    if (data.metrics.http_broadcast_latency) {
        const lat = data.metrics.http_broadcast_latency.values;
        lines.push(`║   Broadcast:   P99: ${lat['p(99)'].toFixed(2).padStart(7)}ms | P95: ${lat['p(95)'].toFixed(2).padStart(7)}ms | Avg: ${lat.avg.toFixed(2).padStart(7)}ms   ║`);
    }

    if (data.metrics.http_send_latency) {
        const lat = data.metrics.http_send_latency.values;
        lines.push(`║   PubSub:      P99: ${lat['p(99)'].toFixed(2).padStart(7)}ms | P95: ${lat['p(95)'].toFixed(2).padStart(7)}ms | Avg: ${lat.avg.toFixed(2).padStart(7)}ms   ║`);
    }

    if (data.metrics.http_acp_send_latency && data.metrics.http_acp_send_latency.values.count > 0) {
        const lat = data.metrics.http_acp_send_latency.values;
        lines.push(`║   ACP Send:    P99: ${lat['p(99)'].toFixed(2).padStart(7)}ms | P95: ${lat['p(95)'].toFixed(2).padStart(7)}ms | Avg: ${lat.avg.toFixed(2).padStart(7)}ms   ║`);
    }

    lines.push('║                                                                            ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════════╣');

    // ==========================================================================
    // VIOLATIONS AND ERRORS
    // ==========================================================================
    lines.push('║ LATENCY VIOLATIONS & ERRORS                                                ║');
    lines.push('║                                                                            ║');

    const v20 = data.metrics.latency_violations_20ms?.values?.count || 0;
    const v40 = data.metrics.latency_violations_40ms?.values?.count || 0;
    const v50 = data.metrics.latency_violations_50ms?.values?.count || 0;
    const v100 = data.metrics.latency_violations_100ms?.values?.count || 0;

    lines.push(`║   >20ms (P50 violations):  ${v20.toString().padStart(8)}  [Threshold: <1000] ${v20 < 1000 ? '✓' : '✗'}              ║`);
    lines.push(`║   >40ms (P95 violations):  ${v40.toString().padStart(8)}  [Threshold: <500]  ${v40 < 500 ? '✓' : '✗'}              ║`);
    lines.push(`║   >50ms (P99 violations):  ${v50.toString().padStart(8)}  [Threshold: <100]  ${v50 < 100 ? '✓' : '✗'}              ║`);
    lines.push(`║   >100ms (Max violations): ${v100.toString().padStart(8)}  [Threshold: <10]   ${v100 < 10 ? '✓' : '✗'}              ║`);

    const totalErrors = data.metrics.message_errors?.values?.count || 0;
    const timeoutErrs = data.metrics.timeout_errors?.values?.count || 0;
    const connErrs = data.metrics.connection_errors?.values?.count || 0;

    lines.push('║                                                                            ║');
    lines.push(`║   Total Errors:     ${totalErrors.toString().padStart(8)}  (Timeouts: ${timeoutErrs}, Connections: ${connErrs.toString().padEnd(4)})   ║`);

    lines.push('║                                                                            ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════════╣');

    // ==========================================================================
    // THRESHOLD SUMMARY
    // ==========================================================================
    lines.push('║ THRESHOLD RESULTS                                                          ║');
    lines.push('║                                                                            ║');

    if (data.thresholds) {
        const passed = Object.values(data.thresholds).filter(t => t.ok).length;
        const total = Object.keys(data.thresholds).length;
        const allPassed = passed === total;
        const status = allPassed ? '✓ ALL PASS' : `✗ ${total - passed} FAILED`;

        lines.push(`║   Thresholds: ${passed}/${total} ${status.padEnd(20)}                                  ║`);

        // List failed thresholds
        const failed = Object.entries(data.thresholds).filter(([_, t]) => !t.ok);
        if (failed.length > 0) {
            lines.push('║                                                                            ║');
            lines.push('║   Failed Thresholds:                                                       ║');
            for (const [name, _] of failed.slice(0, 5)) {
                lines.push(`║     - ${name.padEnd(66)}║`);
            }
            if (failed.length > 5) {
                lines.push(`║     ... and ${(failed.length - 5).toString()} more                                                       ║`);
            }
        }
    }

    lines.push('║                                                                            ║');
    lines.push('╚══════════════════════════════════════════════════════════════════════════════╝');
    lines.push('');

    // ==========================================================================
    // FINAL VERDICT
    // ==========================================================================
    const p99Latency = data.metrics.message_latency?.values?.['p(99)'] || 0;
    const p99Pass = p99Latency < 50;

    lines.push('');
    if (p99Pass) {
        lines.push('  ✅ PRIMARY TARGET MET: P99 Message Latency < 50ms');
    } else {
        lines.push('  ❌ PRIMARY TARGET MISSED: P99 Message Latency >= 50ms');
    }
    lines.push(`     Actual P99: ${p99Latency.toFixed(2)}ms`);
    lines.push('');

    return lines.join('\n');
}
