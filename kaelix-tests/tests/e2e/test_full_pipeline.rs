use kaelix_tests::prelude::*;
use std::time::Duration;

#[test]
fn test_complete_messaging_pipeline() {
    // Validate end-to-end messaging pipeline
    let mut cluster = MemoryStreamerCluster::new(3);  // 3-node cluster
    cluster.start();

    let producer = cluster.create_producer();
    let consumer = cluster.create_consumer();

    let test_messages: Vec<Message> = (0..100_000)
        .map(|i| Message::new(format!("Test {}", i).into_bytes(), "test_topic".to_string()))
        .collect();

    producer.publish_batch(&test_messages).expect("Batch publish should succeed");

    let received = consumer.consume_with_timeout(Duration::from_secs(5));
    
    assert_eq!(received.len(), test_messages.len(), "All messages should be processed");
    cluster.stop();
}

#[test]
fn test_distributed_resilience() {
    let mut cluster = MemoryStreamerCluster::new(5);  // 5-node cluster
    cluster.start();

    // Simulate node failure during messaging
    cluster.fail_random_node();

    let producer = cluster.create_producer();
    let consumer = cluster.create_consumer();

    let test_messages: Vec<Message> = (0..50_000)
        .map(|i| Message::new(format!("Resilience {}", i).into_bytes(), "resilience_topic".to_string()))
        .collect();

    producer.publish_batch(&test_messages).expect("Batch publish should succeed");

    let received = consumer.consume_with_timeout(Duration::from_secs(10));
    
    assert!(received.len() >= test_messages.len() * 0.95, "Most messages should survive node failure");
    cluster.stop();
}
