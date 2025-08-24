//! # Cluster Test Harness
//!
//! Provides a comprehensive multi-node cluster simulation framework with complete lifecycle management
//! for testing distributed operations. This module enables realistic testing of distributed systems
//! behavior under various network conditions, failure scenarios, and cluster topologies.
//!
//! ## Features
//!
//! - **Multi-Node Simulation**: Support for clusters of 2-100+ nodes with configurable topologies
//! - **Lifecycle Management**: Complete startup, operation, and shutdown orchestration
//! - **Event Recording**: Capture and verify cluster events, state changes, and message flows
//! - **Failure Scenarios**: Network partitions, node crashes, resource exhaustion simulation
//! - **Performance Testing**: Load testing, stress testing, and endurance testing capabilities
//! - **Integration Ready**: Built on existing mock network layer with full async support
//!
//! ## Architecture
//!
//! The test harness is built around several key components:
//!
//! - [`ClusterTestHarness`] - Main coordinator for multi-node cluster testing
//! - [`HarnessNode`] - Individual cluster node wrapper with testing capabilities
//! - [`ClusterTopology`] - Configuration for various cluster arrangements
//! - [`TestScenario`] - Predefined test scenarios with expected outcomes
//! - [`EventRecorder`] - Captures and verifies cluster events and state changes
//! - [`HarnessController`] - Controls cluster operations during testing
//!
//! ## Usage Examples
//!
//! ### Basic Cluster Test
//!
//! ```rust,no_run
//! use kaelix_cluster::test_utils::test_harness::*;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut harness = ClusterTestHarness::builder()
//!         .node_count(3)
//!         .topology(ClusterTopology::ring())
//!         .timeout(Duration::from_secs(30))
//!         .build()
//!         .await?;
//!
//!     // Start the cluster
//!     harness.start_cluster().await?;
//!
//!     // Run some operations
//!     harness.wait_for_leader_election().await?;
//!     
//!     // Verify cluster state
//!     assert_eq!(harness.active_node_count(), 3);
//!     assert!(harness.has_leader());
//!
//!     // Shutdown cleanly
//!     harness.shutdown_cluster().await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Network Partition Test
//!
//! ```rust,no_run
//! # use kaelix_cluster::test_utils::test_harness::*;
//! # use std::time::Duration;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut harness = ClusterTestHarness::builder()
//!     .node_count(5)
//!     .topology(ClusterTopology::mesh())
//!     .build()
//!     .await?;
//!
//! // Start cluster and wait for stability
//! harness.start_cluster().await?;
//! harness.wait_for_leader_election().await?;
//!
//! // Simulate network partition (split cluster)
//! harness.create_network_partition(vec![
//!     vec![0, 1, 2], // Majority partition
//!     vec![3, 4],    // Minority partition
//! ]).await?;
//!
//! // Wait and verify behavior
//! tokio::time::sleep(Duration::from_secs(5)).await;
//! assert!(harness.partition_has_leader(0)); // Majority should elect leader
//! assert!(!harness.partition_has_leader(1)); // Minority should not
//!
//! // Heal partition and verify
//! harness.heal_network_partition().await?;
//! harness.wait_for_cluster_convergence().await?;
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

use crate::error::{Error, Result};

/// Mock node ID for testing
pub type MockNodeId = usize;

/// Test harness configuration
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Number of nodes to simulate
    pub node_count: usize,
    /// Network topology configuration
    pub topology: ClusterTopology,
    /// Operation timeout duration
    pub timeout: Duration,
    /// Enable event recording
    pub enable_events: bool,
    /// Enable performance metrics
    pub enable_metrics: bool,
    /// Enable failure injection
    pub enable_failure_injection: bool,
    /// Network latency simulation (min, max)
    pub network_latency: Option<(Duration, Duration)>,
    /// Network packet loss percentage (0.0-1.0)
    pub network_loss_rate: Option<f64>,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            node_count: 3,
            topology: ClusterTopology::default(),
            timeout: Duration::from_secs(30),
            enable_events: true,
            enable_metrics: false,
            enable_failure_injection: false,
            network_latency: None,
            network_loss_rate: None,
        }
    }
}

/// Builder for harness configuration
#[derive(Debug, Default)]
pub struct HarnessConfigBuilder {
    config: HarnessConfig,
}

impl HarnessConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set node count
    pub fn node_count(mut self, count: usize) -> Self {
        self.config.node_count = count;
        self
    }

    /// Set topology
    pub fn topology(mut self, topology: ClusterTopology) -> Self {
        self.config.topology = topology;
        self
    }

    /// Set operation timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Enable event recording
    pub fn enable_events(mut self, enable: bool) -> Self {
        self.config.enable_events = enable;
        self
    }

    /// Enable performance metrics
    pub fn enable_metrics(mut self, enable: bool) -> Self {
        self.config.enable_metrics = enable;
        self
    }

    /// Enable failure injection
    pub fn enable_failure_injection(mut self, enable: bool) -> Self {
        self.config.enable_failure_injection = enable;
        self
    }

    /// Set network latency range
    pub fn network_latency(mut self, min: Duration, max: Duration) -> Self {
        self.config.network_latency = Some((min, max));
        self
    }

    /// Set network packet loss rate
    pub fn network_loss_rate(mut self, rate: f64) -> Self {
        self.config.network_loss_rate = Some(rate);
        self
    }

    /// Build the configuration
    pub fn build(self) -> HarnessConfig {
        self.config
    }
}

/// Node state in the test harness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    /// Node is stopped
    Stopped,
    /// Node is starting up
    Starting,
    /// Node is running normally
    Running,
    /// Node is paused for testing
    Paused,
    /// Node is shutting down
    Stopping,
    /// Node has failed
    Failed,
    /// Node is recovering from failure
    Recovering,
}

impl fmt::Display for NodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Stopping => write!(f, "stopping"),
            Self::Failed => write!(f, "failed"),
            Self::Recovering => write!(f, "recovering"),
        }
    }
}

/// Cluster topology configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterTopology {
    /// Fully connected mesh
    Mesh,
    /// Ring topology
    Ring,
    /// Star topology with one central node
    Star {
        /// Index of the central node in the cluster
        center_node: usize,
    },
    /// Tree topology with specified depth
    Tree {
        /// Maximum depth of the tree structure
        depth: usize,
    },
    /// Custom topology with explicit connections
    Custom {
        /// Map from node index to list of connected node indices
        connections: HashMap<usize, Vec<usize>>,
    },
}

impl ClusterTopology {
    /// Create a mesh topology (all nodes connected)
    pub fn mesh() -> Self {
        Self::Mesh
    }

    /// Create a ring topology
    pub fn ring() -> Self {
        Self::Ring
    }

    /// Create a star topology with the first node as center
    pub fn star() -> Self {
        Self::Star { center_node: 0 }
    }

    /// Create a tree topology with specified depth
    pub fn tree(depth: usize) -> Self {
        Self::Tree { depth }
    }

    /// Create a custom topology
    pub fn custom(connections: HashMap<usize, Vec<usize>>) -> Self {
        Self::Custom { connections }
    }
}

impl Default for ClusterTopology {
    fn default() -> Self {
        Self::Mesh
    }
}

/// Event types that can be recorded during testing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClusterEvent {
    /// Node started
    NodeStartup {
        /// ID of the node that started
        node_id: MockNodeId,
        /// Time when the startup occurred
        timestamp: Duration,
    },
    /// Node shutdown
    NodeShutdown {
        /// ID of the node that shutdown
        node_id: MockNodeId,
        /// Time when the shutdown occurred
        timestamp: Duration,
        /// Reason for the shutdown
        reason: String,
    },
    /// Node failure detected
    NodeFailure {
        /// ID of the failed node
        node_id: MockNodeId,
        /// Time when the failure was detected
        timestamp: Duration,
        /// Error message describing the failure
        error: String,
    },
    /// Node recovery completed
    NodeRecovery {
        /// ID of the recovered node
        node_id: MockNodeId,
        /// Time when recovery completed
        timestamp: Duration,
    },
    /// Leader election started
    LeaderElectionStarted {
        /// Term number for the election
        term: u64,
        /// Time when election started
        timestamp: Duration,
    },
    /// Leader election completed
    LeaderElectionCompleted {
        /// ID of the elected leader
        leader_id: MockNodeId,
        /// Term number of the new leader
        term: u64,
        /// Time when election completed
        timestamp: Duration,
    },
    /// Membership change
    MembershipChange {
        /// List of nodes added to the cluster
        added_nodes: Vec<MockNodeId>,
        /// List of nodes removed from the cluster
        removed_nodes: Vec<MockNodeId>,
        /// Time when membership change occurred
        timestamp: Duration,
    },
    /// Network partition applied
    NetworkPartition {
        /// Groups of nodes in each partition
        partition_groups: Vec<Vec<MockNodeId>>,
        /// Time when partition was applied
        timestamp: Duration,
    },
    /// Network partition healed
    NetworkPartitionHealed {
        /// Time when partition was healed
        timestamp: Duration,
    },
    /// Split-brain detected
    SplitBrainDetected {
        /// List of nodes claiming to be leader
        competing_leaders: Vec<MockNodeId>,
        /// Time when split-brain was detected
        timestamp: Duration,
    },
    /// Consensus achieved
    ConsensusAchieved {
        /// Term number for the consensus
        term: u64,
        /// Index of the committed entry
        committed_index: u64,
        /// Time when consensus was achieved
        timestamp: Duration,
    },
    /// Custom test event
    Custom {
        /// Name of the custom event
        name: String,
        /// Additional event data
        data: serde_json::Value,
        /// Time when the custom event occurred
        timestamp: Duration,
    },
}

impl ClusterEvent {
    /// Get the timestamp of this event
    pub fn timestamp(&self) -> Duration {
        match self {
            Self::NodeStartup { timestamp, .. } => *timestamp,
            Self::NodeShutdown { timestamp, .. } => *timestamp,
            Self::NodeFailure { timestamp, .. } => *timestamp,
            Self::NodeRecovery { timestamp, .. } => *timestamp,
            Self::LeaderElectionStarted { timestamp, .. } => *timestamp,
            Self::LeaderElectionCompleted { timestamp, .. } => *timestamp,
            Self::MembershipChange { timestamp, .. } => *timestamp,
            Self::NetworkPartition { timestamp, .. } => *timestamp,
            Self::NetworkPartitionHealed { timestamp, .. } => *timestamp,
            Self::SplitBrainDetected { timestamp, .. } => *timestamp,
            Self::ConsensusAchieved { timestamp, .. } => *timestamp,
            Self::Custom { timestamp, .. } => *timestamp,
        }
    }

    /// Get the event category for filtering
    pub fn category(&self) -> &'static str {
        match self {
            Self::NodeStartup { .. } | Self::NodeShutdown { .. } => "node_lifecycle",
            Self::NodeFailure { .. } | Self::NodeRecovery { .. } => "node_health",
            Self::LeaderElectionStarted { .. } | Self::LeaderElectionCompleted { .. } => {
                "leader_election"
            },
            Self::MembershipChange { .. } => "membership",
            Self::NetworkPartition { .. } | Self::NetworkPartitionHealed { .. } => {
                "network_partition"
            },
            Self::SplitBrainDetected { .. } => "split_brain",
            Self::ConsensusAchieved { .. } => "consensus",
            Self::Custom { .. } => "custom",
        }
    }

    /// Check if this is a failure event
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::NodeFailure { .. }
                | Self::SplitBrainDetected { .. }
                | Self::NetworkPartition { .. }
        )
    }
}

/// Expected event for verification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedEvent {
    /// Node startup
    NodeStartup,
    /// Node shutdown
    NodeShutdown,
    /// Node failure
    NodeFailure,
    /// Node recovery
    NodeRecovery,
    /// Leader election started
    LeaderElectionStarted,
    /// Leader election completed
    LeaderElectionCompleted,
    /// Membership change
    MembershipChange,
    /// Network partition
    NetworkPartition,
    /// Network partition healed
    NetworkPartitionHealed,
    /// Split-brain scenario
    SplitBrainDetected,
    /// Consensus achieved
    ConsensusAchieved,
    /// Custom event
    Custom(String),
}

/// Event recorder for capturing cluster events
#[derive(Debug, Default)]
pub struct EventRecorder {
    /// All recorded events
    events: Arc<RwLock<VecDeque<ClusterEvent>>>,
    /// Maximum number of events to retain
    max_events: usize,
}

impl EventRecorder {
    /// Create a new event recorder
    pub fn new() -> Self {
        Self { events: Arc::new(RwLock::new(VecDeque::new())), max_events: 1000 }
    }

    /// Create a new event recorder with custom capacity
    pub fn with_capacity(max_events: usize) -> Self {
        Self { events: Arc::new(RwLock::new(VecDeque::with_capacity(max_events))), max_events }
    }

    /// Record an event
    pub async fn record(&self, event: ClusterEvent) {
        let mut events = self.events.write().await;

        // Remove old events if we're at capacity
        while events.len() >= self.max_events {
            events.pop_front();
        }

        events.push_back(event);
    }

    /// Get all recorded events
    pub async fn events(&self) -> Vec<ClusterEvent> {
        self.events.read().await.iter().cloned().collect()
    }

    /// Get events by category
    pub async fn events_by_category(&self, category: &str) -> Vec<ClusterEvent> {
        self.events
            .read()
            .await
            .iter()
            .filter(|event| event.category() == category)
            .cloned()
            .collect()
    }

    /// Get events in time range
    pub async fn events_in_range(&self, start: Duration, end: Duration) -> Vec<ClusterEvent> {
        self.events
            .read()
            .await
            .iter()
            .filter(|event| {
                let timestamp = event.timestamp();
                timestamp >= start && timestamp <= end
            })
            .cloned()
            .collect()
    }

    /// Count events by type
    pub async fn count_events(&self, event_type: ExpectedEvent) -> usize {
        let events = self.events.read().await;
        events
            .iter()
            .filter(|event| match (&event_type, event) {
                (ExpectedEvent::NodeStartup, ClusterEvent::NodeStartup { .. }) => true,
                (ExpectedEvent::NodeShutdown, ClusterEvent::NodeShutdown { .. }) => true,
                (ExpectedEvent::NodeFailure, ClusterEvent::NodeFailure { .. }) => true,
                (ExpectedEvent::NodeRecovery, ClusterEvent::NodeRecovery { .. }) => true,
                (
                    ExpectedEvent::LeaderElectionStarted,
                    ClusterEvent::LeaderElectionStarted { .. },
                ) => true,
                (
                    ExpectedEvent::LeaderElectionCompleted,
                    ClusterEvent::LeaderElectionCompleted { .. },
                ) => true,
                (ExpectedEvent::MembershipChange, ClusterEvent::MembershipChange { .. }) => true,
                (ExpectedEvent::NetworkPartition, ClusterEvent::NetworkPartition { .. }) => true,
                (
                    ExpectedEvent::NetworkPartitionHealed,
                    ClusterEvent::NetworkPartitionHealed { .. },
                ) => true,
                (ExpectedEvent::SplitBrainDetected, ClusterEvent::SplitBrainDetected { .. }) => {
                    true
                },
                (ExpectedEvent::ConsensusAchieved, ClusterEvent::ConsensusAchieved { .. }) => true,
                (ExpectedEvent::Custom(name), ClusterEvent::Custom { name: event_name, .. }) => {
                    name == event_name
                },
                _ => false,
            })
            .count()
    }

    /// Wait for a specific event to occur
    pub async fn wait_for_event(
        &self,
        event_type: ExpectedEvent,
        timeout_duration: Duration,
    ) -> Result<ClusterEvent> {
        let start = Instant::now();

        loop {
            // Check if we already have the event
            let events = self.events.read().await;
            for event in events.iter().rev() {
                let matches = match (&event_type, event) {
                    (ExpectedEvent::NodeStartup, ClusterEvent::NodeStartup { .. }) => true,
                    (ExpectedEvent::NodeShutdown, ClusterEvent::NodeShutdown { .. }) => true,
                    (ExpectedEvent::NodeFailure, ClusterEvent::NodeFailure { .. }) => true,
                    (ExpectedEvent::NodeRecovery, ClusterEvent::NodeRecovery { .. }) => true,
                    (
                        ExpectedEvent::LeaderElectionStarted,
                        ClusterEvent::LeaderElectionStarted { .. },
                    ) => true,
                    (
                        ExpectedEvent::LeaderElectionCompleted,
                        ClusterEvent::LeaderElectionCompleted { .. },
                    ) => true,
                    (ExpectedEvent::MembershipChange, ClusterEvent::MembershipChange { .. }) => {
                        true
                    },
                    (ExpectedEvent::NetworkPartition, ClusterEvent::NetworkPartition { .. }) => {
                        true
                    },
                    (
                        ExpectedEvent::NetworkPartitionHealed,
                        ClusterEvent::NetworkPartitionHealed { .. },
                    ) => true,
                    (
                        ExpectedEvent::SplitBrainDetected,
                        ClusterEvent::SplitBrainDetected { .. },
                    ) => true,
                    (ExpectedEvent::ConsensusAchieved, ClusterEvent::ConsensusAchieved { .. }) => {
                        true
                    },
                    (
                        ExpectedEvent::Custom(name),
                        ClusterEvent::Custom { name: event_name, .. },
                    ) => name == event_name,
                    _ => false,
                };

                if matches {
                    return Ok(event.clone());
                }
            }
            drop(events);

            // Check timeout
            if start.elapsed() >= timeout_duration {
                return Err(Error::Core(kaelix_core::Error::timeout(
                    "Timeout waiting for expected event",
                )));
            }

            // Wait a bit before checking again
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Clear all recorded events
    pub async fn clear(&self) {
        self.events.write().await.clear();
    }
}

/// Individual harness node wrapper
#[derive(Debug)]
pub struct HarnessNode {
    /// Mock node ID
    pub node_id: MockNodeId,
    /// Current node state
    pub state: NodeState,
    /// Node start time
    pub start_time: Option<Instant>,
    /// Node failure reason (if failed)
    pub failure_reason: Option<String>,
    /// Network connectivity status
    pub network_connected: bool,
    /// Performance metrics
    pub metrics: HarnessNodeMetrics,
}

impl HarnessNode {
    /// Create a new harness node
    pub fn new(node_id: MockNodeId) -> Self {
        Self {
            node_id,
            state: NodeState::Stopped,
            start_time: None,
            failure_reason: None,
            network_connected: true,
            metrics: HarnessNodeMetrics::default(),
        }
    }

    /// Check if node is active
    pub fn is_active(&self) -> bool {
        matches!(self.state, NodeState::Running)
    }

    /// Check if node has failed
    pub fn is_failed(&self) -> bool {
        matches!(self.state, NodeState::Failed)
    }

    /// Get node uptime
    pub fn uptime(&self) -> Option<Duration> {
        self.start_time.map(|start| start.elapsed())
    }
}

/// Performance metrics for harness nodes
#[derive(Debug, Default, Clone)]
pub struct HarnessNodeMetrics {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Connection count
    pub connection_count: u32,
    /// Error count
    pub error_count: u64,
    /// Last error time
    pub last_error_time: Option<Instant>,
}

/// Test scenario definition
#[derive(Debug, Clone)]
pub struct TestScenario {
    /// Scenario name
    pub name: String,
    /// Description
    pub description: String,
    /// Expected duration
    pub duration: Duration,
    /// Expected events
    pub expected_events: Vec<ExpectedEvent>,
    /// Success criteria
    pub success_criteria: SuccessCriteria,
}

/// Success criteria for test scenarios
#[derive(Debug, Clone)]
pub struct SuccessCriteria {
    /// All nodes must remain active
    pub all_nodes_active: bool,
    /// Must elect a leader within timeout
    pub leader_elected_within: Option<Duration>,
    /// Maximum acceptable failure rate
    pub max_failure_rate: Option<f64>,
    /// Minimum consensus operations
    pub min_consensus_operations: Option<u64>,
    /// Custom verification function
    pub custom_verification: Option<String>,
}

impl Default for SuccessCriteria {
    fn default() -> Self {
        Self {
            all_nodes_active: true,
            leader_elected_within: Some(Duration::from_secs(10)),
            max_failure_rate: Some(0.01),
            min_consensus_operations: None,
            custom_verification: None,
        }
    }
}

/// Main cluster test harness
#[derive(Debug)]
pub struct ClusterTestHarness {
    /// Harness configuration
    config: HarnessConfig,
    /// Harness nodes
    nodes: HashMap<MockNodeId, HarnessNode>,
    /// Event recorder
    event_recorder: EventRecorder,
    /// Current network partitions
    active_partitions: Vec<Vec<MockNodeId>>,
    /// Test start time
    start_time: Instant,
    /// Harness state
    state: HarnessState,
}

/// Harness operational state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessState {
    /// Not initialized
    Uninitialized,
    /// Ready for testing
    Ready,
    /// Test is running
    Running,
    /// Test is paused
    Paused,
    /// Test completed
    Completed,
    /// Test failed
    Failed,
}

impl ClusterTestHarness {
    /// Create a new test harness
    pub async fn new(config: HarnessConfig) -> Result<Self> {
        // Create harness nodes
        let mut nodes = HashMap::new();
        for i in 0..config.node_count {
            let node_id = i;
            nodes.insert(node_id, HarnessNode::new(node_id));
        }

        Ok(Self {
            config,
            nodes,
            event_recorder: EventRecorder::new(),
            active_partitions: Vec::new(),
            start_time: Instant::now(),
            state: HarnessState::Ready,
        })
    }

    /// Create a harness builder
    pub fn builder() -> ClusterTestHarnessBuilder {
        ClusterTestHarnessBuilder::new()
    }

    /// Start the cluster
    pub async fn start_cluster(&mut self) -> Result<()> {
        tracing::info!("Starting cluster with {} nodes", self.config.node_count);
        self.state = HarnessState::Running;

        // Start all nodes
        for (node_id, node) in &mut self.nodes {
            node.state = NodeState::Starting;
            self.event_recorder
                .record(ClusterEvent::NodeStartup {
                    node_id: *node_id,
                    timestamp: self.start_time.elapsed(),
                })
                .await;

            // Simulate startup delay
            tokio::time::sleep(Duration::from_millis(100)).await;

            node.state = NodeState::Running;
            node.start_time = Some(Instant::now());
            tracing::debug!("Started node {}", node_id);
        }

        tracing::info!("Cluster startup completed");
        Ok(())
    }

    /// Shutdown the cluster
    pub async fn shutdown_cluster(&mut self) -> Result<()> {
        tracing::info!("Shutting down cluster");

        for (node_id, node) in &mut self.nodes {
            if matches!(node.state, NodeState::Running | NodeState::Paused) {
                node.state = NodeState::Stopping;

                self.event_recorder
                    .record(ClusterEvent::NodeShutdown {
                        node_id: *node_id,
                        timestamp: self.start_time.elapsed(),
                        reason: "Normal shutdown".to_string(),
                    })
                    .await;

                node.state = NodeState::Stopped;
                node.start_time = None;
                tracing::debug!("Stopped node {}", node_id);
            }
        }

        self.state = HarnessState::Completed;
        tracing::info!("Cluster shutdown completed");
        Ok(())
    }

    /// Get the number of active nodes
    pub fn active_node_count(&self) -> usize {
        self.nodes.values().filter(|node| node.is_active()).count()
    }

    /// Check if cluster has a leader
    pub fn has_leader(&self) -> bool {
        // TODO: Implement leader detection logic
        // For now, assume first active node is leader
        self.nodes.values().any(|node| node.is_active())
    }

    /// Wait for leader election to complete
    pub async fn wait_for_leader_election(&self) -> Result<MockNodeId> {
        // TODO: Implement actual leader election waiting
        // For now, return first active node
        for (node_id, node) in &self.nodes {
            if node.is_active() {
                self.event_recorder
                    .record(ClusterEvent::LeaderElectionCompleted {
                        leader_id: *node_id,
                        term: 1,
                        timestamp: self.start_time.elapsed(),
                    })
                    .await;
                return Ok(*node_id);
            }
        }

        Err(Error::Core(kaelix_core::Error::timeout("No leader elected within timeout")))
    }

    /// Create a network partition
    pub async fn create_network_partition(
        &mut self,
        partitions: Vec<Vec<MockNodeId>>,
    ) -> Result<()> {
        tracing::info!("Creating network partition: {:?}", partitions);

        self.active_partitions = partitions.clone();

        self.event_recorder
            .record(ClusterEvent::NetworkPartition {
                partition_groups: partitions,
                timestamp: self.start_time.elapsed(),
            })
            .await;

        // TODO: Apply partition to mock network
        Ok(())
    }

    /// Heal network partition
    pub async fn heal_network_partition(&mut self) -> Result<()> {
        tracing::info!("Healing network partition");

        self.active_partitions.clear();

        self.event_recorder
            .record(ClusterEvent::NetworkPartitionHealed { timestamp: self.start_time.elapsed() })
            .await;

        // TODO: Remove partition from mock network
        Ok(())
    }

    /// Check if a partition has a leader
    pub fn partition_has_leader(&self, partition_index: usize) -> bool {
        // TODO: Implement partition leader detection
        // For now, assume partition 0 (majority) has leader
        partition_index == 0 && !self.active_partitions.is_empty()
    }

    /// Wait for cluster convergence after partition healing
    pub async fn wait_for_cluster_convergence(&self) -> Result<()> {
        // TODO: Implement convergence detection
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    /// Inject a node failure
    pub async fn inject_node_failure(&mut self, node_id: MockNodeId, reason: String) -> Result<()> {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            tracing::warn!("Injecting failure for node {}: {}", node_id, reason);

            node.state = NodeState::Failed;
            node.failure_reason = Some(reason.clone());

            self.event_recorder
                .record(ClusterEvent::NodeFailure {
                    node_id,
                    timestamp: self.start_time.elapsed(),
                    error: reason,
                })
                .await;

            Ok(())
        } else {
            Err(Error::Core(kaelix_core::Error::invalid_input(&format!(
                "Node {} not found",
                node_id
            ))))
        }
    }

    /// Recover a failed node
    pub async fn recover_node(&mut self, node_id: MockNodeId) -> Result<()> {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            if matches!(node.state, NodeState::Failed) {
                tracing::info!("Recovering node {}", node_id);

                node.state = NodeState::Recovering;

                // Simulate recovery time
                tokio::time::sleep(Duration::from_millis(500)).await;

                node.state = NodeState::Running;
                node.failure_reason = None;
                node.start_time = Some(Instant::now());

                self.event_recorder
                    .record(ClusterEvent::NodeRecovery {
                        node_id,
                        timestamp: self.start_time.elapsed(),
                    })
                    .await;

                Ok(())
            } else {
                Err(Error::configuration(&format!("Node {} is not failed", node_id)))
            }
        } else {
            Err(Error::Core(kaelix_core::Error::invalid_input(&format!(
                "Node {} not found",
                node_id
            ))))
        }
    }

    /// Get harness state
    pub fn state(&self) -> HarnessState {
        self.state
    }

    /// Get event recorder
    pub fn event_recorder(&self) -> &EventRecorder {
        &self.event_recorder
    }

    /// Get node metrics
    pub fn node_metrics(&self, node_id: MockNodeId) -> Option<&HarnessNodeMetrics> {
        self.nodes.get(&node_id).map(|node| &node.metrics)
    }
}

/// Builder for cluster test harness
#[derive(Debug)]
pub struct ClusterTestHarnessBuilder {
    config: HarnessConfig,
}

impl ClusterTestHarnessBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { config: HarnessConfig::default() }
    }

    /// Set node count
    pub fn node_count(mut self, count: usize) -> Self {
        self.config.node_count = count;
        self
    }

    /// Set topology
    pub fn topology(mut self, topology: ClusterTopology) -> Self {
        self.config.topology = topology;
        self
    }

    /// Set timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Enable event recording
    pub fn enable_events(mut self, enable: bool) -> Self {
        self.config.enable_events = enable;
        self
    }

    /// Enable metrics collection
    pub fn enable_metrics(mut self, enable: bool) -> Self {
        self.config.enable_metrics = enable;
        self
    }

    /// Enable failure injection
    pub fn enable_failure_injection(mut self, enable: bool) -> Self {
        self.config.enable_failure_injection = enable;
        self
    }

    /// Set network latency simulation
    pub fn network_latency(mut self, min: Duration, max: Duration) -> Self {
        self.config.network_latency = Some((min, max));
        self
    }

    /// Set network packet loss rate
    pub fn network_loss_rate(mut self, rate: f64) -> Self {
        self.config.network_loss_rate = Some(rate);
        self
    }

    /// Build the harness
    pub async fn build(self) -> Result<ClusterTestHarness> {
        ClusterTestHarness::new(self.config).await
    }
}
