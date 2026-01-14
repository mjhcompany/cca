/**
 * CCA Load Test: Token Service Performance
 *
 * Tests the token service (TokenCounter, ContextAnalyzer, ContextCompressor)
 * under load conditions. Measures:
 * - Token analysis latency
 * - Context compression performance
 * - Metrics retrieval speed
 * - Recommendations endpoint performance
 *
 * Run with: k6 run token-service.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend, Gauge } from 'k6/metrics';
import { CONFIG, getAuthHeaders, generateId } from './config.js';

// Custom metrics for token service analysis
const tokenAnalyzeLatency = new Trend('token_analyze_latency', true);
const tokenCompressLatency = new Trend('token_compress_latency', true);
const tokenMetricsLatency = new Trend('token_metrics_latency', true);
const tokenRecommendLatency = new Trend('token_recommend_latency', true);

// Success rates
const analyzeSuccess = new Rate('analyze_success');
const compressSuccess = new Rate('compress_success');
const metricsSuccess = new Rate('metrics_success');
const recommendSuccess = new Rate('recommend_success');

// Token metrics
const tokensAnalyzed = new Counter('tokens_analyzed_total');
const compressionRatio = new Trend('compression_ratio', true);
const compressionSavings = new Trend('compression_savings_bytes', true);

// Errors
const tokenServiceErrors = new Counter('token_service_errors');

// Sample content of varying sizes for testing
const SAMPLE_CONTENTS = {
    small: generateSampleCode(500),      // ~500 chars
    medium: generateSampleCode(2000),    // ~2KB
    large: generateSampleCode(10000),    // ~10KB
    xlarge: generateSampleCode(50000),   // ~50KB
};

// Generate sample code content
function generateSampleCode(size) {
    const codePatterns = [
        '// This is a comment explaining the function below\n',
        'function processData(input) {\n',
        '    const result = [];\n',
        '    for (let i = 0; i < input.length; i++) {\n',
        '        if (input[i].valid) {\n',
        '            result.push(transform(input[i]));\n',
        '        }\n',
        '    }\n',
        '    return result;\n',
        '}\n\n',
        '// Another utility function\n',
        'async function fetchData(url, options = {}) {\n',
        '    try {\n',
        '        const response = await fetch(url, options);\n',
        '        if (!response.ok) throw new Error("Failed");\n',
        '        return await response.json();\n',
        '    } catch (error) {\n',
        '        console.error("Error:", error);\n',
        '        throw error;\n',
        '    }\n',
        '}\n\n',
    ];

    let result = '';
    let patternIndex = 0;

    while (result.length < size) {
        result += codePatterns[patternIndex % codePatterns.length];
        patternIndex++;
    }

    return result.substring(0, size);
}

// Get random content size
function getRandomContent() {
    const sizes = Object.keys(SAMPLE_CONTENTS);
    const size = sizes[Math.floor(Math.random() * sizes.length)];
    return { size, content: SAMPLE_CONTENTS[size] };
}

// Test scenarios
export const options = {
    scenarios: {
        // Scenario 1: Baseline - light load
        baseline: {
            executor: 'constant-vus',
            vus: 5,
            duration: '1m',
            tags: { scenario: 'baseline' },
        },
        // Scenario 2: Medium load - mixed operations
        medium_load: {
            executor: 'constant-vus',
            vus: 20,
            duration: '2m',
            startTime: '1m30s',
            tags: { scenario: 'medium' },
        },
        // Scenario 3: High load - concurrent analysis
        high_load: {
            executor: 'constant-vus',
            vus: 50,
            duration: '2m',
            startTime: '4m',
            tags: { scenario: 'high' },
        },
        // Scenario 4: Compression focus - heavy compression workload
        compression_focus: {
            executor: 'constant-arrival-rate',
            rate: 50,
            timeUnit: '1s',
            duration: '2m',
            preAllocatedVUs: 30,
            maxVUs: 60,
            startTime: '6m30s',
            tags: { scenario: 'compression' },
            env: { FOCUS: 'compress' },
        },
        // Scenario 5: Analysis focus - high-frequency analysis
        analysis_focus: {
            executor: 'constant-arrival-rate',
            rate: 100,
            timeUnit: '1s',
            duration: '2m',
            preAllocatedVUs: 50,
            maxVUs: 100,
            startTime: '9m',
            tags: { scenario: 'analysis' },
            env: { FOCUS: 'analyze' },
        },
        // Scenario 6: Large content stress test
        large_content: {
            executor: 'per-vu-iterations',
            vus: 20,
            iterations: 5,
            maxDuration: '3m',
            startTime: '11m30s',
            tags: { scenario: 'large_content' },
            env: { CONTENT_SIZE: 'xlarge' },
        },
    },
    thresholds: {
        // Token analysis should be fast
        'token_analyze_latency': ['p(95)<500', 'p(99)<1000'],
        'analyze_success': ['rate>0.99'],

        // Compression can take longer for large content
        'token_compress_latency': ['p(95)<2000', 'p(99)<5000'],
        'compress_success': ['rate>0.95'],

        // Metrics retrieval should be very fast
        'token_metrics_latency': ['p(95)<200', 'p(99)<500'],
        'metrics_success': ['rate>0.99'],

        // Recommendations should be reasonably fast
        'token_recommend_latency': ['p(95)<500', 'p(99)<1000'],
        'recommend_success': ['rate>0.99'],

        // HTTP general
        'http_req_duration': ['p(95)<2000', 'p(99)<5000'],
        'http_req_failed': ['rate<0.05'],
    },
    summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)', 'count'],
};

// Setup function
export function setup() {
    console.log('=== CCA Token Service Load Test ===');
    console.log(`Target URL: ${CONFIG.HTTP_BASE_URL}`);
    console.log('');

    // Verify API is accessible
    const healthRes = http.get(`${CONFIG.HTTP_BASE_URL}/health`);
    if (healthRes.status !== 200) {
        console.error('Health check failed!');
        return { abort: true };
    }

    // Verify token service endpoints
    const metricsRes = http.get(`${CONFIG.HTTP_BASE_URL}/api/v1/tokens/metrics`, {
        headers: getAuthHeaders(),
    });

    if (metricsRes.status === 200) {
        console.log('[OK] Token metrics endpoint responding');
    } else {
        console.warn(`[WARN] Token metrics returned status ${metricsRes.status}`);
    }

    console.log('');
    console.log('Sample content sizes:');
    for (const [size, content] of Object.entries(SAMPLE_CONTENTS)) {
        console.log(`  ${size}: ${content.length} chars (~${Math.round(content.length/4)} tokens)`);
    }
    console.log('');

    return {
        startTime: Date.now(),
        abort: false,
    };
}

// Test token analysis endpoint
function testTokenAnalysis(content, contentSize) {
    const agentId = `token-test-${generateId()}`;

    const payload = JSON.stringify({
        content: content,
        agent_id: agentId,
    });

    const start = Date.now();
    const res = http.post(
        `${CONFIG.HTTP_BASE_URL}/api/v1/tokens/analyze`,
        payload,
        {
            headers: getAuthHeaders(),
            tags: { name: 'analyze', content_size: contentSize },
            timeout: '30s',
        }
    );
    const duration = Date.now() - start;

    tokenAnalyzeLatency.add(duration);

    const success = check(res, {
        'analyze status 200': (r) => r.status === 200,
        'analyze has token_count': (r) => {
            try {
                const body = JSON.parse(r.body);
                return body.token_count !== undefined;
            } catch (e) {
                return false;
            }
        },
    });

    analyzeSuccess.add(success ? 1 : 0);

    if (success) {
        try {
            const body = JSON.parse(res.body);
            tokensAnalyzed.add(body.token_count || 0);
        } catch (e) {
            // Ignore
        }
    } else {
        tokenServiceErrors.add(1);
    }

    return { success, duration };
}

// Test token compression endpoint
function testTokenCompression(content, contentSize) {
    const agentId = `compress-test-${generateId()}`;

    const payload = JSON.stringify({
        content: content,
        target_reduction: 0.3,  // Target 30% reduction
        agent_id: agentId,
        strategies: ['code_comments', 'deduplication', 'summarize'],
    });

    const start = Date.now();
    const res = http.post(
        `${CONFIG.HTTP_BASE_URL}/api/v1/tokens/compress`,
        payload,
        {
            headers: getAuthHeaders(),
            tags: { name: 'compress', content_size: contentSize },
            timeout: '60s',
        }
    );
    const duration = Date.now() - start;

    tokenCompressLatency.add(duration);

    const success = check(res, {
        'compress status 200': (r) => r.status === 200,
        'compress has result': (r) => {
            try {
                const body = JSON.parse(r.body);
                return body.compressed_content !== undefined || body.result !== undefined;
            } catch (e) {
                return false;
            }
        },
    });

    compressSuccess.add(success ? 1 : 0);

    if (success) {
        try {
            const body = JSON.parse(res.body);
            const original = content.length;
            const compressed = (body.compressed_content || body.result || '').length;

            if (compressed > 0 && original > 0) {
                const ratio = compressed / original;
                compressionRatio.add(ratio);
                compressionSavings.add(original - compressed);
            }
        } catch (e) {
            // Ignore
        }
    } else {
        tokenServiceErrors.add(1);
    }

    return { success, duration };
}

// Test token metrics endpoint
function testTokenMetrics() {
    const start = Date.now();
    const res = http.get(
        `${CONFIG.HTTP_BASE_URL}/api/v1/tokens/metrics`,
        {
            headers: getAuthHeaders(),
            tags: { name: 'metrics' },
            timeout: '10s',
        }
    );
    const duration = Date.now() - start;

    tokenMetricsLatency.add(duration);

    const success = check(res, {
        'metrics status 200': (r) => r.status === 200,
        'metrics fast response': () => duration < 500,
    });

    metricsSuccess.add(success ? 1 : 0);

    if (!success) {
        tokenServiceErrors.add(1);
    }

    return { success, duration };
}

// Test recommendations endpoint
function testTokenRecommendations() {
    const start = Date.now();
    const res = http.get(
        `${CONFIG.HTTP_BASE_URL}/api/v1/tokens/recommendations`,
        {
            headers: getAuthHeaders(),
            tags: { name: 'recommendations' },
            timeout: '10s',
        }
    );
    const duration = Date.now() - start;

    tokenRecommendLatency.add(duration);

    const success = check(res, {
        'recommendations status 200': (r) => r.status === 200,
    });

    recommendSuccess.add(success ? 1 : 0);

    if (!success) {
        tokenServiceErrors.add(1);
    }

    return { success, duration };
}

// Main test function
export default function(data) {
    if (data && data.abort) {
        return;
    }

    const focus = __ENV.FOCUS || 'mixed';
    const contentSizeOverride = __ENV.CONTENT_SIZE;

    // Get content
    let contentSize, content;
    if (contentSizeOverride) {
        contentSize = contentSizeOverride;
        content = SAMPLE_CONTENTS[contentSize] || SAMPLE_CONTENTS.medium;
    } else {
        const sample = getRandomContent();
        contentSize = sample.size;
        content = sample.content;
    }

    if (focus === 'analyze') {
        // Analysis-focused scenario
        group('Token Analysis', function() {
            testTokenAnalysis(content, contentSize);
        });

        // Occasionally check metrics
        if (Math.random() < 0.1) {
            testTokenMetrics();
        }
    } else if (focus === 'compress') {
        // Compression-focused scenario
        group('Token Compression', function() {
            testTokenCompression(content, contentSize);
        });
    } else {
        // Mixed workload
        group('Token Analysis', function() {
            testTokenAnalysis(content, contentSize);
        });

        // 30% chance to also test compression
        if (Math.random() < 0.3) {
            group('Token Compression', function() {
                testTokenCompression(content, contentSize);
            });
        }

        // 20% chance to check metrics
        if (Math.random() < 0.2) {
            group('Token Metrics', function() {
                testTokenMetrics();
            });
        }

        // 10% chance to get recommendations
        if (Math.random() < 0.1) {
            group('Token Recommendations', function() {
                testTokenRecommendations();
            });
        }
    }

    sleep(0.1);
}

// Teardown function
export function teardown(data) {
    if (data && data.abort) {
        return;
    }

    const duration = (Date.now() - data.startTime) / 1000;
    console.log('');
    console.log('=== Token Service Test Complete ===');
    console.log(`Test duration: ${duration.toFixed(2)}s`);
}

// Custom summary handler
export function handleSummary(data) {
    const summary = {
        timestamp: new Date().toISOString(),
        test: 'token-service',
        description: 'Token service performance test',
        metrics: {
            analysis: {
                latency: data.metrics.token_analyze_latency,
                success_rate: data.metrics.analyze_success,
                tokens_analyzed: data.metrics.tokens_analyzed_total,
            },
            compression: {
                latency: data.metrics.token_compress_latency,
                success_rate: data.metrics.compress_success,
                compression_ratio: data.metrics.compression_ratio,
                savings_bytes: data.metrics.compression_savings_bytes,
            },
            metrics_endpoint: {
                latency: data.metrics.token_metrics_latency,
                success_rate: data.metrics.metrics_success,
            },
            recommendations: {
                latency: data.metrics.token_recommend_latency,
                success_rate: data.metrics.recommend_success,
            },
            errors: data.metrics.token_service_errors,
            http: {
                requests: data.metrics.http_reqs,
                duration: data.metrics.http_req_duration,
                failed: data.metrics.http_req_failed,
            },
        },
        thresholds: data.thresholds,
    };

    return {
        'results/token-service-results.json': JSON.stringify(summary, null, 2),
        stdout: textSummary(data),
    };
}

function textSummary(data) {
    const lines = [
        '',
        '╔══════════════════════════════════════════════════════════════════════════╗',
        '║              CCA TOKEN SERVICE LOAD TEST RESULTS                         ║',
        '╠══════════════════════════════════════════════════════════════════════════╣',
    ];

    // Token analysis metrics
    lines.push('║ TOKEN ANALYSIS                                                           ║');
    lines.push('║                                                                          ║');

    if (data.metrics.token_analyze_latency) {
        const lat = data.metrics.token_analyze_latency.values;
        const p95Status = lat['p(95)'] < 500 ? '✓' : '✗';
        lines.push(`║   Latency (ms):   avg: ${lat.avg.toFixed(0).padStart(6)} | p95: ${lat['p(95)'].toFixed(0).padStart(6)} | p99: ${lat['p(99)'].toFixed(0).padStart(6)} ${p95Status}        ║`);
    }

    if (data.metrics.analyze_success) {
        const rate = (data.metrics.analyze_success.values.rate * 100).toFixed(2);
        lines.push(`║   Success Rate:   ${rate.padStart(7)}%                                                  ║`);
    }

    if (data.metrics.tokens_analyzed_total) {
        const count = data.metrics.tokens_analyzed_total.values.count;
        lines.push(`║   Total Analyzed: ${count.toString().padStart(10)} tokens                                       ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════╣');

    // Token compression metrics
    lines.push('║ TOKEN COMPRESSION                                                        ║');
    lines.push('║                                                                          ║');

    if (data.metrics.token_compress_latency) {
        const lat = data.metrics.token_compress_latency.values;
        const p95Status = lat['p(95)'] < 2000 ? '✓' : '✗';
        lines.push(`║   Latency (ms):   avg: ${lat.avg.toFixed(0).padStart(6)} | p95: ${lat['p(95)'].toFixed(0).padStart(6)} | p99: ${lat['p(99)'].toFixed(0).padStart(6)} ${p95Status}        ║`);
    }

    if (data.metrics.compress_success) {
        const rate = (data.metrics.compress_success.values.rate * 100).toFixed(2);
        lines.push(`║   Success Rate:   ${rate.padStart(7)}%                                                  ║`);
    }

    if (data.metrics.compression_ratio) {
        const ratio = data.metrics.compression_ratio.values;
        const avgRatio = ((1 - ratio.avg) * 100).toFixed(1);
        lines.push(`║   Avg Reduction:  ${avgRatio.padStart(7)}%                                                  ║`);
    }

    if (data.metrics.compression_savings_bytes) {
        const savings = data.metrics.compression_savings_bytes.values;
        const totalSaved = Math.round(savings.count * savings.avg / 1024);
        lines.push(`║   Total Saved:    ${totalSaved.toString().padStart(7)} KB                                                ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════╣');

    // Metrics endpoint
    lines.push('║ METRICS & RECOMMENDATIONS ENDPOINTS                                      ║');
    lines.push('║                                                                          ║');

    if (data.metrics.token_metrics_latency) {
        const lat = data.metrics.token_metrics_latency.values;
        lines.push(`║   Metrics P95:    ${lat['p(95)'].toFixed(0).padStart(6)}ms                                                 ║`);
    }

    if (data.metrics.token_recommend_latency) {
        const lat = data.metrics.token_recommend_latency.values;
        lines.push(`║   Recommend P95:  ${lat['p(95)'].toFixed(0).padStart(6)}ms                                                 ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╠══════════════════════════════════════════════════════════════════════════╣');

    // Errors
    lines.push('║ ERRORS & THRESHOLD RESULTS                                               ║');
    lines.push('║                                                                          ║');

    if (data.metrics.token_service_errors) {
        const errors = data.metrics.token_service_errors.values.count;
        lines.push(`║   Total Errors:   ${errors.toString().padStart(8)}                                                  ║`);
    }

    if (data.thresholds) {
        const passed = Object.values(data.thresholds).filter(t => t.ok).length;
        const total = Object.keys(data.thresholds).length;
        const status = passed === total ? 'ALL PASS' : `${total - passed} FAILED`;
        lines.push(`║   Thresholds:     ${passed}/${total} ${status.padEnd(12)}                                        ║`);
    }

    lines.push('║                                                                          ║');
    lines.push('╚══════════════════════════════════════════════════════════════════════════╝');
    lines.push('');

    return lines.join('\n');
}
