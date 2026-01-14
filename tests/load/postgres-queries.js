/**
 * CCA Load Test: PostgreSQL Query Performance
 *
 * Tests PostgreSQL query performance under concurrent access through CCA API.
 * Tests various database operations:
 * - Task creation and retrieval
 * - Pattern/memory search (vector similarity)
 * - Agent state management
 * - RL experience recording
 *
 * Run with: k6 run postgres-queries.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId, getRandomAgentRole, getRandomTask, getRandomPriority } from './config.js';

// Custom metrics
const taskCreateDuration = new Trend('task_create_duration', true);
const taskQueryDuration = new Trend('task_query_duration', true);
const memorySearchDuration = new Trend('memory_search_duration', true);
const postgresStatusDuration = new Trend('postgres_status_duration', true);
const dbOperationSuccess = new Rate('db_operation_success');
const dbOperationErrors = new Counter('db_operation_errors');
const tasksCreated = new Counter('tasks_created');
const queriesExecuted = new Counter('queries_executed');
const dbConnectionsActive = new Gauge('db_connections_active');

// Test scenarios
export const options = {
    scenarios: {
        // Scenario 1: Low concurrent queries (baseline)
        low_concurrency: {
            executor: 'constant-vus',
            vus: 10,
            duration: '2m',
            tags: { scenario: 'low_concurrency' },
        },
        // Scenario 2: Medium concurrent queries
        medium_concurrency: {
            executor: 'constant-vus',
            vus: 50,
            duration: '3m',
            startTime: '2m30s',
            tags: { scenario: 'medium_concurrency' },
        },
        // Scenario 3: High concurrent queries (stress)
        high_concurrency: {
            executor: 'constant-vus',
            vus: 100,
            duration: '5m',
            startTime: '6m',
            tags: { scenario: 'high_concurrency' },
        },
        // Scenario 4: Query rate limiting test
        rate_limited: {
            executor: 'constant-arrival-rate',
            rate: 200,
            timeUnit: '1s',
            duration: '2m',
            preAllocatedVUs: 100,
            maxVUs: 200,
            startTime: '11m30s',
            tags: { scenario: 'rate_limited' },
        },
    },
    thresholds: {
        'task_create_duration': ['p(95)<2000', 'p(99)<5000'],
        'task_query_duration': ['p(95)<1000', 'p(99)<2000'],
        'memory_search_duration': ['p(95)<3000', 'p(99)<5000'],
        'postgres_status_duration': ['p(95)<500'],
        'db_operation_success': ['rate>0.95'],
        'http_req_failed': ['rate<0.05'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)'],
};

// Setup function
export function setup() {
    console.log('=== CCA PostgreSQL Query Load Test ===');
    console.log(`Target URL: ${CONFIG.HTTP_BASE_URL}`);

    // Check PostgreSQL status
    const pgRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/postgres/status`, {
        headers: getAuthHeaders(),
    });

    let pgConnected = false;
    let poolInfo = {};
    if (pgRes.status === 200) {
        try {
            const status = JSON.parse(pgRes.body);
            pgConnected = status.connected !== false;
            poolInfo = status;
            console.log(`PostgreSQL status: ${JSON.stringify(status)}`);
        } catch (e) {
            pgConnected = true;
        }
    }

    if (!pgConnected) {
        console.warn('Warning: PostgreSQL may not be connected');
    }

    return { startTime: Date.now(), pgConnected, poolInfo };
}

// Main test function
export default function(data) {
    const vuId = __VU;
    const iterationId = __ITER;
    const uniqueId = generateId();

    group('PostgreSQL Status', function() {
        const statusStart = Date.now();
        const statusRes = http.get(
            `${CONFIG.HTTP_BASE_URL}/api/v1/postgres/status`,
            {
                headers: getAuthHeaders(),
                tags: { name: 'pg_status' },
            }
        );
        const statusDuration = Date.now() - statusStart;
        postgresStatusDuration.add(statusDuration);
        queriesExecuted.add(1);

        check(statusRes, {
            'postgres status 200': (r) => r.status === 200,
            'postgres connected': (r) => {
                try {
                    const data = JSON.parse(r.body);
                    return data.connected !== false;
                } catch (e) {
                    return true;
                }
            },
        });

        // Track connection pool if available
        try {
            const statusData = JSON.parse(statusRes.body);
            if (statusData.active_connections !== undefined) {
                dbConnectionsActive.add(statusData.active_connections);
            }
        } catch (e) {
            // Ignore
        }
    });

    group('Task Operations', function() {
        // Create a new task
        const taskPayload = JSON.stringify({
            description: `${getRandomTask()} - Load test ${uniqueId}`,
            priority: getRandomPriority(),
            metadata: {
                vu_id: vuId,
                iteration: iterationId,
                test_type: 'postgres_load_test',
            },
        });

        const createStart = Date.now();
        const createRes = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/tasks`,
            taskPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'create_task' },
                timeout: '10s',
            }
        );
        const createDuration = Date.now() - createStart;
        taskCreateDuration.add(createDuration);
        queriesExecuted.add(1);

        const createSuccess = check(createRes, {
            'task create status 200/201': (r) => r.status === 200 || r.status === 201,
            'task has id': (r) => {
                try {
                    const body = JSON.parse(r.body);
                    return body.task_id !== undefined || body.id !== undefined;
                } catch (e) {
                    return false;
                }
            },
            'task create under 2s': () => createDuration < 2000,
        });

        if (createSuccess) {
            dbOperationSuccess.add(1);
            tasksCreated.add(1);
        } else {
            dbOperationSuccess.add(0);
            dbOperationErrors.add(1);
        }

        // Query task list
        const queryStart = Date.now();
        const listRes = http.get(
            `${CONFIG.HTTP_BASE_URL}/api/v1/tasks`,
            {
                headers: getAuthHeaders(),
                tags: { name: 'list_tasks' },
            }
        );
        const queryDuration = Date.now() - queryStart;
        taskQueryDuration.add(queryDuration);
        queriesExecuted.add(1);

        const querySuccess = check(listRes, {
            'task list status 200': (r) => r.status === 200,
            'task list is array': (r) => {
                try {
                    const body = JSON.parse(r.body);
                    return Array.isArray(body.tasks) || Array.isArray(body);
                } catch (e) {
                    return false;
                }
            },
            'task query under 1s': () => queryDuration < 1000,
        });

        if (querySuccess) {
            dbOperationSuccess.add(1);
        } else {
            dbOperationSuccess.add(0);
            dbOperationErrors.add(1);
        }

        // Get specific task if we created one
        if (createRes.status === 200 || createRes.status === 201) {
            try {
                const createBody = JSON.parse(createRes.body);
                const taskId = createBody.task_id || createBody.id;

                if (taskId) {
                    const getStart = Date.now();
                    const getRes = http.get(
                        `${CONFIG.HTTP_BASE_URL}/api/v1/tasks/${taskId}`,
                        {
                            headers: getAuthHeaders(),
                            tags: { name: 'get_task' },
                        }
                    );
                    const getDuration = Date.now() - getStart;
                    taskQueryDuration.add(getDuration);
                    queriesExecuted.add(1);

                    check(getRes, {
                        'get task status 200': (r) => r.status === 200,
                    });
                }
            } catch (e) {
                // Ignore
            }
        }
    });

    group('Memory/Pattern Search', function() {
        // Test vector similarity search (ReasoningBank)
        const searchPayload = JSON.stringify({
            query: getRandomTask(),
            limit: 10,
        });

        const searchStart = Date.now();
        const searchRes = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/memory/search`,
            searchPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'memory_search' },
                timeout: '15s',
            }
        );
        const searchDuration = Date.now() - searchStart;
        memorySearchDuration.add(searchDuration);
        queriesExecuted.add(1);

        const searchSuccess = check(searchRes, {
            'memory search status 200': (r) => r.status === 200,
            'memory search returns results': (r) => {
                try {
                    const body = JSON.parse(r.body);
                    return body.patterns !== undefined || body.results !== undefined || Array.isArray(body);
                } catch (e) {
                    return r.status === 200;
                }
            },
            'memory search under 3s': () => searchDuration < 3000,
        });

        if (searchSuccess) {
            dbOperationSuccess.add(1);
        } else {
            dbOperationSuccess.add(0);
            dbOperationErrors.add(1);
        }

        // Semantic code search (if enabled)
        const codeSearchPayload = JSON.stringify({
            query: 'error handling',
            limit: 5,
        });

        const codeSearchStart = Date.now();
        const codeSearchRes = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/code/search`,
            codeSearchPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'code_search' },
                timeout: '15s',
            }
        );
        const codeSearchDuration = Date.now() - codeSearchStart;
        queriesExecuted.add(1);

        check(codeSearchRes, {
            'code search completes': (r) => r.status === 200 || r.status === 404 || r.status === 503,
        });
    });

    group('Agent State Queries', function() {
        // List agents (queries agent table)
        const listStart = Date.now();
        const listRes = http.get(
            `${CONFIG.HTTP_BASE_URL}/api/v1/agents`,
            {
                headers: getAuthHeaders(),
                tags: { name: 'list_agents' },
            }
        );
        const listDuration = Date.now() - listStart;
        taskQueryDuration.add(listDuration);
        queriesExecuted.add(1);

        check(listRes, {
            'list agents status 200': (r) => r.status === 200,
            'list agents under 1s': () => listDuration < 1000,
        });

        // Get workload distribution (aggregation query)
        const workloadStart = Date.now();
        const workloadRes = http.get(
            `${CONFIG.HTTP_BASE_URL}/api/v1/workloads`,
            {
                headers: getAuthHeaders(),
                tags: { name: 'get_workloads' },
            }
        );
        const workloadDuration = Date.now() - workloadStart;
        taskQueryDuration.add(workloadDuration);
        queriesExecuted.add(1);

        check(workloadRes, {
            'workloads status 200': (r) => r.status === 200,
        });
    });

    group('RL Experience Queries', function() {
        // Get RL statistics (queries rl_experiences table)
        const rlStart = Date.now();
        const rlRes = http.get(
            `${CONFIG.HTTP_BASE_URL}/api/v1/rl/stats`,
            {
                headers: getAuthHeaders(),
                tags: { name: 'rl_stats' },
            }
        );
        const rlDuration = Date.now() - rlStart;
        taskQueryDuration.add(rlDuration);
        queriesExecuted.add(1);

        check(rlRes, {
            'RL stats status 200': (r) => r.status === 200,
            'RL stats under 1s': () => rlDuration < 1000,
        });
    });

    // Small delay between iterations
    sleep(0.2);
}

// Teardown function
export function teardown(data) {
    console.log('\n=== PostgreSQL Query Test Complete ===');
    console.log(`Test duration: ${((Date.now() - data.startTime) / 1000).toFixed(2)}s`);

    // Final PostgreSQL status
    const statusRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/postgres/status`, {
        headers: getAuthHeaders(),
    });

    if (statusRes.status === 200) {
        try {
            const status = JSON.parse(statusRes.body);
            console.log(`Final PostgreSQL status: ${JSON.stringify(status)}`);
        } catch (e) {
            console.log('Final PostgreSQL status: OK');
        }
    }
}

// Custom summary handler
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'postgres-queries',
        metrics: {
            task_create_duration: data.metrics.task_create_duration,
            task_query_duration: data.metrics.task_query_duration,
            memory_search_duration: data.metrics.memory_search_duration,
            postgres_status_duration: data.metrics.postgres_status_duration,
            db_operation_success: data.metrics.db_operation_success,
            db_operation_errors: data.metrics.db_operation_errors,
            tasks_created: data.metrics.tasks_created,
            queries_executed: data.metrics.queries_executed,
            http_reqs: data.metrics.http_reqs,
            http_req_duration: data.metrics.http_req_duration,
            http_req_failed: data.metrics.http_req_failed,
        },
        thresholds: data.thresholds,
    };

    return {
        'results/postgres-queries-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

function textSummary(data) {
    const lines = [
        '\n╔══════════════════════════════════════════════════════════════╗',
        '║         CCA POSTGRESQL QUERY LOAD TEST RESULTS              ║',
        '╠══════════════════════════════════════════════════════════════╣',
    ];

    if (data.metrics.task_create_duration) {
        const dur = data.metrics.task_create_duration.values;
        lines.push(`║ Task Create Duration (ms):                                   ║`);
        lines.push(`║   avg: ${dur.avg.toFixed(2).padStart(10)} | p95: ${dur['p(95)'].toFixed(2).padStart(10)} | max: ${dur.max.toFixed(2).padStart(10)} ║`);
    }

    if (data.metrics.task_query_duration) {
        const dur = data.metrics.task_query_duration.values;
        lines.push(`║ Task Query Duration (ms):                                    ║`);
        lines.push(`║   avg: ${dur.avg.toFixed(2).padStart(10)} | p95: ${dur['p(95)'].toFixed(2).padStart(10)} | max: ${dur.max.toFixed(2).padStart(10)} ║`);
    }

    if (data.metrics.memory_search_duration) {
        const dur = data.metrics.memory_search_duration.values;
        lines.push(`║ Memory Search Duration (ms):                                 ║`);
        lines.push(`║   avg: ${dur.avg.toFixed(2).padStart(10)} | p95: ${dur['p(95)'].toFixed(2).padStart(10)} | max: ${dur.max.toFixed(2).padStart(10)} ║`);
    }

    if (data.metrics.db_operation_success) {
        const rate = (data.metrics.db_operation_success.values.rate * 100).toFixed(2);
        lines.push(`║ DB Operation Success Rate: ${rate.padStart(6)}%                           ║`);
    }

    if (data.metrics.tasks_created) {
        const count = data.metrics.tasks_created.values.count;
        lines.push(`║ Tasks Created: ${count.toString().padStart(8)}                                      ║`);
    }

    if (data.metrics.queries_executed) {
        const count = data.metrics.queries_executed.values.count;
        lines.push(`║ Queries Executed: ${count.toString().padStart(8)}                                   ║`);
    }

    lines.push('╚══════════════════════════════════════════════════════════════╝');

    return lines.join('\n');
}
