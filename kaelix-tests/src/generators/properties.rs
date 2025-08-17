//! Property-based test generators for streaming system validation.
//!
//! This module provides comprehensive property generators for testing the MemoryStreamer
//! system with realistic workloads and edge cases.

use crate::constants::*;
use kaelix_core::{Message, Topic, MessageId, PartitionId, Offset, Timestamp, prelude::*};
use bytes::Bytes;
use proptest::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{Utc, Duration, TimeZone};

/// Generate arbitrary streaming messages with realistic properties.
pub fn arb_message() -> impl Strategy<Value = Message> {
    (arb_topic(), arb_payload(), arb_headers()).prop_map(|(topic, payload, headers)| {
        let mut message = Message::new(topic.as_str(), payload).unwrap();
        
        // Set headers individually since there's no set_headers method
        for (key, value) in headers {
            message.set_header(key, value);
        }
        
        message
    })
}

/// Generate high-throughput message batches for performance testing.
pub fn arb_high_throughput_batch() -> impl Strategy<Value = Vec<Message>> {
    prop::collection::vec(arb_message(), 1000..=10000)
}

/// Generate realistic topic names following streaming conventions.
pub fn arb_topic() -> impl Strategy<Value = Topic> {
    prop_oneof![
        // Standard topic patterns
        r"[a-z]{3,10}\.events\.[a-z]{2,8}",
        r"stream\.[a-z]{2,8}\.[0-9]{1,3}",
        r"data\.[a-z]{3,10}",
        r"metrics\.[a-z]{2,8}",
        r"logs\.[a-z]{2,8}",
        // High-frequency patterns
        r"hf\.[a-z]{2,5}\.[0-9]{1,2}",
        r"realtime\.[a-z]{3,8}",
        // System patterns  
        r"system\.[a-z]{3,8}",
        r"internal\.[a-z]{2,6}",
        // Test patterns
        r"test\.[a-z]{2,8}\.[0-9]{1,3}",
    ].prop_map(|pattern| {
        Topic::new(pattern).unwrap_or_else(|_| Topic::new("fallback.topic").unwrap())
    })
}

/// Generate partition IDs for horizontal scaling scenarios.
pub fn arb_partition() -> impl Strategy<Value = PartitionId> {
    (0u32..=1023u32).prop_map(PartitionId::from)
}

/// Generate realistic message payloads with size distribution.
pub fn arb_payload() -> impl Strategy<Value = Bytes> {
    prop_oneof![
        // Small messages (60% - typical events)
        8 => prop::collection::vec(any::<u8>(), 64..=512).prop_map(Bytes::from),
        // Medium messages (30% - data records)
        3 => prop::collection::vec(any::<u8>(), 512..=4096).prop_map(Bytes::from),
        // Large messages (10% - bulk data)
        1 => prop::collection::vec(any::<u8>(), 4096..=65536).prop_map(Bytes::from),
    ]
}

/// Generate realistic timestamps for temporal ordering tests.
pub fn arb_timestamp() -> impl Strategy<Value = Timestamp> {
    // Generate timestamps within the last hour to recent future
    let now = Utc::now();
    let start = now - Duration::hours(1);
    let end = now + Duration::minutes(10);
    
    (start.timestamp_millis()..=end.timestamp_millis())
        .prop_map(|millis| {
            Utc.timestamp_millis_opt(millis)
                .single()
                .unwrap_or_else(|| Utc::now())
        })
}

/// Generate message headers with common patterns.
pub fn arb_headers() -> impl Strategy<Value = HashMap<String, String>> {
    prop::collection::hash_map(
        prop_oneof![
            Just("content-type".to_string()),
            Just("source".to_string()),
            Just("correlation-id".to_string()),
            Just("trace-id".to_string()),
            Just("span-id".to_string()),
            Just("user-id".to_string()),
            Just("session-id".to_string()),
            Just("version".to_string()),
            Just("priority".to_string()),
            Just("ttl".to_string()),
            r"x-[a-z]{2,8}-[a-z]{2,8}".prop_map(|s| s.to_string()),
        ],
        prop_oneof![
            r"[a-zA-Z0-9-_]{8,32}".prop_map(|s| s.to_string()),
            r"[0-9]+".prop_map(|s| s.to_string()),
            r"v[0-9]+\.[0-9]+".prop_map(|s| s.to_string()),
            Just("application/json".to_string()),
            Just("text/plain".to_string()),
            Just("high".to_string()),
            Just("medium".to_string()),
            Just("low".to_string()),
        ],
        0..=10
    )
}

/// Generate streaming workloads with realistic patterns.
pub fn arb_streaming_workload() -> impl Strategy<Value = StreamingWorkload> {
    (
        arb_workload_type(),
        1000u64..=10_000_000u64, // messages_per_second
        1u32..=3600u32,          // duration_seconds
        prop::collection::vec(arb_topic(), 1..=100),
        arb_message_size_distribution(),
    ).prop_map(|(workload_type, rate, duration, topics, size_dist)| {
        StreamingWorkload {
            workload_type,
            messages_per_second: rate,
            duration_seconds: duration,
            topics,
            message_size_distribution: size_dist,
            partition_strategy: PartitionStrategy::RoundRobin,
            ordering_guarantee: OrderingGuarantee::PerPartition,
        }
    })
}

/// Generate chaos engineering scenarios for fault injection.
pub fn arb_chaos_scenario() -> impl Strategy<Value = ChaosScenario> {
    (
        arb_chaos_type(),
        0.01f64..=0.5f64,  // probability
        1u32..=300u32,     // duration_seconds
        arb_affected_components(),
    ).prop_map(|(chaos_type, probability, duration, components)| {
        ChaosScenario {
            chaos_type,
            probability,
            duration_seconds: duration,
            affected_components: components,
            recovery_strategy: RecoveryStrategy::Automatic,
        }
    })
}

/// Generate workload types for different testing scenarios.
fn arb_workload_type() -> impl Strategy<Value = WorkloadType> {
    prop_oneof![
        Just(WorkloadType::Steady),
        Just(WorkloadType::Burst),
        Just(WorkloadType::Ramp),
        Just(WorkloadType::Spike),
        Just(WorkloadType::Cyclical),
    ]
}

/// Generate message size distributions.
fn arb_message_size_distribution() -> impl Strategy<Value = MessageSizeDistribution> {
    prop_oneof![
        Just(MessageSizeDistribution::Fixed(1024)),
        (256usize..=4096usize).prop_map(MessageSizeDistribution::Fixed),
        (64usize..=512usize, 512usize..=4096usize)
            .prop_map(|(min, max)| MessageSizeDistribution::Uniform { min, max }),
        (512f64..=2048f64, 256f64..=1024f64)
            .prop_map(|(mean, std_dev)| MessageSizeDistribution::Normal { mean, std_dev }),
    ]
}

/// Generate chaos types for fault injection.
fn arb_chaos_type() -> impl Strategy<Value = ChaosType> {
    prop_oneof![
        Just(ChaosType::NetworkPartition),
        Just(ChaosType::NodeFailure),
        Just(ChaosType::SlowNetwork),
        Just(ChaosType::MemoryPressure),
        Just(ChaosType::DiskFull),
        Just(ChaosType::HighCpuLoad),
        Just(ChaosType::MessageCorruption),
        Just(ChaosType::ClockSkew),
    ]
}

/// Generate affected system components.
fn arb_affected_components() -> impl Strategy<Value = Vec<ComponentType>> {
    prop::collection::vec(
        prop_oneof![
            Just(ComponentType::Broker),
            Just(ComponentType::Producer),
            Just(ComponentType::Consumer),
            Just(ComponentType::Network),
            Just(ComponentType::Storage),
        ],
        1..=3
    )
}

// Supporting types for workload and chaos generation

/// Types of streaming workloads.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkloadType {
    /// Steady state load
    Steady,
    /// Burst pattern with high spikes
    Burst,
    /// Gradual ramp up/down
    Ramp,
    /// Sharp spikes
    Spike,
    /// Cyclical patterns
    Cyclical,
}

/// Message size distribution patterns.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageSizeDistribution {
    /// Fixed message size
    Fixed(usize),
    /// Uniform distribution between min and max
    Uniform { min: usize, max: usize },
    /// Normal distribution with mean and standard deviation
    Normal { mean: f64, std_dev: f64 },
}

/// Partitioning strategies for message distribution.
#[derive(Debug, Clone, PartialEq)]
pub enum PartitionStrategy {
    /// Round-robin across partitions
    RoundRobin,
    /// Hash-based partitioning
    Hash,
    /// Random partitioning
    Random,
    /// Key-based partitioning
    KeyBased,
}

/// Ordering guarantee levels.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderingGuarantee {
    /// No ordering guarantees
    None,
    /// Ordering within each partition
    PerPartition,
    /// Global ordering across all partitions
    Global,
}

/// Streaming workload specification.
#[derive(Debug, Clone)]
pub struct StreamingWorkload {
    /// Type of workload pattern
    pub workload_type: WorkloadType,
    /// Target message rate
    pub messages_per_second: u64,
    /// Duration of the workload
    pub duration_seconds: u32,
    /// Topics to use
    pub topics: Vec<Topic>,
    /// Message size distribution
    pub message_size_distribution: MessageSizeDistribution,
    /// Partitioning strategy
    pub partition_strategy: PartitionStrategy,
    /// Ordering requirements
    pub ordering_guarantee: OrderingGuarantee,
}

/// Types of chaos engineering scenarios.
#[derive(Debug, Clone, PartialEq)]
pub enum ChaosType {
    /// Network partition between nodes
    NetworkPartition,
    /// Complete node failure
    NodeFailure,
    /// Slow network conditions
    SlowNetwork,
    /// Memory pressure on nodes
    MemoryPressure,
    /// Disk space exhaustion
    DiskFull,
    /// High CPU load
    HighCpuLoad,
    /// Message corruption
    MessageCorruption,
    /// Clock skew between nodes
    ClockSkew,
}

/// System components that can be affected by chaos.
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentType {
    /// Broker nodes
    Broker,
    /// Producer clients
    Producer,
    /// Consumer clients
    Consumer,
    /// Network infrastructure
    Network,
    /// Storage systems
    Storage,
}

/// Recovery strategies for chaos scenarios.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStrategy {
    /// Automatic recovery
    Automatic,
    /// Manual intervention required
    Manual,
    /// No recovery (permanent failure)
    None,
}

/// Chaos engineering scenario specification.
#[derive(Debug, Clone)]
pub struct ChaosScenario {
    /// Type of chaos to inject
    pub chaos_type: ChaosType,
    /// Probability of chaos occurring
    pub probability: f64,
    /// Duration of chaos in seconds
    pub duration_seconds: u32,
    /// Components affected by chaos
    pub affected_components: Vec<ComponentType>,
    /// Recovery strategy
    pub recovery_strategy: RecoveryStrategy,
}

/// Security-focused property generators.
pub mod security {
    use super::*;
    
    /// Generate authentication tokens for security testing.
    pub fn arb_auth_token() -> impl Strategy<Value = String> {
        prop_oneof![
            r"[A-Za-z0-9]{32}",         // Simple token
            r"Bearer [A-Za-z0-9+/]{128}", // Bearer token
            r"eyJ[A-Za-z0-9+/=]{100,200}\.[A-Za-z0-9+/=]{100,200}\.[A-Za-z0-9+/=]{50,100}", // JWT-like
        ].prop_map(|s| s.to_string())
    }
    
    /// Generate user credentials for authorization testing.
    pub fn arb_user_credentials() -> impl Strategy<Value = UserCredentials> {
        (
            r"[a-zA-Z][a-zA-Z0-9_]{2,15}".prop_map(|s| s.to_string()), // username
            arb_auth_token(),
            prop::collection::vec(
                prop_oneof![
                    Just("read".to_string()),
                    Just("write".to_string()),
                    Just("admin".to_string()),
                    Just("produce".to_string()),
                    Just("consume".to_string()),
                ],
                0..=5
            ),
        ).prop_map(|(username, token, permissions)| {
            UserCredentials {
                username,
                token,
                permissions,
            }
        })
    }
    
    /// User credentials for security testing.
    #[derive(Debug, Clone)]
    pub struct UserCredentials {
        /// Username
        pub username: String,
        /// Authentication token
        pub token: String,
        /// Permissions granted to user
        pub permissions: Vec<String>,
    }
}

/// Performance-focused property generators.
pub mod performance {
    use super::*;
    
    /// Generate performance test scenarios.
    pub fn arb_performance_scenario() -> impl Strategy<Value = PerformanceScenario> {
        (
            1_000u64..=10_000_000u64,    // target_throughput
            1u64..=1000u64,              // max_latency_micros
            1u32..=60u32,                // duration_seconds
            1u32..=1000u32,              // concurrent_clients
            arb_message_size_distribution(),
        ).prop_map(|(throughput, latency, duration, clients, size_dist)| {
            PerformanceScenario {
                target_throughput: throughput,
                max_latency_micros: latency,
                duration_seconds: duration,
                concurrent_clients: clients,
                message_size_distribution: size_dist,
            }
        })
    }
    
    /// Performance test scenario specification.
    #[derive(Debug, Clone)]
    pub struct PerformanceScenario {
        /// Target messages per second
        pub target_throughput: u64,
        /// Maximum acceptable latency in microseconds
        pub max_latency_micros: u64,
        /// Test duration in seconds
        pub duration_seconds: u32,
        /// Number of concurrent clients
        pub concurrent_clients: u32,
        /// Message size distribution
        pub message_size_distribution: MessageSizeDistribution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::test_runner::TestRunner;
    
    #[test]
    fn test_message_generation() {
        let mut runner = TestRunner::default();
        let message_tree = arb_message().new_tree(&mut runner).unwrap();
        let message = message_tree.current();
        
        assert!(!message.topic.as_str().is_empty());
        assert!(!message.payload.is_empty());
    }
    
    #[test]
    fn test_batch_generation() {
        let mut runner = TestRunner::default();
        let batch_tree = arb_high_throughput_batch().new_tree(&mut runner).unwrap();
        let batch = batch_tree.current();
        
        assert!(batch.len() >= 1000);
        assert!(batch.len() <= 10000);
    }
    
    #[test]
    fn test_topic_generation() {
        let mut runner = TestRunner::default();
        let topic_tree = arb_topic().new_tree(&mut runner).unwrap();
        let topic = topic_tree.current();
        
        assert!(!topic.as_str().is_empty());
        assert!(topic.as_str().len() <= 255);
    }
    
    #[test]
    fn test_streaming_workload_generation() {
        let mut runner = TestRunner::default();
        let workload_tree = arb_streaming_workload().new_tree(&mut runner).unwrap();
        let workload = workload_tree.current();
        
        assert!(workload.messages_per_second >= 1000);
        assert!(workload.duration_seconds >= 1);
        assert!(!workload.topics.is_empty());
    }
    
    #[test]
    fn test_chaos_scenario_generation() {
        let mut runner = TestRunner::default();
        let chaos_tree = arb_chaos_scenario().new_tree(&mut runner).unwrap();
        let chaos = chaos_tree.current();
        
        assert!(chaos.probability > 0.0);
        assert!(chaos.probability <= 0.5);
        assert!(chaos.duration_seconds >= 1);
        assert!(!chaos.affected_components.is_empty());
    }
}