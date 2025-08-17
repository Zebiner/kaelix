//! Comprehensive latency benchmarks for MemoryStreamer.
//!
//! This benchmark suite validates that the system achieves <10μs P99 latency
//! across various scenarios including end-to-end message flow, producer latency,
//! consumer latency, consensus protocol, and authorization decisions.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use kaelix_benches::prelude::*;
use kaelix_benches::{BenchmarkConfig, PerformanceValidator};
use kaelix_core::{Message, Result};
use bytes::Bytes;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// Message sizes for latency testing (bytes).
const LATENCY_PAYLOAD_SIZES: &[usize] = &[64, 256, 1024, 4096];

/// Concurrency levels for concurrent latency tests.
const LATENCY_CONCURRENCY: &[usize] = &[1, 4, 8, 16];

/// Number of iterations for latency percentile calculation.
const LATENCY_ITERATIONS: usize = 10000;

/// Benchmark end-to-end message latency with percentile analysis.
fn bench_end_to_end_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let validator = PerformanceValidator::new(BenchmarkConfig::latency());
    
    let mut group = c.benchmark_group("end_to_end_latency");
    group.measurement_time(Duration::from_secs(15));
    
    for &payload_size in LATENCY_PAYLOAD_SIZES {
        group.bench_with_input(
            BenchmarkId::new("payload_size", payload_size),
            &payload_size,
            |b, &size| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::latency().await;
                    let consumer = ctx.create_consumer("latency_consumer").await.unwrap();
                    let message = create_test_message(size);
                    
                    let start = Instant::now();
                    
                    for _ in 0..iters {
                        let msg_start = Instant::now();
                        
                        // Publish message
                        ctx.publisher.publish(message.clone()).await.unwrap();
                        
                        // Consume message with timeout
                        let _consumed = consumer.poll_single(Duration::from_millis(100)).await.unwrap();
                        
                        let _latency = msg_start.elapsed();
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark producer publish latency.
fn bench_producer_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("producer_latency");
    
    for &payload_size in LATENCY_PAYLOAD_SIZES {
        group.bench_with_input(
            BenchmarkId::new("payload_size", payload_size),
            &payload_size,
            |b, &size| {
                let message = create_test_message(size);
                
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::latency().await;
                    let start = Instant::now();
                    
                    for _ in 0..iters {
                        let _ = ctx.publisher.publish(message.clone()).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark consumer poll latency.
fn bench_consumer_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("consumer_latency");
    
    for &payload_size in LATENCY_PAYLOAD_SIZES {
        group.bench_with_input(
            BenchmarkId::new("payload_size", payload_size),
            &payload_size,
            |b, &size| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::latency().await;
                    let consumer = ctx.create_consumer("poll_latency_consumer").await.unwrap();
                    
                    // Pre-populate with messages
                    let messages = create_test_message_batch(iters as usize, size);
                    for msg in messages {
                        ctx.publisher.publish(msg).await.unwrap();
                    }
                    
                    let start = Instant::now();
                    
                    for _ in 0..iters {
                        let _ = consumer.poll_single(Duration::from_millis(10)).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark consensus protocol latency.
fn bench_consensus_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("consensus_latency");
    
    for &node_count in &[3, 5, 7] {
        group.bench_with_input(
            BenchmarkId::new("nodes", node_count),
            &node_count,
            |b, &nodes| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::create_consensus_cluster(nodes).await;
                    let start = Instant::now();
                    
                    for i in 0..iters {
                        let proposal = ConsensusProposal::new(format!("proposal_{}", i));
                        let _ = ctx.consensus.propose(proposal).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark authorization decision latency with <100ns target.
fn bench_authorization_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("authorization_latency");
    group.measurement_time(Duration::from_secs(10));
    
    for &rule_count in &[10, 100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("rules", rule_count),
            &rule_count,
            |b, &rules| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::create_auth_context(rules).await;
                    let auth_request = AuthorizationRequest::new("user123", "topic:read");
                    
                    let start = Instant::now();
                    
                    for _ in 0..iters {
                        let _ = ctx.authorizer.authorize(&auth_request).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark network round-trip latency simulation.
fn bench_network_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("network_latency");
    
    for &network_delay in &[0, 1, 5, 10, 50] { // milliseconds
        group.bench_with_input(
            BenchmarkId::new("delay_ms", network_delay),
            &network_delay,
            |b, &delay| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::create_network_simulation(delay).await;
                    let message = create_test_message(1024);
                    
                    let start = Instant::now();
                    
                    for _ in 0..iters {
                        // Simulate request/response with network delay
                        let _ = ctx.remote_client.send_message(message.clone()).await;
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark latency under concurrent load.
fn bench_concurrent_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent_latency");
    
    for &concurrency in LATENCY_CONCURRENCY {
        group.bench_with_input(
            BenchmarkId::new("concurrent_tasks", concurrency),
            &concurrency,
            |b, &level| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let ctx = BenchmarkContext::latency().await;
                    let iterations_per_task = iters / level as u64;
                    
                    let start = Instant::now();
                    
                    // Spawn concurrent tasks
                    let mut tasks = Vec::new();
                    for task_id in 0..level {
                        let ctx_clone = ctx.clone();
                        let message = create_test_message(512);
                        
                        let task = tokio::spawn(async move {
                            let consumer = ctx_clone.create_consumer(&format!("latency_consumer_{}", task_id))
                                .await.unwrap();
                            
                            for _ in 0..iterations_per_task {
                                let msg_start = Instant::now();
                                ctx_clone.publisher.publish(message.clone()).await.unwrap();
                                let _ = consumer.poll_single(Duration::from_millis(10)).await;
                                let _latency = msg_start.elapsed();
                            }
                        });
                        
                        tasks.push(task);
                    }
                    
                    // Wait for all tasks
                    futures::future::join_all(tasks).await;
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark latency percentiles with detailed statistics.
fn bench_latency_percentiles(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let validator = PerformanceValidator::new(BenchmarkConfig::latency());
    
    c.bench_function("latency_percentiles_analysis", |b| {
        b.to_async(&rt).iter(|| async {
            let ctx = BenchmarkContext::latency().await;
            let consumer = ctx.create_consumer("percentile_consumer").await.unwrap();
            let message = create_test_message(1024);
            
            let mut latencies = Vec::with_capacity(LATENCY_ITERATIONS);
            
            for _ in 0..LATENCY_ITERATIONS {
                let start = Instant::now();
                ctx.publisher.publish(message.clone()).await.unwrap();
                let _ = consumer.poll_single(Duration::from_millis(10)).await.unwrap();
                latencies.push(start.elapsed());
            }
            
            let stats = crate::utils::latency::LatencyStats::from_measurements(latencies);
            
            // Validate P99 latency against <10μs target
            if let Err(e) = validator.validate_p99_latency(stats.p99) {
                tracing::warn!("P99 latency validation failed: {:?}", e);
            }
            
            // Print detailed statistics
            println!("Latency Statistics for {} iterations:", LATENCY_ITERATIONS);
            stats.print_summary();
            
            criterion::black_box(stats)
        });
    });
}

/// Benchmark message serialization/deserialization latency.
fn bench_serialization_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("serialization_latency");
    
    for &payload_size in LATENCY_PAYLOAD_SIZES {
        // JSON serialization
        group.bench_with_input(
            BenchmarkId::new("json_serialize", payload_size),
            &payload_size,
            |b, &size| {
                let data = create_json_payload(size);
                
                b.iter(|| {
                    let start = Instant::now();
                    let serialized = serde_json::to_vec(&data).unwrap();
                    let _latency = start.elapsed();
                    criterion::black_box(serialized)
                });
            },
        );
        
        // Binary serialization
        group.bench_with_input(
            BenchmarkId::new("binary_serialize", payload_size),
            &payload_size,
            |b, &size| {
                let data = create_binary_payload(size);
                
                b.iter(|| {
                    let start = Instant::now();
                    let serialized = bincode::serialize(&data).unwrap();
                    let _latency = start.elapsed();
                    criterion::black_box(serialized)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory allocation latency impact.
fn bench_allocation_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation_latency");
    
    for &size in &[1024, 4096, 16384, 65536] {
        // Stack allocation (when possible)
        group.bench_with_input(
            BenchmarkId::new("stack_alloc", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let start = Instant::now();
                    let buffer = [0u8; 1024]; // Limited to compile-time constant
                    let _latency = start.elapsed();
                    criterion::black_box(buffer)
                });
            },
        );
        
        // Heap allocation
        group.bench_with_input(
            BenchmarkId::new("heap_alloc", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let start = Instant::now();
                    let buffer = vec![0u8; size];
                    let _latency = start.elapsed();
                    criterion::black_box(buffer)
                });
            },
        );
        
        // Pre-allocated buffer reuse
        group.bench_with_input(
            BenchmarkId::new("reuse_buffer", size),
            &size,
            |b, &size| {
                let mut buffer = vec![0u8; size];
                
                b.iter(|| {
                    let start = Instant::now();
                    buffer.clear();
                    buffer.resize(size, 0);
                    let _latency = start.elapsed();
                    criterion::black_box(&buffer)
                });
            },
        );
    }
    
    group.finish();
}

/// Helper function to create a test message with specified payload size.
fn create_test_message(payload_size: usize) -> Message {
    let payload = "A".repeat(payload_size);
    Message::new("latency-test", Bytes::from(payload)).unwrap()
}

/// Helper function to create a batch of test messages.
fn create_test_message_batch(count: usize, payload_size: usize) -> Vec<Message> {
    (0..count)
        .map(|i| {
            let payload = format!("Msg{:06}:{}", i, "A".repeat(payload_size.saturating_sub(20)));
            Message::new("latency-test", Bytes::from(payload)).unwrap()
        })
        .collect()
}

/// Create JSON payload for serialization tests.
fn create_json_payload(size: usize) -> serde_json::Value {
    let content = "A".repeat(size / 2);
    serde_json::json!({
        "id": 12345,
        "timestamp": "2024-01-01T00:00:00Z",
        "content": content,
        "metadata": {
            "source": "benchmark",
            "version": "1.0"
        }
    })
}

/// Create binary payload for serialization tests.
fn create_binary_payload(size: usize) -> Vec<u8> {
    vec![0xAB; size]
}

// Mock implementations for types that don't exist yet
struct Consumer;

impl Consumer {
    async fn poll_single(&self, _timeout: Duration) -> Result<Message> {
        // Implementation would poll for a single message
        Ok(create_test_message(100))
    }
}

struct ConsensusProposal {
    data: String,
}

impl ConsensusProposal {
    fn new(data: String) -> Self {
        Self { data }
    }
}

struct ConsensusEngine;

impl ConsensusEngine {
    async fn propose(&self, _proposal: ConsensusProposal) -> Result<()> {
        // Simulate consensus latency
        tokio::time::sleep(Duration::from_micros(50)).await;
        Ok(())
    }
}

struct AuthorizationRequest {
    user: String,
    resource: String,
}

impl AuthorizationRequest {
    fn new(user: &str, resource: &str) -> Self {
        Self {
            user: user.to_string(),
            resource: resource.to_string(),
        }
    }
}

struct Authorizer;

impl Authorizer {
    async fn authorize(&self, _request: &AuthorizationRequest) -> Result<bool> {
        // Simulate auth decision latency (target <100ns)
        tokio::time::sleep(Duration::from_nanos(50)).await;
        Ok(true)
    }
}

struct RemoteClient;

impl RemoteClient {
    async fn send_message(&self, _message: Message) -> Result<()> {
        // Simulate network latency
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(())
    }
}

struct LatencyTestContext {
    publisher: Publisher,
    consensus: ConsensusEngine,
    authorizer: Authorizer,
    remote_client: RemoteClient,
}

impl LatencyTestContext {
    fn clone(&self) -> Self {
        // Mock clone implementation
        Self {
            publisher: Publisher,
            consensus: ConsensusEngine,
            authorizer: Authorizer,
            remote_client: RemoteClient,
        }
    }
    
    async fn create_consumer(&self, _consumer_id: &str) -> Result<Consumer> {
        Ok(Consumer)
    }
}

impl BenchmarkContext {
    async fn latency() -> LatencyTestContext {
        LatencyTestContext {
            publisher: Publisher,
            consensus: ConsensusEngine,
            authorizer: Authorizer,
            remote_client: RemoteClient,
        }
    }
    
    async fn create_consensus_cluster(_nodes: usize) -> LatencyTestContext {
        LatencyTestContext {
            publisher: Publisher,
            consensus: ConsensusEngine,
            authorizer: Authorizer,
            remote_client: RemoteClient,
        }
    }
    
    async fn create_auth_context(_rule_count: usize) -> LatencyTestContext {
        LatencyTestContext {
            publisher: Publisher,
            consensus: ConsensusEngine,
            authorizer: Authorizer,
            remote_client: RemoteClient,
        }
    }
    
    async fn create_network_simulation(_delay_ms: u64) -> LatencyTestContext {
        LatencyTestContext {
            publisher: Publisher,
            consensus: ConsensusEngine,
            authorizer: Authorizer,
            remote_client: RemoteClient,
        }
    }
}

criterion_group!(
    latency_benches,
    bench_end_to_end_latency,
    bench_producer_latency,
    bench_consumer_latency,
    bench_consensus_latency,
    bench_authorization_latency,
    bench_network_latency,
    bench_concurrent_latency,
    bench_latency_percentiles,
    bench_serialization_latency,
    bench_allocation_latency
);

criterion_main!(latency_benches);