//! Comprehensive Test Suite for Cluster Message System
//!
//! This module provides extensive testing for the cluster message system,
//! ensuring reliability, performance, and correctness across all message types and scenarios.

use super::*;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// MessageFactory for generating test data and scenarios
pub struct MessageFactory;

impl MessageFactory {
    /// Create a variety of test messages across different categories
    pub fn create_test_messages(count: usize) -> Vec<ClusterMessage> {
        let source = NodeId::generate();
        let destination = NodeId::generate();

        let payloads = vec![
            // Consensus messages
            MessagePayload::RequestVote {
                term: 1,
                candidate_id: source,
                last_log_index: 0,
                last_log_term: 0,
            },
            MessagePayload::RequestVoteResponse { term: 1, vote_granted: true },
            MessagePayload::AppendEntries { term: 1, leader_id: source, entries: vec![1, 2, 3] },
            MessagePayload::AppendEntriesResponse { term: 1, success: true },
            // Membership messages
            MessagePayload::JoinRequest { node_id: source },
            MessagePayload::JoinResponse { accepted: true },
            MessagePayload::LeaveRequest { node_id: source },
            // Discovery messages
            MessagePayload::Ping { node_id: source },
            MessagePayload::Pong { node_id: source },
            // Health messages
            MessagePayload::HealthCheck,
            MessagePayload::HealthResponse { status: "healthy".to_string() },
        ];

        (0..count)
            .map(|_| {
                let payload = payloads[fastrand::usize(..payloads.len())].clone();
                ClusterMessage::new(source, destination, payload)
            })
            .collect()
    }
}

/// Basic testing module for core functionality validation
mod basic_tests {
    use super::*;

    #[test]
    fn test_message_serialization_roundtrip() {
        let source = NodeId::generate();
        let destination = NodeId::generate();
        let payload = MessagePayload::HealthCheck;
        let msg = ClusterMessage::new(source, destination, payload);

        // JSON serialization
        let json = serde_json::to_string(&msg).expect("JSON serialization failed");
        let json_deserialized: ClusterMessage =
            serde_json::from_str(&json).expect("JSON deserialization failed");
        assert_eq!(msg, json_deserialized);

        // Binary serialization
        let binary = bincode::serialize(&msg).expect("Binary serialization failed");
        let binary_deserialized: ClusterMessage =
            bincode::deserialize(&binary).expect("Binary deserialization failed");
        assert_eq!(msg, binary_deserialized);
    }

    #[test]
    fn test_message_id_uniqueness() {
        let source = NodeId::generate();
        let destination = NodeId::generate();
        let payload = MessagePayload::HealthCheck;

        const COUNT: usize = 1000;
        let message_ids: HashSet<MessageId> = (0..COUNT)
            .map(|_| ClusterMessage::new(source, destination, payload.clone()).header.message_id)
            .collect();

        assert_eq!(message_ids.len(), COUNT);
    }

    #[test]
    fn test_timestamp_monotonicity() {
        let source = NodeId::generate();
        let destination = NodeId::generate();
        let payload = MessagePayload::HealthCheck;

        const COUNT: usize = 100;
        let timestamps: Vec<u64> = (0..COUNT)
            .map(|_| ClusterMessage::new(source, destination, payload.clone()).header.timestamp)
            .collect();

        for window in timestamps.windows(2) {
            assert!(window[1] >= window[0], "Timestamps must be monotonically increasing");
        }
    }
}

/// Performance and stress testing module
mod performance_tests {
    use super::*;

    /// Benchmark message creation performance
    #[test]
    fn message_creation_performance() {
        let source = NodeId::generate();
        let destination = NodeId::generate();
        let payload = MessagePayload::HealthCheck;

        let start = Instant::now();
        const ITERATIONS: usize = 100_000;

        for _ in 0..ITERATIONS {
            let _msg = ClusterMessage::new(source, destination, payload.clone());
        }

        let duration = start.elapsed();
        let avg_time_ns = duration.as_nanos() / ITERATIONS as u128;

        println!(
            "Message Creation Performance: {:.2} ns/message, Total: {:.2} ms",
            avg_time_ns,
            duration.as_secs_f64() * 1000.0
        );

        assert!(avg_time_ns < 500, "Message creation must be < 500 ns");
    }

    /// Benchmark serialization performance
    #[test]
    fn serialization_performance() {
        let messages = MessageFactory::create_test_messages(1000);

        // JSON Serialization
        let json_start = Instant::now();
        let _json_serialized: Vec<String> =
            messages.iter().map(|msg| serde_json::to_string(msg).unwrap()).collect();
        let json_duration = json_start.elapsed();

        // Binary Serialization
        let binary_start = Instant::now();
        let _binary_serialized: Vec<Vec<u8>> =
            messages.iter().map(|msg| bincode::serialize(msg).unwrap()).collect();
        let binary_duration = binary_start.elapsed();

        println!(
            "Serialization Performance:\\n\
             JSON: {:.2} ms total, {:.2} ns/message\\n\
             Binary: {:.2} ms total, {:.2} ns/message",
            json_duration.as_secs_f64() * 1000.0,
            json_duration.as_nanos() as f64 / messages.len() as f64,
            binary_duration.as_secs_f64() * 1000.0,
            binary_duration.as_nanos() as f64 / messages.len() as f64
        );

        assert!(json_duration < Duration::from_millis(50), "JSON serialization too slow");
        assert!(binary_duration < Duration::from_millis(30), "Binary serialization too slow");
    }
}

/// Comprehensive error handling and edge case testing
mod error_handling_tests {
    use super::*;

    /// Test malformed serialization data handling
    #[test]
    fn malformed_serialization_handling() {
        // Corrupt JSON data
        let corrupt_json = r#"{"header":{"message_id":1234}, "payload": "Invalid"}"#;
        let json_result = serde_json::from_str::<ClusterMessage>(corrupt_json);
        assert!(json_result.is_err(), "Must reject malformed JSON");

        // Corrupt binary data
        let corrupt_binary = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let binary_result = bincode::deserialize::<ClusterMessage>(&corrupt_binary);
        assert!(binary_result.is_err(), "Must reject malformed binary data");
    }

    /// Test maximum message size handling
    #[test]
    fn message_size_limits() {
        let source = NodeId::generate();
        let destination = NodeId::generate();

        // Create a large payload
        let large_entries = vec![0u8; 1_048_576]; // 1 MB
        let large_payload =
            MessagePayload::AppendEntries { term: 1, leader_id: source, entries: large_entries };

        let large_message = ClusterMessage::new(source, destination, large_payload);

        // Serialize and check size
        let json_size =
            serde_json::to_string(&large_message).expect("JSON serialization failed").len();
        let binary_size =
            bincode::serialize(&large_message).expect("Binary serialization failed").len();

        println!(
            "Large Message Sizes:\\nJSON: {} bytes\\nBinary: {} bytes",
            json_size, binary_size
        );

        assert!(json_size <= 2_097_152, "JSON message too large (>2MB)");
        assert!(binary_size <= 1_048_576, "Binary message too large (>1MB)");
    }
}

// Integration test with other system types
mod integration_tests {
    use super::*;
    use crate::types::NodeId;

    #[test]
    fn message_node_id_integration() {
        let node_id = NodeId::generate();
        let destination = NodeId::generate();

        let message = ClusterMessage::new(node_id, destination, MessagePayload::Ping { node_id });

        assert_eq!(message.header.source, node_id);
        assert_eq!(message.header.destination, destination);
    }
}

// Ensure all tests run concurrently for performance
#[cfg(test)]
mod concurrent_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn concurrent_message_generation() {
        const THREAD_COUNT: usize = 8;
        const MESSAGES_PER_THREAD: usize = 1000;

        let counter = Arc::new(AtomicUsize::new(0));
        let source = NodeId::generate();
        let destination = NodeId::generate();

        std::thread::scope(|s| {
            let threads: Vec<_> = (0..THREAD_COUNT)
                .map(|_| {
                    let local_counter = Arc::clone(&counter);
                    s.spawn(move || {
                        for _ in 0..MESSAGES_PER_THREAD {
                            let _message = ClusterMessage::new(
                                source,
                                destination,
                                MessagePayload::HealthCheck,
                            );
                            local_counter.fetch_add(1, Ordering::Relaxed);
                        }
                    })
                })
                .collect();

            for thread in threads {
                thread.join().expect("Thread failed");
            }
        });

        assert_eq!(
            counter.load(Ordering::Relaxed),
            THREAD_COUNT * MESSAGES_PER_THREAD,
            "All messages must be generated concurrently"
        );
    }
}
