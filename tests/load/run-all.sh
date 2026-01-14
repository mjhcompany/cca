#!/bin/bash
# CCA Load Test Runner
# Runs all load tests and generates a comprehensive report

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="${SCRIPT_DIR}/results"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
CCA_HTTP_URL="${CCA_HTTP_URL:-http://localhost:9200}"
CCA_WS_URL="${CCA_WS_URL:-ws://localhost:9100}"
CCA_API_KEY="${CCA_API_KEY:-test-api-key}"

# Test selection (default: all)
TESTS="${TESTS:-all}"
# Quick mode runs shorter duration tests
QUICK_MODE="${QUICK_MODE:-false}"

print_header() {
    echo ""
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}           CCA Load Test Suite                              ${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

print_status() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

check_prerequisites() {
    echo -e "${BLUE}Checking prerequisites...${NC}"

    # Check k6
    if ! command -v k6 &> /dev/null; then
        print_error "k6 is not installed. Install it from https://k6.io/docs/getting-started/installation/"
        echo ""
        echo "Quick install options:"
        echo "  macOS:  brew install k6"
        echo "  Linux:  sudo gpg -k && sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69 && echo 'deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main' | sudo tee /etc/apt/sources.list.d/k6.list && sudo apt-get update && sudo apt-get install k6"
        echo "  Docker: docker pull grafana/k6"
        exit 1
    fi
    print_status "k6 is installed ($(k6 version | head -1))"

    # Check Node.js for report generation
    if ! command -v node &> /dev/null; then
        print_warning "Node.js is not installed. Report generation will be skipped."
    else
        print_status "Node.js is installed ($(node --version))"
    fi

    echo ""
}

check_cca_services() {
    echo -e "${BLUE}Checking CCA services...${NC}"

    # Check HTTP API
    if curl -s --connect-timeout 5 "${CCA_HTTP_URL}/health" > /dev/null 2>&1; then
        print_status "CCA HTTP API is reachable at ${CCA_HTTP_URL}"
    else
        print_error "CCA HTTP API is not reachable at ${CCA_HTTP_URL}"
        echo "  Make sure the CCA daemon is running: cargo run --bin ccad"
        exit 1
    fi

    # Check PostgreSQL via API
    POSTGRES_STATUS=$(curl -s "${CCA_HTTP_URL}/api/v1/postgres/status" -H "X-API-Key: ${CCA_API_KEY}" 2>/dev/null || echo '{}')
    if echo "$POSTGRES_STATUS" | grep -q '"connected"'; then
        print_status "PostgreSQL is connected"
    else
        print_warning "PostgreSQL status unknown (may affect some tests)"
    fi

    # Check Redis via API
    REDIS_STATUS=$(curl -s "${CCA_HTTP_URL}/api/v1/redis/status" -H "X-API-Key: ${CCA_API_KEY}" 2>/dev/null || echo '{}')
    if echo "$REDIS_STATUS" | grep -q '"connected"'; then
        print_status "Redis is connected"
    else
        print_warning "Redis status unknown (may affect some tests)"
    fi

    echo ""
}

create_results_dir() {
    mkdir -p "${RESULTS_DIR}"
    print_status "Results directory: ${RESULTS_DIR}"
}

run_test() {
    local test_name=$1
    local test_file=$2
    local extra_args="${3:-}"

    echo ""
    echo -e "${BLUE}Running: ${test_name}${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Set environment variables
    export CCA_HTTP_URL
    export CCA_WS_URL
    export CCA_API_KEY

    # Quick mode uses shorter durations
    if [ "${QUICK_MODE}" = "true" ]; then
        extra_args="${extra_args} --duration 30s"
    fi

    # Run k6 test
    cd "${SCRIPT_DIR}"
    if k6 run ${extra_args} "${test_file}"; then
        print_status "${test_name} completed successfully"
    else
        print_warning "${test_name} completed with errors (check results)"
    fi
}

run_all_tests() {
    local start_time=$(date +%s)

    echo -e "${BLUE}Starting load tests at $(date)${NC}"
    echo ""

    # Create results directory
    create_results_dir

    # Run individual tests based on selection
    case "${TESTS}" in
        all)
            run_test "API Throughput Test" "api-throughput.js"
            run_test "Agent Spawning Test (<2s target)" "agent-spawning.js"
            run_test "Message Latency Test (<50ms P99 target)" "message-latency.js"
            run_test "Token Service Test" "token-service.js"
            run_test "WebSocket Throughput Test" "websocket-throughput.js"
            run_test "Redis Pub/Sub Test" "redis-pubsub.js"
            run_test "PostgreSQL Query Test" "postgres-queries.js"
            run_test "Task Submission Test" "task-submission.js"
            run_test "Full System Integration Test" "full-system.js"
            ;;
        api)
            run_test "API Throughput Test" "api-throughput.js"
            ;;
        agent)
            run_test "Agent Spawning Test (<2s target)" "agent-spawning.js"
            ;;
        message|latency)
            run_test "Message Latency Test (<50ms P99 target)" "message-latency.js"
            ;;
        token|tokens)
            run_test "Token Service Test" "token-service.js"
            ;;
        websocket|ws)
            run_test "WebSocket Throughput Test" "websocket-throughput.js"
            ;;
        redis)
            run_test "Redis Pub/Sub Test" "redis-pubsub.js"
            ;;
        postgres|pg)
            run_test "PostgreSQL Query Test" "postgres-queries.js"
            ;;
        task|tasks)
            run_test "Task Submission Test" "task-submission.js"
            ;;
        system|full)
            run_test "Full System Integration Test" "full-system.js"
            ;;
        primary)
            # Run only the primary target tests
            run_test "Agent Spawning Test (<2s target)" "agent-spawning.js"
            run_test "Message Latency Test (<50ms P99 target)" "message-latency.js"
            run_test "API Throughput Test" "api-throughput.js"
            ;;
        *)
            print_error "Unknown test: ${TESTS}"
            echo "Available tests: all, api, agent, message, token, websocket, redis, postgres, task, system, primary"
            exit 1
            ;;
    esac

    local end_time=$(date +%s)
    local duration=$((end_time - start_time))

    echo ""
    echo -e "${BLUE}All tests completed in ${duration} seconds${NC}"
}

generate_report() {
    echo ""
    echo -e "${BLUE}Generating report...${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if command -v node &> /dev/null; then
        cd "${SCRIPT_DIR}"
        node generate-report.js "${RESULTS_DIR}"

        echo ""
        print_status "Report generated successfully!"
        echo ""
        echo "  HTML Report: ${RESULTS_DIR}/load-test-report.html"
        echo "  JSON Report: ${RESULTS_DIR}/load-test-report.json"
    else
        print_warning "Node.js not available. Skipping report generation."
        echo "  Install Node.js and run: node generate-report.js"
    fi
}

print_usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -t, --test TEST    Run specific test (see available tests below)"
    echo "  -q, --quick        Run tests in quick mode (shorter durations)"
    echo "  -u, --url URL      CCA HTTP API URL (default: http://localhost:9200)"
    echo "  -w, --ws URL       CCA WebSocket URL (default: ws://localhost:9100)"
    echo "  -k, --key KEY      API key for authentication"
    echo "  -h, --help         Show this help message"
    echo ""
    echo "Available Tests:"
    echo "  all       - Run all load tests"
    echo "  primary   - Run primary target tests (agent spawn, message latency, API)"
    echo "  api       - API endpoint throughput testing"
    echo "  agent     - Agent spawning (<2s target)"
    echo "  message   - Message latency (<50ms P99 target)"
    echo "  token     - Token service performance"
    echo "  websocket - WebSocket throughput"
    echo "  redis     - Redis pub/sub performance"
    echo "  postgres  - PostgreSQL query performance"
    echo "  task      - Task submission stress test"
    echo "  system    - Full system integration test"
    echo ""
    echo "Primary Performance Targets:"
    echo "  - Agent spawn time: <2s (P95)"
    echo "  - Message latency: <50ms (P99)"
    echo "  - API endpoint latency: <200ms (P95)"
    echo ""
    echo "Examples:"
    echo "  $0                           # Run all tests"
    echo "  $0 -t primary                # Run primary target tests only"
    echo "  $0 -t agent                  # Run only agent spawning test"
    echo "  $0 -t message                # Run only message latency test"
    echo "  $0 -q                        # Run all tests in quick mode"
    echo "  $0 -t redis -u http://prod:9200  # Run Redis test against prod"
    echo ""
    echo "Environment Variables:"
    echo "  CCA_HTTP_URL   HTTP API URL"
    echo "  CCA_WS_URL     WebSocket URL"
    echo "  CCA_API_KEY    API key"
    echo "  TESTS          Test selection"
    echo "  QUICK_MODE     Enable quick mode (true/false)"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--test)
            TESTS="$2"
            shift 2
            ;;
        -q|--quick)
            QUICK_MODE="true"
            shift
            ;;
        -u|--url)
            CCA_HTTP_URL="$2"
            shift 2
            ;;
        -w|--ws)
            CCA_WS_URL="$2"
            shift 2
            ;;
        -k|--key)
            CCA_API_KEY="$2"
            shift 2
            ;;
        -h|--help)
            print_usage
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac
done

# Main execution
print_header
check_prerequisites
check_cca_services
run_all_tests
generate_report

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}           Load testing complete!                            ${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo ""
