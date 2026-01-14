/**
 * CCA Load Test: Task Submission Stress Test
 *
 * Tests the coordinator's task queue and routing capabilities under high concurrent load.
 * Submits 50+ tasks simultaneously to stress test:
 * - Task queue mechanism
 * - Coordinator routing decisions
 * - Response times under load
 * - Failure handling and timeouts
 *
 * Run with: k6 run task-submission.js
 * Run specific scenario: k6 run -e SCENARIO=stress task-submission.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId, getRandomTask, getRandomPriority, TASK_PRIORITIES } from './config.js';

// Custom metrics for task submission analysis
const taskSubmitDuration = new Trend('task_submit_duration', true);
const taskSubmitSuccess = new Rate('task_submit_success');
const taskSubmitErrors = new Counter('task_submit_errors');
const taskSubmitTimeouts = new Counter('task_submit_timeouts');
const tasksSubmitted = new Counter('tasks_submitted_total');
const tasksCompleted = new Counter('tasks_completed');
const tasksFailed = new Counter('tasks_failed');
const tasksPending = new Gauge('tasks_pending');

// Queue metrics
const queueLatency = new Trend('queue_latency', true);
const queueDepth = new Gauge('queue_depth');
const routingDecisionTime = new Trend('routing_decision_time', true);

// Throughput metrics
const taskThroughput = new Rate('task_throughput');
const coordinatorResponseTime = new Trend('coordinator_response_time', true);

// Error tracking by type
const errorsByType = {
    noCoordinator: new Counter('errors_no_coordinator'),
    timeout: new Counter('errors_timeout'),
    validation: new Counter('errors_validation'),
    routing: new Counter('errors_routing'),
    other: new Counter('errors_other'),
};

// Test scenarios for different load patterns
export const options = {
    scenarios: {
        // Scenario 1: 50 concurrent task submissions (burst)
        fifty_tasks_burst: {
            executor: 'per-vu-iterations',
            vus: 50,
            iterations: 1,
            maxDuration: '3m',
            tags: { scenario: 'fifty_burst' },
        },
        // Scenario 2: 100 concurrent task submissions (high load)
        hundred_tasks_burst: {
            executor: 'per-vu-iterations',
            vus: 100,
            iterations: 1,
            maxDuration: '5m',
            startTime: '4m',
            tags: { scenario: 'hundred_burst' },
        },
        // Scenario 3: Sustained load - 50 VUs submitting multiple tasks
        sustained_load: {
            executor: 'constant-vus',
            vus: 50,
            duration: '3m',
            startTime: '10m',
            tags: { scenario: 'sustained' },
        },
        // Scenario 4: Ramping load - gradual increase to find breaking point
        ramping_stress: {
            executor: 'ramping-vus',
            startVUs: 10,
            stages: [
                { duration: '1m', target: 25 },
                { duration: '1m', target: 50 },
                { duration: '1m', target: 75 },
                { duration: '2m', target: 100 },
                { duration: '1m', target: 50 },
            ],
            startTime: '14m',
            tags: { scenario: 'ramping' },
        },
        // Scenario 5: Spike test - sudden burst of tasks
        spike_test: {
            executor: 'ramping-vus',
            startVUs: 10,
            stages: [
                { duration: '30s', target: 10 },
                { duration: '10s', target: 100 },  // Sudden spike
                { duration: '1m', target: 100 },   // Maintain spike
                { duration: '10s', target: 10 },   // Quick drop
                { duration: '30s', target: 10 },   // Recovery
            ],
            startTime: '21m',
            tags: { scenario: 'spike' },
        },
    },
    thresholds: {
        // Task submission thresholds
        'task_submit_duration': ['p(95)<5000', 'p(99)<10000'],
        'task_submit_success': ['rate>0.90'],

        // Queue performance thresholds
        'queue_latency': ['p(95)<3000'],
        'routing_decision_time': ['p(95)<2000'],

        // Coordinator response thresholds
        'coordinator_response_time': ['p(95)<8000'],

        // HTTP thresholds
        'http_req_duration': ['p(95)<5000', 'p(99)<10000'],
        'http_req_failed': ['rate<0.10'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)', 'count'],
};

// Sample task descriptions for load testing
const LOAD_TEST_TASKS = [
    'Analyze authentication flow for security vulnerabilities',
    'Review database query performance and suggest optimizations',
    'Check API endpoint input validation',
    'Audit logging implementation for compliance',
    'Verify error handling in payment processing',
    'Analyze memory usage patterns in background workers',
    'Review rate limiting configuration',
    'Check session management security',
    'Audit file upload handling',
    'Review cross-site scripting protections',
    'Analyze SQL injection prevention measures',
    'Check CORS configuration',
    'Review JWT token validation',
    'Audit password hashing implementation',
    'Check encryption at rest configuration',
    'Review API versioning strategy',
    'Analyze caching layer efficiency',
    'Check connection pooling settings',
    'Review error message sanitization',
    'Audit access control lists',
];

function getLoadTestTask() {
    return LOAD_TEST_TASKS[Math.floor(Math.random() * LOAD_TEST_TASKS.length)];
}

// Setup: Check system health and coordinator availability
export function setup() {
    console.log('=== CCA Task Submission Load Test ===');
    console.log(`Target URL: ${CONFIG.HTTP_BASE_URL}`);
    console.log('');

    const setupData = {
        startTime: Date.now(),
        abort: false,
        healthStatus: {},
        initialTaskCount: 0,
        coordinatorAvailable: false,
    };

    // Health check
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    setupData.healthStatus.http = healthRes.status === 200;

    if (!setupData.healthStatus.http) {
        console.error('HTTP API health check failed!');
        console.error(`Status: ${healthRes.status}, Body: ${healthRes.body}`);
        setupData.abort = true;
        return setupData;
    }
    console.log('[OK] HTTP API is healthy');

    // Check ACP status for coordinator
    const acpRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/acp/status`, {
        headers: getAuthHeaders(),
    });

    if (acpRes.status === 200) {
        try {
            const acpData = JSON.parse(acpRes.body);
            setupData.healthStatus.acp = true;
            setupData.coordinatorAvailable = acpData.agents &&
                acpData.agents.some(a => a.role === 'coordinator');

            console.log(`[OK] ACP server running - ${acpData.agents?.length || 0} agents connected`);

            if (!setupData.coordinatorAvailable) {
                console.warn('[WARN] No coordinator worker connected');
                console.warn('       Tasks will fail - start coordinator with: cca agent worker coordinator');
            } else {
                console.log('[OK] Coordinator worker is connected');
            }
        } catch (e) {
            setupData.healthStatus.acp = false;
            console.warn('[WARN] Could not parse ACP status');
        }
    } else {
        setupData.healthStatus.acp = false;
        console.warn(`[WARN] ACP status check returned ${acpRes.status}`);
    }

    // Check initial task count
    const tasksRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/tasks`, {
        headers: getAuthHeaders(),
    });

    if (tasksRes.status === 200) {
        try {
            const tasksData = JSON.parse(tasksRes.body);
            setupData.initialTaskCount = tasksData.tasks ? tasksData.tasks.length : 0;
            console.log(`[OK] Initial task count: ${setupData.initialTaskCount}`);
        } catch (e) {
            console.warn('[WARN] Could not parse initial task count');
        }
    }

    // Check system status
    const statusRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/status`, {
        headers: getAuthHeaders(),
    });

    if (statusRes.status === 200) {
        console.log('[OK] System status endpoint responding');
    }

    console.log('');
    console.log('Setup complete. Starting load test...');
    console.log('');

    return setupData;
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
    const scenario = __ENV.SCENARIO || 'default';

    // Submit task and measure
    group('Task Submission', function() {
        const taskDescription = `[Load Test VU-${vuId} Iter-${iterationId}] ${getLoadTestTask()}`;
        const priority = getRandomPriority();

        const taskPayload = JSON.stringify({
            description: taskDescription,
            priority: priority,
        });

        const submitStart = Date.now();

        const res = http.post(
            `${CONFIG.HTTP_BASE_URL}/api/v1/tasks`,
            taskPayload,
            {
                headers: getAuthHeaders(),
                tags: { name: 'submit_task', priority: priority },
                timeout: '30s',
            }
        );

        const submitDuration = Date.now() - submitStart;
        taskSubmitDuration.add(submitDuration);
        tasksSubmitted.add(1);

        // Parse and analyze response
        let responseData = null;
        let taskId = null;
        let taskStatus = null;
        let errorMessage = null;

        try {
            responseData = JSON.parse(res.body);
            taskId = responseData.task_id;
            taskStatus = responseData.status;
            errorMessage = responseData.error;
        } catch (e) {
            // Response not JSON
        }

        // Check success criteria
        const success = check(res, {
            'status is 200': (r) => r.status === 200,
            'has task_id': () => taskId !== null && taskId !== '',
            'status is not error': () => taskStatus !== 'error',
            'submit duration under 5s': () => submitDuration < 5000,
        });

        if (success && taskStatus !== 'failed') {
            taskSubmitSuccess.add(1);
            taskThroughput.add(1);

            if (taskStatus === 'completed' || taskStatus === 'running') {
                tasksCompleted.add(1);
            }

            // Record routing/coordinator response time
            coordinatorResponseTime.add(submitDuration);

        } else {
            taskSubmitSuccess.add(0);
            taskThroughput.add(0);
            taskSubmitErrors.add(1);
            tasksFailed.add(1);

            // Categorize the error
            if (errorMessage) {
                if (errorMessage.includes('No coordinator')) {
                    errorsByType.noCoordinator.add(1);
                } else if (errorMessage.includes('timeout') || errorMessage.includes('Timeout')) {
                    errorsByType.timeout.add(1);
                    taskSubmitTimeouts.add(1);
                } else if (errorMessage.includes('validation') || errorMessage.includes('too long')) {
                    errorsByType.validation.add(1);
                } else if (errorMessage.includes('routing') || errorMessage.includes('delegate')) {
                    errorsByType.routing.add(1);
                } else {
                    errorsByType.other.add(1);
                }
            } else {
                errorsByType.other.add(1);
            }

            // Log errors for debugging (limit to avoid flooding)
            if (Math.random() < 0.1) {  // Log 10% of errors
                console.error(`VU-${vuId}: Task submit failed - Status: ${res.status}, Error: ${errorMessage || 'Unknown'}`);
            }
        }

        // Small random delay to simulate realistic submission patterns
        sleep(0.1 + Math.random() * 0.2);
    });

    // Periodically check queue status
    if (Math.random() < 0.2) {  // 20% of iterations check queue
        group('Queue Status Check', function() {
            const queueStart = Date.now();

            const tasksRes = http.get(
                `${CONFIG.HTTP_BASE_URL}/api/v1/tasks`,
                {
                    headers: getAuthHeaders(),
                    tags: { name: 'list_tasks' },
                }
            );

            queueLatency.add(Date.now() - queueStart);

            if (tasksRes.status === 200) {
                try {
                    const tasksData = JSON.parse(tasksRes.body);
                    const tasks = tasksData.tasks || [];

                    // Count tasks by status
                    const pending = tasks.filter(t => t.status === 'pending').length;
                    const running = tasks.filter(t => t.status === 'running').length;

                    tasksPending.add(pending);
                    queueDepth.add(pending + running);
                } catch (e) {
                    // Ignore parse errors
                }
            }
        });
    }

    // Check activity periodically
    if (Math.random() < 0.1) {  // 10% of iterations check activity
        group('Activity Check', function() {
            const activityRes = http.get(
                `${CONFIG.HTTP_BASE_URL}/api/v1/activity`,
                {
                    headers: getAuthHeaders(),
                    tags: { name: 'activity' },
                }
            );

            check(activityRes, {
                'activity status is 200': (r) => r.status === 200,
            });
        });
    }
}

// Teardown: Generate summary report
export function teardown(data) {
    if (data && data.abort) {
        console.log('Test was aborted during setup');
        return;
    }

    const testDuration = (Date.now() - data.startTime) / 1000;

    console.log('');
    console.log('=== Task Submission Load Test Complete ===');
    console.log(`Total test duration: ${testDuration.toFixed(2)}s`);
    console.log('');

    // Get final task count
    const tasksRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/tasks`, {
        headers: getAuthHeaders(),
    });

    if (tasksRes.status === 200) {
        try {
            const tasksData = JSON.parse(tasksRes.body);
            const tasks = tasksData.tasks || [];
            const finalCount = tasks.length;

            // Count by status
            const statusCounts = {
                pending: tasks.filter(t => t.status === 'pending').length,
                running: tasks.filter(t => t.status === 'running').length,
                completed: tasks.filter(t => t.status === 'completed').length,
                failed: tasks.filter(t => t.status === 'failed').length,
            };

            console.log(`Initial task count: ${data.initialTaskCount}`);
            console.log(`Final task count: ${finalCount}`);
            console.log(`Tasks created during test: ${finalCount - data.initialTaskCount}`);
            console.log('');
            console.log('Final task status breakdown:');
            console.log(`  Pending: ${statusCounts.pending}`);
            console.log(`  Running: ${statusCounts.running}`);
            console.log(`  Completed: ${statusCounts.completed}`);
            console.log(`  Failed: ${statusCounts.failed}`);
        } catch (e) {
            console.warn('Could not parse final task count');
        }
    }

    // Check system health after test
    console.log('');
    console.log('Post-test health check...');

    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    if (healthRes.status === 200) {
        console.log('[OK] System remained healthy after load test');
    } else {
        console.warn('[WARN] System health degraded after load test');
    }
}

// Custom summary handler for detailed reporting
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'task-submission',
        description: 'Concurrent task submission load test for coordinator queue and routing',
        metrics: {
            // Task submission metrics
            task_submit_duration: data.metrics.task_submit_duration,
            task_submit_success: data.metrics.task_submit_success,
            tasks_submitted_total: data.metrics.tasks_submitted_total,
            tasks_completed: data.metrics.tasks_completed,
            tasks_failed: data.metrics.tasks_failed,

            // Queue metrics
            queue_latency: data.metrics.queue_latency,
            queue_depth: data.metrics.queue_depth,
            routing_decision_time: data.metrics.routing_decision_time,

            // Coordinator metrics
            coordinator_response_time: data.metrics.coordinator_response_time,

            // Error breakdown
            errors: {
                total: data.metrics.task_submit_errors,
                timeouts: data.metrics.task_submit_timeouts,
                no_coordinator: data.metrics.errors_no_coordinator,
                validation: data.metrics.errors_validation,
                routing: data.metrics.errors_routing,
                other: data.metrics.errors_other,
            },

            // HTTP metrics
            http_reqs: data.metrics.http_reqs,
            http_req_duration: data.metrics.http_req_duration,
            http_req_failed: data.metrics.http_req_failed,
        },
        thresholds: data.thresholds,
    };

    return {
        'results/task-submission-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

// Generate formatted text summary
function textSummary(data) {
    const lines = [
        '',
        '\u2554\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2557',
        '\u2551              CCA TASK SUBMISSION LOAD TEST RESULTS                      \u2551',
        '\u2560\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2563',
    ];

    // Task submission metrics
    lines.push('\u2551 TASK SUBMISSION METRICS                                                  \u2551');
    lines.push('\u2551                                                                          \u2551');

    if (data.metrics.task_submit_duration) {
        const dur = data.metrics.task_submit_duration.values;
        lines.push(`\u2551   Duration (ms):    avg: ${dur.avg.toFixed(0).padStart(6)} | p95: ${dur['p(95)'].toFixed(0).padStart(6)} | p99: ${dur['p(99)'].toFixed(0).padStart(6)} | max: ${dur.max.toFixed(0).padStart(6)} \u2551`);
    }

    if (data.metrics.task_submit_success) {
        const rate = (data.metrics.task_submit_success.values.rate * 100).toFixed(2);
        lines.push(`\u2551   Success Rate:     ${rate.padStart(6)}%                                                 \u2551`);
    }

    if (data.metrics.tasks_submitted_total) {
        const count = data.metrics.tasks_submitted_total.values.count;
        lines.push(`\u2551   Total Submitted:  ${count.toString().padStart(6)}                                                    \u2551`);
    }

    lines.push('\u2551                                                                          \u2551');
    lines.push('\u2560\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2563');

    // Queue performance
    lines.push('\u2551 QUEUE PERFORMANCE                                                        \u2551');
    lines.push('\u2551                                                                          \u2551');

    if (data.metrics.queue_latency) {
        const ql = data.metrics.queue_latency.values;
        lines.push(`\u2551   Queue Latency:    avg: ${ql.avg.toFixed(0).padStart(6)} | p95: ${ql['p(95)'].toFixed(0).padStart(6)} | max: ${ql.max.toFixed(0).padStart(6)}ms          \u2551`);
    }

    if (data.metrics.coordinator_response_time) {
        const crt = data.metrics.coordinator_response_time.values;
        lines.push(`\u2551   Coord Response:   avg: ${crt.avg.toFixed(0).padStart(6)} | p95: ${crt['p(95)'].toFixed(0).padStart(6)} | max: ${crt.max.toFixed(0).padStart(6)}ms          \u2551`);
    }

    lines.push('\u2551                                                                          \u2551');
    lines.push('\u2560\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2563');

    // Error breakdown
    lines.push('\u2551 ERROR ANALYSIS                                                           \u2551');
    lines.push('\u2551                                                                          \u2551');

    if (data.metrics.task_submit_errors) {
        const errors = data.metrics.task_submit_errors.values.count;
        lines.push(`\u2551   Total Errors:     ${errors.toString().padStart(6)}                                                    \u2551`);
    }

    if (data.metrics.task_submit_timeouts) {
        const timeouts = data.metrics.task_submit_timeouts.values.count;
        lines.push(`\u2551   Timeouts:         ${timeouts.toString().padStart(6)}                                                    \u2551`);
    }

    if (data.metrics.errors_no_coordinator) {
        const nc = data.metrics.errors_no_coordinator.values.count;
        lines.push(`\u2551   No Coordinator:   ${nc.toString().padStart(6)}                                                    \u2551`);
    }

    lines.push('\u2551                                                                          \u2551');
    lines.push('\u2560\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2563');

    // HTTP metrics
    lines.push('\u2551 HTTP METRICS                                                             \u2551');
    lines.push('\u2551                                                                          \u2551');

    if (data.metrics.http_reqs) {
        const reqs = data.metrics.http_reqs.values;
        const rps = reqs.rate ? reqs.rate.toFixed(2) : '0.00';
        lines.push(`\u2551   Total Requests:   ${reqs.count.toString().padStart(6)} (${rps} req/s)                                  \u2551`);
    }

    if (data.metrics.http_req_duration) {
        const hd = data.metrics.http_req_duration.values;
        lines.push(`\u2551   HTTP Duration:    avg: ${hd.avg.toFixed(0).padStart(6)} | p95: ${hd['p(95)'].toFixed(0).padStart(6)} | max: ${hd.max.toFixed(0).padStart(6)}ms          \u2551`);
    }

    if (data.metrics.http_req_failed) {
        const failRate = (data.metrics.http_req_failed.values.rate * 100).toFixed(2);
        lines.push(`\u2551   HTTP Fail Rate:   ${failRate.padStart(6)}%                                                 \u2551`);
    }

    lines.push('\u2551                                                                          \u2551');

    // Thresholds summary
    lines.push('\u2560\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2563');
    lines.push('\u2551 THRESHOLD RESULTS                                                        \u2551');
    lines.push('\u2551                                                                          \u2551');

    if (data.thresholds) {
        const passed = Object.values(data.thresholds).filter(t => t.ok).length;
        const total = Object.keys(data.thresholds).length;
        const status = passed === total ? 'PASS' : 'FAIL';
        lines.push(`\u2551   Thresholds: ${passed}/${total} ${status.padEnd(4)}                                                     \u2551`);
    }

    lines.push('\u2551                                                                          \u2551');
    lines.push('\u255a\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u255d');
    lines.push('');

    return lines.join('\n');
}
