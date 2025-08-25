#!/bin/bash

# WAL Performance Benchmark Runner Script
# Provides convenient commands for running comprehensive performance benchmarks

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Project root directory
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Print colored output
print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Function to check prerequisites
check_prerequisites() {
    print_info "Checking prerequisites..."
    
    # Check if cargo is installed
    if ! command -v cargo &> /dev/null; then
        print_error "cargo is not installed"
        exit 1
    fi
    
    # Check if criterion is available
    if ! grep -q "criterion" "${PROJECT_ROOT}/Cargo.toml"; then
        print_warning "Criterion not found in workspace dependencies"
    fi
    
    # Check if benchmark files exist
    if [[ ! -f "${PROJECT_ROOT}/benches/wal_benchmarks.rs" ]]; then
        print_error "WAL benchmarks file not found"
        exit 1
    fi
    
    print_success "Prerequisites check passed"
}

# Function to build project in release mode
build_project() {
    print_info "Building project in release mode..."
    cd "${PROJECT_ROOT}"
    
    if cargo build --release; then
        print_success "Project built successfully"
    else
        print_error "Project build failed"
        exit 1
    fi
}

# Function to run quick validation
run_quick_validation() {
    print_info "Running quick performance validation..."
    cd "${PROJECT_ROOT}"
    
    # Run specific quick benchmarks
    cargo bench --bench wal_benchmarks -- \
        "single_write_latency" \
        "batch_write_throughput" \
        "read_operations/sequential_reads" \
        "message_conversion"
    
    print_success "Quick validation completed"
}

# Function to run comprehensive benchmarks
run_comprehensive() {
    print_info "Running comprehensive performance benchmarks..."
    print_warning "This may take 5-10 minutes..."
    cd "${PROJECT_ROOT}"
    
    # Run all benchmarks
    if cargo bench --bench wal_benchmarks; then
        print_success "Comprehensive benchmarks completed"
    else
        print_error "Benchmarks failed"
        exit 1
    fi
}

# Function to run latency-focused benchmarks
run_latency_focused() {
    print_info "Running latency-focused benchmarks..."
    cd "${PROJECT_ROOT}"
    
    cargo bench --bench wal_benchmarks -- \
        "single_write_latency" \
        "read_operations" \
        "message_conversion"
    
    print_success "Latency benchmarks completed"
}

# Function to run throughput-focused benchmarks
run_throughput_focused() {
    print_info "Running throughput-focused benchmarks..."
    cd "${PROJECT_ROOT}"
    
    cargo bench --bench wal_benchmarks -- \
        "batch_write_throughput" \
        "concurrent_writes" \
        "transaction_processing" \
        "streaming_api"
    
    print_success "Throughput benchmarks completed"
}

# Function to run stress tests
run_stress_tests() {
    print_info "Running stress test benchmarks..."
    print_warning "This may take several minutes and use significant system resources"
    cd "${PROJECT_ROOT}"
    
    cargo bench --bench wal_benchmarks -- \
        "concurrent_writes" \
        "comprehensive_stress"
    
    print_success "Stress tests completed"
}

# Function to validate performance targets
validate_performance() {
    print_info "Validating performance targets..."
    cd "${PROJECT_ROOT}"
    
    # Run performance validation tests
    if cargo test --release -- performance_validation_tests; then
        print_success "Performance targets validated"
    else
        print_error "Performance validation failed"
        exit 1
    fi
}

# Function to generate performance report
generate_report() {
    print_info "Generating performance report..."
    cd "${PROJECT_ROOT}"
    
    # Create reports directory
    mkdir -p target/reports
    
    # Copy criterion results to reports
    if [[ -d "target/criterion" ]]; then
        cp -r target/criterion target/reports/
        print_success "Criterion results copied to target/reports/"
    fi
    
    # Generate HTML report if criterion supports it
    if command -v criterion-to-html &> /dev/null; then
        criterion-to-html target/criterion target/reports/benchmark_report.html
        print_success "HTML report generated: target/reports/benchmark_report.html"
    fi
    
    # Generate summary report
    cat > target/reports/performance_summary.md << EOF
# WAL Performance Benchmark Summary

## Test Environment
- Date: $(date)
- System: $(uname -a)
- Rust Version: $(rustc --version)

## Performance Targets
- Write Latency: <10μs P99 end-to-end
- Read Latency: <5μs P99 for memory-mapped reads  
- Throughput: 10M+ messages/second
- Recovery Speed: <500ms for 1GB WAL
- Memory Efficiency: <1KB per inactive stream
- Batch Performance: <1μs amortized per message in batches

## Benchmark Results
Results are available in the Criterion output files in target/criterion/

## Commands Used
- Quick validation: ./scripts/run_benchmarks.sh quick
- Comprehensive: ./scripts/run_benchmarks.sh comprehensive
- Validation: ./scripts/run_benchmarks.sh validate

## Next Steps
1. Review detailed results in target/criterion/
2. Analyze any performance regressions
3. Run stress tests if needed: ./scripts/run_benchmarks.sh stress
EOF
    
    print_success "Performance summary generated: target/reports/performance_summary.md"
}

# Function to clean benchmark artifacts
clean_benchmarks() {
    print_info "Cleaning benchmark artifacts..."
    cd "${PROJECT_ROOT}"
    
    # Remove criterion results
    rm -rf target/criterion
    rm -rf target/reports
    
    print_success "Benchmark artifacts cleaned"
}

# Function to run continuous integration benchmarks
run_ci_benchmarks() {
    print_info "Running CI-friendly benchmarks..."
    cd "${PROJECT_ROOT}"
    
    # Run benchmarks with shorter duration for CI
    export CARGO_BENCH_DURATION=30  # 30 seconds per benchmark
    
    cargo bench --bench wal_benchmarks -- \
        "single_write_latency" \
        "batch_write_throughput" \
        "read_operations/sequential_reads" \
        "message_conversion/msg_to_entry" \
        --sample-size 10 \
        --measurement-time 5
    
    print_success "CI benchmarks completed"
}

# Function to show system info for benchmarking
show_system_info() {
    print_info "System Information for Benchmarking:"
    echo "======================================"
    echo "OS: $(uname -a)"
    echo "CPU: $(lscpu | grep 'Model name' | head -1 | cut -d':' -f2 | xargs)"
    echo "CPU Cores: $(nproc)"
    echo "Memory: $(free -h | awk '/^Mem:/ {print $2}')"
    echo "Rust Version: $(rustc --version)"
    echo "Cargo Version: $(cargo --version)"
    echo "======================================"
}

# Function to run flamegraph profiling
run_flamegraph() {
    print_info "Running flamegraph profiling..."
    
    if ! command -v flamegraph &> /dev/null; then
        print_warning "flamegraph not installed. Installing..."
        cargo install flamegraph
    fi
    
    cd "${PROJECT_ROOT}"
    
    # Run specific benchmark with flamegraph
    flamegraph --bench --output target/flamegraph.svg -- \
        --bench wal_benchmarks single_write_latency
    
    print_success "Flamegraph generated: target/flamegraph.svg"
}

# Main function
main() {
    case "${1:-help}" in
        "quick"|"q")
            check_prerequisites
            build_project
            run_quick_validation
            ;;
        "comprehensive"|"comp"|"c")
            check_prerequisites
            build_project
            run_comprehensive
            generate_report
            ;;
        "latency"|"lat"|"l")
            check_prerequisites
            build_project
            run_latency_focused
            ;;
        "throughput"|"tp"|"t")
            check_prerequisites
            build_project
            run_throughput_focused
            ;;
        "stress"|"s")
            check_prerequisites
            build_project
            run_stress_tests
            ;;
        "validate"|"val"|"v")
            check_prerequisites
            build_project
            validate_performance
            ;;
        "report"|"r")
            generate_report
            ;;
        "clean")
            clean_benchmarks
            ;;
        "ci")
            check_prerequisites
            build_project
            run_ci_benchmarks
            ;;
        "info"|"i")
            show_system_info
            ;;
        "profile"|"p")
            check_prerequisites
            build_project
            run_flamegraph
            ;;
        "help"|"h"|*)
            echo "WAL Performance Benchmark Runner"
            echo "================================"
            echo ""
            echo "Usage: $0 <command>"
            echo ""
            echo "Commands:"
            echo "  quick (q)           - Quick performance validation (~2 minutes)"
            echo "  comprehensive (c)   - Full benchmark suite (~10 minutes)"
            echo "  latency (l)        - Latency-focused benchmarks"
            echo "  throughput (t)     - Throughput-focused benchmarks"
            echo "  stress (s)         - High-load stress tests"
            echo "  validate (v)       - Validate performance targets"
            echo "  report (r)         - Generate performance report"
            echo "  clean              - Clean benchmark artifacts"
            echo "  ci                 - CI-friendly benchmark run"
            echo "  info (i)           - Show system information"
            echo "  profile (p)        - Run flamegraph profiling"
            echo "  help (h)           - Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0 quick           # Quick validation"
            echo "  $0 comprehensive   # Full benchmark suite"
            echo "  $0 validate        # Check performance targets"
            ;;
    esac
}

# Run main function with all arguments
main "$@"