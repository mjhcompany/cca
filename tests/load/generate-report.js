#!/usr/bin/env node
/**
 * CCA Load Test Report Generator
 *
 * Aggregates results from all load tests and generates a comprehensive report
 * with metrics including:
 * - Response times (avg, p95, p99, max)
 * - Throughput (requests/second)
 * - Error rates
 * - Resource utilization estimates
 *
 * Usage: node generate-report.js [results-dir]
 */

const fs = require('fs');
const path = require('path');

// Default results directory
const RESULTS_DIR = process.argv[2] || path.join(__dirname, 'results');

// Report output file
const REPORT_FILE = path.join(RESULTS_DIR, 'load-test-report.html');
const JSON_REPORT_FILE = path.join(RESULTS_DIR, 'load-test-report.json');

// Test result files
const TEST_FILES = [
    'agent-spawning-results.json',
    'websocket-throughput-results.json',
    'redis-pubsub-results.json',
    'postgres-queries-results.json',
    'full-system-results.json',
];

// Threshold configurations for grading
const GRADE_THRESHOLDS = {
    latency_p95: { excellent: 500, good: 1000, acceptable: 2000 },
    latency_p99: { excellent: 1000, good: 2000, acceptable: 5000 },
    success_rate: { excellent: 0.99, good: 0.95, acceptable: 0.90 },
    error_rate: { excellent: 0.01, good: 0.05, acceptable: 0.10 },
};

function loadResults() {
    const results = {};

    for (const file of TEST_FILES) {
        const filePath = path.join(RESULTS_DIR, file);
        if (fs.existsSync(filePath)) {
            try {
                const content = fs.readFileSync(filePath, 'utf8');
                const data = JSON.parse(content);
                const testName = file.replace('-results.json', '');
                results[testName] = data;
                console.log(`Loaded: ${file}`);
            } catch (e) {
                console.warn(`Warning: Could not parse ${file}: ${e.message}`);
            }
        } else {
            console.warn(`Warning: ${file} not found`);
        }
    }

    return results;
}

function calculateGrade(value, type, inverse = false) {
    const thresholds = GRADE_THRESHOLDS[type];
    if (!thresholds) return 'N/A';

    if (inverse) {
        // Lower is better (latency, error rate)
        if (value <= thresholds.excellent) return 'A';
        if (value <= thresholds.good) return 'B';
        if (value <= thresholds.acceptable) return 'C';
        return 'F';
    } else {
        // Higher is better (success rate)
        if (value >= thresholds.excellent) return 'A';
        if (value >= thresholds.good) return 'B';
        if (value >= thresholds.acceptable) return 'C';
        return 'F';
    }
}

function extractMetrics(results) {
    const metrics = {
        summary: {
            total_tests: Object.keys(results).length,
            timestamp: new Date().toISOString(),
            overall_grade: 'N/A',
        },
        tests: {},
        aggregated: {
            latency: { avg: [], p95: [], p99: [], max: [] },
            success_rates: [],
            error_counts: [],
            throughput: [],
        },
    };

    // Process each test result
    for (const [testName, data] of Object.entries(results)) {
        const testMetrics = {
            timestamp: data.timestamp,
            latency: {},
            success_rate: null,
            errors: 0,
            throughput: null,
            grade: 'N/A',
        };

        // Extract latency metrics from different possible locations
        const latencyKeys = [
            'agent_spawn_duration',
            'ws_connect_duration',
            'ws_message_latency',
            'broadcast_duration',
            'task_create_duration',
            'task_query_duration',
            'memory_search_duration',
            'http_api_latency',
            'db_latency',
            'redis_latency',
            'http_req_duration',
        ];

        for (const key of latencyKeys) {
            let metric = data.metrics?.[key];

            // Handle nested metrics structure
            if (!metric && data.metrics?.http_api?.[key.replace('http_api_', '')]) {
                metric = data.metrics.http_api[key.replace('http_api_', '')];
            }

            if (metric?.values) {
                testMetrics.latency[key] = {
                    avg: metric.values.avg || metric.values.mean || 0,
                    p95: metric.values['p(95)'] || 0,
                    p99: metric.values['p(99)'] || 0,
                    max: metric.values.max || 0,
                };

                metrics.aggregated.latency.avg.push(testMetrics.latency[key].avg);
                metrics.aggregated.latency.p95.push(testMetrics.latency[key].p95);
                metrics.aggregated.latency.p99.push(testMetrics.latency[key].p99);
                metrics.aggregated.latency.max.push(testMetrics.latency[key].max);
            }
        }

        // Extract success rate metrics
        const successKeys = [
            'agent_spawn_success',
            'ws_connection_success',
            'ws_message_success',
            'broadcast_success',
            'db_operation_success',
            'http_api_success',
            'redis_success',
        ];

        for (const key of successKeys) {
            const metric = data.metrics?.[key];
            if (metric?.values?.rate !== undefined) {
                testMetrics.success_rate = metric.values.rate;
                metrics.aggregated.success_rates.push(metric.values.rate);
                break;
            }
        }

        // Extract error counts
        const errorKeys = [
            'agent_spawn_errors',
            'ws_errors',
            'broadcast_errors',
            'db_operation_errors',
            'http_api_errors',
        ];

        for (const key of errorKeys) {
            const metric = data.metrics?.[key];
            if (metric?.values?.count !== undefined) {
                testMetrics.errors += metric.values.count;
            }
        }
        metrics.aggregated.error_counts.push(testMetrics.errors);

        // Extract throughput
        if (data.metrics?.http_reqs?.values?.rate) {
            testMetrics.throughput = data.metrics.http_reqs.values.rate;
            metrics.aggregated.throughput.push(testMetrics.throughput);
        }

        // Calculate test grade
        const grades = [];
        if (testMetrics.latency && Object.keys(testMetrics.latency).length > 0) {
            const avgP95 = Object.values(testMetrics.latency).reduce((a, b) => a + (b.p95 || 0), 0) / Object.keys(testMetrics.latency).length;
            grades.push(calculateGrade(avgP95, 'latency_p95', true));
        }
        if (testMetrics.success_rate !== null) {
            grades.push(calculateGrade(testMetrics.success_rate, 'success_rate', false));
        }

        if (grades.length > 0) {
            const gradeValues = { A: 4, B: 3, C: 2, F: 1 };
            const avgGrade = grades.reduce((a, b) => a + (gradeValues[b] || 0), 0) / grades.length;
            if (avgGrade >= 3.5) testMetrics.grade = 'A';
            else if (avgGrade >= 2.5) testMetrics.grade = 'B';
            else if (avgGrade >= 1.5) testMetrics.grade = 'C';
            else testMetrics.grade = 'F';
        }

        metrics.tests[testName] = testMetrics;
    }

    // Calculate overall grade
    const allGrades = Object.values(metrics.tests).map(t => t.grade).filter(g => g !== 'N/A');
    if (allGrades.length > 0) {
        const gradeValues = { A: 4, B: 3, C: 2, F: 1 };
        const avgGrade = allGrades.reduce((a, b) => a + (gradeValues[b] || 0), 0) / allGrades.length;
        if (avgGrade >= 3.5) metrics.summary.overall_grade = 'A';
        else if (avgGrade >= 2.5) metrics.summary.overall_grade = 'B';
        else if (avgGrade >= 1.5) metrics.summary.overall_grade = 'C';
        else metrics.summary.overall_grade = 'F';
    }

    return metrics;
}

function generateHtmlReport(metrics) {
    const gradeColor = (grade) => {
        switch (grade) {
            case 'A': return '#22c55e';
            case 'B': return '#84cc16';
            case 'C': return '#eab308';
            case 'F': return '#ef4444';
            default: return '#6b7280';
        }
    };

    const formatNumber = (num, decimals = 2) => {
        if (num === null || num === undefined) return 'N/A';
        return Number(num).toFixed(decimals);
    };

    const html = `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>CCA Load Test Report</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #0f172a;
            color: #e2e8f0;
            padding: 2rem;
            line-height: 1.6;
        }
        .container { max-width: 1400px; margin: 0 auto; }
        h1 { font-size: 2.5rem; margin-bottom: 0.5rem; color: #f8fafc; }
        h2 { font-size: 1.5rem; margin: 2rem 0 1rem; color: #94a3b8; border-bottom: 2px solid #334155; padding-bottom: 0.5rem; }
        h3 { font-size: 1.25rem; margin: 1rem 0 0.5rem; color: #cbd5e1; }
        .header { text-align: center; margin-bottom: 3rem; }
        .subtitle { color: #64748b; font-size: 1.1rem; }
        .summary-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 1.5rem;
            margin-bottom: 2rem;
        }
        .card {
            background: #1e293b;
            border-radius: 12px;
            padding: 1.5rem;
            border: 1px solid #334155;
        }
        .card-title { color: #94a3b8; font-size: 0.9rem; text-transform: uppercase; letter-spacing: 0.05em; }
        .card-value { font-size: 2rem; font-weight: 700; margin: 0.5rem 0; }
        .card-detail { color: #64748b; font-size: 0.85rem; }
        .grade {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            width: 48px;
            height: 48px;
            border-radius: 50%;
            font-size: 1.5rem;
            font-weight: 700;
        }
        .grade-large {
            width: 80px;
            height: 80px;
            font-size: 2.5rem;
        }
        .test-section { margin-bottom: 2rem; }
        .metrics-table {
            width: 100%;
            border-collapse: collapse;
            margin: 1rem 0;
        }
        .metrics-table th,
        .metrics-table td {
            padding: 0.75rem 1rem;
            text-align: left;
            border-bottom: 1px solid #334155;
        }
        .metrics-table th {
            background: #1e293b;
            color: #94a3b8;
            font-weight: 600;
            text-transform: uppercase;
            font-size: 0.75rem;
            letter-spacing: 0.05em;
        }
        .metrics-table tr:hover { background: #1e293b; }
        .metric-value { font-family: 'SF Mono', Monaco, monospace; }
        .status-bar {
            height: 8px;
            background: #334155;
            border-radius: 4px;
            overflow: hidden;
            margin-top: 0.5rem;
        }
        .status-bar-fill {
            height: 100%;
            border-radius: 4px;
            transition: width 0.3s ease;
        }
        .threshold-info {
            background: #1e293b;
            border-radius: 8px;
            padding: 1rem;
            margin-top: 2rem;
        }
        .threshold-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 1rem;
            margin-top: 1rem;
        }
        .threshold-item {
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }
        .threshold-dot {
            width: 12px;
            height: 12px;
            border-radius: 50%;
        }
        .recommendations {
            background: linear-gradient(135deg, #1e293b 0%, #0f172a 100%);
            border: 1px solid #334155;
            border-radius: 12px;
            padding: 1.5rem;
            margin-top: 2rem;
        }
        .recommendations ul {
            list-style: none;
            padding-left: 0;
        }
        .recommendations li {
            padding: 0.5rem 0;
            padding-left: 1.5rem;
            position: relative;
        }
        .recommendations li:before {
            content: "→";
            position: absolute;
            left: 0;
            color: #3b82f6;
        }
        .footer {
            text-align: center;
            margin-top: 3rem;
            padding-top: 2rem;
            border-top: 1px solid #334155;
            color: #64748b;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🚀 CCA Load Test Report</h1>
            <p class="subtitle">Generated: ${metrics.summary.timestamp}</p>
        </div>

        <div class="summary-grid">
            <div class="card">
                <div class="card-title">Overall Grade</div>
                <div class="card-value">
                    <span class="grade grade-large" style="background: ${gradeColor(metrics.summary.overall_grade)}20; color: ${gradeColor(metrics.summary.overall_grade)};">
                        ${metrics.summary.overall_grade}
                    </span>
                </div>
                <div class="card-detail">Based on ${metrics.summary.total_tests} test suites</div>
            </div>
            <div class="card">
                <div class="card-title">Tests Executed</div>
                <div class="card-value">${metrics.summary.total_tests}</div>
                <div class="card-detail">Load test scenarios completed</div>
            </div>
            <div class="card">
                <div class="card-title">Avg Latency (p95)</div>
                <div class="card-value">${metrics.aggregated.latency.p95.length > 0 ? formatNumber(metrics.aggregated.latency.p95.reduce((a, b) => a + b, 0) / metrics.aggregated.latency.p95.length) : 'N/A'} ms</div>
                <div class="card-detail">95th percentile response time</div>
            </div>
            <div class="card">
                <div class="card-title">Avg Success Rate</div>
                <div class="card-value">${metrics.aggregated.success_rates.length > 0 ? formatNumber(metrics.aggregated.success_rates.reduce((a, b) => a + b, 0) / metrics.aggregated.success_rates.length * 100, 1) : 'N/A'}%</div>
                <div class="card-detail">Request success percentage</div>
            </div>
        </div>

        <h2>Test Results by Component</h2>

        ${Object.entries(metrics.tests).map(([testName, testData]) => `
        <div class="test-section card">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;">
                <h3>${testName.replace(/-/g, ' ').replace(/\b\w/g, l => l.toUpperCase())}</h3>
                <span class="grade" style="background: ${gradeColor(testData.grade)}20; color: ${gradeColor(testData.grade)};">
                    ${testData.grade}
                </span>
            </div>

            ${Object.keys(testData.latency).length > 0 ? `
            <table class="metrics-table">
                <thead>
                    <tr>
                        <th>Metric</th>
                        <th>Average</th>
                        <th>P95</th>
                        <th>P99</th>
                        <th>Max</th>
                    </tr>
                </thead>
                <tbody>
                    ${Object.entries(testData.latency).map(([metric, values]) => `
                    <tr>
                        <td>${metric.replace(/_/g, ' ')}</td>
                        <td class="metric-value">${formatNumber(values.avg)} ms</td>
                        <td class="metric-value">${formatNumber(values.p95)} ms</td>
                        <td class="metric-value">${formatNumber(values.p99)} ms</td>
                        <td class="metric-value">${formatNumber(values.max)} ms</td>
                    </tr>
                    `).join('')}
                </tbody>
            </table>
            ` : '<p style="color: #64748b;">No latency metrics available</p>'}

            <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; margin-top: 1rem;">
                <div>
                    <div class="card-title">Success Rate</div>
                    <div class="card-value" style="font-size: 1.5rem;">${testData.success_rate !== null ? formatNumber(testData.success_rate * 100, 1) + '%' : 'N/A'}</div>
                    ${testData.success_rate !== null ? `
                    <div class="status-bar">
                        <div class="status-bar-fill" style="width: ${testData.success_rate * 100}%; background: ${testData.success_rate >= 0.95 ? '#22c55e' : testData.success_rate >= 0.90 ? '#eab308' : '#ef4444'};"></div>
                    </div>
                    ` : ''}
                </div>
                <div>
                    <div class="card-title">Errors</div>
                    <div class="card-value" style="font-size: 1.5rem; color: ${testData.errors > 0 ? '#ef4444' : '#22c55e'};">${testData.errors}</div>
                </div>
                <div>
                    <div class="card-title">Throughput</div>
                    <div class="card-value" style="font-size: 1.5rem;">${testData.throughput !== null ? formatNumber(testData.throughput, 1) + ' req/s' : 'N/A'}</div>
                </div>
            </div>
        </div>
        `).join('')}

        <div class="threshold-info">
            <h3>Grade Thresholds</h3>
            <div class="threshold-grid">
                <div class="threshold-item">
                    <span class="threshold-dot" style="background: #22c55e;"></span>
                    <span>A (Excellent): p95 < 500ms, Success > 99%</span>
                </div>
                <div class="threshold-item">
                    <span class="threshold-dot" style="background: #84cc16;"></span>
                    <span>B (Good): p95 < 1000ms, Success > 95%</span>
                </div>
                <div class="threshold-item">
                    <span class="threshold-dot" style="background: #eab308;"></span>
                    <span>C (Acceptable): p95 < 2000ms, Success > 90%</span>
                </div>
                <div class="threshold-item">
                    <span class="threshold-dot" style="background: #ef4444;"></span>
                    <span>F (Failing): p95 > 2000ms or Success < 90%</span>
                </div>
            </div>
        </div>

        <div class="recommendations">
            <h3>💡 Recommendations</h3>
            <ul>
                ${generateRecommendations(metrics).map(rec => `<li>${rec}</li>`).join('')}
            </ul>
        </div>

        <div class="footer">
            <p>CCA Load Test Suite • Generated by k6 and custom analysis</p>
            <p>Report generated: ${metrics.summary.timestamp}</p>
        </div>
    </div>
</body>
</html>`;

    return html;
}

function generateRecommendations(metrics) {
    const recommendations = [];

    // Analyze latency
    const avgP95 = metrics.aggregated.latency.p95.length > 0
        ? metrics.aggregated.latency.p95.reduce((a, b) => a + b, 0) / metrics.aggregated.latency.p95.length
        : 0;

    if (avgP95 > 2000) {
        recommendations.push('High latency detected (p95 > 2s). Consider optimizing database queries and enabling connection pooling.');
        recommendations.push('Review slow endpoints and consider implementing caching for frequently accessed data.');
    } else if (avgP95 > 1000) {
        recommendations.push('Moderate latency observed. Monitor database connection pool utilization and consider query optimization.');
    }

    // Analyze success rate
    const avgSuccessRate = metrics.aggregated.success_rates.length > 0
        ? metrics.aggregated.success_rates.reduce((a, b) => a + b, 0) / metrics.aggregated.success_rates.length
        : 1;

    if (avgSuccessRate < 0.95) {
        recommendations.push('Success rate below 95%. Investigate error logs and consider implementing circuit breakers.');
        recommendations.push('Review rate limiting configuration and adjust based on expected load patterns.');
    }

    // Analyze errors
    const totalErrors = metrics.aggregated.error_counts.reduce((a, b) => a + b, 0);
    if (totalErrors > 100) {
        recommendations.push(`High error count (${totalErrors}). Review application logs for common error patterns.`);
    }

    // Component-specific recommendations
    if (metrics.tests['agent-spawning']) {
        const spawnLatency = metrics.tests['agent-spawning'].latency?.agent_spawn_duration?.p95 || 0;
        if (spawnLatency > 3000) {
            recommendations.push('Agent spawn time is high. Consider pre-warming agent pools or optimizing Claude Code initialization.');
        }
    }

    if (metrics.tests['websocket-throughput']) {
        const wsLatency = metrics.tests['websocket-throughput'].latency?.ws_message_latency?.p95 || 0;
        if (wsLatency > 500) {
            recommendations.push('WebSocket message latency is elevated. Review backpressure handling and connection limits.');
        }
    }

    if (metrics.tests['postgres-queries']) {
        const dbLatency = metrics.tests['postgres-queries'].latency?.memory_search_duration?.p95 || 0;
        if (dbLatency > 2000) {
            recommendations.push('Vector similarity search is slow. Consider optimizing IVFFlat index parameters (lists count).');
            recommendations.push('Review PostgreSQL connection pool settings (max_connections, statement_timeout).');
        }
    }

    if (recommendations.length === 0) {
        recommendations.push('All metrics are within acceptable thresholds. Continue monitoring for regression.');
        recommendations.push('Consider running stress tests with higher load to identify breaking points.');
    }

    return recommendations;
}

function main() {
    console.log('=== CCA Load Test Report Generator ===\n');

    // Ensure results directory exists
    if (!fs.existsSync(RESULTS_DIR)) {
        fs.mkdirSync(RESULTS_DIR, { recursive: true });
        console.log(`Created results directory: ${RESULTS_DIR}`);
    }

    // Load test results
    const results = loadResults();

    if (Object.keys(results).length === 0) {
        console.error('\nNo test results found. Run load tests first:');
        console.error('  k6 run agent-spawning.js');
        console.error('  k6 run websocket-throughput.js');
        console.error('  k6 run redis-pubsub.js');
        console.error('  k6 run postgres-queries.js');
        console.error('  k6 run full-system.js');
        process.exit(1);
    }

    // Extract and analyze metrics
    const metrics = extractMetrics(results);

    // Generate JSON report
    fs.writeFileSync(JSON_REPORT_FILE, JSON.stringify(metrics, null, 2));
    console.log(`\nJSON report saved: ${JSON_REPORT_FILE}`);

    // Generate HTML report
    const htmlReport = generateHtmlReport(metrics);
    fs.writeFileSync(REPORT_FILE, htmlReport);
    console.log(`HTML report saved: ${REPORT_FILE}`);

    // Print summary to console
    console.log('\n=== Summary ===');
    console.log(`Overall Grade: ${metrics.summary.overall_grade}`);
    console.log(`Tests Analyzed: ${metrics.summary.total_tests}`);

    if (metrics.aggregated.latency.p95.length > 0) {
        const avgP95 = metrics.aggregated.latency.p95.reduce((a, b) => a + b, 0) / metrics.aggregated.latency.p95.length;
        console.log(`Avg Latency (p95): ${avgP95.toFixed(2)} ms`);
    }

    if (metrics.aggregated.success_rates.length > 0) {
        const avgSuccess = metrics.aggregated.success_rates.reduce((a, b) => a + b, 0) / metrics.aggregated.success_rates.length;
        console.log(`Avg Success Rate: ${(avgSuccess * 100).toFixed(2)}%`);
    }

    console.log('\n=== Recommendations ===');
    generateRecommendations(metrics).forEach(rec => console.log(`• ${rec}`));

    console.log('\n✅ Report generation complete!');
}

main();
