//! Core messaging property tests for streaming system validation.
//!
//! This module contains comprehensive property-based tests for message handling,
//! routing, and delivery guarantees in the MemoryStreamer system.

use crate::{
    generators::properties::*,
    validators::invariants::*,
    macros::*,
    TestContext, TestResult,
    constants::*,
};
use kaelix_core::{Message, Topic, MessageId, PartitionId, prelude::*};
use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// Property tests for message publication and delivery.
pub mod publication {
    use super::*;

    proptest_throughput! {
        test_high_throughput_publishing(
            batch in arb_high_throughput_batch(),
            target_rate: u64 = TARGET_THROUGHPUT
        ) {
            let ctx = TestContext::new().await?;
            let start = Instant::now();
            
            // Publish the entire batch
            ctx.publish_batch(batch.clone()).await?;
            
            let elapsed = start.elapsed();
            let actual_rate = batch.len() as f64 / elapsed.as_secs_f64();
            
            // Should achieve at least 90% of target throughput
            assertions::assert_throughput(actual_rate, target_rate as f64, 10.0)?;
            
            Ok(())
        }
    }

    proptest_latency! {
        test_low_latency_single_message(
            message in arb_message(),
            target_latency_micros: u64 = TARGET_LATENCY_P99_MICROS
        ) {
            let ctx = TestContext::new().await?;
            let start = Instant::now();
            
            // Publish single message
            ctx.publish_single(message).await?;
            
            let latency_micros = start.elapsed().as_micros() as u64;
            assertions::assert_latency(latency_micros, target_latency_micros)?;
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_message_ordering_preservation(
            messages in prop::collection::vec(arb_message(), 10..=100)
        ) {
            let ctx = TestContext::new().await?;
            let mut checker = InvariantChecker::new();
            
            // Publish messages in sequence
            for message in messages {
                ctx.publish_single(message).await?;
            }
            
            // Allow time for processing
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            let state = ctx.get_system_state().await?;
            let result = checker.check_all(&state).await;
            
            if !result.all_satisfied() {
                return Err(TestError::AssertionFailed {
                    message: format!("Invariants violated: {}", result.summary()),
                });
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_message_deduplication(
            original_message in arb_message(),
            duplicate_count in 2usize..=10usize
        ) {
            let ctx = TestContext::new().await?;
            
            // Send the same message multiple times
            for _ in 0..duplicate_count {
                ctx.publish_single(original_message.clone()).await?;
            }
            
            // Allow processing time
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Verify only one copy is stored
            let stored_messages = ctx.get_stored_messages().await?;
            let matching_messages: Vec<_> = stored_messages
                .iter()
                .filter(|msg| msg.id() == original_message.id())
                .collect();
            
            if matching_messages.len() != 1 {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Expected 1 message after deduplication, found {}",
                        matching_messages.len()
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_benchmark! {
        bench_variable_message_sizes(
            batch_size in 100usize..=1000usize,
            payload_size in 64usize..=65536usize
        ) {
            let ctx = TestContext::new().await?;
            
            // Generate messages with specific payload size
            let mut messages = Vec::with_capacity(batch_size);
            for i in 0..batch_size {
                let topic = Topic::new(format!("bench-topic-{}", i % 10))?;
                let payload = bytes::Bytes::from(vec![0u8; payload_size]);
                messages.push(Message::new(topic.as_str(), payload)?);
            }
            
            let start = Instant::now();
            ctx.publish_batch(messages).await?;
            let duration = start.elapsed();
            
            let throughput = batch_size as f64 / duration.as_secs_f64();
            let bandwidth_mbps = (batch_size * payload_size) as f64 / duration.as_secs_f64() / 1_000_000.0;
            
            println!("Batch: {}, Size: {} bytes, Throughput: {:.2} msg/sec, Bandwidth: {:.2} MB/s", 
                batch_size, payload_size, throughput, bandwidth_mbps);
            
            // Minimum performance requirement
            if throughput < 100.0 {
                return Err(TestError::AssertionFailed {
                    message: format!("Throughput too low: {:.2} msg/sec", throughput),
                });
            }
            
            Ok(())
        }
    }
}

/// Property tests for message consumption and acknowledgment.
pub mod consumption {
    use super::*;

    proptest_integration! {
        test_consumer_message_ordering(
            messages in prop::collection::vec(arb_message(), 50..=200),
            partition_id in arb_partition()
        ) {
            let ctx = TestContext::new().await?;
            
            // Configure all messages for the same partition to test ordering
            let mut partition_messages = Vec::new();
            for (i, mut message) in messages.into_iter().enumerate() {
                // Ensure messages go to the same partition
                message.set_partition_key(&format!("key-{}", partition_id.as_u32()));
                partition_messages.push(message);
            }
            
            // Publish messages
            ctx.publish_batch(partition_messages.clone()).await?;
            
            // Create consumer
            let consumer = ctx.create_consumer().await?;
            
            // Consume messages
            let mut consumed = Vec::new();
            let timeout_duration = Duration::from_secs(10);
            
            for _ in 0..partition_messages.len() {
                match timeout(timeout_duration, consumer.receive()).await {
                    Ok(Some(message)) => consumed.push(message),
                    Ok(None) => break,
                    Err(_) => return Err(TestError::Timeout {
                        message: "Consumer timeout waiting for message".to_string(),
                    }),
                }
            }
            
            // Verify ordering within partition
            let mut prev_offset = None;
            for message in consumed {
                if message.partition_id() == partition_id {
                    if let Some(prev) = prev_offset {
                        if message.offset() <= prev {
                            return Err(TestError::AssertionFailed {
                                message: format!(
                                    "Message ordering violation: offset {} after {}",
                                    message.offset().as_u64(), prev.as_u64()
                                ),
                            });
                        }
                    }
                    prev_offset = Some(message.offset());
                }
            }
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_at_least_once_delivery(
            messages in prop::collection::vec(arb_message(), 20..=50)
        ) {
            let ctx = TestContext::new().await?;
            
            // Publish messages
            ctx.publish_batch(messages.clone()).await?;
            
            // Create consumer
            let consumer = ctx.create_consumer().await?;
            
            // Track received message IDs
            let mut received_ids = HashSet::new();
            let timeout_duration = Duration::from_secs(5);
            
            // Consume messages
            for _ in 0..messages.len() * 2 { // Allow for potential duplicates
                match timeout(timeout_duration, consumer.receive()).await {
                    Ok(Some(message)) => {
                        received_ids.insert(message.id());
                    },
                    Ok(None) => break,
                    Err(_) => break, // Timeout is expected when no more messages
                }
            }
            
            // Verify all original messages were received at least once
            let original_ids: HashSet<_> = messages.iter().map(|m| m.id()).collect();
            for original_id in original_ids {
                if !received_ids.contains(&original_id) {
                    return Err(TestError::AssertionFailed {
                        message: format!("Message {} was not delivered", original_id),
                    });
                }
            }
            
            Ok(())
        }
    }

    proptest_latency! {
        test_consumer_latency(
            message in arb_message(),
            target_latency_micros: u64 = TARGET_LATENCY_P99_MICROS * 2 // Allow extra time for end-to-end
        ) {
            let ctx = TestContext::new().await?;
            let consumer = ctx.create_consumer().await?;
            
            let start = Instant::now();
            
            // Publish message
            ctx.publish_single(message.clone()).await?;
            
            // Consume message
            if let Some(consumed) = timeout(Duration::from_secs(1), consumer.receive()).await? {
                let latency_micros = start.elapsed().as_micros() as u64;
                
                // Verify it's the same message
                if consumed.id() != message.id() {
                    return Err(TestError::AssertionFailed {
                        message: "Received different message than expected".to_string(),
                    });
                }
                
                assertions::assert_latency(latency_micros, target_latency_micros)?;
            } else {
                return Err(TestError::AssertionFailed {
                    message: "No message received".to_string(),
                });
            }
            
            Ok(())
        }
    }
}

/// Property tests for topic management and partitioning.
pub mod topics {
    use super::*;

    proptest_invariant! {
        test_topic_creation_idempotency(
            topic_name in "[a-z]{3,20}",
            creation_attempts in 2usize..=5usize
        ) {
            let ctx = TestContext::new().await?;
            
            // Attempt to create the same topic multiple times
            let mut topic_ids = Vec::new();
            for _ in 0..creation_attempts {
                let topic_id = ctx.create_topic(&topic_name).await?;
                topic_ids.push(topic_id);
            }
            
            // All attempts should return the same topic ID
            let first_id = topic_ids[0];
            for id in topic_ids {
                if id != first_id {
                    return Err(TestError::AssertionFailed {
                        message: format!(
                            "Topic creation not idempotent: got different IDs {} and {}",
                            first_id, id
                        ),
                    });
                }
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_partition_distribution(
            topics in prop::collection::vec(arb_topic(), 5..=20),
            messages_per_topic in 50usize..=100usize
        ) {
            let ctx = TestContext::new().await?;
            
            // Create topics
            for topic in &topics {
                ctx.create_topic(topic.as_str()).await?;
            }
            
            // Send messages to each topic
            let mut all_messages = Vec::new();
            for topic in &topics {
                for i in 0..messages_per_topic {
                    let payload = bytes::Bytes::from(format!("message-{}", i));
                    let message = Message::new(topic.as_str(), payload)?;
                    all_messages.push(message);
                }
            }
            
            // Publish all messages
            ctx.publish_batch(all_messages).await?;
            
            // Allow processing
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Verify partition distribution
            let state = ctx.get_system_state().await?;
            let mut partition_counts = HashMap::new();
            
            for message_info in state.messages.values() {
                *partition_counts.entry(message_info.partition_id).or_insert(0) += 1;
            }
            
            // Check that messages are distributed across multiple partitions
            if partition_counts.len() < 2 {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Poor partition distribution: only {} partitions used",
                        partition_counts.len()
                    ),
                });
            }
            
            // Check that no single partition is overloaded
            let total_messages = partition_counts.values().sum::<u32>();
            let max_partition_load = partition_counts.values().max().unwrap_or(&0);
            let load_ratio = *max_partition_load as f64 / total_messages as f64;
            
            if load_ratio > 0.7 { // No partition should have >70% of messages
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Unbalanced partition distribution: max partition has {:.1}% of messages",
                        load_ratio * 100.0
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_topic_deletion_cleanup(
            topic in arb_topic(),
            messages in prop::collection::vec(arb_message(), 10..=50)
        ) {
            let ctx = TestContext::new().await?;
            
            // Create topic and publish messages
            ctx.create_topic(topic.as_str()).await?;
            
            let mut topic_messages = Vec::new();
            for message in messages {
                let mut topic_message = Message::new(topic.as_str(), message.payload().clone())?;
                topic_messages.push(topic_message);
            }
            
            ctx.publish_batch(topic_messages).await?;
            
            // Verify messages exist
            let state_before = ctx.get_system_state().await?;
            let messages_before = state_before.messages.len();
            
            if messages_before == 0 {
                return Err(TestError::AssertionFailed {
                    message: "No messages found before deletion".to_string(),
                });
            }
            
            // Delete topic
            ctx.delete_topic(topic.as_str()).await?;
            
            // Allow cleanup time
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            // Verify messages are cleaned up
            let state_after = ctx.get_system_state().await?;
            let topic_messages_after: Vec<_> = state_after.messages
                .values()
                .filter(|msg| msg.message.topic().as_str() == topic.as_str())
                .collect();
            
            if !topic_messages_after.is_empty() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Topic deletion cleanup failed: {} messages remain",
                        topic_messages_after.len()
                    ),
                });
            }
            
            Ok(())
        }
    }
}

/// Property tests for error handling and edge cases.
pub mod error_handling {
    use super::*;

    proptest_invariant! {
        test_invalid_topic_rejection(
            invalid_topic_name in ".*[^a-zA-Z0-9._/-].*" // Contains invalid characters
        ) {
            let ctx = TestContext::new().await?;
            
            // Attempt to create topic with invalid name
            let result = ctx.create_topic(&invalid_topic_name).await;
            
            // Should fail with appropriate error
            if result.is_ok() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Invalid topic name '{}' was incorrectly accepted",
                        invalid_topic_name
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_oversized_message_rejection(
            topic in arb_topic(),
            oversized_payload_size in 10_000_000usize..=50_000_000usize // 10-50MB
        ) {
            let ctx = TestContext::new().await?;
            
            // Create oversized message
            let payload = bytes::Bytes::from(vec![0u8; oversized_payload_size]);
            let message = Message::new(topic.as_str(), payload)?;
            
            // Attempt to publish oversized message
            let result = ctx.publish_single(message).await;
            
            // Should fail with size limit error
            if result.is_ok() {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Oversized message ({} bytes) was incorrectly accepted",
                        oversized_payload_size
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_graceful_resource_exhaustion(
            message_count in 10000usize..=50000usize,
            message_size in 1024usize..=4096usize
        ) {
            let ctx = TestContext::new().await?;
            
            // Generate large number of messages to stress system
            let mut messages = Vec::with_capacity(message_count);
            for i in 0..message_count {
                let topic = Topic::new(format!("stress-topic-{}", i % 100))?;
                let payload = bytes::Bytes::from(vec![i as u8; message_size]);
                messages.push(Message::new(topic.as_str(), payload)?);
            }
            
            let start = Instant::now();
            
            // Attempt to publish all messages
            let result = timeout(Duration::from_secs(30), ctx.publish_batch(messages)).await;
            
            match result {
                Ok(Ok(())) => {
                    // Success - verify system is still responsive
                    let test_message = Message::new("test", bytes::Bytes::from("ping"))?;
                    let ping_result = timeout(
                        Duration::from_secs(5), 
                        ctx.publish_single(test_message)
                    ).await;
                    
                    if ping_result.is_err() {
                        return Err(TestError::AssertionFailed {
                            message: "System unresponsive after stress test".to_string(),
                        });
                    }
                },
                Ok(Err(e)) => {
                    // Expected failure due to resource limits
                    // Verify system provides meaningful error
                    let error_msg = e.to_string().to_lowercase();
                    if !error_msg.contains("resource") && !error_msg.contains("limit") && !error_msg.contains("capacity") {
                        return Err(TestError::AssertionFailed {
                            message: format!("Unexpected error during resource exhaustion: {}", e),
                        });
                    }
                },
                Err(_) => {
                    // Timeout - verify system is still responsive
                    let test_message = Message::new("test", bytes::Bytes::from("ping"))?;
                    let ping_result = timeout(
                        Duration::from_secs(5), 
                        ctx.publish_single(test_message)
                    ).await;
                    
                    if ping_result.is_err() {
                        return Err(TestError::AssertionFailed {
                            message: "System unresponsive after timeout".to_string(),
                        });
                    }
                }
            }
            
            let duration = start.elapsed();
            println!("Stress test duration: {:.2}s", duration.as_secs_f64());
            
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_property_test_framework() {
        // Smoke test to ensure the property test framework compiles and runs
        let result: Result<(), TestError> = async {
            let _ctx = TestContext::new().await?;
            Ok(())
        }.await;
        
        assert!(result.is_ok(), "Property test framework should initialize successfully");
    }
}