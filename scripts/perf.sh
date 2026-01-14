#!/bin/bash
#
# CCA Performance Testing and Profiling Script
#
# This script provides commands for running benchmarks, generating flamegraphs,
# and analyzing performance of the CCA daemon.
#
# Usage:
#   ./scripts/perf.sh <command> [options]
#
# Commands:
#   bench           Run all criterion benchmarks
#   bench-quick     Run benchmarks with fewer samples (faster)
#   flamegraph      Generate flamegraph from benchmark
#   profile         Build with profiling symbols and run perf
#   compare         Compare benchmark results between runs
#   report          Generate HTML benchmark report
#   help            Show this help message

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check for required tools
check_dependencies() {
    local missing=()

    if ! command -v cargo &> /dev/null; then
        missing+=("cargo")
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        exit 1
    fi
}

# Run all benchmarks
cmd_bench() {
    log_info "Running all benchmarks..."
    cd "$PROJECT_ROOT"

    cargo bench --package cca-daemon "$@"

    log_success "Benchmarks complete. Results in target/criterion/"
}

# Run benchmarks with quick settings
cmd_bench_quick() {
    log_info "Running quick benchmarks (reduced samples)..."
    cd "$PROJECT_ROOT"

    cargo bench --package cca-daemon -- --sample-size 10 --warm-up-time 1 --measurement-time 3 "$@"

    log_success "Quick benchmarks complete."
}

# Run specific benchmark suite
cmd_bench_suite() {
    local suite=$1
    shift

    if [[ -z "$suite" ]]; then
        log_error "Please specify a benchmark suite:"
        echo "  - token_benchmarks"
        echo "  - postgres_queries"
        echo "  - orchestrator"
        echo "  - code_indexing"
        echo "  - flamegraph_profile"
        exit 1
    fi

    log_info "Running benchmark suite: $suite"
    cd "$PROJECT_ROOT"

    cargo bench --package cca-daemon --bench "$suite" "$@"

    log_success "Benchmark suite '$suite' complete."
}

# Generate flamegraph from profiling benchmark
cmd_flamegraph() {
    log_info "Generating flamegraph..."
    cd "$PROJECT_ROOT"

    # Check if pprof feature is available
    cargo bench --package cca-daemon --bench flamegraph_profile -- --profile-time=10 "$@"

    # Find the generated flamegraph
    local flamegraph_dir="target/criterion"
    if [[ -d "$flamegraph_dir" ]]; then
        local svg_files=$(find "$flamegraph_dir" -name "flamegraph.svg" 2>/dev/null | head -5)
        if [[ -n "$svg_files" ]]; then
            log_success "Flamegraphs generated:"
            echo "$svg_files"
        fi
    fi
}

# Build with profiling symbols and optionally run perf
cmd_profile() {
    log_info "Building with profiling profile..."
    cd "$PROJECT_ROOT"

    cargo build --profile profiling --package cca-daemon

    log_success "Profiling build complete: target/profiling/ccad"

    echo ""
    log_info "To profile with perf (Linux):"
    echo "  perf record -g --call-graph=dwarf ./target/profiling/ccad"
    echo "  perf report"
    echo ""
    log_info "To generate flamegraph with perf:"
    echo "  perf record -g ./target/profiling/ccad &"
    echo "  # ... run some load ..."
    echo "  perf script | stackcollapse-perf.pl | flamegraph.pl > profile.svg"
    echo ""
    log_info "To use cargo-flamegraph (if installed):"
    echo "  cargo flamegraph --profile profiling --bin ccad"
}

# Compare benchmark results
cmd_compare() {
    local baseline=$1

    if [[ -z "$baseline" ]]; then
        log_info "Running benchmarks and saving as baseline..."
        cd "$PROJECT_ROOT"

        cargo bench --package cca-daemon -- --save-baseline main

        log_success "Baseline saved as 'main'"
        echo ""
        echo "To compare against this baseline later, run:"
        echo "  ./scripts/perf.sh compare main"
    else
        log_info "Comparing current results against baseline: $baseline"
        cd "$PROJECT_ROOT"

        cargo bench --package cca-daemon -- --baseline "$baseline"

        log_success "Comparison complete."
    fi
}

# Generate HTML report
cmd_report() {
    log_info "Generating HTML benchmark report..."
    cd "$PROJECT_ROOT"

    local report_dir="target/criterion"

    if [[ ! -d "$report_dir" ]]; then
        log_warning "No benchmark results found. Running benchmarks first..."
        cargo bench --package cca-daemon
    fi

    local index_file="$report_dir/report/index.html"

    if [[ -f "$index_file" ]]; then
        log_success "HTML report available at: $index_file"

        # Try to open in browser
        if command -v xdg-open &> /dev/null; then
            xdg-open "$index_file" 2>/dev/null || true
        elif command -v open &> /dev/null; then
            open "$index_file" 2>/dev/null || true
        fi
    else
        log_warning "Report index not found. Run benchmarks first."
    fi
}

# List available benchmarks
cmd_list() {
    log_info "Available benchmark suites:"
    echo ""
    echo "  token_benchmarks      - Token counting and analysis"
    echo "  postgres_queries      - Database query patterns and vector ops"
    echo "  orchestrator          - Task routing and workload distribution"
    echo "  code_indexing         - Code parsing and chunk extraction"
    echo "  flamegraph_profile    - CPU profiling with flamegraph output"
    echo "  compression_benchmarks - Context compression algorithms"
    echo "  rl_benchmarks         - Reinforcement learning engine"
    echo "  communication_benchmarks - Inter-agent communication"
    echo ""
    echo "Run a specific suite with:"
    echo "  ./scripts/perf.sh bench-suite <suite-name>"
}

# Show help
cmd_help() {
    cat << EOF
CCA Performance Testing and Profiling Script

Usage:
  ./scripts/perf.sh <command> [options]

Commands:
  bench              Run all criterion benchmarks
  bench-quick        Run benchmarks with fewer samples (faster)
  bench-suite <name> Run specific benchmark suite
  flamegraph         Generate flamegraph from profiling benchmark
  profile            Build with profiling symbols
  compare [baseline] Compare results against baseline
  report             Generate/open HTML benchmark report
  list               List available benchmark suites
  help               Show this help message

Examples:
  # Run all benchmarks
  ./scripts/perf.sh bench

  # Run quick benchmarks (faster, less accurate)
  ./scripts/perf.sh bench-quick

  # Run only token benchmarks
  ./scripts/perf.sh bench-suite token_benchmarks

  # Generate flamegraph
  ./scripts/perf.sh flamegraph

  # Build for profiling with perf
  ./scripts/perf.sh profile

  # Save baseline and compare later
  ./scripts/perf.sh compare          # Save as 'main' baseline
  ./scripts/perf.sh compare main     # Compare against 'main'

  # View HTML report
  ./scripts/perf.sh report

Environment Variables:
  CRITERION_DEBUG    Set to '1' for verbose criterion output
  RUST_BACKTRACE     Set to '1' for backtraces on errors

Output Locations:
  Benchmark results: target/criterion/
  HTML reports:      target/criterion/report/
  Flamegraphs:       target/criterion/*/profile/flamegraph.svg
  Profiling binary:  target/profiling/ccad

For more information, see:
  - docs/PERFORMANCE.md
  - target/criterion/report/index.html (after running benchmarks)
EOF
}

# Main command dispatcher
main() {
    check_dependencies

    local cmd=${1:-help}
    shift 2>/dev/null || true

    case "$cmd" in
        bench)
            cmd_bench "$@"
            ;;
        bench-quick)
            cmd_bench_quick "$@"
            ;;
        bench-suite)
            cmd_bench_suite "$@"
            ;;
        flamegraph)
            cmd_flamegraph "$@"
            ;;
        profile)
            cmd_profile "$@"
            ;;
        compare)
            cmd_compare "$@"
            ;;
        report)
            cmd_report "$@"
            ;;
        list)
            cmd_list "$@"
            ;;
        help|--help|-h)
            cmd_help
            ;;
        *)
            log_error "Unknown command: $cmd"
            echo ""
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
