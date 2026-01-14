// CCA Load Test Configuration
// Shared configuration for all load tests
//
// PRIMARY TARGETS:
// - Agent spawn time: <2s (2000ms)
// - Message latency P99: <50ms
// - API endpoint P95: <200ms

export const CONFIG = {
    // Base URLs
    HTTP_BASE_URL: __ENV.CCA_HTTP_URL || 'http://localhost:9200',
    WS_BASE_URL: __ENV.CCA_WS_URL || 'ws://localhost:9100',
    REDIS_URL: __ENV.CCA_REDIS_URL || 'redis://localhost:6379',
    POSTGRES_URL: __ENV.CCA_POSTGRES_URL || 'postgres://cca:cca@localhost:5432/cca',

    // Authentication
    API_KEY: __ENV.CCA_API_KEY || 'test-api-key',

    // Agent roles available in CCA
    AGENT_ROLES: ['coordinator', 'frontend', 'backend', 'dba', 'devops', 'security', 'qa'],

    // Default thresholds - PRIMARY TARGETS
    THRESHOLDS: {
        // Agent spawn targets (PRIMARY TARGET: <2s)
        AGENT_SPAWN_P95: 2000,         // 95% of spawns under 2s
        AGENT_SPAWN_P99: 3000,         // 99% of spawns under 3s

        // Message latency targets (PRIMARY TARGET: P99 <50ms)
        MESSAGE_LATENCY_P50: 20,       // 50% of messages under 20ms
        MESSAGE_LATENCY_P95: 40,       // 95% of messages under 40ms
        MESSAGE_LATENCY_P99: 50,       // 99% of messages under 50ms

        // HTTP request thresholds
        HTTP_REQ_DURATION_P95: 200,   // 95% of requests under 200ms
        HTTP_REQ_DURATION_P99: 500,   // 99% of requests under 500ms
        HTTP_REQ_FAILED_RATE: 0.01,   // Less than 1% failures

        // WebSocket thresholds
        WS_CONNECTING_DURATION_P95: 1000,  // 95% connections under 1s
        WS_MESSAGE_LATENCY_P95: 40,        // 95% messages under 40ms
        WS_MESSAGE_LATENCY_P99: 50,        // 99% messages under 50ms

        // Database thresholds
        DB_QUERY_DURATION_P95: 500,    // 95% queries under 500ms
        DB_CONNECTION_TIME_P95: 1000,  // 95% connections under 1s

        // Token service thresholds
        TOKEN_ANALYZE_P95: 500,        // 95% analysis under 500ms
        TOKEN_COMPRESS_P95: 2000,      // 95% compression under 2s
    },

    // Test scenarios
    SCENARIOS: {
        SMOKE: {
            vus: 1,
            duration: '30s',
        },
        LOW_LOAD: {
            vus: 10,
            duration: '2m',
        },
        MEDIUM_LOAD: {
            vus: 50,
            duration: '5m',
        },
        HIGH_LOAD: {
            vus: 100,
            duration: '10m',
        },
        STRESS: {
            vus: 200,
            duration: '15m',
        },
        SPIKE: {
            stages: [
                { duration: '1m', target: 10 },
                { duration: '30s', target: 100 },
                { duration: '1m', target: 100 },
                { duration: '30s', target: 10 },
                { duration: '1m', target: 10 },
            ],
        },
    },
};

// Helper to get random agent role
export function getRandomAgentRole() {
    return CONFIG.AGENT_ROLES[Math.floor(Math.random() * CONFIG.AGENT_ROLES.length)];
}

// Helper to create auth headers
export function getAuthHeaders() {
    return {
        'Content-Type': 'application/json',
        'X-API-Key': CONFIG.API_KEY,
    };
}

// Helper to generate unique ID
export function generateId() {
    return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

// Helper to format bytes
export function formatBytes(bytes) {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

// Task priorities
export const TASK_PRIORITIES = ['low', 'normal', 'high', 'critical'];

// Sample tasks for load testing
export const SAMPLE_TASKS = [
    'Analyze code for potential bugs',
    'Refactor function for better performance',
    'Write unit tests for module',
    'Review database schema',
    'Check security vulnerabilities',
    'Optimize API endpoint',
    'Debug authentication issue',
    'Implement caching strategy',
];

export function getRandomTask() {
    return SAMPLE_TASKS[Math.floor(Math.random() * SAMPLE_TASKS.length)];
}

export function getRandomPriority() {
    return TASK_PRIORITIES[Math.floor(Math.random() * TASK_PRIORITIES.length)];
}
