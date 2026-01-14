/**
 * CCA Load Test: Agent Spawning
 *
 * Tests concurrent agent spawning at different levels (10, 50, 100 agents)
 * Measures:
 * - Agent spawn time
 * - Success/failure rates
 * - System behavior under concurrent spawn requests
 *
 * Run with: k6 run agent-spawning.js
 * Run specific scenario: k6 run -e SCENARIO=high_load agent-spawning.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getRandomAgentRole, getAuthHeaders, generateId } from './config.js';

// Custom metrics
const agentSpawnDuration = new Trend('agent_spawn_duration', true);
const agentSpawnSuccess = new Rate('agent_spawn_success');
const agentSpawnErrors = new Counter('agent_spawn_errors');
const activeAgents = new Gauge('active_agents');
const agentsSpawned = new Counter('agents_spawned_total');

// Track spawned agents for cleanup
const spawnedAgentIds = [];

// Test options with multiple scenarios
// TARGET: Agent spawn time <2s (2000ms)
export const options = {
    scenarios: {
        // Scenario 1: 10 concurrent agent spawns
        ten_agents: {
            executor: 'per-vu-iterations',
            vus: 10,
            iterations: 1,
            maxDuration: '2m',
            tags: { scenario: 'ten_agents' },
            env: { AGENT_COUNT: '10' },
        },
        // Scenario 2: 50 concurrent agent spawns
        fifty_agents: {
            executor: 'per-vu-iterations',
            vus: 50,
            iterations: 1,
            maxDuration: '5m',
            startTime: '3m',
            tags: { scenario: 'fifty_agents' },
            env: { AGENT_COUNT: '50' },
        },
        // Scenario 3: 100 concurrent agent spawns
        hundred_agents: {
            executor: 'per-vu-iterations',
            vus: 100,
            iterations: 1,
            maxDuration: '10m',
            startTime: '9m',
            tags: { scenario: 'hundred_agents' },
            env: { AGENT_COUNT: '100' },
        },
    },
    thresholds: {
        // PRIMARY TARGET: <2s (2000ms) agent spawn time
        'agent_spawn_duration': ['p(95)<2000', 'p(99)<3000', 'avg<1500'],
        'agent_spawn_success': ['rate>0.95'],
        'http_req_duration': ['p(95)<2000'],
        'http_req_failed': ['rate<0.05'],
    },
    // Output results to JSON for report generation
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)'],
};

// Setup: Check system health before testing
export function setup() {
    console.log('=== CCA Agent Spawning Load Test ===');
    console.log(`Target URL: ${CONFIG.HTTP_BASE_URL}`);

    // Health check
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    if (healthRes.status !== 200) {
        console.error('Health check failed! Aborting test.');
        return { abort: true };
    }

    // Get initial agent count
    const agentsRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/agents`, {
        headers: getAuthHeaders(),
    });

    let initialAgentCount = 0;
    if (agentsRes.status === 200) {
        try {
            const data = JSON.parse(agentsRes.body);
            initialAgentCount = data.agents ? data.agents.length : 0;
        } catch (e) {
            console.warn('Could not parse initial agent count');
        }
    }

    console.log(`Initial agent count: ${initialAgentCount}`);
    return { initialAgentCount, startTime: Date.now() };
}

// Main test function
export default function(data) {
    if (data && data.abort) {
        console.error('Test aborted due to setup failure');
        return;
    }

    const vuId = __VU;
    const iterationId = __ITER;
    const uniqueId = generateId();

    group('Agent Spawning', function() {
        // Select a role for this agent
        const role = getRandomAgentRole();

        // Spawn agent request
        const spawnPayload = JSON.stringify({
            role: role,
            name: `load-test-agent-${uniqueId}`,
            config: {
                test_mode: true,
                vu_id: vuId,
            },
        });

        const startTime = Date.now();

        const res = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/agents`,
            spawnPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'spawn_agent' },
                timeout: '30s',
            }
        );

        const duration = Date.now() - startTime;
        agentSpawnDuration.add(duration);

        const success = check(res, {
            'spawn status is 200 or 201': (r) => r.status === 200 || r.status === 201,
            'response has agent_id': (r) => {
                try {
                    const body = JSON.parse(r.body);
                    return body.agent_id !== undefined || body.id !== undefined;
                } catch (e) {
                    return false;
                }
            },
            'spawn duration under 2s (target)': () => duration < 2000,
            'spawn duration under 3s (acceptable)': () => duration < 3000,
        });

        if (success) {
            agentSpawnSuccess.add(1);
            agentsSpawned.add(1);

            // Track agent ID for potential cleanup
            try {
                const body = JSON.parse(res.body);
                const agentId = body.agent_id || body.id;
                if (agentId) {
                    spawnedAgentIds.push(agentId);
                }
            } catch (e) {
                // Ignore parse errors
            }
        } else {
            agentSpawnSuccess.add(0);
            agentSpawnErrors.add(1);
            console.error(`VU ${vuId}: Spawn failed - Status: ${res.status}, Body: ${res.body}`);
        }

        // Small delay between operations
        sleep(0.5);
    });

    group('Agent Listing', function() {
        // List all agents to verify spawning
        const listRes = http.get(
            `${CONFIG.HTTP_BASE_URL}/api/v1/agents`,
            {
                headers: getAuthHeaders(),
                tags: { name: 'list_agents' },
            }
        );

        check(listRes, {
            'list status is 200': (r) => r.status === 200,
            'response is valid JSON': (r) => {
                try {
                    JSON.parse(r.body);
                    return true;
                } catch (e) {
                    return false;
                }
            },
        });

        // Update active agents gauge
        try {
            const data = JSON.parse(listRes.body);
            const count = data.agents ? data.agents.length : 0;
            activeAgents.add(count);
        } catch (e) {
            // Ignore parse errors
        }
    });
}

// Teardown: Report summary and cleanup
export function teardown(data) {
    if (data && data.abort) {
        return;
    }

    console.log('\n=== Agent Spawning Test Complete ===');
    console.log(`Test duration: ${((Date.now() - data.startTime) / 1000).toFixed(2)}s`);

    // Final agent count
    const agentsRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/agents`, {
        headers: getAuthHeaders(),
    });

    if (agentsRes.status === 200) {
        try {
            const agentData = JSON.parse(agentsRes.body);
            const finalCount = agentData.agents ? agentData.agents.length : 0;
            console.log(`Initial agents: ${data.initialAgentCount}`);
            console.log(`Final agents: ${finalCount}`);
            console.log(`Net spawned: ${finalCount - data.initialAgentCount}`);
        } catch (e) {
            console.warn('Could not parse final agent count');
        }
    }
}

// Custom summary handler for JSON output
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'agent-spawning',
        metrics: {
            agent_spawn_duration: data.metrics.agent_spawn_duration,
            agent_spawn_success: data.metrics.agent_spawn_success,
            agents_spawned_total: data.metrics.agents_spawned_total,
            http_reqs: data.metrics.http_reqs,
            http_req_duration: data.metrics.http_req_duration,
            http_req_failed: data.metrics.http_req_failed,
        },
        thresholds: data.thresholds,
    };

    return {
        'results/agent-spawning-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data, { indent: '  ', enableColors: true }),
    };
}

// Text summary helper
function textSummary(data, options) {
    const lines = [
        '\n╔══════════════════════════════════════════════════════════════╗',
        '║           CCA AGENT SPAWNING LOAD TEST RESULTS              ║',
        '╠══════════════════════════════════════════════════════════════╣',
    ];

    if (data.metrics.agent_spawn_duration) {
        const dur = data.metrics.agent_spawn_duration.values;
        lines.push(`║ Spawn Duration (ms):                                         ║`);
        lines.push(`║   avg: ${dur.avg.toFixed(2).padStart(10)} | p95: ${dur['p(95)'].toFixed(2).padStart(10)} | max: ${dur.max.toFixed(2).padStart(10)} ║`);
    }

    if (data.metrics.agent_spawn_success) {
        const rate = (data.metrics.agent_spawn_success.values.rate * 100).toFixed(2);
        lines.push(`║ Success Rate: ${rate.padStart(6)}%                                      ║`);
    }

    if (data.metrics.agents_spawned_total) {
        const count = data.metrics.agents_spawned_total.values.count;
        lines.push(`║ Total Agents Spawned: ${count.toString().padStart(6)}                               ║`);
    }

    lines.push('╚══════════════════════════════════════════════════════════════╝');

    return lines.join('\n');
}
