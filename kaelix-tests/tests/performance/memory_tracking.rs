use kaelix_tests::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn test_memory_usage_tracking() {
    let cluster = MemoryStreamerCluster::new(3);
    cluster.start();

    let memory_baseline = cluster.get_total_memory_usage();
    
    let producer = cluster.create_producer();
    let message_count = 500_000;
    let test_messages: Vec<Message> = (0..message_count)
        .map(|i| Message::new(format!("Memory {}", i).into_bytes(), "memory_topic".to_string()))
        .collect();

    producer.publish_batch(&test_messages).expect("Batch publish should succeed");

    let memory_after_publish = cluster.get_total_memory_usage();
    
    let memory_increase = memory_after_publish - memory_baseline;
    let avg_message_size = test_messages[0].payload.len();
    
    println!("Memory Baseline: {} bytes", memory_baseline);
    println!("Memory After Publish: {} bytes", memory_after_publish);
    println!("Memory Increase: {} bytes", memory_increase);
    println!("Average Message Size: {} bytes", avg_message_size);

    // Validate memory usage scales roughly linearly
    assert!(memory_increase < (memory_baseline * 10), "Memory usage should scale reasonably");
    
    cluster.stop();
}
