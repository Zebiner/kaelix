use kaelix_tests::prelude::*;
use std::time::Instant;

#[test]
fn test_10m_messages_per_second() {
    let cluster = MemoryStreamerCluster::new(3);
    cluster.start();

    let producer = cluster.create_producer();
    let consumer = cluster.create_consumer();

    let message_count = 10_000_000;
    let test_messages: Vec<Message> = (0..message_count)
        .map(|i| Message::new(format!("Perf {}", i).into_bytes(), "perf_topic".to_string()))
        .collect();

    let start = Instant::now();
    
    producer.publish_batch(&test_messages).expect("Batch publish should succeed");
    
    let received = consumer.consume_with_timeout(Duration::from_secs(2));
    
    let duration = start.elapsed();
    let msgs_per_sec = (received.len() as f64) / duration.as_secs_f64();
    
    println!("Performance: {:.2} messages/second", msgs_per_sec);
    
    assert!(msgs_per_sec >= 10_000_000.0, "Should achieve 10M messages/second");
    cluster.stop();
}

#[test]
fn test_latency_target() {
    let cluster = MemoryStreamerCluster::new(3);
    cluster.start();

    let producer = cluster.create_producer();
    let consumer = cluster.create_consumer();

    let test_message = Message::new("Latency Test".into_bytes(), "latency_topic".to_string());
    
    let start = Instant::now();
    producer.publish(&test_message).expect("Publish should succeed");
    
    let received = consumer.consume_with_timeout(Duration::from_secs(1));
    
    let latency = start.elapsed();
    
    println!("P99 Latency: {:?}", latency);
    
    assert!(latency.as_micros() < 10, "Latency should be under 10 microseconds");
    cluster.stop();
}
