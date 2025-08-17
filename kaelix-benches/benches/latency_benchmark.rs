//! Latency measurement benchmarks.

use kaelix_benches::prelude::*;
use kaelix_benches::utils::latency::*;
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::{Duration, Instant};

/// Benchmark end-to-end message latency.
fn bench_end_to_end_latency(c: &mut Criterion) {
    let rt = BenchmarkRuntime::create();
    
    c.bench_function("end_to_end_message_latency", |b| {
        b.to_async(&rt).iter_custom(|iters| async move {
            let mut ctx = BenchmarkContext::latency().await;
            ctx.start_brokers().await.expect("Failed to start brokers");
            ctx.create_publishers().await.expect("Failed to create publishers");
            ctx.create_consumers().await.expect("Failed to create consumers");
            
            let messages = BenchmarkData::small_messages(iters as usize)
                .expect("Failed to generate messages");
            
            let start = Instant::now();
            
            let _result = ctx.publish_batch(messages).await
                .expect("Failed to publish messages");
            
            start.elapsed()
        });
    });
}

/// Benchmark message processing latency.
fn bench_message_processing_latency(c: &mut Criterion) {
    let message_sizes = [64, 256, 1024, 4096];
    
    let mut group = c.benchmark_group("message_processing_latency");
    
    for size in message_sizes {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |b, &size| {
                let message = BenchmarkData::single_message(size)
                    .expect("Failed to create message");
                
                measure_latency(c, &format!("process_message_{}_bytes", size), || {
                    // Simulate message processing
                    let _topic = &message.topic;
                    let _payload_len = message.payload.len();
                    let _id = message.id;
                    
                    // Simulate some processing work
                    for i in 0..100 {
                        criterion::black_box(i * 2);
                    }
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark serialization latency.
fn bench_serialization_latency(c: &mut Criterion) {
    let message = BenchmarkData::json_messages(1)
        .expect("Failed to create message")
        .into_iter()
        .next()
        .expect("No message generated");
    
    c.bench_function("json_serialization_latency", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                let _serialized = criterion::black_box(
                    serde_json::to_vec(&message).expect("Serialization failed")
                );
            }
            start.elapsed()
        });
    });
    
    let serialized = serde_json::to_vec(&message).expect("Failed to serialize");
    
    c.bench_function("json_deserialization_latency", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                let _message: Message = criterion::black_box(
                    serde_json::from_slice(&serialized).expect("Deserialization failed")
                );
            }
            start.elapsed()
        });
    });
}

/// Benchmark memory allocation latency.
fn bench_allocation_latency(c: &mut Criterion) {
    let sizes = [1024, 4096, 16384, 65536];
    
    let mut group = c.benchmark_group("allocation_latency");
    
    for size in sizes {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |b, &size| {
                b.iter_custom(|iters| {
                    let start = Instant::now();
                    for _ in 0..iters {
                        let data = criterion::black_box(vec![0u8; size]);
                        criterion::black_box(data);
                    }
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark latency under different load conditions.
fn bench_latency_under_load(c: &mut Criterion) {
    let rt = BenchmarkRuntime::create();
    let load_levels = [100, 1000, 10000]; // messages per second
    
    let mut group = c.benchmark_group("latency_under_load");
    
    for load in load_levels {
        group.bench_with_input(
            BenchmarkId::from_parameter(load),
            &load,
            |b, &load| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let messages = BenchmarkData::small_messages(load)
                        .expect("Failed to generate messages");
                    
                    let start = Instant::now();
                    
                    // Simulate processing under load
                    for _ in 0..iters {
                        for message in &messages {
                            criterion::black_box(message.payload.len());
                        }
                    }
                    
                    start.elapsed()
                });
            },
        );
    }
    
    group.finish();
}

/// Measure detailed latency percentiles.
fn measure_latency_percentiles_detailed() {
    println!("Measuring detailed latency percentiles...");
    
    let stats = measure_latency_percentiles("message_creation", 10000, || {
        BenchmarkData::single_message(1024).expect("Failed to create message")
    });
    
    stats.print_summary();
}

criterion_group!(
    benches,
    bench_end_to_end_latency,
    bench_message_processing_latency,
    bench_serialization_latency,
    bench_allocation_latency,
    bench_latency_under_load
);

criterion_main!(benches);