# MemoryStreamer Performance Benchmarks

Comprehensive performance benchmarking infrastructure for the MemoryStreamer distributed streaming system.

## Overview

This benchmark suite validates that MemoryStreamer meets its performance targets:
- **Throughput**: 10M+ messages/second
- **Latency**: <10μs P99 latency
- **Memory**: Efficient memory usage with zero-copy operations
- **Concurrency**: High-performance concurrent operations

## Benchmark Categories

### 1. Throughput Benchmarks (`throughput.rs`)
- Single message publishing throughput
- Batch message publishing throughput  
- Consumer throughput across multiple partitions
- End-to-end pipeline throughput
- Concurrent producer/consumer scenarios
- Memory-mapped vs standard I/O comparison
- Zero-copy message handling validation
- Sustained high-throughput performance (10M+ msg/sec)

### 2. Latency Benchmarks (`latency.rs`)
- End-to-end message latency with percentile analysis (P50, P95, P99, P99.9)
- Producer publish latency
- Consumer poll latency
- Consensus protocol latency
- Authorization decision latency (<100ns target)
- Network round-trip latency simulation
- Concurrent latency under load
- Memory allocation latency impact

### 3. Memory Benchmarks (`memory.rs`)
- Memory allocation patterns (Vec, Box, Bytes, SmallVec)
- Zero-copy operation validation
- Memory usage under sustained load
- Memory leak detection
- Garbage collection impact analysis
- Memory-mapped file operations
- Buffer pooling strategies
- Stack vs heap allocation comparison

### 4. Concurrent Performance (`concurrent.rs`)
- Multi-producer single-consumer scenarios
- Single-producer multi-consumer scenarios
- Multi-producer multi-consumer scenarios
- Lock-free data structure performance (crossbeam vs mutex)
- Async task scheduling overhead
- Contention scenarios (high vs low contention)
- Work-stealing vs dedicated queues

### 5. Stress Testing (`stress.rs`)
- Maximum throughput under resource constraints
- Performance degradation under memory pressure
- CPU utilization efficiency
- Network saturation handling
- Sustained high-load endurance testing
- Cascading failure scenarios
- Resource-constrained performance

### 6. Baseline Comparisons (`baseline.rs`)
- MemoryStreamer vs Apache Kafka
- MemoryStreamer vs Redis Streams
- MemoryStreamer vs NATS
- System baseline performance
- Feature comparison matrix
- Latency comparisons across systems

## Running Benchmarks

### Quick Start

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark category
cargo bench throughput
cargo bench latency
cargo bench memory
cargo bench concurrent
cargo bench stress
cargo bench baseline

# Run with profiling
cargo bench --features pprof throughput
cargo bench --features dhat memory
```

### Detailed Commands

```bash
# Throughput validation (targets 10M+ msg/sec)
cargo bench throughput::bench_sustained_throughput

# Latency validation (targets <10μs P99)
cargo bench latency::bench_latency_percentiles

# Memory efficiency validation
cargo bench memory::bench_zero_copy_validation

# Concurrent performance validation
cargo bench concurrent::bench_multi_producer_multi_consumer

# Stress testing
cargo bench stress::bench_sustained_endurance

# Baseline comparisons
cargo bench baseline::bench_kafka_comparison
```

## Performance Profiling

### CPU Profiling with pprof

```bash
# Generate flamegraphs
cargo bench --features flamegraph throughput

# View flamegraphs
open target/benchmark-throughput.svg
```

### Memory Profiling with dhat

```bash
# Memory allocation profiling
cargo bench --features dhat memory

# View memory reports
dhat-viewer dhat-heap.json
```

### Advanced Profiling

```bash
# Combined CPU and memory profiling
cargo bench --features detailed-profiling

# Instruction-level profiling with iai-callgrind
cargo bench --bench throughput -- --iai
```

## Benchmark Configuration

### Environment Setup

```bash
# Optimize environment for benchmarking
export RUST_LOG=error
export TOKIO_WORKER_THREADS=$(nproc)
export NUMA_OPTIMIZE=1

# Set CPU scaling governor to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

### Custom Configuration

Create a `benchmark.toml` configuration file:

```toml
[throughput]
target_throughput = 10_000_000
max_latency_micros = 10
test_duration_secs = 60

[latency]
target_p99_micros = 10
measurement_iterations = 10000
warmup_iterations = 1000

[memory]
enable_leak_detection = true
enable_allocation_tracking = true
memory_limit_mb = 2048

[stress]
test_duration_secs = 300
memory_pressure_mb = 1024
cpu_stress_level = 8
```

## Performance Targets & Validation

### Throughput Targets
- **Primary**: 10M+ messages/second sustained
- **Batch Publishing**: 15M+ messages/second in optimal conditions
- **Concurrent**: 5M+ messages/second with 32 concurrent producers/consumers

### Latency Targets
- **P99 End-to-End**: <10μs
- **P95 End-to-End**: <5μs
- **Mean Latency**: <2μs
- **Authorization**: <100ns per decision

### Memory Targets
- **Zero Memory Leaks**: 0 bytes leaked over sustained operation
- **Zero-Copy Efficiency**: >90% operations use zero-copy
- **Peak Memory**: <2GB under normal load
- **Allocation Rate**: <1MB/sec allocation churn

### Concurrency Targets
- **32 Concurrent Producers**: No performance degradation
- **Lock-Free Operations**: 95%+ of operations should be lock-free
- **Context Switching**: <10% CPU time in context switches

## Regression Detection

### Automatic Regression Detection

The benchmark suite includes automatic regression detection with 1% thresholds:

```bash
# Set baseline
cargo bench -- --save-baseline main

# Detect regressions
cargo bench -- --baseline main
```

### CI/CD Integration

```yaml
# GitHub Actions example
- name: Performance Regression Detection
  run: |
    cargo bench --message-format=json | tee benchmark-results.json
    python scripts/analyze-regressions.py benchmark-results.json
```

### Performance Reports

```bash
# Generate HTML reports
cargo bench -- --output-format html

# Generate JSON for analysis
cargo bench -- --output-format json > results.json

# Generate comparison reports
cargo bench -- --compare baseline.json
```

## Interpreting Results

### Throughput Results
- **Target**: 10M+ msg/sec
- **Good**: >8M msg/sec
- **Warning**: 5-8M msg/sec  
- **Critical**: <5M msg/sec

### Latency Results
- **Target**: P99 <10μs
- **Good**: P99 <8μs
- **Warning**: P99 8-15μs
- **Critical**: P99 >15μs

### Memory Results
- **Zero Leaks**: No memory growth over time
- **Efficient Allocation**: <100MB/sec allocation rate
- **Zero-Copy**: >90% operations using shared references

## Troubleshooting

### Common Issues

1. **Low Throughput**
   ```bash
   # Check CPU scaling
   cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
   
   # Check system load
   htop
   ```

2. **High Latency**
   ```bash
   # Check for swap usage
   free -h
   
   # Check context switching
   vmstat 1
   ```

3. **Memory Issues**
   ```bash
   # Run with memory debugging
   cargo bench --features dhat memory::bench_memory_leak_detection
   ```

### Performance Tuning

1. **System Tuning**
   ```bash
   # Increase open file limits
   ulimit -n 65536
   
   # Disable swap
   sudo swapoff -a
   
   # Set process priority
   sudo nice -n -20 cargo bench
   ```

2. **Benchmark Tuning**
   ```rust
   // Increase measurement time for stability
   group.measurement_time(Duration::from_secs(60));
   
   // Increase sample size for accuracy  
   group.sample_size(1000);
   ```

## Development

### Adding New Benchmarks

1. **Create benchmark function**:
   ```rust
   fn bench_new_feature(c: &mut Criterion) {
       let mut group = c.benchmark_group("new_feature");
       // Implementation
       group.finish();
   }
   ```

2. **Add to criterion_group**:
   ```rust
   criterion_group!(
       benches,
       bench_existing,
       bench_new_feature  // Add here
   );
   ```

3. **Update documentation**:
   - Add description to this README
   - Document performance targets
   - Include usage examples

### Testing Benchmarks

```bash
# Test benchmark compilation
cargo check --benches

# Run quick validation
cargo bench -- --quick

# Test specific benchmark
cargo test --bench throughput
```

## Performance History

Track performance over time:

```bash
# Store results with git commit
cargo bench | tee "results/$(git rev-parse HEAD).json"

# Compare with previous results
python scripts/compare-performance.py results/
```

## Contributing

1. All benchmarks must validate against documented performance targets
2. Include both positive and negative test cases
3. Add comprehensive documentation for new benchmarks
4. Ensure benchmarks are deterministic and repeatable
5. Include appropriate error handling and validation

## Links

- [Criterion.rs Documentation](https://docs.rs/criterion/)
- [pprof Profiling Guide](https://docs.rs/pprof/)
- [DHAT Memory Profiler](https://docs.rs/dhat/)
- [MemoryStreamer Performance Guide](../docs/performance.md)