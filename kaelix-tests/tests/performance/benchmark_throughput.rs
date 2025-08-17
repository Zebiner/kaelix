use kaelix_tests::prelude::*;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::{Duration, Instant};

fn benchmark_message_throughput(c: &mut Criterion) {
    let cluster = MemoryStreamerCluster::new(3);
    cluster.start();

    let producer = cluster.create_producer();
    let consumer = cluster.create_consumer();

    let message_count = 1_000_000;
    let test_messages: Vec<Message> = (0..message_count)
        .map(|i| Message::new(format!("Bench {}", i).into_bytes(), "bench_topic".to_string()))
        .collect();

    c.bench_function("publish_1m_messages", |b| {
        b.iter(|| {
            let start = Instant::now();
            producer.publish_batch(black_box(&test_messages)).expect("Batch publish should succeed");
            let duration = start.elapsed();
            
            let msgs_per_sec = (message_count as f64) / duration.as_secs_f64();
            println!("Throughput: {:.2} messages/second", msgs_per_sec);
        });
    });

    cluster.stop();
}

fn benchmark_latency(c: &mut Criterion) {
    let cluster = MemoryStreamerCluster::new(3);
    cluster.start();

    let producer = cluster.create_producer();
    let consumer = cluster.create_consumer();

    let test_message = Message::new("Latency Bench".into_bytes(), "latency_bench".to_string());

    c.bench_function("single_message_latency", |b| {
        b.iter(|| {
            let start = Instant::now();
            producer.publish(black_box(&test_message)).expect("Publish should succeed");
            let received = consumer.consume_with_timeout(Duration::from_secs(1));
            let latency = start.elapsed();
            
            assert!(!received.is_empty(), "Message should be received");
            assert!(latency.as_micros() < 10, "Latency should be under 10 microseconds");
        });
    });

    cluster.stop();
}

criterion_group!(benches, benchmark_message_throughput, benchmark_latency);
criterion_main!(benches);
