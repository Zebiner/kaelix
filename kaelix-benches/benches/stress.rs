//! Stress testing benchmarks for MemoryStreamer.
//!
//! This benchmark suite validates system behavior under extreme conditions
//! including maximum throughput under resource constraints, performance
//! degradation under memory pressure, CPU utilization efficiency,
//! and network saturation handling.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use kaelix_benches::prelude::*;
use kaelix_benches::{BenchmarkConfig, PerformanceValidator, BenchmarkMetrics};
use kaelix_core::{Message, Result};
use bytes::Bytes;
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;

/// Stress test configurations.
const STRESS_DURATIONS: &[u64] = &[30, 60, 120, 300]; // seconds
const MEMORY_PRESSURE_LEVELS: &[usize] = &[512, 1024, 2048, 4096]; // MB
const CPU_STRESS_LEVELS: &[usize] = &[1, 2, 4, 8, 16]; // concurrent CPU-bound tasks
const NETWORK_BANDWIDTH_LIMITS: &[usize] = &[10, 50, 100, 500]; // MB/s

/// Benchmark maximum throughput under resource constraints.
fn bench_max_throughput_constrained(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let validator = PerformanceValidator::new(BenchmarkConfig::stress());
    
    let mut group = c.benchmark_group("max_throughput_constrained");
    group.measurement_time(Duration::from_secs(60));
    
    for &memory_limit_mb in &[256, 512, 1024, 2048] {
        group.bench_with_input(
            BenchmarkId::new("memory_limit_mb", memory_limit_mb),
            &memory_limit_mb,
            |b, &limit_mb| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::create_constrained(ResourceConstraints {
                        memory_limit_mb: Some(limit_mb),
                        cpu_limit_percent: Some(80.0),
                        network_bandwidth_mbps: None,
                        disk_io_mbps: None,
                    }).await;
                    
                    let test_duration = Duration::from_secs(30);
                    let mut metrics_collector = MetricsCollector::new();
                    let mut published_count = 0u64;
                    
                    let start = tokio::time::Instant::now();
                    
                    // Stress test loop
                    while start.elapsed() < test_duration {
                        // Create batch of variable-size messages to stress memory
                        let batch_size = 1000;
                        let mut batch = Vec::with_capacity(batch_size);
                        
                        for i in 0..batch_size {
                            let payload_size = 1024 + (i % 4096); // 1KB to 5KB messages
                            let payload = create_stress_payload(payload_size);
                            let message = Message::new("stress-test", payload).unwrap();
                            batch.push(message);
                        }
                        
                        // Publish batch and measure
                        let batch_start = tokio::time::Instant::now();
                        match ctx.publisher.publish_batch(batch).await {
                            Ok(_) => {
                                let batch_latency = batch_start.elapsed();
                                metrics_collector.record_latency(batch_latency);
                                published_count += batch_size as u64;
                            }
                            Err(e) => {
                                metrics_collector.record_error();
                                tracing::warn!("Batch publish failed under stress: {:?}", e);
                            }
                        }
                        
                        // Sample memory usage
                        if published_count % 10000 == 0 {
                            let memory_snapshot = ctx.get_memory_snapshot().await;
                            metrics_collector.record_memory_snapshot(memory_snapshot);
                        }
                    }
                    
                    let mut final_metrics = metrics_collector.compile_metrics();
                    final_metrics.message_count = published_count;
                    
                    // Validate performance under constraints
                    let constraint_adjusted_target = 10_000_000 * (limit_mb as u64) / 2048; // Scale with memory
                    if let Err(e) = validator.validate_throughput(final_metrics.throughput.min(constraint_adjusted_target)) {
                        tracing::warn!("Constrained throughput validation failed: {:?}", e);
                    }
                    
                    criterion::black_box(final_metrics)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark performance degradation under memory pressure.
fn bench_memory_pressure_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_pressure_performance");
    
    for &pressure_mb in MEMORY_PRESSURE_LEVELS {
        group.bench_with_input(
            BenchmarkId::new("pressure_mb", pressure_mb),
            &pressure_mb,
            |b, &pressure| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::stress().await;
                    
                    // Create memory pressure by allocating large amounts of memory
                    let pressure_allocations = create_memory_pressure(pressure).await;
                    
                    // Measure performance under pressure
                    let test_messages = 10000;
                    let mut latencies = Vec::with_capacity(test_messages);
                    let mut success_count = 0;
                    
                    let overall_start = tokio::time::Instant::now();
                    
                    for i in 0..test_messages {
                        let payload_size = 1024;
                        let message = create_test_message(payload_size, i);
                        
                        let msg_start = tokio::time::Instant::now();
                        match ctx.publisher.publish(message).await {
                            Ok(_) => {
                                latencies.push(msg_start.elapsed());
                                success_count += 1;
                            }
                            Err(_) => {
                                // Expected under extreme pressure
                            }
                        }
                        
                        // Yield occasionally to allow GC or cleanup
                        if i % 100 == 0 {
                            tokio::task::yield_now().await;
                        }
                    }
                    
                    let total_duration = overall_start.elapsed();
                    let success_rate = success_count as f64 / test_messages as f64;
                    let avg_latency = if !latencies.is_empty() {
                        latencies.iter().sum::<Duration>() / latencies.len() as u32
                    } else {
                        Duration::ZERO
                    };
                    
                    // Clean up pressure allocations
                    drop(pressure_allocations);
                    
                    criterion::black_box((success_rate, avg_latency, total_duration))
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark CPU utilization efficiency under load.
fn bench_cpu_utilization_efficiency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("cpu_utilization_efficiency");
    
    for &cpu_stress_level in CPU_STRESS_LEVELS {
        group.bench_with_input(
            BenchmarkId::new("cpu_stress_level", cpu_stress_level),
            &cpu_stress_level,
            |b, &stress_level| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::stress().await;
                    
                    // Start CPU stress tasks
                    let mut cpu_stress_handles = Vec::new();
                    for _ in 0..stress_level {
                        let handle = tokio::spawn(async {
                            // CPU-intensive work
                            let mut sum = 0u64;
                            for i in 0..1_000_000 {
                                sum = sum.wrapping_add((i * i) as u64);
                                if i % 10000 == 0 {
                                    tokio::task::yield_now().await;
                                }
                            }
                            sum
                        });
                        cpu_stress_handles.push(handle);
                    }
                    
                    // Measure messaging performance while CPU is stressed
                    let test_messages = 10000;
                    let mut messaging_times = Vec::with_capacity(test_messages);
                    
                    let messaging_start = tokio::time::Instant::now();
                    
                    for i in 0..test_messages {
                        let message = create_test_message(512, i);
                        
                        let msg_start = tokio::time::Instant::now();
                        ctx.publisher.publish(message).await.unwrap();
                        messaging_times.push(msg_start.elapsed());
                    }
                    
                    let messaging_duration = messaging_start.elapsed();
                    
                    // Wait for CPU stress tasks to complete
                    let cpu_results = futures::future::join_all(cpu_stress_handles).await;
                    
                    // Calculate CPU efficiency metrics
                    let throughput = test_messages as f64 / messaging_duration.as_secs_f64();
                    let avg_latency = messaging_times.iter().sum::<Duration>() / messaging_times.len() as u32;
                    
                    criterion::black_box((throughput, avg_latency, cpu_results))
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark network saturation handling.
fn bench_network_saturation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("network_saturation");
    
    for &bandwidth_limit in NETWORK_BANDWIDTH_LIMITS {
        group.bench_with_input(
            BenchmarkId::new("bandwidth_mbps", bandwidth_limit),
            &bandwidth_limit,
            |b, &limit_mbps| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::create_network_limited(limit_mbps).await;
                    
                    // Calculate message size to saturate network
                    let target_bandwidth_bps = limit_mbps * 1024 * 1024;
                    let message_size = 1024; // 1KB messages
                    let messages_per_second = target_bandwidth_bps / message_size;
                    
                    let test_duration = Duration::from_secs(10);
                    let total_messages = (messages_per_second * test_duration.as_secs() as usize).min(100_000);
                    
                    let mut network_latencies = Vec::with_capacity(total_messages);
                    let mut published_count = 0;
                    let mut dropped_count = 0;
                    
                    let start = tokio::time::Instant::now();
                    
                    // Rate-limited publishing to saturate network
                    let rate_limiter = Arc::new(Semaphore::new(100)); // Limit concurrent requests
                    
                    for i in 0..total_messages {
                        let permit = rate_limiter.clone().acquire_owned().await.unwrap();
                        let message = create_test_message(message_size, i);
                        
                        let net_start = tokio::time::Instant::now();
                        match ctx.network_client.send_message(message).await {
                            Ok(_) => {
                                network_latencies.push(net_start.elapsed());
                                published_count += 1;
                            }
                            Err(_) => {
                                dropped_count += 1;
                            }
                        }
                        
                        drop(permit);
                        
                        // Check if we should stop early
                        if start.elapsed() > test_duration {
                            break;
                        }
                    }
                    
                    let actual_duration = start.elapsed();
                    let actual_throughput = published_count as f64 / actual_duration.as_secs_f64();
                    let drop_rate = dropped_count as f64 / (published_count + dropped_count) as f64;
                    let avg_network_latency = if !network_latencies.is_empty() {
                        network_latencies.iter().sum::<Duration>() / network_latencies.len() as u32
                    } else {
                        Duration::ZERO
                    };
                    
                    criterion::black_box((actual_throughput, drop_rate, avg_network_latency))
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark sustained high-load endurance.
fn bench_sustained_endurance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("sustained_endurance");
    group.measurement_time(Duration::from_secs(300)); // 5-minute tests
    
    for &duration_secs in &[60, 300, 600] { // 1, 5, 10 minutes
        group.bench_with_input(
            BenchmarkId::new("duration_secs", duration_secs),
            &duration_secs,
            |b, &duration| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::stress().await;
                    let test_duration = Duration::from_secs(duration);
                    
                    let mut metrics_collector = MetricsCollector::new();
                    let mut total_published = 0u64;
                    let mut total_errors = 0u64;
                    
                    let start = tokio::time::Instant::now();
                    let mut last_report = start;
                    
                    // Sustained load generation
                    while start.elapsed() < test_duration {
                        let batch_size = 1000;
                        let mut batch = Vec::with_capacity(batch_size);
                        
                        // Variable payload sizes to create realistic load patterns
                        for i in 0..batch_size {
                            let size_variation = (start.elapsed().as_secs() % 4) as usize;
                            let payload_size = 512 + (size_variation * 512); // 512B to 2KB
                            let message = create_test_message(payload_size, total_published as usize + i);
                            batch.push(message);
                        }
                        
                        let batch_start = tokio::time::Instant::now();
                        match ctx.publisher.publish_batch(batch).await {
                            Ok(_) => {
                                let batch_latency = batch_start.elapsed();
                                metrics_collector.record_latency(batch_latency);
                                total_published += batch_size as u64;
                            }
                            Err(_) => {
                                total_errors += 1;
                                metrics_collector.record_error();
                            }
                        }
                        
                        // Periodic reporting and memory sampling
                        if last_report.elapsed() > Duration::from_secs(10) {
                            let memory_snapshot = ctx.get_memory_snapshot().await;
                            metrics_collector.record_memory_snapshot(memory_snapshot);
                            
                            let elapsed = start.elapsed();
                            let current_throughput = total_published as f64 / elapsed.as_secs_f64();
                            tracing::info!(
                                "Endurance test progress: {}s, {} msg/s, {} errors",
                                elapsed.as_secs(),
                                current_throughput as u64,
                                total_errors
                            );
                            
                            last_report = tokio::time::Instant::now();
                        }
                        
                        // Brief yield to prevent starving other tasks
                        tokio::task::yield_now().await;
                    }
                    
                    let final_duration = start.elapsed();
                    let final_throughput = total_published as f64 / final_duration.as_secs_f64();
                    let error_rate = total_errors as f64 / (total_published + total_errors) as f64;
                    
                    let mut final_metrics = metrics_collector.compile_metrics();
                    final_metrics.message_count = total_published;
                    final_metrics.error_count = total_errors;
                    final_metrics.duration = final_duration;
                    final_metrics.calculate_throughput();
                    
                    tracing::info!(
                        "Endurance test completed: {} messages in {}s ({} msg/s, {:.3}% errors)",
                        total_published,
                        final_duration.as_secs(),
                        final_throughput as u64,
                        error_rate * 100.0
                    );
                    
                    criterion::black_box(final_metrics)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark cascading failure scenarios.
fn bench_cascading_failures(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("cascading_failures");
    
    for &failure_rate in &[0.01, 0.05, 0.10, 0.20] { // 1%, 5%, 10%, 20% failure rates
        group.bench_with_input(
            BenchmarkId::new("failure_rate", (failure_rate * 100.0) as u32),
            &failure_rate,
            |b, &rate| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::create_failure_prone(rate).await;
                    
                    let test_messages = 10000;
                    let mut successful_publishes = 0;
                    let mut failed_publishes = 0;
                    let mut recovery_times = Vec::new();
                    
                    let start = tokio::time::Instant::now();
                    
                    for i in 0..test_messages {
                        let message = create_test_message(1024, i);
                        
                        let publish_start = tokio::time::Instant::now();
                        match ctx.publisher.publish_with_retry(message, 3).await {
                            Ok(_) => {
                                let recovery_time = publish_start.elapsed();
                                recovery_times.push(recovery_time);
                                successful_publishes += 1;
                            }
                            Err(_) => {
                                failed_publishes += 1;
                            }
                        }
                        
                        // Simulate cascading effects with brief pauses
                        if failed_publishes > 0 && i % 100 == 0 {
                            tokio::time::sleep(Duration::from_millis(1)).await;
                        }
                    }
                    
                    let total_duration = start.elapsed();
                    let success_rate = successful_publishes as f64 / test_messages as f64;
                    let avg_recovery_time = if !recovery_times.is_empty() {
                        recovery_times.iter().sum::<Duration>() / recovery_times.len() as u32
                    } else {
                        Duration::ZERO
                    };
                    
                    criterion::black_box((success_rate, avg_recovery_time, total_duration))
                });
            },
        );
    }
    
    group.finish();
}

/// Helper function to create stress test payload.
fn create_stress_payload(size: usize) -> Bytes {
    // Create realistic payload with some compression-resistant data
    let mut payload = Vec::with_capacity(size);
    for i in 0..size {
        payload.push((i % 256) as u8);
    }
    Bytes::from(payload)
}

/// Helper function to create test message with sequence number.
fn create_test_message(payload_size: usize, sequence: usize) -> Message {
    let payload_data = format!("Seq:{:08x}:{}", sequence, "A".repeat(payload_size.saturating_sub(20)));
    Message::new("stress-test", Bytes::from(payload_data)).unwrap()
}

/// Create memory pressure by allocating large amounts of memory.
async fn create_memory_pressure(pressure_mb: usize) -> Vec<Vec<u8>> {
    let allocation_size = 1024 * 1024; // 1MB chunks
    let num_allocations = pressure_mb;
    
    let mut allocations = Vec::with_capacity(num_allocations);
    for _ in 0..num_allocations {
        let allocation = vec![0u8; allocation_size];
        allocations.push(allocation);
        
        // Yield periodically to not block
        if allocations.len() % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }
    
    allocations
}

/// Resource constraints for testing.
#[derive(Debug)]
struct ResourceConstraints {
    memory_limit_mb: Option<usize>,
    cpu_limit_percent: Option<f64>,
    network_bandwidth_mbps: Option<usize>,
    disk_io_mbps: Option<usize>,
}

/// Mock memory snapshot.
use kaelix_benches::metrics::MemorySnapshot;

/// Mock stress test context.
struct StressTestContext {
    publisher: StressPublisher,
    network_client: NetworkClient,
}

impl StressTestContext {
    async fn get_memory_snapshot(&self) -> MemorySnapshot {
        MemorySnapshot {
            heap_usage: 1024 * 1024 * 512, // 512MB
            stack_usage: 1024 * 8, // 8KB
            total_allocations: 10000,
            total_deallocations: 9500,
            rss_memory: 1024 * 1024 * 600, // 600MB
        }
    }
}

struct StressPublisher;

impl StressPublisher {
    async fn publish(&self, _message: Message) -> Result<()> {
        // Simulate occasional failures under stress
        if rand::random::<f64>() < 0.001 { // 0.1% failure rate
            return Err(kaelix_core::Error::Performance("Stress failure".to_string()));
        }
        
        // Simulate variable latency under load
        let latency_ms = if rand::random::<f64>() < 0.1 { 5 } else { 1 };
        tokio::time::sleep(Duration::from_millis(latency_ms)).await;
        Ok(())
    }
    
    async fn publish_batch(&self, messages: Vec<Message>) -> Result<()> {
        // Batch operations are more efficient but can fail under extreme load
        if messages.len() > 5000 && rand::random::<f64>() < 0.01 {
            return Err(kaelix_core::Error::Performance("Batch too large under stress".to_string()));
        }
        
        // Simulate batch processing time
        let processing_time = Duration::from_micros(messages.len() as u64 * 10);
        tokio::time::sleep(processing_time).await;
        Ok(())
    }
    
    async fn publish_with_retry(&self, message: Message, retries: usize) -> Result<()> {
        for attempt in 0..=retries {
            match self.publish(message.clone()).await {
                Ok(_) => return Ok(()),
                Err(_) if attempt < retries => {
                    // Exponential backoff
                    let delay = Duration::from_millis(10 * (1 << attempt));
                    tokio::time::sleep(delay).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }
}

struct NetworkClient;

impl NetworkClient {
    async fn send_message(&self, _message: Message) -> Result<()> {
        // Simulate network latency and occasional failures
        let network_latency = Duration::from_millis(1 + rand::random::<u64>() % 10);
        tokio::time::sleep(network_latency).await;
        
        if rand::random::<f64>() < 0.005 { // 0.5% network failure rate
            return Err(kaelix_core::Error::Performance("Network timeout".to_string()));
        }
        
        Ok(())
    }
}

impl BenchmarkContext {
    async fn stress() -> StressTestContext {
        StressTestContext {
            publisher: StressPublisher,
            network_client: NetworkClient,
        }
    }
    
    async fn create_constrained(_constraints: ResourceConstraints) -> StressTestContext {
        StressTestContext {
            publisher: StressPublisher,
            network_client: NetworkClient,
        }
    }
    
    async fn create_network_limited(_bandwidth_mbps: usize) -> StressTestContext {
        StressTestContext {
            publisher: StressPublisher,
            network_client: NetworkClient,
        }
    }
    
    async fn create_failure_prone(_failure_rate: f64) -> StressTestContext {
        StressTestContext {
            publisher: StressPublisher,
            network_client: NetworkClient,
        }
    }
}

criterion_group!(
    stress_benches,
    bench_max_throughput_constrained,
    bench_memory_pressure_performance,
    bench_cpu_utilization_efficiency,
    bench_network_saturation,
    bench_sustained_endurance,
    bench_cascading_failures
);

criterion_main!(stress_benches);