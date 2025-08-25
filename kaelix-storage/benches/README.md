# WAL Performance Benchmarks

This directory contains comprehensive performance benchmarks for the Kaelix Storage WAL (Write-Ahead Log) system, designed to validate ultra-high-performance targets.

## Performance Targets

The benchmarks validate these critical performance requirements:

### 🎯 Primary Targets
- **Write Latency**: <10μs P99 end-to-end
- **Read Latency**: <5μs P99 for memory-mapped reads  
- **Throughput**: 10M+ messages/second
- **Recovery Speed**: <500ms for 1GB WAL
- **Memory Efficiency**: <1KB per inactive stream
- **Batch Performance**: <1μs amortized per message in batches

### 🚀 Extended Targets
- **Concurrent Write Performance**: <20μs per write under high concurrency
- **Message Conversion**: <5μs message ↔ storage entry conversion
- **Transaction Processing**: <1ms per transaction
- **Streaming Performance**: <10μs per message during replay

## Benchmark Categories

### Core Operations (`core_operations`)
- **single_write_latency**: Individual message write performance
- **batch_write_throughput**: Batch operation throughput validation
- **concurrent_writes**: Multi-threaded write performance
- **read_operations**: Sequential, random, and range read performance

### System Operations (`system_operations`)
- **recovery_operations**: WAL recovery and repair performance
- **segment_rotation**: Segment lifecycle management performance
- **memory_efficiency**: Memory usage validation

### Integration Operations (`integration_operations`)
- **message_conversion**: Broker-storage format conversion performance
- **transaction_processing**: ACID transaction performance
- **streaming_api**: Message replay and streaming performance

### Stress Tests (`stress_tests`)
- **comprehensive_stress**: Mixed workload under high load

## Running Benchmarks

### Quick Start

```bash
# Quick performance validation (2 minutes)
./scripts/run_benchmarks.sh quick

# Comprehensive benchmark suite (10 minutes)  
./scripts/run_benchmarks.sh comprehensive

# Validate performance targets
./scripts/run_benchmarks.sh validate
```

### Detailed Commands

```bash
# Individual benchmark categories
cargo bench --bench wal_benchmarks single_write_latency
cargo bench --bench wal_benchmarks batch_write_throughput
cargo bench --bench wal_benchmarks read_operations
cargo bench --bench wal_benchmarks recovery_operations

# All benchmarks
cargo bench --bench wal_benchmarks

# With specific configuration
CARGO_BENCH_DURATION=60 cargo bench --bench wal_benchmarks
```

### Specialized Test Suites

```bash
# Latency-focused benchmarks
./scripts/run_benchmarks.sh latency

# Throughput-focused benchmarks  
./scripts/run_benchmarks.sh throughput

# High-load stress testing
./scripts/run_benchmarks.sh stress

# CI-friendly quick tests
./scripts/run_benchmarks.sh ci
```

## Benchmark Implementation

### Test Data Configuration

The benchmarks use realistic test scenarios:

- **Small Messages**: 64 bytes (typical metadata/control messages)
- **Medium Messages**: 1KB (typical application messages)
- **Large Messages**: 64KB (bulk data transfer)
- **Batch Sizes**: 1, 10, 100, 1,000, 10,000 messages
- **Concurrency Levels**: 1, 4, 8, 16, 32 threads
- **WAL Sizes**: 1MB, 10MB, 100MB, 1GB for recovery tests

### Performance Validation

Each benchmark includes automatic validation against performance targets:

```rust
// Example: Write latency validation
assert!(latency.as_micros() < 10, 
    "Write latency {}μs exceeds 10μs target", latency.as_micros());

// Example: Throughput validation
assert!(throughput > 10_000_000.0, 
    "Throughput {:.0} msg/s below 10M target", throughput);
```

### Statistical Analysis

The benchmarks use Criterion.rs for statistical analysis:

- **Measurement Time**: 10 seconds per benchmark
- **Warm-up Time**: 2-3 seconds
- **Sample Size**: 50-100 iterations
- **Outlier Detection**: Automatic outlier filtering
- **Regression Detection**: Automatic performance regression alerts

## Results Analysis

### Criterion Output

Results are generated in `target/criterion/` with:

- **HTML Reports**: Visual performance analysis
- **JSON Data**: Raw performance measurements
- **Performance Plots**: Latency distribution graphs
- **Regression Analysis**: Performance change detection

### Custom Analysis

```bash
# Generate performance report
./scripts/run_benchmarks.sh report

# View system information
./scripts/run_benchmarks.sh info

# Profile with flamegraph
./scripts/run_benchmarks.sh profile
```

### Key Metrics

Monitor these critical metrics:

1. **P99 Write Latency**: Should be <10μs consistently
2. **Peak Throughput**: Should exceed 10M messages/second
3. **Memory Usage**: Should remain under 1KB per inactive stream
4. **Recovery Time**: Should be <500ms for 1GB of WAL data
5. **CPU Utilization**: Should remain efficient under load

## Optimization Guidelines

### Hardware Recommendations

For optimal benchmark results:

- **CPU**: Modern multi-core processor (8+ cores recommended)
- **Memory**: 16GB+ RAM for large-scale benchmarks
- **Storage**: NVMe SSD for realistic I/O performance
- **OS**: Linux with minimal background processes

### Environment Setup

```bash
# Optimize for benchmarking
echo 'performance' | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
ulimit -n 65536
export RUST_LOG=error  # Reduce logging overhead
```

### Compiler Optimizations

The benchmarks use release mode with optimizations:

```toml
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
```

## Benchmark Scenarios

### Scenario 1: Ultra-Low Latency
- Focus: Individual operation latency
- Target: <10μs P99 write latency
- Workload: Single-threaded, small messages

### Scenario 2: High Throughput
- Focus: Maximum message processing rate
- Target: >10M messages/second
- Workload: Large batches, optimized I/O

### Scenario 3: High Concurrency
- Focus: Multi-threaded performance
- Target: Linear scaling with cores
- Workload: Concurrent writers and readers

### Scenario 4: Stress Testing
- Focus: Performance under extreme load
- Target: Graceful degradation
- Workload: Mixed operations, resource pressure

### Scenario 5: Recovery Performance
- Focus: System reliability and recovery speed
- Target: <500ms for 1GB WAL recovery
- Workload: Large WAL files with corruption scenarios

## Continuous Integration

### Automated Testing

The benchmarks integrate with CI/CD:

```yaml
# Example GitHub Actions step
- name: Run Performance Benchmarks
  run: ./scripts/run_benchmarks.sh ci
  
- name: Validate Performance Targets
  run: ./scripts/run_benchmarks.sh validate
```

### Performance Regression Detection

- **Baseline Comparison**: Compare against previous runs
- **Alert Thresholds**: Alert on >10% performance degradation
- **Performance Tracking**: Long-term performance trend analysis

## Troubleshooting

### Common Issues

1. **High Latency Measurements**
   - Check system load and background processes
   - Verify SSD performance and available space
   - Ensure adequate memory allocation

2. **Low Throughput Results**
   - Verify multi-core CPU utilization
   - Check memory bandwidth limitations
   - Review network/disk I/O bottlenecks

3. **Inconsistent Results**
   - Run on dedicated hardware when possible
   - Disable frequency scaling and power management
   - Increase measurement time for more stable results

### Performance Analysis

```bash
# Check system resources during benchmarks
top -p $(pgrep cargo)
iostat -x 1
free -m

# Analyze benchmark results
cat target/criterion/*/report/index.html
grep -r "time:" target/criterion/*/report/
```

## Contributing

When adding new benchmarks:

1. **Follow Naming Convention**: Use descriptive benchmark names
2. **Add Validation**: Include performance target assertions
3. **Document Expected Results**: Specify target metrics
4. **Test on Multiple Platforms**: Verify cross-platform performance
5. **Update Documentation**: Keep this README current

## License

These benchmarks are part of the Kaelix Storage project and follow the same licensing terms.