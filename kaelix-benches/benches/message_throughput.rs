//! Message throughput benchmarks.

use kaelix_benches::prelude::*;
use kaelix_core::{Message, Result};
use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;

/// Benchmark message creation throughput.
fn bench_message_creation(c: &mut Criterion) {
    let payload_sizes = [64, 256, 1024, 4096, 16384];
    
    for size in payload_sizes {
        let payload = bytes::Bytes::from("A".repeat(size));
        let topic = "benchmark-topic";
        
        c.bench_function(&format!("message_creation_{}_bytes", size), |b| {
            b.iter(|| {
                criterion::black_box(Message::new(topic, payload.clone()))
            });
        });
    }
}

/// Benchmark message serialization throughput.
fn bench_message_serialization(c: &mut Criterion) {
    let messages = BenchmarkData::variable_messages(1000).expect("Failed to generate messages");
    
    c.bench_function("message_serialization", |b| {
        b.iter(|| {
            for message in &messages {
                let _serialized = criterion::black_box(
                    serde_json::to_vec(message).expect("Serialization failed")
                );
            }
        });
    });
}

/// Benchmark batch message processing.
fn bench_batch_processing(c: &mut Criterion) {
    use kaelix_benches::utils::patterns::*;
    
    benchmark_batch_sizes(c, "batch_message_processing", |batch_size| {
        let messages = BenchmarkData::medium_messages(batch_size).expect("Failed to generate messages");
        
        // Simulate batch processing
        for message in messages {
            criterion::black_box(message.payload.len());
        }
    });
}

/// Benchmark concurrent message creation.
fn bench_concurrent_creation(c: &mut Criterion) {
    let rt = BenchmarkRuntime::create();
    
    c.bench_function("concurrent_message_creation", |b| {
        b.to_async(&rt).iter(|| async {
            let tasks = (0..100).map(|i| {
                tokio::spawn(async move {
                    let payload = bytes::Bytes::from(format!("message-{}", i));
                    Message::new("concurrent-topic", payload)
                })
            });
            
            let results: Vec<Result<Message>> = futures::future::join_all(tasks)
                .await
                .into_iter()
                .map(|r| r.expect("Task panicked"))
                .collect();
            
            criterion::black_box(results);
        });
    });
}

/// Benchmark memory allocation patterns.
fn bench_memory_allocation(c: &mut Criterion) {
    c.bench_function("zero_copy_message_clone", |b| {
        let message = BenchmarkData::single_message(1024).expect("Failed to create message");
        
        b.iter(|| {
            let cloned = criterion::black_box(message.clone());
            // Test that cloning is zero-copy for the payload
            assert_eq!(cloned.payload.as_ptr(), message.payload.as_ptr());
        });
    });
}

criterion_group!(
    benches,
    bench_message_creation,
    bench_message_serialization,
    bench_batch_processing,
    bench_concurrent_creation,
    bench_memory_allocation
);

criterion_main!(benches);