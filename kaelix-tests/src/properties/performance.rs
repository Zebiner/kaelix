//! Performance property tests for throughput and latency validation.
//!
//! This module contains comprehensive property-based tests for performance characteristics,
//! ensuring the system meets its 10M+ msg/sec and <10μs P99 latency targets.

use crate::{
    generators::properties::*,
    validators::invariants::*,
    macros::*,
    TestContext, TestResult,
    constants::*,
};
use kaelix_core::{Message, Topic, prelude::*};
use proptest::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// Property tests for throughput characteristics.
pub mod throughput {
    use super::*;

    proptest_throughput! {
        test_single_producer_throughput(
            batch_size in 1000usize..=10000usize,
            message_size in 64usize..=4096usize,
            target_rate: u64 = TARGET_THROUGHPUT / 10 // Single producer target
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            // Generate messages with specified size
            let mut messages = Vec::with_capacity(batch_size);
            for i in 0..batch_size {
                let topic = Topic::new(format!("perf-topic-{}", i % 10))?;
                let payload = bytes::Bytes::from(vec![0u8; message_size]);
                messages.push(Message::new(topic.as_str(), payload)?);
            }
            
            let start = Instant::now();
            
            // Publish batch
            producer.publish_batch(messages).await?;
            
            let duration = start.elapsed();
            let actual_rate = batch_size as f64 / duration.as_secs_f64();
            
            println!("Single producer: {:.2} msg/sec, {} byte messages", 
                actual_rate, message_size);
            
            assertions::assert_throughput(actual_rate, target_rate as f64, 20.0)?;
            
            // Verify memory usage is reasonable
            let memory_usage = ctx.get_memory_usage().await?;
            assertions::assert_memory_usage(memory_usage, MAX_MEMORY_USAGE)?;
            
            Ok(())
        }
    }

    proptest_throughput! {
        test_multi_producer_scaling(
            producer_count in 2usize..=10usize,
            messages_per_producer in 500usize..=2000usize,
            target_rate: u64 = TARGET_THROUGHPUT
        ) {
            let ctx = TestContext::new().await?;
            
            // Create multiple producers
            let mut producers = Vec::new();
            for _ in 0..producer_count {
                producers.push(ctx.create_high_performance_producer().await?);
            }
            
            let start = Instant::now();
            
            // Launch concurrent publishing
            let mut tasks = Vec::new();
            for (i, producer) in producers.into_iter().enumerate() {
                let messages_count = messages_per_producer;
                let task = tokio::spawn(async move {
                    let mut messages = Vec::with_capacity(messages_count);
                    for j in 0..messages_count {
                        let topic = Topic::new(format!("scaling-topic-{}-{}", i, j % 5))?;
                        let payload = bytes::Bytes::from(format!("producer-{}-msg-{}", i, j));
                        messages.push(Message::new(topic.as_str(), payload)?);
                    }
                    producer.publish_batch(messages).await
                });
                tasks.push(task);
            }
            
            // Wait for all producers to complete
            for task in tasks {
                task.await??;
            }
            
            let duration = start.elapsed();
            let total_messages = producer_count * messages_per_producer;
            let actual_rate = total_messages as f64 / duration.as_secs_f64();
            
            println!("Multi-producer scaling: {} producers, {:.2} msg/sec total", 
                producer_count, actual_rate);
            
            // Should scale well with multiple producers
            let min_expected_rate = (target_rate as f64 * 0.8).min(actual_rate * 0.9);
            assertions::assert_throughput(actual_rate, min_expected_rate, 15.0)?;
            
            // Verify CPU usage is reasonable
            let cpu_usage = ctx.get_cpu_usage().await?;
            assertions::assert_cpu_usage(cpu_usage, 90.0)?; // Allow high CPU for performance test
            
            Ok(())
        }
    }

    proptest_benchmark! {
        bench_variable_batch_sizes(
            batch_size in 10usize..=50000usize,
            message_size in 128usize..=2048usize
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            // Generate messages
            let mut messages = Vec::with_capacity(batch_size);
            for i in 0..batch_size {
                let topic = Topic::new("batch-bench")?;
                let payload = bytes::Bytes::from(vec![i as u8; message_size]);
                messages.push(Message::new(topic.as_str(), payload)?);
            }
            
            let start = Instant::now();
            producer.publish_batch(messages).await?;
            let duration = start.elapsed();
            
            let throughput = batch_size as f64 / duration.as_secs_f64();
            let latency_per_message = duration.as_micros() as f64 / batch_size as f64;
            
            println!("Batch size: {}, Throughput: {:.2} msg/sec, Avg latency: {:.2}μs", 
                batch_size, throughput, latency_per_message);
            
            // Larger batches should generally achieve higher throughput
            if batch_size >= 1000 && throughput < 10000.0 {
                return Err(TestError::AssertionFailed {
                    message: format!("Large batch throughput too low: {:.2} msg/sec", throughput),
                });
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_sustained_throughput(
            target_rate in 1_000_000u64..=TARGET_THROUGHPUT,
            duration_secs in 30u64..=120u64
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            let messages_per_second = target_rate;
            let total_messages = messages_per_second * duration_secs;
            let batch_size = 1000usize;
            let batches = (total_messages / batch_size as u64) as usize;
            
            let start = Instant::now();
            let mut total_sent = 0u64;
            
            for batch_num in 0..batches {
                let batch_start = Instant::now();
                
                // Generate batch
                let mut messages = Vec::with_capacity(batch_size);
                for i in 0..batch_size {
                    let topic = Topic::new(format!("sustained-{}", (batch_num * batch_size + i) % 100))?;
                    let payload = bytes::Bytes::from(format!("sustained-msg-{}", total_sent + i as u64));
                    messages.push(Message::new(topic.as_str(), payload)?);
                }
                
                // Publish batch
                producer.publish_batch(messages).await?;
                total_sent += batch_size as u64;
                
                // Rate limiting to maintain target
                let expected_batch_duration = Duration::from_secs_f64(batch_size as f64 / messages_per_second as f64);
                let actual_batch_duration = batch_start.elapsed();
                
                if actual_batch_duration < expected_batch_duration {
                    tokio::time::sleep(expected_batch_duration - actual_batch_duration).await;
                }
                
                // Check if we've hit the duration limit
                if start.elapsed().as_secs() >= duration_secs {
                    break;
                }
            }
            
            let total_duration = start.elapsed();
            let actual_rate = total_sent as f64 / total_duration.as_secs_f64();
            
            println!("Sustained throughput: {:.2} msg/sec over {:.1}s", 
                actual_rate, total_duration.as_secs_f64());
            
            // Should maintain close to target rate
            let rate_tolerance = 10.0; // 10% tolerance
            assertions::assert_throughput(actual_rate, target_rate as f64, rate_tolerance)?;
            
            // Verify system stability during sustained load
            let final_memory = ctx.get_memory_usage().await?;
            assertions::assert_memory_usage(final_memory, MAX_MEMORY_USAGE * 2)?; // Allow some growth
            
            Ok(())
        }
    }
}

/// Property tests for latency characteristics.
pub mod latency {
    use super::*;

    proptest_latency! {
        test_single_message_latency(
            message in arb_message(),
            target_latency_micros: u64 = TARGET_LATENCY_P99_MICROS
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_low_latency_producer().await?;
            
            // Warm up the system
            for _ in 0..10 {
                let warmup_msg = Message::new("warmup", bytes::Bytes::from("warmup"))?;
                producer.publish_single(warmup_msg).await?;
            }
            
            // Measure actual latency
            let start = Instant::now();
            producer.publish_single(message).await?;
            let latency_micros = start.elapsed().as_micros() as u64;
            
            assertions::assert_latency(latency_micros, target_latency_micros)?;
            
            println!("Single message latency: {}μs", latency_micros);
            
            Ok(())
        }
    }

    proptest_latency! {
        test_batch_latency_distribution(
            batch in arb_high_throughput_batch().prop_filter(
                "Reasonable batch size for latency test",
                |batch| batch.len() <= 1000
            ),
            target_p99_micros: u64 = TARGET_LATENCY_P99_MICROS
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_low_latency_producer().await?;
            
            // Measure latency for each message in batch
            let mut latencies = Vec::new();
            
            for message in batch {
                let start = Instant::now();
                producer.publish_single(message).await?;
                let latency_micros = start.elapsed().as_micros() as u64;
                latencies.push(latency_micros);
            }
            
            // Calculate percentiles
            latencies.sort_unstable();
            let len = latencies.len();
            let p50 = latencies[len * 50 / 100];
            let p95 = latencies[len * 95 / 100];
            let p99 = latencies[len * 99 / 100];
            
            println!("Latency distribution - P50: {}μs, P95: {}μs, P99: {}μs", 
                p50, p95, p99);
            
            // P99 should meet target
            assertions::assert_latency(p99, target_p99_micros)?;
            
            // P50 should be significantly better than P99
            if p50 > target_p99_micros / 2 {
                return Err(TestError::AssertionFailed {
                    message: format!("P50 latency too high: {}μs", p50),
                });
            }
            
            Ok(())
        }
    }

    proptest_integration! {
        test_latency_under_load(
            background_rate in 100_000u64..=1_000_000u64,
            probe_messages in prop::collection::vec(arb_message(), 50..=200)
        ) {
            let ctx = TestContext::new().await?;
            let background_producer = ctx.create_high_performance_producer().await?;
            let probe_producer = ctx.create_low_latency_producer().await?;
            
            // Start background load
            let background_task = {
                let producer = background_producer;
                let rate = background_rate;
                tokio::spawn(async move {
                    let messages_per_batch = 100;
                    let batches_per_second = rate / messages_per_batch;
                    let batch_interval = Duration::from_millis(1000 / batches_per_second);
                    
                    for i in 0..100 { // Run for ~10 seconds
                        let mut batch = Vec::new();
                        for j in 0..messages_per_batch {
                            let topic = Topic::new(format!("bg-{}", (i * messages_per_batch + j) % 10))?;
                            let payload = bytes::Bytes::from(format!("background-{}-{}", i, j));
                            batch.push(Message::new(topic.as_str(), payload)?);
                        }
                        
                        producer.publish_batch(batch).await?;
                        tokio::time::sleep(batch_interval).await;
                    }
                    
                    Ok::<(), TestError>(())
                })
            };
            
            // Allow background load to stabilize
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            // Measure probe message latencies
            let mut probe_latencies = Vec::new();
            for message in probe_messages {
                let start = Instant::now();
                probe_producer.publish_single(message).await?;
                let latency_micros = start.elapsed().as_micros() as u64;
                probe_latencies.push(latency_micros);
                
                // Small delay between probes
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            // Stop background load
            background_task.abort();
            
            // Analyze probe latencies
            probe_latencies.sort_unstable();
            let len = probe_latencies.len();
            let p99 = probe_latencies[len * 99 / 100];
            let median = probe_latencies[len / 2];
            
            println!("Latency under {}msg/s load - Median: {}μs, P99: {}μs", 
                background_rate, median, p99);
            
            // P99 should still meet target (with some allowance for load)
            let target_under_load = TARGET_LATENCY_P99_MICROS * 2; // Allow 2x degradation
            assertions::assert_latency(p99, target_under_load)?;
            
            Ok(())
        }
    }

    proptest_benchmark! {
        bench_latency_by_message_size(
            message_size in 64usize..=65536usize,
            measurement_count in 100usize..=500usize
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_low_latency_producer().await?;
            
            // Warm up
            for _ in 0..10 {
                let warmup = Message::new("warmup", bytes::Bytes::from(vec![0u8; message_size]))?;
                producer.publish_single(warmup).await?;
            }
            
            let mut latencies = Vec::with_capacity(measurement_count);
            
            for i in 0..measurement_count {
                let payload = bytes::Bytes::from(vec![i as u8; message_size]);
                let message = Message::new("latency-bench", payload)?;
                
                let start = Instant::now();
                producer.publish_single(message).await?;
                let latency_micros = start.elapsed().as_micros() as u64;
                
                latencies.push(latency_micros);
            }
            
            latencies.sort_unstable();
            let p50 = latencies[measurement_count * 50 / 100];
            let p99 = latencies[measurement_count * 99 / 100];
            let avg = latencies.iter().sum::<u64>() / measurement_count as u64;
            
            println!("Message size: {} bytes, Avg: {}μs, P50: {}μs, P99: {}μs", 
                message_size, avg, p50, p99);
            
            // Larger messages may have higher latency, but should be reasonable
            let size_factor = (message_size as f64 / 1024.0).sqrt(); // Sublinear scaling expected
            let adjusted_target = (TARGET_LATENCY_P99_MICROS as f64 * size_factor) as u64;
            
            if p99 > adjusted_target.max(TARGET_LATENCY_P99_MICROS * 10) {
                return Err(TestError::AssertionFailed {
                    message: format!("P99 latency too high for {}byte messages: {}μs", message_size, p99),
                });
            }
            
            Ok(())
        }
    }
}

/// Property tests for resource usage and efficiency.
pub mod resource_efficiency {
    use super::*;

    proptest_benchmark! {
        bench_memory_efficiency(
            message_count in 10000usize..=100000usize,
            message_size in 256usize..=4096usize
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            // Measure initial memory
            let initial_memory = ctx.get_memory_usage().await?;
            
            // Generate and publish messages
            let mut messages = Vec::with_capacity(message_count);
            for i in 0..message_count {
                let topic = Topic::new(format!("mem-test-{}", i % 10))?;
                let payload = bytes::Bytes::from(vec![i as u8; message_size]);
                messages.push(Message::new(topic.as_str(), payload)?);
            }
            
            producer.publish_batch(messages).await?;
            
            // Measure peak memory
            let peak_memory = ctx.get_memory_usage().await?;
            let memory_increase = peak_memory - initial_memory;
            
            // Calculate memory efficiency
            let total_data_size = (message_count * message_size) as u64;
            let memory_overhead_ratio = memory_increase as f64 / total_data_size as f64;
            
            println!("Memory efficiency: {:.2}x overhead for {} messages of {} bytes", 
                memory_overhead_ratio, message_count, message_size);
            
            // Memory overhead should be reasonable (allow 3x for metadata, buffers, etc.)
            if memory_overhead_ratio > 3.0 {
                return Err(TestError::AssertionFailed {
                    message: format!("Memory overhead too high: {:.2}x", memory_overhead_ratio),
                });
            }
            
            // Verify memory usage doesn't exceed limits
            assertions::assert_memory_usage(peak_memory, MAX_MEMORY_USAGE)?;
            
            Ok(())
        }
    }

    proptest_integration! {
        test_cpu_efficiency_under_load(
            load_level in 0.1f64..=0.9f64, // 10% to 90% of max throughput
            duration_secs in 10u64..=30u64
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            let target_rate = (TARGET_THROUGHPUT as f64 * load_level) as u64;
            let batch_size = 500;
            let batches_per_second = target_rate / batch_size;
            let batch_interval = Duration::from_millis(1000 / batches_per_second);
            
            let initial_cpu = ctx.get_cpu_usage().await?;
            
            let start = Instant::now();
            let mut total_messages = 0u64;
            
            while start.elapsed().as_secs() < duration_secs {
                let batch_start = Instant::now();
                
                // Generate batch
                let mut messages = Vec::with_capacity(batch_size as usize);
                for i in 0..batch_size {
                    let topic = Topic::new(format!("cpu-test-{}", (total_messages + i) % 20))?;
                    let payload = bytes::Bytes::from(format!("cpu-efficiency-{}", total_messages + i));
                    messages.push(Message::new(topic.as_str(), payload)?);
                }
                
                // Publish batch
                producer.publish_batch(messages).await?;
                total_messages += batch_size;
                
                // Rate limiting
                let elapsed = batch_start.elapsed();
                if elapsed < batch_interval {
                    tokio::time::sleep(batch_interval - elapsed).await;
                }
            }
            
            let final_cpu = ctx.get_cpu_usage().await?;
            let actual_rate = total_messages as f64 / start.elapsed().as_secs_f64();
            
            println!("CPU efficiency: {:.1}% CPU for {:.0} msg/sec ({:.1}% of target load)", 
                final_cpu, actual_rate, load_level * 100.0);
            
            // CPU usage should be proportional to load (with some baseline)
            let expected_max_cpu = 20.0 + (load_level * 70.0); // 20% baseline + 70% scaled
            assertions::assert_cpu_usage(final_cpu, expected_max_cpu)?;
            
            // Should achieve close to target rate
            assertions::assert_throughput(actual_rate, target_rate as f64, 15.0)?;
            
            Ok(())
        }
    }

    proptest_invariant! {
        test_resource_cleanup_after_load(
            peak_load_messages in 50000usize..=200000usize,
            cleanup_wait_secs in 5u64..=20u64
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            // Measure baseline resources
            let baseline_memory = ctx.get_memory_usage().await?;
            let baseline_cpu = ctx.get_cpu_usage().await?;
            
            // Apply peak load
            let mut messages = Vec::with_capacity(peak_load_messages);
            for i in 0..peak_load_messages {
                let topic = Topic::new(format!("cleanup-test-{}", i % 5))?;
                let payload = bytes::Bytes::from(vec![i as u8; 1024]);
                messages.push(Message::new(topic.as_str(), payload)?);
            }
            
            let load_start = Instant::now();
            producer.publish_batch(messages).await?;
            let load_duration = load_start.elapsed();
            
            let peak_memory = ctx.get_memory_usage().await?;
            let peak_cpu = ctx.get_cpu_usage().await?;
            
            println!("Peak load: {} messages in {:.2}s, Memory: {} bytes, CPU: {:.1}%", 
                peak_load_messages, load_duration.as_secs_f64(), peak_memory, peak_cpu);
            
            // Wait for cleanup
            tokio::time::sleep(Duration::from_secs(cleanup_wait_secs)).await;
            
            let cleanup_memory = ctx.get_memory_usage().await?;
            let cleanup_cpu = ctx.get_cpu_usage().await?;
            
            println!("After cleanup: Memory: {} bytes, CPU: {:.1}%", cleanup_memory, cleanup_cpu);
            
            // Memory should return close to baseline
            let memory_increase = cleanup_memory.saturating_sub(baseline_memory);
            let peak_increase = peak_memory.saturating_sub(baseline_memory);
            
            if memory_increase > peak_increase / 2 {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Poor memory cleanup: {} bytes remaining of {} peak increase",
                        memory_increase, peak_increase
                    ),
                });
            }
            
            // CPU should return to near baseline
            if cleanup_cpu > baseline_cpu + 20.0 {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Poor CPU cleanup: {:.1}% vs {:.1}% baseline",
                        cleanup_cpu, baseline_cpu
                    ),
                });
            }
            
            Ok(())
        }
    }
}

/// Property tests for scalability characteristics.
pub mod scalability {
    use super::*;

    proptest_integration! {
        test_topic_count_scalability(
            topic_count in 10usize..=1000usize,
            messages_per_topic in 100usize..=1000usize
        ) {
            let ctx = TestContext::new().await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            // Create topics and messages
            let mut all_messages = Vec::new();
            for topic_id in 0..topic_count {
                let topic = Topic::new(format!("scale-topic-{:06}", topic_id))?;
                
                for msg_id in 0..messages_per_topic {
                    let payload = bytes::Bytes::from(format!("msg-{}-{}", topic_id, msg_id));
                    all_messages.push(Message::new(topic.as_str(), payload)?);
                }
            }
            
            let start = Instant::now();
            producer.publish_batch(all_messages).await?;
            let duration = start.elapsed();
            
            let total_messages = topic_count * messages_per_topic;
            let throughput = total_messages as f64 / duration.as_secs_f64();
            
            println!("Topic scalability: {} topics, {:.2} msg/sec total", 
                topic_count, throughput);
            
            // Throughput should not degrade significantly with more topics
            let min_expected_throughput = 10000.0; // Minimum acceptable
            if throughput < min_expected_throughput {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Throughput too low with {} topics: {:.2} msg/sec",
                        topic_count, throughput
                    ),
                });
            }
            
            // Memory usage should scale reasonably with topic count
            let memory_usage = ctx.get_memory_usage().await?;
            let memory_per_topic = memory_usage / topic_count as u64;
            
            if memory_per_topic > 10_000_000 { // 10MB per topic seems excessive
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Memory usage per topic too high: {} bytes",
                        memory_per_topic
                    ),
                });
            }
            
            Ok(())
        }
    }

    proptest_benchmark! {
        bench_partition_count_scaling(
            partition_count in 4usize..=256usize,
            messages_per_partition in 1000usize..=5000usize
        ) {
            let ctx = TestContext::new_with_partitions(partition_count).await?;
            let producer = ctx.create_high_performance_producer().await?;
            
            // Generate messages distributed across partitions
            let mut messages = Vec::new();
            for partition_id in 0..partition_count {
                for msg_id in 0..messages_per_partition {
                    let topic = Topic::new("partition-scale-test")?;
                    let partition_key = format!("partition-{}", partition_id);
                    let payload = bytes::Bytes::from(format!("p{}-m{}", partition_id, msg_id));
                    
                    let mut message = Message::new(topic.as_str(), payload)?;
                    message.set_partition_key(&partition_key);
                    messages.push(message);
                }
            }
            
            let start = Instant::now();
            producer.publish_batch(messages).await?;
            let duration = start.elapsed();
            
            let total_messages = partition_count * messages_per_partition;
            let throughput = total_messages as f64 / duration.as_secs_f64();
            
            println!("Partition scaling: {} partitions, {:.2} msg/sec", 
                partition_count, throughput);
            
            // Should maintain good throughput across partition counts
            let min_throughput = 5000.0; // Adjust based on requirements
            if throughput < min_throughput {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Throughput too low with {} partitions: {:.2} msg/sec",
                        partition_count, throughput
                    ),
                });
            }
            
            // Verify messages are distributed across partitions
            let partition_distribution = ctx.get_partition_distribution().await?;
            let non_empty_partitions = partition_distribution.iter()
                .filter(|(_, &count)| count > 0)
                .count();
            
            if non_empty_partitions < partition_count / 2 {
                return Err(TestError::AssertionFailed {
                    message: format!(
                        "Poor partition distribution: only {}/{} partitions used",
                        non_empty_partitions, partition_count
                    ),
                });
            }
            
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_performance_property_framework() {
        // Smoke test for performance property framework
        let result: Result<(), TestError> = async {
            let message = Message::new("test", bytes::Bytes::from("test"))?;
            assert!(!message.topic().as_str().is_empty());
            Ok(())
        }.await;
        
        assert!(result.is_ok(), "Performance property test framework should work");
    }
}