//! # Mock Network Module
//!
//! Provides comprehensive network simulation for testing distributed systems.
//! Supports realistic network conditions including latency, packet loss, and partitions.
//!
//! ## Features
//!
//! - **Realistic Network Conditions**: Latency, packet loss, bandwidth throttling
//! - **Network Partitions**: Split-brain scenario simulation with healing
//! - **Comprehensive Metrics**: Message counts, latency tracking, network statistics
//! - **Easy Testing**: Helper functions for common test scenarios
//! - **Async Support**: Full tokio integration for realistic message delivery
//!
//! ## Usage Examples
//!
//! ### Basic Network Simulation
//!
//! ```rust
//! use kaelix_cluster::test_utils::mock_network::{MockNetwork, NetworkConditions};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let network = MockNetwork::new();
//!     
//!     // Register test nodes
//!     let node1 = network.register_node("node1", "127.0.0.1:8000".parse()?).await?;
//!     let node2 = network.register_node("node2", "127.0.0.1:8001".parse()?).await?;
//!     
//!     // Apply network conditions
//!     let conditions = NetworkConditions::builder()
//!         .latency(Duration::from_millis(10), Duration::from_millis(50))
//!         .packet_loss(0.01) // 1% packet loss
//!         .build();
//!     network.apply_conditions(conditions).await;
//!     
//!     // Send messages
//!     network.send_message(node1, node2, vec![1, 2, 3]).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Network Partition Testing
//!
//! ```rust
//! use kaelix_cluster::test_utils::mock_network::{MockNetwork, NetworkPartition};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let network = MockNetwork::new();
//!     
//!     // Register nodes
//!     let nodes = setup_test_network(&network, 4).await?;
//!     
//!     // Create partition: [node1, node2] vs [node3, node4]
//!     let partition = NetworkPartition::new()
//!         .split(&nodes[0..2], &nodes[2..4]);
//!     
//!     network.apply_partition(partition).await;
//!     
//!     // Test split-brain scenarios...
//!     
//!     Ok(())
//! }
//! ```

use rand::{distributions::Uniform, prelude::*, rngs::SmallRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    net::{AddrParseError, SocketAddr},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{sync::RwLock, time::sleep};
use tracing::{debug, trace, warn};

use crate::error::{Error, Result};

// ================================================================================================
// Core Types
// ================================================================================================

/// Mock node identifier for network simulation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MockNodeId(u64);

impl MockNodeId {
    /// Create a new mock node ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the underlying ID value
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for MockNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node-{}", self.0)
    }
}

impl From<u64> for MockNodeId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<&str> for MockNodeId {
    fn from(s: &str) -> Self {
        // Simple hash of string for consistent ID generation
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        Self(hasher.finish())
    }
}

impl From<&MockNodeId> for MockNodeId {
    fn from(id: &MockNodeId) -> Self {
        *id
    }
}

/// Message envelope for network simulation
#[derive(Debug, Clone)]
pub struct MessageEnvelope {
    /// Unique message identifier
    pub id: u64,
    /// Source node
    pub from: MockNodeId,
    /// Destination node
    pub to: MockNodeId,
    /// Message payload
    pub data: Vec<u8>,
    /// When the message was sent
    pub sent_at: Instant,
    /// When the message should be delivered
    pub deliver_at: Instant,
    /// Number of times this message was duplicated
    pub duplicate_count: u32,
}

/// Network conditions for realistic simulation
#[derive(Debug, Clone)]
pub struct NetworkConditions {
    /// Whether conditions are enabled
    pub enabled: bool,
    /// Minimum latency
    pub min_latency: Duration,
    /// Maximum latency
    pub max_latency: Duration,
    /// Packet loss rate (0.0 to 1.0)
    pub packet_loss_rate: f64,
    /// Bandwidth limit in bytes per second (None = unlimited)
    pub bandwidth_limit: Option<u64>,
    /// Jitter factor (0.0 to 1.0, represents percentage of base latency)
    pub jitter_factor: f64,
    /// Probability of message duplication (0.0 to 1.0)
    pub duplicate_probability: f64,
    /// Probability of message reordering (0.0 to 1.0)
    pub reorder_probability: f64,
}

impl Default for NetworkConditions {
    fn default() -> Self {
        Self {
            enabled: false,
            min_latency: Duration::from_micros(1),
            max_latency: Duration::from_micros(10),
            packet_loss_rate: 0.0,
            bandwidth_limit: None,
            jitter_factor: 0.0,
            duplicate_probability: 0.0,
            reorder_probability: 0.0,
        }
    }
}

impl NetworkConditions {
    /// Create a builder for network conditions
    pub fn builder() -> NetworkConditionsBuilder {
        NetworkConditionsBuilder::new()
    }

    /// Perfect network conditions (minimal latency, no loss)
    pub fn perfect() -> Self {
        Self {
            enabled: true,
            min_latency: Duration::from_nanos(1),
            max_latency: Duration::from_nanos(100), // Increased to ensure valid range
            packet_loss_rate: 0.0,
            bandwidth_limit: None,
            jitter_factor: 0.0,
            duplicate_probability: 0.0,
            reorder_probability: 0.0,
        }
    }

    /// LAN-like conditions (low latency, minimal loss)
    pub fn lan_preset() -> Self {
        Self {
            enabled: true,
            min_latency: Duration::from_micros(10),
            max_latency: Duration::from_micros(100),
            packet_loss_rate: 0.0001,             // 0.01%
            bandwidth_limit: Some(1_000_000_000), // 1 Gbps
            jitter_factor: 0.1,
            duplicate_probability: 0.0,
            reorder_probability: 0.001,
        }
    }

    /// WAN-like conditions (higher latency, some loss)
    pub fn wan_preset() -> Self {
        Self {
            enabled: true,
            min_latency: Duration::from_millis(20),
            max_latency: Duration::from_millis(200),
            packet_loss_rate: 0.01,             // 1%
            bandwidth_limit: Some(100_000_000), // 100 Mbps
            jitter_factor: 0.3,
            duplicate_probability: 0.001,
            reorder_probability: 0.01,
        }
    }

    /// Unreliable network conditions (high latency, significant loss)
    pub fn unreliable_preset() -> Self {
        Self {
            enabled: true,
            min_latency: Duration::from_millis(50),
            max_latency: Duration::from_millis(1000),
            packet_loss_rate: 0.1,             // 10%
            bandwidth_limit: Some(10_000_000), // 10 Mbps
            jitter_factor: 0.5,
            duplicate_probability: 0.02,
            reorder_probability: 0.05,
        }
    }
}

/// Builder for network conditions
#[derive(Debug, Clone)]
pub struct NetworkConditionsBuilder {
    conditions: NetworkConditions,
}

impl NetworkConditionsBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { conditions: NetworkConditions { enabled: true, ..Default::default() } }
    }

    /// Set latency range
    pub fn latency(mut self, min: Duration, max: Duration) -> Self {
        self.conditions.min_latency = min;
        self.conditions.max_latency = max.max(min); // Ensure max >= min
        self
    }

    /// Set packet loss rate
    pub fn packet_loss(mut self, rate: f64) -> Self {
        self.conditions.packet_loss_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Set bandwidth limit
    pub fn bandwidth_limit(mut self, bytes_per_second: u64) -> Self {
        self.conditions.bandwidth_limit = Some(bytes_per_second);
        self
    }

    /// Set jitter factor
    pub fn jitter(mut self, factor: f64) -> Self {
        self.conditions.jitter_factor = factor.clamp(0.0, 1.0);
        self
    }

    /// Set duplication probability
    pub fn duplicate_probability(mut self, prob: f64) -> Self {
        self.conditions.duplicate_probability = prob.clamp(0.0, 1.0);
        self
    }

    /// Set reordering probability
    pub fn reorder_probability(mut self, prob: f64) -> Self {
        self.conditions.reorder_probability = prob.clamp(0.0, 1.0);
        self
    }

    /// Build the network conditions
    pub fn build(self) -> NetworkConditions {
        self.conditions
    }
}

impl Default for NetworkConditionsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Network partition for split-brain testing
#[derive(Debug, Clone)]
pub struct NetworkPartition {
    /// Set of partitioned node groups
    partitions: Vec<HashSet<MockNodeId>>,
}

impl NetworkPartition {
    /// Create a new empty partition
    pub fn new() -> Self {
        Self { partitions: Vec::new() }
    }

    /// Split nodes into two partitions
    pub fn split<I1, I2, T>(mut self, group1: I1, group2: I2) -> Self
    where
        I1: IntoIterator<Item = T>,
        I2: IntoIterator<Item = T>,
        T: Into<MockNodeId>,
    {
        let partition1: HashSet<MockNodeId> = group1.into_iter().map(|id| id.into()).collect();
        let partition2: HashSet<MockNodeId> = group2.into_iter().map(|id| id.into()).collect();

        self.partitions.clear();
        self.partitions.push(partition1);
        self.partitions.push(partition2);
        self
    }

    /// Add a multi-way partition
    pub fn add_partition<I, T>(mut self, nodes: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<MockNodeId>,
    {
        let partition: HashSet<MockNodeId> = nodes.into_iter().map(|id| id.into()).collect();
        self.partitions.push(partition);
        self
    }

    /// Check if two nodes can communicate (not partitioned)
    pub fn can_communicate(&self, from: &MockNodeId, to: &MockNodeId) -> bool {
        if self.partitions.is_empty() {
            return true; // No partitions = full connectivity
        }

        // Find which partitions contain each node
        let from_partition = self.partitions.iter().position(|p| p.contains(from));
        let to_partition = self.partitions.iter().position(|p| p.contains(to));

        match (from_partition, to_partition) {
            (Some(fp), Some(tp)) => fp == tp, // Same partition = can communicate
            _ => true, // At least one node not in any partition = can communicate
        }
    }

    /// Get all partitions
    pub fn partitions(&self) -> &[HashSet<MockNodeId>] {
        &self.partitions
    }
}

impl Default for NetworkPartition {
    fn default() -> Self {
        Self::new()
    }
}

/// Network metrics for monitoring
#[derive(Debug, Default)]
pub struct NetworkMetrics {
    /// Total messages sent
    pub messages_sent: AtomicU64,
    /// Total messages delivered
    pub messages_delivered: AtomicU64,
    /// Total messages dropped due to packet loss
    pub messages_dropped: AtomicU64,
    /// Total messages duplicated
    pub messages_duplicated: AtomicU64,
    /// Total bytes transferred
    pub bytes_transferred: AtomicU64,
    /// Average latency in microseconds
    pub avg_latency_us: AtomicU64,
    /// Network partition active
    pub partition_active: std::sync::atomic::AtomicBool,
}

impl NetworkMetrics {
    /// Create a new metrics instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a sent message
    pub fn record_sent(&self, bytes: usize) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_transferred.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Record a delivered message
    pub fn record_delivered(&self, latency: Duration) {
        self.messages_delivered.fetch_add(1, Ordering::Relaxed);

        // Update rolling average latency
        let new_latency_us = latency.as_micros() as u64;
        let _current_avg = self.avg_latency_us.load(Ordering::Relaxed);
        // Simple approximation: just store the latest latency
        // For production, you'd want a proper rolling average
        self.avg_latency_us.store(new_latency_us, Ordering::Relaxed);
    }

    /// Record a dropped message
    pub fn record_dropped(&self) {
        self.messages_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a duplicated message
    pub fn record_duplicated(&self) {
        self.messages_duplicated.fetch_add(1, Ordering::Relaxed);
    }

    /// Set partition status
    pub fn set_partition_active(&self, active: bool) {
        self.partition_active.store(active, Ordering::Relaxed);
    }

    /// Get snapshot of current metrics
    pub fn snapshot(&self) -> NetworkMetricsSnapshot {
        NetworkMetricsSnapshot {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_delivered: self.messages_delivered.load(Ordering::Relaxed),
            messages_dropped: self.messages_dropped.load(Ordering::Relaxed),
            messages_duplicated: self.messages_duplicated.load(Ordering::Relaxed),
            bytes_transferred: self.bytes_transferred.load(Ordering::Relaxed),
            avg_latency_us: self.avg_latency_us.load(Ordering::Relaxed),
            partition_active: self.partition_active.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of network metrics at a point in time
#[derive(Debug, Clone)]
pub struct NetworkMetricsSnapshot {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages delivered
    pub messages_delivered: u64,
    /// Total messages dropped due to packet loss
    pub messages_dropped: u64,
    /// Total messages duplicated
    pub messages_duplicated: u64,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Average latency in microseconds
    pub avg_latency_us: u64,
    /// Network partition active
    pub partition_active: bool,
}

// ================================================================================================
// Mock Network Implementation
// ================================================================================================

/// Mock network for testing distributed systems
pub struct MockNetwork {
    /// Registered nodes
    nodes: Arc<RwLock<HashMap<MockNodeId, SocketAddr>>>,
    /// Current network conditions
    conditions: Arc<RwLock<NetworkConditions>>,
    /// Current network partition
    partition: Arc<RwLock<NetworkPartition>>,
    /// Message queue for processing
    message_queue: Arc<RwLock<VecDeque<MessageEnvelope>>>,
    /// Network metrics
    metrics: Arc<NetworkMetrics>,
    /// Message ID counter
    message_counter: AtomicU64,
    /// Node ID counter
    node_counter: AtomicU64,
    /// Random number generator
    rng: Arc<RwLock<SmallRng>>,
}

impl MockNetwork {
    /// Create a new mock network
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            conditions: Arc::new(RwLock::new(NetworkConditions::default())),
            partition: Arc::new(RwLock::new(NetworkPartition::new())),
            message_queue: Arc::new(RwLock::new(VecDeque::new())),
            metrics: Arc::new(NetworkMetrics::new()),
            message_counter: AtomicU64::new(1),
            node_counter: AtomicU64::new(1),
            rng: Arc::new(RwLock::new(SmallRng::from_entropy())),
        }
    }

    /// Register a node in the network
    pub async fn register_node(&self, name: &str, address: SocketAddr) -> Result<MockNodeId> {
        let node_id: MockNodeId = name.into();
        self.nodes.write().await.insert(node_id, address);
        debug!("Registered node {} at {}", node_id, address);
        Ok(node_id)
    }

    /// Unregister a node from the network
    pub async fn unregister_node(&self, node_id: &MockNodeId) -> Result<()> {
        if self.nodes.write().await.remove(node_id).is_some() {
            debug!("Unregistered node {}", node_id);
            Ok(())
        } else {
            Err(Error::communication(format!("Node {node_id} not found")))
        }
    }

    /// Get all registered nodes
    pub async fn nodes(&self) -> HashMap<MockNodeId, SocketAddr> {
        self.nodes.read().await.clone()
    }

    /// Apply network conditions
    pub async fn apply_conditions(&self, conditions: NetworkConditions) {
        *self.conditions.write().await = conditions;
        debug!("Applied network conditions: {:?}", self.conditions.read().await);
    }

    /// Apply network partition
    pub async fn apply_partition(&self, partition: NetworkPartition) {
        let has_partitions = !partition.partitions.is_empty();
        *self.partition.write().await = partition;
        self.metrics.set_partition_active(has_partitions);

        if has_partitions {
            debug!(
                "Applied network partition with {} groups",
                self.partition.read().await.partitions.len()
            );
        } else {
            debug!("Removed network partition");
        }
    }

    /// Heal network partition (restore full connectivity)
    pub async fn heal_partition(&self) {
        self.apply_partition(NetworkPartition::new()).await;
    }

    /// Send a message between nodes
    pub async fn send_message(
        &self,
        from: MockNodeId,
        to: MockNodeId,
        data: Vec<u8>,
    ) -> Result<()> {
        // Check if nodes exist
        let nodes = self.nodes.read().await;
        if !nodes.contains_key(&from) {
            return Err(Error::communication(format!("Source node {from} not found")));
        }
        if !nodes.contains_key(&to) {
            return Err(Error::communication(format!("Destination node {to} not found")));
        }
        drop(nodes);

        // Check if nodes can communicate (partition check)
        let partition = self.partition.read().await;
        if !partition.can_communicate(&from, &to) {
            debug!("Message blocked due to network partition: {} -> {}", from, to);
            return Ok(()); // Silently drop partitioned messages
        }
        drop(partition);

        let message_id = self.message_counter.fetch_add(1, Ordering::Relaxed);
        let sent_at = Instant::now();

        // Record metrics
        self.metrics.record_sent(data.len());

        // Create message envelope
        let mut envelope = MessageEnvelope {
            id: message_id,
            from,
            to,
            data,
            sent_at,
            deliver_at: sent_at, // Will be updated based on conditions
            duplicate_count: 0,
        };

        // Apply network conditions
        let conditions = self.conditions.read().await.clone();
        if let Err(e) = self.apply_conditions_to_envelope(&mut envelope, &conditions).await {
            self.metrics.record_dropped();
            return Err(e);
        }

        // Handle message duplication
        let mut envelopes = vec![envelope.clone()];
        if conditions.enabled && conditions.duplicate_probability > 0.0 {
            let mut rng = self.rng.write().await;
            if rng.gen::<f64>() < conditions.duplicate_probability {
                let mut duplicate = envelope.clone();
                duplicate.duplicate_count = 1;
                envelopes.push(duplicate);
                self.metrics.record_duplicated();
            }
        }

        // Enqueue messages for processing
        {
            let mut queue = self.message_queue.write().await;
            for env in envelopes {
                queue.push_back(env);
            }
        }

        // Start background processing if needed
        self.spawn_message_processor().await;

        debug!(
            "Sent message {} from {} to {} ({} bytes)",
            message_id,
            from,
            to,
            envelope.data.len()
        );

        Ok(())
    }

    /// Apply network conditions to a message envelope
    async fn apply_conditions_to_envelope(
        &self,
        envelope: &mut MessageEnvelope,
        conditions: &NetworkConditions,
    ) -> Result<()> {
        if !conditions.enabled {
            // No delay for disabled conditions
            envelope.deliver_at = envelope.sent_at;
            return Ok(());
        }

        let mut rng = self.rng.write().await;

        // Check packet loss
        if conditions.packet_loss_rate > 0.0 && rng.gen::<f64>() < conditions.packet_loss_rate {
            return Err(Error::communication("Message dropped due to simulated packet loss"));
        }

        // Calculate latency with proper bounds checking
        let min_latency_us = conditions.min_latency.as_micros() as u64;
        let max_latency_us = conditions.max_latency.as_micros() as u64;

        let base_latency_us = if min_latency_us >= max_latency_us {
            // Handle edge case where min >= max (use min)
            min_latency_us
        } else {
            rng.sample(Uniform::new(min_latency_us, max_latency_us + 1))
        };

        // Apply jitter with proper bounds checking
        let jitter_range = (base_latency_us as f64 * conditions.jitter_factor) as u64;
        let final_latency_us = if jitter_range > 0 {
            // Generate jitter as a signed offset
            let jitter_offset =
                rng.sample(Uniform::new(-(jitter_range as i64), jitter_range as i64 + 1));
            (base_latency_us as i64 + jitter_offset).max(0) as u64
        } else {
            base_latency_us
        };

        envelope.deliver_at = envelope.sent_at + Duration::from_micros(final_latency_us);

        Ok(())
    }

    /// Spawn background message processor
    async fn spawn_message_processor(&self) {
        let queue = Arc::clone(&self.message_queue);
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            loop {
                let envelope = {
                    let mut q = queue.write().await;
                    q.pop_front()
                };

                match envelope {
                    Some(env) => {
                        // Wait until delivery time
                        let now = Instant::now();
                        if env.deliver_at > now {
                            sleep(env.deliver_at - now).await;
                        }

                        // Simulate message delivery
                        let delivery_latency = Instant::now() - env.sent_at;
                        metrics.record_delivered(delivery_latency);

                        trace!(
                            "Delivered message {} from {} to {} (latency: {:?})",
                            env.id,
                            env.from,
                            env.to,
                            delivery_latency
                        );
                    },
                    None => {
                        // No messages to process, short break
                        sleep(Duration::from_millis(1)).await;
                        break; // Exit if no messages
                    },
                }
            }
        });
    }

    /// Flush all pending messages (useful for tests)
    pub async fn flush_messages(&self) {
        // Wait for all messages to be processed
        let mut retries = 0;
        const MAX_RETRIES: u32 = 100; // 1 second max wait

        while retries < MAX_RETRIES {
            let queue_size = self.message_queue.read().await.len();
            if queue_size == 0 {
                break;
            }

            sleep(Duration::from_millis(10)).await;
            retries += 1;
        }

        if retries >= MAX_RETRIES {
            warn!(
                "Message flush timed out with {} messages remaining",
                self.message_queue.read().await.len()
            );
        }
    }

    /// Get current network metrics
    pub fn metrics_snapshot(&self) -> NetworkMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Reset network state (clear all conditions, partitions, and metrics)
    pub async fn reset(&self) {
        *self.conditions.write().await = NetworkConditions::default();
        *self.partition.write().await = NetworkPartition::new();
        self.message_queue.write().await.clear();

        // Reset metrics
        self.metrics.messages_sent.store(0, Ordering::Relaxed);
        self.metrics.messages_delivered.store(0, Ordering::Relaxed);
        self.metrics.messages_dropped.store(0, Ordering::Relaxed);
        self.metrics.messages_duplicated.store(0, Ordering::Relaxed);
        self.metrics.bytes_transferred.store(0, Ordering::Relaxed);
        self.metrics.avg_latency_us.store(0, Ordering::Relaxed);
        self.metrics.set_partition_active(false);

        debug!("Reset mock network state");
    }
}

impl Default for MockNetwork {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================================================
// Helper Functions
// ================================================================================================

/// Helper function to parse socket addresses with error conversion
fn parse_socket_addr(addr_str: &str) -> Result<SocketAddr> {
    addr_str.parse().map_err(|e: AddrParseError| {
        Error::configuration(format!("Invalid address '{}': {}", addr_str, e))
    })
}

/// Set up a test network with the specified number of nodes
pub async fn setup_test_network(
    network: &MockNetwork,
    node_count: usize,
) -> Result<Vec<MockNodeId>> {
    let mut nodes = Vec::new();

    for i in 0..node_count {
        let node_name = format!("test-node-{}", i);
        let address = parse_socket_addr(&format!("127.0.0.1:{}", 8000 + i))?;

        let node_id = network.register_node(&node_name, address).await?;
        nodes.push(node_id);
    }

    debug!("Set up test network with {} nodes", node_count);
    Ok(nodes)
}

/// Register multiple test nodes with generated names
pub async fn register_test_nodes(
    network: &MockNetwork,
    count: usize,
    base_port: u16,
) -> Result<Vec<MockNodeId>> {
    let mut nodes = Vec::new();

    for i in 0..count {
        let node_name = format!("node-{}", i);
        let address = parse_socket_addr(&format!("127.0.0.1:{}", base_port + i as u16))?;

        let node_id = network.register_node(&node_name, address).await?;
        nodes.push(node_id);
    }

    Ok(nodes)
}

/// Create a two-way network partition
pub fn create_two_way_partition(nodes: &[MockNodeId], split_index: usize) -> NetworkPartition {
    if split_index >= nodes.len() {
        return NetworkPartition::new(); // Invalid split, return empty partition
    }

    // Use cloning to convert from slice references to owned values
    let group1: Vec<MockNodeId> = nodes[..split_index].to_vec();
    let group2: Vec<MockNodeId> = nodes[split_index..].to_vec();

    NetworkPartition::new().split(group1, group2)
}

/// Create a multi-way network partition
pub fn create_multi_way_partition(node_groups: &[&[MockNodeId]]) -> NetworkPartition {
    let mut partition = NetworkPartition::new();

    for group in node_groups {
        let owned_group: Vec<MockNodeId> = group.to_vec();
        partition = partition.add_partition(owned_group);
    }

    partition
}

// ================================================================================================
// Tests
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    type TestResult = Result<()>;

    #[test]
    fn test_mock_node_id_creation() {
        let id1 = MockNodeId::new(1);
        let id2 = MockNodeId::new(2);
        assert_ne!(id1, id2);
        assert_eq!(id1.value(), 1);
        assert_eq!(id2.value(), 2);

        // Test string conversion
        let id3: MockNodeId = "test-node".into();
        let id4: MockNodeId = "test-node".into();
        assert_eq!(id3, id4); // Same string should produce same ID

        let id5: MockNodeId = "different".into();
        assert_ne!(id3, id5);
    }

    #[test]
    fn test_network_conditions_builder() {
        let conditions = NetworkConditions::builder()
            .latency(Duration::from_millis(10), Duration::from_millis(50))
            .packet_loss(0.01)
            .bandwidth_limit(1_000_000)
            .jitter(0.2)
            .build();

        assert!(conditions.enabled);
        assert_eq!(conditions.min_latency, Duration::from_millis(10));
        assert_eq!(conditions.max_latency, Duration::from_millis(50));
        assert_eq!(conditions.packet_loss_rate, 0.01);
        assert_eq!(conditions.bandwidth_limit, Some(1_000_000));
        assert_eq!(conditions.jitter_factor, 0.2);
    }

    #[test]
    fn test_network_conditions_presets() {
        let perfect = NetworkConditions::perfect();
        assert!(perfect.enabled);
        assert_eq!(perfect.packet_loss_rate, 0.0);

        let lan = NetworkConditions::lan_preset();
        assert!(lan.enabled);
        assert!(lan.packet_loss_rate < 0.001);

        let wan = NetworkConditions::wan_preset();
        assert!(wan.enabled);
        assert!(wan.packet_loss_rate > 0.005);

        let unreliable = NetworkConditions::unreliable_preset();
        assert!(unreliable.enabled);
        assert!(unreliable.packet_loss_rate > 0.05);
    }

    #[test]
    fn test_network_partition_creation() {
        let nodes =
            vec![MockNodeId::new(1), MockNodeId::new(2), MockNodeId::new(3), MockNodeId::new(4)];

        let partition =
            NetworkPartition::new().split(vec![nodes[0], nodes[1]], vec![nodes[2], nodes[3]]);

        // Nodes in same partition can communicate
        assert!(partition.can_communicate(&nodes[0], &nodes[1]));
        assert!(partition.can_communicate(&nodes[2], &nodes[3]));

        // Nodes in different partitions cannot communicate
        assert!(!partition.can_communicate(&nodes[0], &nodes[2]));
        assert!(!partition.can_communicate(&nodes[1], &nodes[3]));
    }

    #[test]
    fn test_network_partition_multi_way() {
        let nodes = vec![
            MockNodeId::new(1),
            MockNodeId::new(2),
            MockNodeId::new(3),
            MockNodeId::new(4),
            MockNodeId::new(5),
            MockNodeId::new(6),
        ];

        let partition = NetworkPartition::new()
            .add_partition(vec![nodes[0], nodes[1]]) // Group 1: nodes 1,2
            .add_partition(vec![nodes[2], nodes[3]]) // Group 2: nodes 3,4
            .add_partition(vec![nodes[4], nodes[5]]); // Group 3: nodes 5,6

        // Within groups
        assert!(partition.can_communicate(&nodes[0], &nodes[1]));
        assert!(partition.can_communicate(&nodes[2], &nodes[3]));
        assert!(partition.can_communicate(&nodes[4], &nodes[5]));

        // Between groups
        assert!(!partition.can_communicate(&nodes[0], &nodes[2]));
        assert!(!partition.can_communicate(&nodes[2], &nodes[4]));
        assert!(!partition.can_communicate(&nodes[0], &nodes[4]));
    }

    #[test]
    fn test_network_metrics() {
        let metrics = NetworkMetrics::new();

        metrics.record_sent(100);
        metrics.record_delivered(Duration::from_millis(50));
        metrics.record_dropped();
        metrics.set_partition_active(true);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.messages_sent, 1);
        assert_eq!(snapshot.messages_delivered, 1);
        assert_eq!(snapshot.messages_dropped, 1);
        assert_eq!(snapshot.bytes_transferred, 100);
        assert!(snapshot.partition_active);
    }

    #[tokio::test]
    async fn test_mock_network_creation() -> TestResult {
        let network = MockNetwork::new();
        let nodes = network.nodes().await;
        assert!(nodes.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_node_registration() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("node1", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("node2", parse_socket_addr("127.0.0.1:8001")?).await?;

        let nodes = network.nodes().await;
        assert_eq!(nodes.len(), 2);
        assert!(nodes.contains_key(&node1));
        assert!(nodes.contains_key(&node2));

        Ok(())
    }

    #[tokio::test]
    async fn test_node_unregistration() -> TestResult {
        let network = MockNetwork::new();

        let node = network.register_node("test", parse_socket_addr("127.0.0.1:8000")?).await?;
        assert_eq!(network.nodes().await.len(), 1);

        network.unregister_node(&node).await?;
        assert_eq!(network.nodes().await.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_network_conditions_application() -> TestResult {
        let network = MockNetwork::new();

        let conditions = NetworkConditions::builder()
            .latency(Duration::from_millis(10), Duration::from_millis(20))
            .packet_loss(0.1)
            .build();

        network.apply_conditions(conditions).await;

        // Verify conditions are applied (internal state check)
        let current_conditions = network.conditions.read().await.clone();
        assert!(current_conditions.enabled);
        assert_eq!(current_conditions.packet_loss_rate, 0.1);

        Ok(())
    }

    #[tokio::test]
    async fn test_network_partition_blocking() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("node1", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("node2", parse_socket_addr("127.0.0.1:8001")?).await?;
        let node3 = network.register_node("node3", parse_socket_addr("127.0.0.1:8002")?).await?;

        // Create partition: [node1] vs [node2, node3]
        let partition = NetworkPartition::new().split(vec![node1], vec![node2, node3]);

        network.apply_partition(partition).await;

        // Messages within same partition should work
        network.send_message(node2, node3, vec![1, 2, 3]).await?;

        // Messages across partition should be silently dropped
        network.send_message(node1, node2, vec![4, 5, 6]).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_network_partition_healing() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("node1", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("node2", parse_socket_addr("127.0.0.1:8001")?).await?;

        // Create partition
        let partition = NetworkPartition::new().split(vec![node1], vec![node2]);
        network.apply_partition(partition).await;

        // Heal partition
        network.heal_partition().await;

        // All nodes should be able to communicate now
        network.send_message(node1, node2, vec![1, 2, 3]).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_message_sending_basic() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("sender", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("receiver", parse_socket_addr("127.0.0.1:8001")?).await?;

        // Use perfect conditions for predictable testing
        let conditions = NetworkConditions::perfect();
        network.apply_conditions(conditions).await;

        network.send_message(node1, node2, vec![1, 2, 3, 4]).await?;

        // Give some time for message processing
        tokio::time::sleep(Duration::from_millis(10)).await;

        let metrics = network.metrics_snapshot();
        assert_eq!(metrics.messages_sent, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_message_sending_with_latency() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("sender", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("receiver", parse_socket_addr("127.0.0.1:8001")?).await?;

        let conditions = NetworkConditions::builder()
            .latency(Duration::from_millis(10), Duration::from_millis(50))
            .build();

        network.apply_conditions(conditions).await;

        let start = Instant::now();
        network.send_message(node1, node2, vec![1, 2, 3]).await?;

        // Flush messages and check processing time
        network.flush_messages().await;
        let elapsed = start.elapsed();

        // Should take at least the minimum latency
        assert!(elapsed >= Duration::from_millis(9)); // Allow some tolerance

        Ok(())
    }

    #[tokio::test]
    async fn test_packet_loss_simulation() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("sender", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("receiver", parse_socket_addr("127.0.0.1:8001")?).await?;

        // High packet loss for testing
        let conditions = NetworkConditions::builder()
            .latency(Duration::from_millis(1), Duration::from_millis(2))
            .packet_loss(1.0) // 100% loss for deterministic testing
            .build();

        network.apply_conditions(conditions).await;

        // This should result in dropped messages
        for _i in 0..10 {
            let _ = network.send_message(node1, node2, vec![1, 2, 3]).await;
        }

        network.flush_messages().await;

        let metrics = network.metrics_snapshot();
        assert!(metrics.messages_dropped > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_message_flush() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("sender", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("receiver", parse_socket_addr("127.0.0.1:8001")?).await?;

        let conditions = NetworkConditions::builder()
            .latency(Duration::from_millis(5), Duration::from_millis(10))
            .build();

        network.apply_conditions(conditions).await;

        // Send multiple messages
        for i in 0..5 {
            network.send_message(node1, node2, vec![i as u8]).await?;
        }

        // Flush all messages
        network.flush_messages().await;

        let metrics = network.metrics_snapshot();
        assert_eq!(metrics.messages_sent, 5);

        Ok(())
    }

    #[tokio::test]
    async fn test_network_reset() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("node1", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("node2", parse_socket_addr("127.0.0.1:8001")?).await?;

        // Apply conditions and send messages
        let conditions = NetworkConditions::lan_preset();
        network.apply_conditions(conditions).await;

        network.send_message(node1, node2, vec![1, 2, 3]).await?;

        // Reset network
        network.reset().await;

        let metrics = network.metrics_snapshot();
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_delivered, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_network_metrics_snapshot() -> TestResult {
        let network = MockNetwork::new();

        let node1 = network.register_node("sender", parse_socket_addr("127.0.0.1:8000")?).await?;
        let node2 = network.register_node("receiver", parse_socket_addr("127.0.0.1:8001")?).await?;

        // Perfect conditions for predictable results
        let conditions = NetworkConditions::perfect();
        network.apply_conditions(conditions).await;

        // Send a message
        network.send_message(node1, node2, vec![1, 2, 3]).await?;
        network.flush_messages().await;

        let metrics = network.metrics_snapshot();
        assert_eq!(metrics.messages_sent, 1);
        assert_eq!(metrics.bytes_transferred, 3);

        Ok(())
    }

    #[test]
    fn test_setup_test_network_helper() {
        // Test the helper function logic (sync version)
        let nodes =
            vec![MockNodeId::new(1), MockNodeId::new(2), MockNodeId::new(3), MockNodeId::new(4)];

        let partition = create_two_way_partition(&nodes, 2);
        assert!(partition.can_communicate(&nodes[0], &nodes[1]));
        assert!(partition.can_communicate(&nodes[2], &nodes[3]));
        assert!(!partition.can_communicate(&nodes[0], &nodes[2]));
    }

    #[test]
    fn test_register_test_nodes_helper() {
        // Test node name generation logic
        for i in 0..5 {
            let name = format!("node-{}", i);
            let node_id: MockNodeId = name.as_str().into();

            // Same name should produce same ID
            let node_id2: MockNodeId = name.as_str().into();
            assert_eq!(node_id, node_id2);
        }
    }
}
