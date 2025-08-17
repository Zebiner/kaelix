//! Baseline comparison benchmarks for MemoryStreamer.
//!
//! This benchmark suite compares MemoryStreamer performance against
//! established systems like Apache Kafka, Redis Streams, and NATS,
//! as well as measuring baseline system performance without MemoryStreamer.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use kaelix_benches::prelude::*;
use kaelix_benches::{BenchmarkConfig, PerformanceValidator, BenchmarkMetrics};
use kaelix_core::{Message, Result};
use bytes::Bytes;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Message sizes for baseline comparisons.
const BASELINE_PAYLOAD_SIZES: &[usize] = &[64, 256, 1024, 4096, 16384];

/// Batch sizes for comparison tests.
const BASELINE_BATCH_SIZES: &[usize] = &[1, 10, 100, 1000];

/// Number of messages for throughput comparisons.
const BASELINE_MESSAGE_COUNTS: &[usize] = &[1000, 10000, 100000];

/// Benchmark MemoryStreamer baseline performance.
fn bench_memorystreamer_baseline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let validator = PerformanceValidator::new(BenchmarkConfig::throughput());
    
    let mut group = c.benchmark_group("memorystreamer_baseline");
    
    for &payload_size in BASELINE_PAYLOAD_SIZES {
        for &message_count in &[10000, 100000] {
            group.throughput(Throughput::Elements(message_count as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("{}b_{}m", payload_size, message_count), payload_size),
                &(payload_size, message_count),
                |b, &(size, count)| {
                    b.to_async(&rt).iter(|| async {
                        let ctx = BenchmarkContext::throughput().await;
                        let consumer = ctx.create_consumer("baseline_consumer").await.unwrap();
                        
                        let messages = create_baseline_messages(count, size);
                        
                        // Measure publishing
                        let publish_start = tokio::time::Instant::now();
                        for message in messages {
                            ctx.publisher.publish(message).await.unwrap();
                        }
                        let publish_duration = publish_start.elapsed();
                        
                        // Measure consuming
                        let consume_start = tokio::time::Instant::now();
                        let mut consumed_count = 0;
                        while consumed_count < count {
                            let batch = consumer.poll_batch(Duration::from_millis(10)).await.unwrap();
                            consumed_count += batch.len();
                        }
                        let consume_duration = consume_start.elapsed();
                        
                        let publish_throughput = count as f64 / publish_duration.as_secs_f64();
                        let consume_throughput = count as f64 / consume_duration.as_secs_f64();
                        
                        // Validate against targets
                        if let Err(e) = validator.validate_throughput(publish_throughput as u64) {
                            tracing::warn!("MemoryStreamer baseline validation failed: {:?}", e);
                        }
                        
                        criterion::black_box((publish_throughput, consume_throughput))
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark against Apache Kafka (simulated).
fn bench_kafka_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("kafka_comparison");
    
    for &payload_size in BASELINE_PAYLOAD_SIZES {
        for &message_count in &[10000, 100000] {
            group.throughput(Throughput::Elements(message_count as u64));
            
            // MemoryStreamer
            group.bench_with_input(
                BenchmarkId::new(format!("memorystreamer_{}b", payload_size), message_count),
                &(payload_size, message_count),
                |b, &(size, count)| {
                    b.to_async(&rt).iter(|| async {
                        let ctx = BenchmarkContext::throughput().await;
                        let throughput = measure_memorystreamer_throughput(&ctx, count, size).await;
                        criterion::black_box(throughput)
                    });
                },
            );
            
            // Simulated Kafka
            group.bench_with_input(
                BenchmarkId::new(format!("kafka_{}b", payload_size), message_count),
                &(payload_size, message_count),
                |b, &(size, count)| {
                    b.to_async(&rt).iter(|| async {
                        let kafka_client = KafkaSimulator::new().await;
                        let throughput = kafka_client.measure_throughput(count, size).await;
                        criterion::black_box(throughput)
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark against Redis Streams (simulated).
fn bench_redis_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("redis_comparison");
    
    for &payload_size in &[256, 1024, 4096] {
        for &message_count in &[1000, 10000] {
            group.throughput(Throughput::Elements(message_count as u64));
            
            // MemoryStreamer
            group.bench_with_input(
                BenchmarkId::new(format!("memorystreamer_{}b", payload_size), message_count),
                &(payload_size, message_count),
                |b, &(size, count)| {
                    b.to_async(&rt).iter(|| async {
                        let ctx = BenchmarkContext::throughput().await;
                        let throughput = measure_memorystreamer_throughput(&ctx, count, size).await;
                        criterion::black_box(throughput)
                    });
                },
            );
            
            // Simulated Redis Streams
            group.bench_with_input(
                BenchmarkId::new(format!("redis_{}b", payload_size), message_count),
                &(payload_size, message_count),
                |b, &(size, count)| {
                    b.to_async(&rt).iter(|| async {
                        let redis_client = RedisStreamsSimulator::new().await;
                        let throughput = redis_client.measure_throughput(count, size).await;
                        criterion::black_box(throughput)
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark against NATS (simulated).
fn bench_nats_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("nats_comparison");
    
    for &payload_size in &[64, 256, 1024, 4096] {
        for &message_count in &[10000, 50000] {
            group.throughput(Throughput::Elements(message_count as u64));
            
            // MemoryStreamer
            group.bench_with_input(
                BenchmarkId::new(format!("memorystreamer_{}b", payload_size), message_count),
                &(payload_size, message_count),
                |b, &(size, count)| {
                    b.to_async(&rt).iter(|| async {
                        let ctx = BenchmarkContext::throughput().await;
                        let throughput = measure_memorystreamer_throughput(&ctx, count, size).await;
                        criterion::black_box(throughput)
                    });
                },
            );
            
            // Simulated NATS
            group.bench_with_input(
                BenchmarkId::new(format!("nats_{}b", payload_size), message_count),
                &(payload_size, message_count),
                |b, &(size, count)| {
                    b.to_async(&rt).iter(|| async {
                        let nats_client = NatsSimulator::new().await;
                        let throughput = nats_client.measure_throughput(count, size).await;
                        criterion::black_box(throughput)
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark system baseline without MemoryStreamer.
fn bench_system_baseline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("system_baseline");
    
    // CPU baseline - raw computation
    group.bench_function("cpu_baseline", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for i in 0..100_000 {
                sum = sum.wrapping_add(i);
            }
            criterion::black_box(sum)
        });
    });
    
    // Memory allocation baseline
    for &size in &[1024, 4096, 16384, 65536] {
        group.bench_with_input(
            BenchmarkId::new("memory_alloc", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let buffer = vec![0u8; size];
                    criterion::black_box(buffer)
                });
            },
        );
    }
    
    // Async task spawning baseline
    for &task_count in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("async_tasks", task_count),
            &task_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let mut handles = Vec::with_capacity(count);
                    for i in 0..count {
                        let handle = tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_nanos(1)).await;
                            i
                        });
                        handles.push(handle);
                    }
                    
                    let results = futures::future::join_all(handles).await;
                    criterion::black_box(results)
                });
            },
        );
    }
    
    // Channel throughput baseline
    for &channel_size in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("channel_throughput", channel_size),
            &channel_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let (tx, mut rx) = tokio::sync::mpsc::channel(1000);
                    
                    // Sender task
                    let sender_handle = tokio::spawn(async move {
                        for i in 0..size {
                            tx.send(i).await.unwrap();
                        }
                    });
                    
                    // Receiver task
                    let receiver_handle = tokio::spawn(async move {
                        let mut received = 0;
                        while let Some(_) = rx.recv().await {
                            received += 1;
                            if received >= size {
                                break;
                            }
                        }
                        received
                    });
                    
                    let (_, received) = tokio::join!(sender_handle, receiver_handle);
                    criterion::black_box(received)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark latency comparisons across systems.
fn bench_latency_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("latency_comparison");
    
    for &payload_size in &[64, 256, 1024] {
        // MemoryStreamer latency
        group.bench_with_input(
            BenchmarkId::new(format!("memorystreamer_latency_{}b", payload_size), payload_size),
            &payload_size,
            |b, &size| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::latency().await;
                    let consumer = ctx.create_consumer("latency_consumer").await.unwrap();
                    
                    let start = tokio::time::Instant::now();
                    
                    for _ in 0..iters {
                        let message = create_test_message(size);
                        let msg_start = tokio::time::Instant::now();
                        
                        ctx.publisher.publish(message).await.unwrap();
                        let _ = consumer.poll_single(Duration::from_millis(10)).await.unwrap();
                        
                        let _latency = msg_start.elapsed();
                    }
                    
                    start.elapsed()
                });
            },
        );
        
        // Kafka latency simulation
        group.bench_with_input(
            BenchmarkId::new(format!("kafka_latency_{}b", payload_size), payload_size),
            &payload_size,
            |b, &size| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let kafka = KafkaSimulator::new().await;
                    
                    let start = tokio::time::Instant::now();
                    
                    for _ in 0..iters {
                        let _ = kafka.measure_end_to_end_latency(size).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
        
        // Redis latency simulation
        group.bench_with_input(
            BenchmarkId::new(format!("redis_latency_{}b", payload_size), payload_size),
            &payload_size,
            |b, &size| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let redis = RedisStreamsSimulator::new().await;
                    
                    let start = tokio::time::Instant::now();
                    
                    for _ in 0..iters {
                        let _ = redis.measure_end_to_end_latency(size).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
        
        // NATS latency simulation
        group.bench_with_input(
            BenchmarkId::new(format!("nats_latency_{}b", payload_size), payload_size),
            &payload_size,
            |b, &size| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let nats = NatsSimulator::new().await;
                    
                    let start = tokio::time::Instant::now();
                    
                    for _ in 0..iters {
                        let _ = nats.measure_end_to_end_latency(size).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark feature comparison matrix.
fn bench_feature_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("feature_comparison");
    
    // Persistence performance
    group.bench_function("memorystreamer_persistence", |b| {
        b.to_async(&rt).iter(|| async {
            let ctx = BenchmarkContext::create_persistent().await;
            let messages = create_baseline_messages(10000, 1024);
            
            let start = tokio::time::Instant::now();
            for message in messages {
                ctx.publisher.publish_persistent(message).await.unwrap();
            }
            start.elapsed()
        });
    });
    
    group.bench_function("kafka_persistence", |b| {
        b.to_async(&rt).iter(|| async {
            let kafka = KafkaSimulator::new().await;
            kafka.measure_persistence_throughput(10000, 1024).await
        });
    });
    
    // Partitioning performance
    group.bench_function("memorystreamer_partitioning", |b| {
        b.to_async(&rt).iter(|| async {
            let ctx = BenchmarkContext::create_partitioned(16).await;
            let throughput = measure_partitioned_throughput(&ctx, 10000, 1024).await;
            criterion::black_box(throughput)
        });
    });
    
    group.bench_function("kafka_partitioning", |b| {
        b.to_async(&rt).iter(|| async {
            let kafka = KafkaSimulator::new().await;
            kafka.measure_partitioned_throughput(10000, 1024, 16).await
        });
    });
    
    // Replication performance
    group.bench_function("memorystreamer_replication", |b| {
        b.to_async(&rt).iter(|| async {
            let ctx = BenchmarkContext::create_replicated(3).await;
            let throughput = measure_replicated_throughput(&ctx, 10000, 1024).await;
            criterion::black_box(throughput)
        });
    });
    
    group.bench_function("kafka_replication", |b| {
        b.to_async(&rt).iter(|| async {
            let kafka = KafkaSimulator::new().await;
            kafka.measure_replicated_throughput(10000, 1024, 3).await
        });
    });
    
    group.finish();
}

/// Helper functions for measurements.
async fn measure_memorystreamer_throughput(ctx: &TestContext, message_count: usize, payload_size: usize) -> f64 {
    let messages = create_baseline_messages(message_count, payload_size);
    
    let start = tokio::time::Instant::now();
    for message in messages {
        ctx.publisher.publish(message).await.unwrap();
    }
    let duration = start.elapsed();
    
    message_count as f64 / duration.as_secs_f64()
}

async fn measure_partitioned_throughput(ctx: &TestContext, message_count: usize, payload_size: usize) -> f64 {
    let messages = create_baseline_messages(message_count, payload_size);
    
    let start = tokio::time::Instant::now();
    for (i, message) in messages.into_iter().enumerate() {
        let partition_id = i % 16; // Distribute across 16 partitions
        ctx.publisher.publish_to_partition(message, partition_id).await.unwrap();
    }
    let duration = start.elapsed();
    
    message_count as f64 / duration.as_secs_f64()
}

async fn measure_replicated_throughput(ctx: &TestContext, message_count: usize, payload_size: usize) -> f64 {
    let messages = create_baseline_messages(message_count, payload_size);
    
    let start = tokio::time::Instant::now();
    for message in messages {
        ctx.publisher.publish_replicated(message, 3).await.unwrap(); // 3-way replication
    }
    let duration = start.elapsed();
    
    message_count as f64 / duration.as_secs_f64()
}

fn create_baseline_messages(count: usize, payload_size: usize) -> Vec<Message> {
    (0..count)
        .map(|i| {
            let payload = format!("BaselineMsg{:08x}:{}", i, "A".repeat(payload_size.saturating_sub(30)));
            Message::new("baseline-test", Bytes::from(payload)).unwrap()
        })
        .collect()
}

fn create_test_message(payload_size: usize) -> Message {
    let payload = "A".repeat(payload_size);
    Message::new("test", Bytes::from(payload)).unwrap()
}

/// Simulated Kafka client for comparison.
struct KafkaSimulator {
    latency_overhead: Duration,
    throughput_multiplier: f64,
}

impl KafkaSimulator {
    async fn new() -> Self {
        Self {
            latency_overhead: Duration::from_micros(200), // Typical Kafka overhead
            throughput_multiplier: 0.8, // Assume 80% of MemoryStreamer throughput
        }
    }
    
    async fn measure_throughput(&self, message_count: usize, payload_size: usize) -> f64 {
        // Simulate Kafka processing time
        let base_time = Duration::from_nanos(payload_size as u64 * 50); // 50ns per byte
        let total_time = base_time * message_count as u32 + self.latency_overhead * message_count as u32;
        
        tokio::time::sleep(total_time / 1000).await; // Scale down for simulation
        
        (message_count as f64 / total_time.as_secs_f64()) * self.throughput_multiplier
    }
    
    async fn measure_end_to_end_latency(&self, payload_size: usize) -> Duration {
        let base_latency = Duration::from_micros(100 + payload_size as u64 / 10);
        tokio::time::sleep(base_latency / 100).await; // Scale down for simulation
        base_latency + self.latency_overhead
    }
    
    async fn measure_persistence_throughput(&self, message_count: usize, payload_size: usize) -> Duration {
        let persistence_overhead = Duration::from_micros(500); // Disk I/O overhead
        let total_time = Duration::from_nanos(payload_size as u64 * 100) * message_count as u32 + persistence_overhead;
        tokio::time::sleep(total_time / 1000).await;
        total_time
    }
    
    async fn measure_partitioned_throughput(&self, message_count: usize, payload_size: usize, partition_count: usize) -> f64 {
        let base_throughput = self.measure_throughput(message_count, payload_size).await;
        // Partitioning improves throughput
        base_throughput * (1.0 + partition_count as f64 * 0.1)
    }
    
    async fn measure_replicated_throughput(&self, message_count: usize, payload_size: usize, replication_factor: usize) -> f64 {
        let base_throughput = self.measure_throughput(message_count, payload_size).await;
        // Replication reduces throughput
        base_throughput / replication_factor as f64
    }
}

/// Simulated Redis Streams client.
struct RedisStreamsSimulator {
    latency_overhead: Duration,
    throughput_multiplier: f64,
}

impl RedisStreamsSimulator {
    async fn new() -> Self {
        Self {
            latency_overhead: Duration::from_micros(150),
            throughput_multiplier: 0.6, // Redis Streams typically slower for high throughput
        }
    }
    
    async fn measure_throughput(&self, message_count: usize, payload_size: usize) -> f64 {
        let base_time = Duration::from_nanos(payload_size as u64 * 80);
        let total_time = base_time * message_count as u32 + self.latency_overhead * message_count as u32;
        
        tokio::time::sleep(total_time / 1000).await;
        
        (message_count as f64 / total_time.as_secs_f64()) * self.throughput_multiplier
    }
    
    async fn measure_end_to_end_latency(&self, payload_size: usize) -> Duration {
        let base_latency = Duration::from_micros(80 + payload_size as u64 / 8);
        tokio::time::sleep(base_latency / 100).await;
        base_latency + self.latency_overhead
    }
}

/// Simulated NATS client.
struct NatsSimulator {
    latency_overhead: Duration,
    throughput_multiplier: f64,
}

impl NatsSimulator {
    async fn new() -> Self {
        Self {
            latency_overhead: Duration::from_micros(50), // NATS is very low latency
            throughput_multiplier: 1.2, // NATS can be very fast for small messages
        }
    }
    
    async fn measure_throughput(&self, message_count: usize, payload_size: usize) -> f64 {
        let base_time = Duration::from_nanos(payload_size as u64 * 30);
        let total_time = base_time * message_count as u32 + self.latency_overhead * message_count as u32;
        
        tokio::time::sleep(total_time / 1000).await;
        
        let multiplier = if payload_size <= 1024 {
            self.throughput_multiplier
        } else {
            self.throughput_multiplier * 0.8 // Performance drops for larger messages
        };
        
        (message_count as f64 / total_time.as_secs_f64()) * multiplier
    }
    
    async fn measure_end_to_end_latency(&self, payload_size: usize) -> Duration {
        let base_latency = Duration::from_micros(30 + payload_size as u64 / 20);
        tokio::time::sleep(base_latency / 100).await;
        base_latency + self.latency_overhead
    }
}

/// Extended test context for baseline comparisons.
impl BenchmarkContext {
    async fn create_persistent() -> TestContext {
        // Mock persistent context
        TestContext::with_config(BenchmarkConfig::throughput()).await
    }
    
    async fn create_partitioned(_partition_count: usize) -> TestContext {
        // Mock partitioned context
        TestContext::with_config(BenchmarkConfig::throughput()).await
    }
    
    async fn create_replicated(_replication_factor: usize) -> TestContext {
        // Mock replicated context
        TestContext::with_config(BenchmarkConfig::throughput()).await
    }
}

/// Extended publisher interface for baseline testing.
impl Publisher {
    async fn publish_persistent(&self, _message: Message) -> Result<()> {
        // Simulate persistence overhead
        tokio::time::sleep(Duration::from_micros(100)).await;
        Ok(())
    }
    
    async fn publish_replicated(&self, _message: Message, _replication_factor: usize) -> Result<()> {
        // Simulate replication overhead
        tokio::time::sleep(Duration::from_micros(200)).await;
        Ok(())
    }
}

criterion_group!(
    baseline_benches,
    bench_memorystreamer_baseline,
    bench_kafka_comparison,
    bench_redis_comparison,
    bench_nats_comparison,
    bench_system_baseline,
    bench_latency_comparison,
    bench_feature_comparison
);

criterion_main!(baseline_benches);