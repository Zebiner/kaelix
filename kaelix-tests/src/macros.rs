//! Property test macros for streamlined testing.
//!
//! This module provides convenient macros for creating property-based tests
//! specifically designed for streaming system validation.

/// Macro for creating throughput validation property tests.
///
/// # Example
/// ```rust
/// proptest_throughput! {
///     test_high_throughput_publishing(
///         batch in arb_high_throughput_batch(),
///         target_rate: u64 = 10_000_000
///     ) {
///         let ctx = TestContext::new().await;
///         let start = Instant::now();
///         
///         ctx.publish_batch(batch).await?;
///         
///         let elapsed = start.elapsed();
///         let actual_rate = batch.len() as f64 / elapsed.as_secs_f64();
///         
///         prop_assert!(actual_rate >= target_rate as f64 * 0.9); // 90% of target
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_throughput {
    (
        $test_name:ident(
            $($arg:ident in $strategy:expr),* $(,)?
            $(target_rate: $target_type:ty = $target_value:expr)?
        ) $body:block
    ) => {
        proptest! {
            #[test]
            fn $test_name($($arg in $strategy),*) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    use std::time::Instant;
                    use $crate::constants::*;
                    
                    $(let target_rate: $target_type = $target_value;)?
                    
                    let result: Result<(), TestError> = async move $body.await;
                    
                    match result {
                        Ok(()) => (),
                        Err(e) => prop_assert!(false, "Test failed: {}", e),
                    }
                });
            }
        }
    };
}

/// Macro for creating latency validation property tests.
///
/// # Example
/// ```rust
/// proptest_latency! {
///     test_low_latency_messaging(
///         message in arb_message(),
///         target_latency_micros: u64 = 10
///     ) {
///         let ctx = TestContext::new().await;
///         let start = Instant::now();
///         
///         ctx.publish_single(message).await?;
///         
///         let latency_micros = start.elapsed().as_micros() as u64;
///         prop_assert!(latency_micros <= target_latency_micros);
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_latency {
    (
        $test_name:ident(
            $($arg:ident in $strategy:expr),* $(,)?
            $(target_latency_micros: $target_type:ty = $target_value:expr)?
        ) $body:block
    ) => {
        proptest! {
            #[test]
            fn $test_name($($arg in $strategy),*) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    use std::time::Instant;
                    use $crate::constants::*;
                    
                    $(let target_latency_micros: $target_type = $target_value;)?
                    
                    let result: Result<(), TestError> = async move $body.await;
                    
                    match result {
                        Ok(()) => (),
                        Err(e) => prop_assert!(false, "Test failed: {}", e),
                    }
                });
            }
        }
    };
}

/// Macro for creating general invariant property tests.
///
/// # Example
/// ```rust
/// proptest_invariant! {
///     test_message_ordering(
///         messages in prop::collection::vec(arb_message(), 10..=100)
///     ) {
///         let ctx = TestContext::new().await;
///         let mut checker = InvariantChecker::new();
///         
///         for message in messages {
///             ctx.publish_single(message).await?;
///         }
///         
///         let state = ctx.get_system_state().await?;
///         let result = checker.check_all(&state).await;
///         
///         prop_assert!(result.all_satisfied(), "Invariants violated: {}", result.summary());
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_invariant {
    (
        $test_name:ident(
            $($arg:ident in $strategy:expr),* $(,)?
        ) $body:block
    ) => {
        proptest! {
            #[test]
            fn $test_name($($arg in $strategy),*) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    use $crate::validators::invariants::*;
                    use $crate::constants::*;
                    
                    let result: Result<(), TestError> = async move $body.await;
                    
                    match result {
                        Ok(()) => (),
                        Err(e) => prop_assert!(false, "Test failed: {}", e),
                    }
                });
            }
        }
    };
}

/// Macro for creating chaos engineering property tests.
///
/// # Example
/// ```rust
/// proptest_chaos! {
///     test_resilience_to_node_failures(
///         scenario in arb_chaos_scenario(),
///         workload in arb_streaming_workload()
///     ) {
///         let mut ctx = TestContext::new().await;
///         let mut chaos_engine = ChaosEngine::new();
///         
///         // Start the workload
///         let workload_handle = ctx.start_workload(workload).await?;
///         
///         // Inject chaos
///         chaos_engine.inject_failure(scenario).await?;
///         
///         // Wait for recovery
///         tokio::time::sleep(Duration::from_secs(10)).await;
///         
///         // Verify system recovered
///         let state = ctx.get_system_state().await?;
///         let checker = InvariantChecker::new();
///         let result = checker.check_all(&state).await;
///         
///         prop_assert!(!result.has_critical_violations(), 
///             "Critical invariants violated after chaos: {}", result.summary());
///         
///         workload_handle.stop().await?;
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_chaos {
    (
        $test_name:ident(
            $($arg:ident in $strategy:expr),* $(,)?
        ) $body:block
    ) => {
        proptest! {
            #[test]
            fn $test_name($($arg in $strategy),*) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    use $crate::simulators::ChaosEngine;
                    use $crate::validators::invariants::*;
                    use $crate::generators::properties::*;
                    use std::time::Duration;
                    
                    let result: Result<(), TestError> = async move $body.await;
                    
                    match result {
                        Ok(()) => (),
                        Err(e) => prop_assert!(false, "Test failed: {}", e),
                    }
                });
            }
        }
    };
}

/// Macro for creating performance benchmark property tests.
///
/// # Example
/// ```rust
/// proptest_benchmark! {
///     bench_message_processing(
///         batch_size in 1000usize..=10000usize,
///         message_size in 64usize..=65536usize
///     ) {
///         let ctx = TestContext::new().await;
///         let messages = generate_messages(batch_size, message_size).await?;
///         
///         let start = Instant::now();
///         ctx.publish_batch(messages).await?;
///         let duration = start.elapsed();
///         
///         let throughput = batch_size as f64 / duration.as_secs_f64();
///         
///         // Record benchmark results
///         println!("Batch size: {}, Message size: {}, Throughput: {:.2} msg/sec", 
///             batch_size, message_size, throughput);
///         
///         prop_assert!(throughput >= 1000.0, "Throughput too low: {}", throughput);
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_benchmark {
    (
        $test_name:ident(
            $($arg:ident in $strategy:expr),* $(,)?
        ) $body:block
    ) => {
        proptest! {
            #[test]
            fn $test_name($($arg in $strategy),*) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    use std::time::Instant;
                    use $crate::constants::*;
                    
                    let result: Result<(), TestError> = async move $body.await;
                    
                    match result {
                        Ok(()) => (),
                        Err(e) => prop_assert!(false, "Test failed: {}", e),
                    }
                });
            }
        }
    };
}

/// Macro for creating security-focused property tests.
///
/// # Example
/// ```rust
/// proptest_security! {
///     test_authorization_consistency(
///         token in security::arb_auth_token(),
///         operation in arb_operation()
///     ) {
///         let ctx = TestContext::new().await;
///         
///         // Perform operation with token
///         let result1 = ctx.authorize_operation(&token, &operation).await?;
///         
///         // Perform same operation again - should get same result
///         let result2 = ctx.authorize_operation(&token, &operation).await?;
///         
///         prop_assert_eq!(result1, result2, "Authorization decisions inconsistent");
///         
///         // If token is invalid, operation should fail
///         if matches!(token.validity, TokenValidity::Expired | TokenValidity::Revoked | TokenValidity::Malformed) {
///             prop_assert!(!result1, "Invalid token should not be authorized");
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_security {
    (
        $test_name:ident(
            $($arg:ident in $strategy:expr),* $(,)?
        ) $body:block
    ) => {
        proptest! {
            #[test]
            fn $test_name($($arg in $strategy),*) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    use $crate::generators::properties::security::*;
                    
                    let result: Result<(), TestError> = async move $body.await;
                    
                    match result {
                        Ok(()) => (),
                        Err(e) => prop_assert!(false, "Test failed: {}", e),
                    }
                });
            }
        }
    };
}

/// Macro for creating integration property tests across multiple components.
///
/// # Example
/// ```rust
/// proptest_integration! {
///     test_end_to_end_flow(
///         producer_config in arb_producer_config(),
///         consumer_config in arb_consumer_config(),
///         messages in prop::collection::vec(arb_message(), 100..=1000)
///     ) {
///         let ctx = TestContext::new().await;
///         
///         // Setup producer and consumer
///         let producer = ctx.create_producer(producer_config).await?;
///         let consumer = ctx.create_consumer(consumer_config).await?;
///         
///         // Produce messages
///         for message in &messages {
///             producer.send(message.clone()).await?;
///         }
///         
///         // Consume messages
///         let mut consumed = Vec::new();
///         for _ in 0..messages.len() {
///             if let Some(msg) = consumer.receive().await? {
///                 consumed.push(msg);
///             }
///         }
///         
///         prop_assert_eq!(consumed.len(), messages.len(), "Message count mismatch");
///         
///         // Verify message content (order may vary depending on partitioning)
///         for original in &messages {
///             prop_assert!(consumed.iter().any(|c| c.id() == original.id()), 
///                 "Message {} not found in consumed messages", original.id());
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_integration {
    (
        $test_name:ident(
            $($arg:ident in $strategy:expr),* $(,)?
        ) $body:block
    ) => {
        proptest! {
            #[test]
            fn $test_name($($arg in $strategy),*) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    use $crate::constants::*;
                    
                    let result: Result<(), TestError> = async move $body.await;
                    
                    match result {
                        Ok(()) => (),
                        Err(e) => prop_assert!(false, "Test failed: {}", e),
                    }
                });
            }
        }
    };
}

/// Helper macro for creating custom property test strategies.
///
/// # Example
/// ```rust
/// proptest_strategy! {
///     pub fn arb_custom_message() -> impl Strategy<Value = CustomMessage> {
///         (arb_message(), arb_timestamp(), arb_headers()).prop_map(|(msg, ts, headers)| {
///             CustomMessage::new(msg, ts, headers)
///         })
///     }
/// }
/// ```
#[macro_export]
macro_rules! proptest_strategy {
    (
        $(#[$attr:meta])*
        $vis:vis fn $name:ident() -> impl Strategy<Value = $output:ty> $body:block
    ) => {
        $(#[$attr])*
        $vis fn $name() -> impl proptest::strategy::Strategy<Value = $output> {
            use proptest::prelude::*;
            $body
        }
    };
}

/// Error type for property tests.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Test assertion failed: {message}")]
    AssertionFailed { message: String },
    
    #[error("Test setup failed: {message}")]
    SetupFailed { message: String },
    
    #[error("Test timeout: {message}")]
    Timeout { message: String },
    
    #[error("Core error: {0}")]
    Core(#[from] kaelix_core::Error),
    
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Assertion helpers for property tests.
pub mod assertions {
    use super::*;
    
    /// Assert that throughput meets minimum requirements.
    pub fn assert_throughput(actual: f64, target: f64, tolerance_percent: f64) -> Result<(), TestError> {
        let min_acceptable = target * (1.0 - tolerance_percent / 100.0);
        if actual < min_acceptable {
            return Err(TestError::AssertionFailed {
                message: format!(
                    "Throughput {:.2} msg/sec below minimum {:.2} msg/sec (target: {:.2})",
                    actual, min_acceptable, target
                ),
            });
        }
        Ok(())
    }
    
    /// Assert that latency is within acceptable bounds.
    pub fn assert_latency(actual_micros: u64, target_micros: u64) -> Result<(), TestError> {
        if actual_micros > target_micros {
            return Err(TestError::AssertionFailed {
                message: format!(
                    "Latency {}μs exceeds target {}μs",
                    actual_micros, target_micros
                ),
            });
        }
        Ok(())
    }
    
    /// Assert that memory usage is within bounds.
    pub fn assert_memory_usage(actual_bytes: u64, max_bytes: u64) -> Result<(), TestError> {
        if actual_bytes > max_bytes {
            return Err(TestError::AssertionFailed {
                message: format!(
                    "Memory usage {} bytes exceeds maximum {} bytes",
                    actual_bytes, max_bytes
                ),
            });
        }
        Ok(())
    }
    
    /// Assert that CPU usage is reasonable.
    pub fn assert_cpu_usage(actual_percent: f64, max_percent: f64) -> Result<(), TestError> {
        if actual_percent > max_percent {
            return Err(TestError::AssertionFailed {
                message: format!(
                    "CPU usage {:.2}% exceeds maximum {:.2}%",
                    actual_percent, max_percent
                ),
            });
        }
        Ok(())
    }
}

/// Shrinking strategies optimized for streaming system debugging.
pub mod shrinking {
    use proptest::prelude::*;
    use kaelix_core::Message;
    
    /// Custom shrinking strategy for message batches.
    /// Prioritizes removing messages while preserving ordering patterns.
    pub fn shrink_message_batch(batch: Vec<Message>) -> impl Iterator<Item = Vec<Message>> + Clone {
        // Start with empty batch
        std::iter::once(Vec::new())
            // Then try removing messages from the end
            .chain((0..batch.len()).rev().map(move |len| batch[..len].to_vec()))
            // Then try removing messages from the beginning
            .chain((1..batch.len()).map(move |start| batch[start..].to_vec()))
            // Then try removing every other message
            .chain(std::iter::once(
                batch.iter().step_by(2).cloned().collect()
            ))
    }
    
    /// Custom shrinking for workload patterns.
    /// Simplifies complex patterns while maintaining essential characteristics.
    pub fn shrink_workload_duration(duration_seconds: u64) -> impl Iterator<Item = u64> + Clone {
        // Shrink duration while maintaining minimum viable test time
        (1..=duration_seconds)
            .rev()
            .filter(|&d| d >= 10) // Minimum 10 seconds for meaningful tests
            .chain(std::iter::once(10)) // Always try minimum duration
    }
    
    /// Shrinking strategy for chaos scenarios.
    /// Reduces complexity while preserving failure characteristics.
    pub fn shrink_failure_duration(duration_seconds: u64) -> impl Iterator<Item = u64> + Clone {
        // Shrink failure duration to find minimal reproducing case
        (1..=duration_seconds)
            .rev()
            .filter(|&d| d >= 1) // Minimum 1 second failure
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    
    // Test that macros compile correctly
    proptest_strategy! {
        fn test_strategy() -> impl Strategy<Value = u32> {
            any::<u32>()
        }
    }
    
    #[test]
    fn test_assertion_helpers() {
        // Test throughput assertion
        assert!(assertions::assert_throughput(1000.0, 1000.0, 10.0).is_ok());
        assert!(assertions::assert_throughput(900.0, 1000.0, 5.0).is_err());
        
        // Test latency assertion
        assert!(assertions::assert_latency(5, 10).is_ok());
        assert!(assertions::assert_latency(15, 10).is_err());
        
        // Test memory assertion
        assert!(assertions::assert_memory_usage(512_000_000, 1_000_000_000).is_ok());
        assert!(assertions::assert_memory_usage(2_000_000_000, 1_000_000_000).is_err());
        
        // Test CPU assertion
        assert!(assertions::assert_cpu_usage(50.0, 80.0).is_ok());
        assert!(assertions::assert_cpu_usage(90.0, 80.0).is_err());
    }
    
    #[test]
    fn test_shrinking_strategies() {
        // Test message batch shrinking
        let messages = vec![
            kaelix_core::Message::new("topic1", bytes::Bytes::from("msg1")).unwrap(),
            kaelix_core::Message::new("topic1", bytes::Bytes::from("msg2")).unwrap(),
            kaelix_core::Message::new("topic1", bytes::Bytes::from("msg3")).unwrap(),
        ];
        
        let shrunk: Vec<_> = shrinking::shrink_message_batch(messages).take(5).collect();
        assert!(!shrunk.is_empty());
        assert!(shrunk[0].is_empty()); // First shrink should be empty
        
        // Test duration shrinking
        let durations: Vec<_> = shrinking::shrink_workload_duration(60).take(10).collect();
        assert!(durations.iter().all(|&d| d >= 10)); // All should be >= minimum
        assert!(durations.contains(&10)); // Should include minimum
    }
}