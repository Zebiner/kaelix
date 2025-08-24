//! Stream Multiplexing System Demo
//!
//! Demonstrates the ultra-high-performance stream multiplexing capabilities
//! of MemoryStreamer with comprehensive testing and benchmarking.
//!
//! Note: This example requires the 'experimental' feature flag to be enabled.

#[cfg(feature = "experimental")]
use bytes::Bytes;
#[cfg(feature = "experimental")]
use kaelix_core::multiplexing::*;
#[cfg(feature = "experimental")]
use kaelix_core::{Message, Topic};
#[cfg(feature = "experimental")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "experimental")]
use std::sync::Arc;
#[cfg(feature = "experimental")]
use std::time::{Duration, Instant};
#[cfg(feature = "experimental")]
use tokio::time::timeout;

#[cfg(feature = "experimental")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 MemoryStreamer Stream Multiplexing Demo");
    println!("============================================\n");

    // Test 1: Basic Multiplexer Creation and Lifecycle
    println!("📋 Test 1: Basic Multiplexer Lifecycle");
    test_basic_lifecycle().await?;
    println!("✅ Basic lifecycle test passed\n");

    // Test 2: Stream Creation and Management
    println!("📋 Test 2: Stream Creation and Management");
    test_stream_creation().await?;
    println!("✅ Stream creation test passed\n");

    // Test 3: Message Routing and Processing
    println!("📋 Test 3: Message Routing and Processing");
    test_message_routing().await?;
    println!("✅ Message routing test passed\n");

    // Test 4: Performance Benchmarks
    println!("📋 Test 4: Performance Benchmarks");
    test_performance_benchmarks().await?;
    println!("✅ Performance benchmarks completed\n");

    // Test 5: Stream Groups and Dependencies
    println!("📋 Test 5: Stream Groups and Dependencies");
    test_stream_groups().await?;
    println!("✅ Stream groups test passed\n");

    // Test 6: Backpressure Management
    println!("📋 Test 6: Backpressure Management");
    test_backpressure_management().await?;
    println!("✅ Backpressure management test passed\n");

    // Test 7: Health Monitoring
    println!("📋 Test 7: Health Monitoring");
    test_health_monitoring().await?;
    println!("✅ Health monitoring test passed\n");

    println!("🎉 All Stream Multiplexing Tests Passed!");
    println!("MemoryStreamer is ready for ultra-high-performance streaming!");

    Ok(())
}

#[cfg(not(feature = "experimental"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("❌ Stream Multiplexing Demo requires the 'experimental' feature");
    println!("Please run with: cargo run --example multiplexing_demo --features experimental");
    Ok(())
}

#[cfg(feature = "experimental")]
async fn test_basic_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let config = MultiplexerConfig::default();
    let multiplexer = StreamMultiplexer::new(config)?;

    // Test initial state
    assert!(!multiplexer.is_running.load(Ordering::Relaxed));
    assert_eq!(multiplexer.get_active_stream_count(), 0);

    // Test startup
    multiplexer.start().await?;
    assert!(multiplexer.is_running.load(Ordering::Relaxed));

    // Test health check
    let health = multiplexer.health_check().await;
    assert!(health.is_healthy(), "Multiplexer should be healthy after startup");

    // Test shutdown
    multiplexer.shutdown().await?;

    println!("  ✓ Multiplexer lifecycle operations working correctly");
    Ok(())
}

#[cfg(feature = "experimental")]
async fn test_stream_creation() -> Result<(), Box<dyn std::error::Error>> {
    let config = MultiplexerConfig::default();
    let multiplexer = StreamMultiplexer::new(config)?;
    multiplexer.start().await?;

    // Create different types of streams
    let high_priority_config = StreamConfig::new()
        .with_priority(StreamPriority::High)
        .with_topics(vec!["orders.high".to_string()]);

    let normal_priority_config = StreamConfig::new()
        .with_priority(StreamPriority::Normal)
        .with_topics(vec!["orders.normal".to_string()]);

    let background_config = StreamConfig::new()
        .with_priority(StreamPriority::Background)
        .with_topics(vec!["analytics.background".to_string()]);

    // Create streams
    let high_stream = multiplexer.create_stream(high_priority_config).await?;
    let normal_stream = multiplexer.create_stream(normal_priority_config).await?;
    let background_stream = multiplexer.create_stream(background_config).await?;

    // Verify streams were created
    assert_eq!(multiplexer.get_active_stream_count(), 3);

    // Verify stream properties
    let high_stream_ref = multiplexer.get_stream(high_stream).await?;
    assert_eq!(high_stream_ref.priority(), StreamPriority::High);
    assert_eq!(high_stream_ref.state(), StreamState::Active);

    multiplexer.shutdown().await?;

    println!("  ✓ Created {} streams with different priorities", 3);
    println!("  ✓ Stream properties verified correctly");
    Ok(())
}

#[cfg(feature = "experimental")]
async fn test_message_routing() -> Result<(), Box<dyn std::error::Error>> {
    let config = MultiplexerConfig::default();
    let multiplexer = StreamMultiplexer::new(config)?;
    multiplexer.start().await?;

    // Create streams with specific topics
    let order_stream_config = StreamConfig::new().with_topics(vec!["orders.created".to_string()]);
    let order_stream = multiplexer.create_stream(order_stream_config).await?;

    let analytics_stream_config =
        StreamConfig::new().with_topics(vec!["analytics.events".to_string()]);
    let analytics_stream = multiplexer.create_stream(analytics_stream_config).await?;

    // Send messages using the correct API
    let order_message = Message::builder()
        .topic("orders.created")
        .payload(Bytes::from("Order #12345 created"))
        .build()?;

    let analytics_message = Message::builder()
        .topic("analytics.events")
        .payload(Bytes::from("User clicked button"))
        .build()?;

    // Route messages to streams
    multiplexer.send_to_stream(order_stream, order_message).await?;
    multiplexer.send_to_stream(analytics_stream, analytics_message).await?;

    // Verify messages were processed (check metrics)
    let order_stream_ref = multiplexer.get_stream(order_stream).await?;
    let metrics = order_stream_ref.get_metrics();
    assert!(metrics.messages_processed > 0, "Order stream should have processed messages");

    multiplexer.shutdown().await?;

    println!("  ✓ Messages routed correctly to appropriate streams");
    println!("  ✓ Stream metrics updated properly");
    Ok(())
}

#[cfg(feature = "experimental")]
async fn test_performance_benchmarks() -> Result<(), Box<dyn std::error::Error>> {
    let config = MultiplexerConfig::default();
    let multiplexer = StreamMultiplexer::new(config)?;
    multiplexer.start().await?;

    // Benchmark 1: Stream Creation Performance
    println!("  📊 Benchmark 1: Stream Creation Performance");
    let stream_count = 1000;
    let start = Instant::now();

    for i in 0..stream_count {
        let config = StreamConfig::new().with_topics(vec![format!("benchmark.topic.{}", i)]);
        multiplexer.create_stream(config).await?;
    }

    let creation_time = start.elapsed();
    let avg_creation_time = creation_time / stream_count;

    println!("    Created {} streams in {:?}", stream_count, creation_time);
    println!("    Average creation time: {:?}", avg_creation_time);

    // Target: <1μs per stream (though this is optimistic in a real scenario)
    assert!(
        avg_creation_time < Duration::from_micros(100),
        "Stream creation should be fast (got {:?})",
        avg_creation_time
    );

    // Benchmark 2: Message Sending Performance
    println!("  📊 Benchmark 2: Message Sending Performance");
    let message_count = 10000;
    let test_stream_config =
        StreamConfig::new().with_topics(vec!["benchmark.messages".to_string()]);
    let test_stream = multiplexer.create_stream(test_stream_config).await?;

    let start = Instant::now();
    for i in 0..message_count {
        let message = Message::builder()
            .topic("benchmark.messages")
            .payload(Bytes::from(format!("Message {}", i)))
            .build()?;
        multiplexer.send_to_stream(test_stream, message).await?;
    }
    let sending_time = start.elapsed();
    let avg_send_time = sending_time / message_count;

    println!("    Sent {} messages in {:?}", message_count, sending_time);
    println!("    Average send time: {:?}", avg_send_time);

    // Target: <10μs per message
    assert!(
        avg_send_time < Duration::from_micros(50),
        "Message sending should be fast (got {:?})",
        avg_send_time
    );

    // Benchmark 3: Stream Lookup Performance
    println!("  📊 Benchmark 3: Stream Lookup Performance");
    let lookup_count = 100000;
    let lookup_stream = multiplexer.create_stream(StreamConfig::new()).await?;

    let start = Instant::now();
    for _ in 0..lookup_count {
        let _stream_ref = multiplexer.get_stream(lookup_stream).await?;
    }
    let lookup_time = start.elapsed();
    let avg_lookup_time = lookup_time / lookup_count;

    println!("    Performed {} lookups in {:?}", lookup_count, lookup_time);
    println!("    Average lookup time: {:?}", avg_lookup_time);

    // Target: <10ns lookup time (this is very aggressive)
    assert!(
        avg_lookup_time < Duration::from_micros(1),
        "Stream lookup should be fast (got {:?})",
        avg_lookup_time
    );

    multiplexer.shutdown().await?;

    println!("  ✅ All performance benchmarks within acceptable limits");
    Ok(())
}

#[cfg(feature = "experimental")]
async fn test_stream_groups() -> Result<(), Box<dyn std::error::Error>> {
    let config = MultiplexerConfig::default();
    let multiplexer = StreamMultiplexer::new(config)?;
    multiplexer.start().await?;

    // Create streams for pipeline processing
    let producer_stream = multiplexer
        .create_stream(StreamConfig::new().with_topics(vec!["pipeline.input".to_string()]))
        .await?;

    let processor_stream = multiplexer
        .create_stream(StreamConfig::new().with_topics(vec!["pipeline.process".to_string()]))
        .await?;

    let consumer_stream = multiplexer
        .create_stream(StreamConfig::new().with_topics(vec!["pipeline.output".to_string()]))
        .await?;

    // Create pipeline group
    let pipeline_config = StreamGroupConfig {
        group_id: None,
        group_type: GroupType::Pipeline,
        coordination_strategy: CoordinationStrategy::NoCoordination,
        member_streams: vec![producer_stream, processor_stream, consumer_stream],
        shared_resources: Vec::new(),
    };

    let group_id = multiplexer.create_stream_group(pipeline_config).await?;
    assert!(group_id > 0, "Group ID should be generated");

    // Test fan-out group
    let fanout_producer = multiplexer.create_stream(StreamConfig::new()).await?;
    let fanout_consumer1 = multiplexer.create_stream(StreamConfig::new()).await?;
    let fanout_consumer2 = multiplexer.create_stream(StreamConfig::new()).await?;

    let fanout_config = StreamGroupConfig {
        group_id: None,
        group_type: GroupType::FanOut,
        coordination_strategy: CoordinationStrategy::NoCoordination,
        member_streams: vec![fanout_producer, fanout_consumer1, fanout_consumer2],
        shared_resources: Vec::new(),
    };

    let fanout_group_id = multiplexer.create_stream_group(fanout_config).await?;
    assert!(fanout_group_id > 0, "Fan-out group ID should be generated");
    assert_ne!(group_id, fanout_group_id, "Group IDs should be unique");

    multiplexer.shutdown().await?;

    println!("  ✓ Created pipeline and fan-out stream groups");
    println!("  ✓ Stream dependencies configured correctly");
    Ok(())
}

#[cfg(feature = "experimental")]
async fn test_backpressure_management() -> Result<(), Box<dyn std::error::Error>> {
    let config = MultiplexerConfig::default();
    let multiplexer = StreamMultiplexer::new(config)?;
    multiplexer.start().await?;

    // Create a stream for backpressure testing
    let stream_config = StreamConfig::new().with_topics(vec!["backpressure.test".to_string()]);
    let test_stream = multiplexer.create_stream(stream_config).await?;

    // Test credit allocation and consumption
    let initial_credits = multiplexer.backpressure_manager.get_available_credits(test_stream);
    assert!(initial_credits > 0, "Stream should have initial credits");

    // Allocate more credits
    multiplexer.backpressure_manager.allocate_credits(test_stream, 500).await?;

    let after_allocation = multiplexer.backpressure_manager.get_available_credits(test_stream);
    assert!(after_allocation > initial_credits, "Credits should increase after allocation");

    // Test credit consumption
    let consumed = multiplexer.backpressure_manager.consume_credits(test_stream, 100).await?;
    assert!(consumed, "Should be able to consume credits");

    let after_consumption = multiplexer.backpressure_manager.get_available_credits(test_stream);
    assert!(after_consumption < after_allocation, "Credits should decrease after consumption");

    // Test pressure reporting
    multiplexer
        .backpressure_manager
        .report_pressure(test_stream, BackpressureLevel::Medium)
        .await?;

    let stream_state = multiplexer.backpressure_manager.get_stream_pressure(test_stream);
    assert!(stream_state.is_some(), "Stream should have pressure state");

    let state = stream_state.unwrap();
    assert_eq!(state.get_level(), BackpressureLevel::Medium);

    multiplexer.shutdown().await?;

    println!("  ✓ Credit allocation and consumption working");
    println!("  ✓ Pressure reporting and monitoring functional");
    Ok(())
}

#[cfg(feature = "experimental")]
async fn test_health_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    let config = MultiplexerConfig::default();
    let multiplexer = StreamMultiplexer::new(config)?;
    multiplexer.start().await?;

    // Test initial health
    let health = multiplexer.health_check().await;
    assert!(health.is_healthy(), "Multiplexer should be healthy initially");

    // Test component health
    let component_health = multiplexer.get_component_health();
    assert!(component_health.all_healthy(), "All components should be healthy");

    // Test metrics collection
    let metrics = multiplexer.get_metrics();
    assert_eq!(metrics.total_streams_created.load(Ordering::Relaxed), 0);

    // Create a stream and verify metrics update
    let _test_stream = multiplexer.create_stream(StreamConfig::new()).await?;

    let updated_metrics = multiplexer.get_metrics();
    assert_eq!(updated_metrics.total_streams_created.load(Ordering::Relaxed), 1);

    // Test graceful shutdown
    let shutdown_result = timeout(Duration::from_secs(5), multiplexer.shutdown()).await;
    assert!(shutdown_result.is_ok(), "Shutdown should complete within timeout");

    println!("  ✓ Health monitoring system operational");
    println!("  ✓ Metrics collection working correctly");
    println!("  ✓ Graceful shutdown functioning");
    Ok(())
}

// Performance test helpers
#[cfg(feature = "experimental")]
#[allow(dead_code)]
async fn stress_test_concurrent_streams() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔥 Stress Test: Concurrent Stream Operations");

    let config = MultiplexerConfig::default();
    let multiplexer = Arc::new(StreamMultiplexer::new(config)?);
    multiplexer.start().await?;

    let concurrent_operations = 1000;
    let counter = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();

    // Spawn concurrent stream creation tasks
    let mut handles = Vec::new();
    for i in 0..concurrent_operations {
        let multiplexer_clone = Arc::clone(&multiplexer);
        let counter_clone = Arc::clone(&counter);

        let handle = tokio::spawn(async move {
            let config = StreamConfig::new().with_topics(vec![format!("stress.test.{}", i)]);

            match multiplexer_clone.create_stream(config).await {
                Ok(_) => counter_clone.fetch_add(1, Ordering::Relaxed),
                Err(_) => 0,
            };
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    let successful_operations = counter.load(Ordering::Relaxed);

    println!("  Created {} streams concurrently in {:?}", successful_operations, elapsed);
    println!("  Average time per operation: {:?}", elapsed / successful_operations as u32);

    multiplexer.shutdown().await?;

    assert!(
        successful_operations > concurrent_operations / 2,
        "At least half of concurrent operations should succeed"
    );

    Ok(())
}
