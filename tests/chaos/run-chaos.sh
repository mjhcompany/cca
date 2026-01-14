#!/bin/bash
# CCA Chaos Testing Script
# Runs chaos experiments using Toxiproxy to inject network faults
#
# Prerequisites:
#   - docker compose -f docker-compose.test.yml up -d
#   - CCA daemon running (pointed at toxiproxy endpoints)
#
# Usage:
#   ./run-chaos.sh [experiment]
#
# Experiments:
#   redis-latency     - Add 100-500ms latency to Redis
#   redis-timeout     - Cause Redis connection timeouts
#   redis-down        - Take Redis completely offline
#   postgres-latency  - Add 100-500ms latency to PostgreSQL
#   postgres-timeout  - Cause PostgreSQL connection timeouts
#   postgres-down     - Take PostgreSQL completely offline
#   network-jitter    - Random latency on all connections
#   all               - Run all experiments sequentially

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOXIPROXY_API="${TOXIPROXY_API:-http://localhost:8474}"
RESULTS_DIR="${SCRIPT_DIR}/results"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_header() {
    echo ""
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}           CCA Chaos Engineering Tests                       ${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

print_status() { echo -e "${GREEN}[✓]${NC} $1"; }
print_warning() { echo -e "${YELLOW}[!]${NC} $1"; }
print_error() { echo -e "${RED}[✗]${NC} $1"; }
print_info() { echo -e "${BLUE}[i]${NC} $1"; }

# Check Toxiproxy is available
check_toxiproxy() {
    if ! curl -s "${TOXIPROXY_API}/version" > /dev/null 2>&1; then
        print_error "Toxiproxy is not available at ${TOXIPROXY_API}"
        echo "Start the test environment: docker compose -f docker-compose.test.yml up -d"
        exit 1
    fi
    print_status "Toxiproxy is available"
}

# List all proxies
list_proxies() {
    curl -s "${TOXIPROXY_API}/proxies" | jq -r 'keys[]'
}

# Add a toxic to a proxy
add_toxic() {
    local proxy=$1
    local toxic_name=$2
    local toxic_type=$3
    local attributes=$4
    local stream=${5:-downstream}

    curl -s -X POST "${TOXIPROXY_API}/proxies/${proxy}/toxics" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"${toxic_name}\", \"type\": \"${toxic_type}\", \"stream\": \"${stream}\", \"attributes\": ${attributes}}"
}

# Remove a toxic from a proxy
remove_toxic() {
    local proxy=$1
    local toxic_name=$2

    curl -s -X DELETE "${TOXIPROXY_API}/proxies/${proxy}/toxics/${toxic_name}" > /dev/null 2>&1 || true
}

# Remove all toxics from a proxy
clear_toxics() {
    local proxy=$1

    for toxic in $(curl -s "${TOXIPROXY_API}/proxies/${proxy}/toxics" | jq -r '.[].name' 2>/dev/null); do
        remove_toxic "$proxy" "$toxic"
    done
}

# Clear all toxics from all proxies
clear_all_toxics() {
    print_info "Clearing all toxics..."
    for proxy in $(list_proxies); do
        clear_toxics "$proxy"
    done
    print_status "All toxics cleared"
}

# Disable a proxy
disable_proxy() {
    local proxy=$1
    curl -s -X POST "${TOXIPROXY_API}/proxies/${proxy}" \
        -H "Content-Type: application/json" \
        -d '{"enabled": false}'
}

# Enable a proxy
enable_proxy() {
    local proxy=$1
    curl -s -X POST "${TOXIPROXY_API}/proxies/${proxy}" \
        -H "Content-Type: application/json" \
        -d '{"enabled": true}'
}

# Run a chaos experiment
run_experiment() {
    local name=$1
    local duration=${2:-60}
    local setup_fn=$3
    local teardown_fn=$4

    echo ""
    print_info "Running experiment: ${name}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Record start time
    local start_time=$(date +%s)

    # Setup the chaos condition
    $setup_fn

    print_warning "Chaos condition active for ${duration} seconds"
    print_info "Monitor the system behavior during this period"

    # Run load test during chaos (if k6 is available)
    if command -v k6 &> /dev/null; then
        print_info "Running concurrent load test..."
        k6 run --quiet --duration "${duration}s" --vus 5 "${SCRIPT_DIR}/../load/baseline.js" 2>&1 | tee "${RESULTS_DIR}/${name}-loadtest.log" &
        local k6_pid=$!
    fi

    # Wait for the chaos duration
    sleep "$duration"

    # Kill k6 if running
    if [ -n "$k6_pid" ]; then
        kill $k6_pid 2>/dev/null || true
        wait $k6_pid 2>/dev/null || true
    fi

    # Teardown the chaos condition
    $teardown_fn

    # Record end time
    local end_time=$(date +%s)
    local total_duration=$((end_time - start_time))

    print_status "Experiment '${name}' completed in ${total_duration}s"

    # Save results
    echo "{\"experiment\": \"${name}\", \"duration\": ${total_duration}, \"timestamp\": \"$(date -Iseconds)\"}" > "${RESULTS_DIR}/${name}-result.json"
}

# Experiment: Redis Latency
setup_redis_latency() {
    add_toxic "redis" "latency" "latency" '{"latency": 200, "jitter": 100}'
    print_status "Added 200ms (±100ms) latency to Redis"
}

teardown_redis_latency() {
    remove_toxic "redis" "latency"
    print_status "Removed Redis latency"
}

# Experiment: Redis Timeout
setup_redis_timeout() {
    add_toxic "redis" "timeout" "timeout" '{"timeout": 5000}'
    print_status "Added 5s timeout to Redis connections"
}

teardown_redis_timeout() {
    remove_toxic "redis" "timeout"
    print_status "Removed Redis timeout"
}

# Experiment: Redis Down
setup_redis_down() {
    disable_proxy "redis"
    print_status "Disabled Redis proxy (simulating Redis down)"
}

teardown_redis_down() {
    enable_proxy "redis"
    print_status "Re-enabled Redis proxy"
}

# Experiment: PostgreSQL Latency
setup_postgres_latency() {
    add_toxic "postgres" "latency" "latency" '{"latency": 300, "jitter": 150}'
    print_status "Added 300ms (±150ms) latency to PostgreSQL"
}

teardown_postgres_latency() {
    remove_toxic "postgres" "latency"
    print_status "Removed PostgreSQL latency"
}

# Experiment: PostgreSQL Timeout
setup_postgres_timeout() {
    add_toxic "postgres" "timeout" "timeout" '{"timeout": 10000}'
    print_status "Added 10s timeout to PostgreSQL connections"
}

teardown_postgres_timeout() {
    remove_toxic "postgres" "timeout"
    print_status "Removed PostgreSQL timeout"
}

# Experiment: PostgreSQL Down
setup_postgres_down() {
    disable_proxy "postgres"
    print_status "Disabled PostgreSQL proxy (simulating PostgreSQL down)"
}

teardown_postgres_down() {
    enable_proxy "postgres"
    print_status "Re-enabled PostgreSQL proxy"
}

# Experiment: Network Jitter
setup_network_jitter() {
    add_toxic "redis" "jitter" "latency" '{"latency": 50, "jitter": 100}'
    add_toxic "postgres" "jitter" "latency" '{"latency": 50, "jitter": 100}'
    print_status "Added network jitter to all connections"
}

teardown_network_jitter() {
    remove_toxic "redis" "jitter"
    remove_toxic "postgres" "jitter"
    print_status "Removed network jitter"
}

# Create results directory
mkdir -p "${RESULTS_DIR}"

# Main
print_header
check_toxiproxy

EXPERIMENT="${1:-help}"
DURATION="${2:-60}"

case "$EXPERIMENT" in
    redis-latency)
        run_experiment "redis-latency" "$DURATION" setup_redis_latency teardown_redis_latency
        ;;
    redis-timeout)
        run_experiment "redis-timeout" "$DURATION" setup_redis_timeout teardown_redis_timeout
        ;;
    redis-down)
        run_experiment "redis-down" "$DURATION" setup_redis_down teardown_redis_down
        ;;
    postgres-latency)
        run_experiment "postgres-latency" "$DURATION" setup_postgres_latency teardown_postgres_latency
        ;;
    postgres-timeout)
        run_experiment "postgres-timeout" "$DURATION" setup_postgres_timeout teardown_postgres_timeout
        ;;
    postgres-down)
        run_experiment "postgres-down" "$DURATION" setup_postgres_down teardown_postgres_down
        ;;
    network-jitter)
        run_experiment "network-jitter" "$DURATION" setup_network_jitter teardown_network_jitter
        ;;
    all)
        run_experiment "redis-latency" 30 setup_redis_latency teardown_redis_latency
        sleep 10
        run_experiment "postgres-latency" 30 setup_postgres_latency teardown_postgres_latency
        sleep 10
        run_experiment "network-jitter" 30 setup_network_jitter teardown_network_jitter
        sleep 10
        run_experiment "redis-down" 30 setup_redis_down teardown_redis_down
        sleep 10
        run_experiment "postgres-down" 30 setup_postgres_down teardown_postgres_down
        ;;
    clear)
        clear_all_toxics
        ;;
    help|*)
        echo "Usage: $0 [experiment] [duration_seconds]"
        echo ""
        echo "Experiments:"
        echo "  redis-latency     - Add 100-500ms latency to Redis"
        echo "  redis-timeout     - Cause Redis connection timeouts"
        echo "  redis-down        - Take Redis completely offline"
        echo "  postgres-latency  - Add 100-500ms latency to PostgreSQL"
        echo "  postgres-timeout  - Cause PostgreSQL connection timeouts"
        echo "  postgres-down     - Take PostgreSQL completely offline"
        echo "  network-jitter    - Random latency on all connections"
        echo "  all               - Run all experiments sequentially"
        echo "  clear             - Clear all toxics"
        echo ""
        echo "Examples:"
        echo "  $0 redis-latency 60    # Run Redis latency test for 60s"
        echo "  $0 all                 # Run all experiments"
        echo "  $0 clear               # Clear all injected faults"
        ;;
esac

echo ""
print_status "Chaos testing complete. Results in: ${RESULTS_DIR}"
