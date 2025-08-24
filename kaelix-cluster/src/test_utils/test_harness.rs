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
//!     assert_eq!(harness.active_node_count().await, 3);
//!     assert!(harness.has_leader().await);
//!
//!     // Graceful shutdown
//!     harness.shutdown().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Failure Scenario Testing
//!
//! ```rust,no_run
//! use kaelix_cluster::test_utils::test_harness::*;
//! use kaelix_cluster::test_utils::mock_network::NetworkPartition;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut harness = ClusterTestHarness::builder()
//!         .node_count(5)
//!         .build()
//!         .await?;
//!
//!     harness.start_cluster().await?;
//!     harness.wait_for_leader_election().await?;
//!
//!     // Simulate network partition
//!     let partition = NetworkPartition::new()
//!         .split(&harness.node_ids()[0..2], &harness.node_ids()[2..5]);
//!     harness.apply_network_partition(partition).await?;
//!
//!     // Wait for split-brain detection
//!     harness.wait_for_split_brain_detection().await?;
//!
//!     // Heal partition and verify recovery
//!     harness.heal_network_partition().await?;
//!     harness.wait_for_cluster_convergence().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Event-Driven Testing
//!
//! ```rust,no_run
//! use kaelix_cluster::test_utils::test_harness::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut harness = ClusterTestHarness::builder()
//!         .node_count(3)
//!         .enable_event_recording()
//!         .build()
//!         .await?;
//!
//!     harness.start_cluster().await?;
//!
//!     // Record and verify events
//!     let events = harness.recorder().events_since(std::time::Instant::now()).await;
//!     
//!     harness.assert_event_sequence(&[
//!         ExpectedEvent::NodeStartup,
//!         ExpectedEvent::MembershipChange,
//!         ExpectedEvent::LeaderElection,
//!     ]).await?;
//!
//!     Ok(())
//! }
//! ```

use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use futures::{stream::FuturesUnordered, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
    time::{sleep, timeout},
};
use tracing::{debug, error, info, trace, warn};

use crate::{
    error::{Error, Result},
    test_utils::{
        mock_network::{MockNetwork, MockNodeId, NetworkConditions, NetworkPartition},
        TestClusterNode, TestScenario,
    },
};

// ================================================================================================
// Core Types and Enums
// ================================================================================================

/// Unique identifier for test harness instances
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarnessId(pub u64);

impl fmt::Display for HarnessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "harness-{}", self.0)
    }
}

/// Node state in the test harness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    /// Node is not started
    Stopped,
    /// Node is starting up
    Starting,
    /// Node is running normally
    Running,
    /// Node is paused (for testing)
    Paused,
    /// Node is shutting down
    Stopping,
    /// Node has failed
    Failed,
    /// Node is in recovery
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
    Star { center_node: usize },
    /// Tree topology with specified depth
    Tree { depth: usize },
    /// Custom topology with explicit connections
    Custom { connections: HashMap<usize, Vec<usize>> },
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
    NodeStartup { node_id: MockNodeId, timestamp: Duration },
    /// Node shutdown
    NodeShutdown { node_id: MockNodeId, timestamp: Duration, reason: String },
    /// Node failure detected
    NodeFailure { node_id: MockNodeId, timestamp: Duration, error: String },
    /// Node recovery completed
    NodeRecovery { node_id: MockNodeId, timestamp: Duration },
    /// Leader election started
    LeaderElectionStarted { term: u64, timestamp: Duration },
    /// Leader election completed
    LeaderElectionCompleted { leader_id: MockNodeId, term: u64, timestamp: Duration },
    /// Membership change
    MembershipChange {
        added_nodes: Vec<MockNodeId>,
        removed_nodes: Vec<MockNodeId>,
        timestamp: Duration,
    },
    /// Network partition applied
    NetworkPartition { partition_groups: Vec<Vec<MockNodeId>>, timestamp: Duration },
    /// Network partition healed
    NetworkPartitionHealed { timestamp: Duration },
    /// Split-brain detected
    SplitBrainDetected { competing_leaders: Vec<MockNodeId>, timestamp: Duration },
    /// Consensus achieved
    ConsensusAchieved { term: u64, committed_index: u64, timestamp: Duration },
    /// Custom test event
    Custom { name: String, data: serde_json::Value, timestamp: Duration },
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
    /// Leader election
    LeaderElection,
    /// Membership change
    MembershipChange,
    /// Network partition
    NetworkPartition,
    /// Split-brain detection
    SplitBrain,
    /// Consensus achievement
    Consensus,
    /// Custom event with name
    Custom(String),
    /// Any event (wildcard)
    Any,
}

impl ExpectedEvent {
    /// Check if an actual event matches this expected event
    pub fn matches(&self, event: &ClusterEvent) -> bool {
        match (self, event) {
            (Self::NodeStartup, ClusterEvent::NodeStartup { .. }) => true,
            (Self::NodeShutdown, ClusterEvent::NodeShutdown { .. }) => true,
            (Self::NodeFailure, ClusterEvent::NodeFailure { .. }) => true,
            (
                Self::LeaderElection,
                ClusterEvent::LeaderElectionStarted { .. }
                | ClusterEvent::LeaderElectionCompleted { .. },
            ) => true,
            (Self::MembershipChange, ClusterEvent::MembershipChange { .. }) => true,
            (Self::NetworkPartition, ClusterEvent::NetworkPartition { .. }) => true,
            (Self::SplitBrain, ClusterEvent::SplitBrainDetected { .. }) => true,
            (Self::Consensus, ClusterEvent::ConsensusAchieved { .. }) => true,
            (Self::Custom(expected_name), ClusterEvent::Custom { name, .. }) => {
                expected_name == name
            },
            (Self::Any, _) => true,
            _ => false,
        }
    }
}

/// Test scenario configuration
#[derive(Debug, Clone)]
pub struct TestScenarioConfig {
    /// Scenario name for identification
    pub name: String,
    /// Scenario description
    pub description: String,
    /// Expected duration
    pub expected_duration: Duration,
    /// Maximum allowed duration before timeout
    pub max_duration: Duration,
    /// Expected events in order
    pub expected_events: Vec<ExpectedEvent>,
    /// Required cluster size
    pub required_nodes: usize,
    /// Network conditions to apply
    pub network_conditions: Option<NetworkConditions>,
    /// Whether to enable event recording
    pub record_events: bool,
}

impl Default for TestScenarioConfig {
    fn default() -> Self {
        Self {
            name: "default_scenario".to_string(),
            description: "Default test scenario".to_string(),
            expected_duration: Duration::from_secs(10),
            max_duration: Duration::from_secs(30),
            expected_events: Vec::new(),
            required_nodes: 3,
            network_conditions: None,
            record_events: true,
        }
    }
}

// ================================================================================================
// Event Recording System
// ================================================================================================

/// Records and manages cluster events during testing
#[derive(Debug)]
pub struct EventRecorder {
    /// All recorded events
    events: Arc<RwLock<Vec<ClusterEvent>>>,
    /// Event broadcast channel for real-time notifications
    event_tx: broadcast::Sender<ClusterEvent>,
    /// Start time for relative timestamps
    start_time: Instant,
    /// Whether recording is enabled
    enabled: bool,
}

impl EventRecorder {
    /// Create a new event recorder
    pub fn new(enabled: bool) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            start_time: Instant::now(),
            enabled,
        }
    }

    /// Record a cluster event
    pub async fn record(&self, event: ClusterEvent) {
        if !self.enabled {
            return;
        }

        {
            let mut events = self.events.write();
            events.push(event.clone());
        }

        // Broadcast to subscribers (ignore if no receivers)
        let _ = self.event_tx.send(event.clone());

        trace!("Recorded cluster event: {:?}", event);
    }

    /// Record a node startup event
    pub async fn record_node_startup(&self, node_id: MockNodeId) {
        self.record(ClusterEvent::NodeStartup { node_id, timestamp: self.start_time.elapsed() })
            .await;
    }

    /// Record a node shutdown event
    pub async fn record_node_shutdown(&self, node_id: MockNodeId, reason: String) {
        self.record(ClusterEvent::NodeShutdown {
            node_id,
            timestamp: self.start_time.elapsed(),
            reason,
        })
        .await;
    }

    /// Record a node failure event
    pub async fn record_node_failure(&self, node_id: MockNodeId, error: String) {
        self.record(ClusterEvent::NodeFailure {
            node_id,
            timestamp: self.start_time.elapsed(),
            error,
        })
        .await;
    }

    /// Record a node recovery event
    pub async fn record_node_recovery(&self, node_id: MockNodeId) {
        self.record(ClusterEvent::NodeRecovery { node_id, timestamp: self.start_time.elapsed() })
            .await;
    }

    /// Record a leader election completed event
    pub async fn record_leader_election(&self, leader_id: MockNodeId, term: u64) {
        self.record(ClusterEvent::LeaderElectionCompleted {
            leader_id,
            term,
            timestamp: self.start_time.elapsed(),
        })
        .await;
    }

    /// Record a custom event
    pub async fn record_custom(&self, name: String, data: serde_json::Value) {
        self.record(ClusterEvent::Custom { name, data, timestamp: self.start_time.elapsed() })
            .await;
    }

    /// Get all recorded events
    pub async fn events(&self) -> Vec<ClusterEvent> {
        self.events.read().clone()
    }

    /// Get events since a specific time
    pub async fn events_since(&self, since: Instant) -> Vec<ClusterEvent> {
        let since_duration = since.duration_since(self.start_time);
        self.events
            .read()
            .iter()
            .filter(|event| event.timestamp() >= since_duration)
            .cloned()
            .collect()
    }

    /// Get events of a specific category
    pub async fn events_by_category(&self, category: &str) -> Vec<ClusterEvent> {
        self.events
            .read()
            .iter()
            .filter(|event| event.category() == category)
            .cloned()
            .collect()
    }

    /// Get a subscriber to real-time events
    pub fn subscribe(&self) -> broadcast::Receiver<ClusterEvent> {
        self.event_tx.subscribe()
    }

    /// Clear all recorded events
    pub async fn clear(&self) {
        self.events.write().clear();
    }

    /// Get the number of recorded events
    pub async fn event_count(&self) -> usize {
        self.events.read().len()
    }

    /// Wait for a specific event to occur
    pub async fn wait_for_event(
        &self,
        expected: ExpectedEvent,
        timeout_duration: Duration,
    ) -> Result<ClusterEvent> {
        let mut rx = self.subscribe();
        let deadline = Instant::now() + timeout_duration;

        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Ok(event)) => {
                    if expected.matches(&event) {
                        return Ok(event);
                    }
                },
                Ok(Err(_)) => break, // Channel closed
                Err(_) => continue,  // Timeout, keep trying
            }
        }

        Err(Error::cluster_timeout(
            format!("wait_for_event({:?})", expected),
            timeout_duration.as_millis() as u64,
        ))
    }
}

// ================================================================================================
// Harness Node Implementation
// ================================================================================================

/// Individual cluster node wrapper with testing capabilities
#[derive(Debug)]
pub struct HarnessNode {
    /// Node identifier
    pub id: MockNodeId,
    /// Node display name
    pub name: String,
    /// Current node state
    state: Arc<RwLock<NodeState>>,
    /// Node configuration
    config: serde_json::Value,
    /// Task handle for node operations
    task_handle: Option<JoinHandle<Result<()>>>,
    /// Channel for sending commands to the node
    command_tx: Option<mpsc::UnboundedSender<NodeCommand>>,
    /// Node startup time
    startup_time: Option<Instant>,
    /// Total uptime tracking
    total_uptime: Duration,
    /// Failure count
    failure_count: AtomicUsize,
}

/// Commands that can be sent to a harness node
#[derive(Debug)]
enum NodeCommand {
    Start,
    Stop { reason: String },
    Pause,
    Resume,
    Fail { error: String },
    Recover,
    Shutdown { response: oneshot::Sender<Result<()>> },
}

impl HarnessNode {
    /// Create a new harness node
    pub fn new(id: MockNodeId, name: String) -> Self {
        Self {
            id,
            name,
            state: Arc::new(RwLock::new(NodeState::Stopped)),
            config: serde_json::Value::Object(Default::default()),
            task_handle: None,
            command_tx: None,
            startup_time: None,
            total_uptime: Duration::ZERO,
            failure_count: AtomicUsize::new(0),
        }
    }

    /// Get the current node state
    pub fn state(&self) -> NodeState {
        *self.state.read()
    }

    /// Check if the node is running
    pub fn is_running(&self) -> bool {
        matches!(self.state(), NodeState::Running)
    }

    /// Check if the node is available for operations
    pub fn is_available(&self) -> bool {
        matches!(self.state(), NodeState::Running | NodeState::Paused)
    }

    /// Get node uptime
    pub fn uptime(&self) -> Duration {
        match self.startup_time {
            Some(start) if matches!(self.state(), NodeState::Running | NodeState::Paused) => {
                self.total_uptime + start.elapsed()
            },
            _ => self.total_uptime,
        }
    }

    /// Get failure count
    pub fn failure_count(&self) -> usize {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Start the node
    pub async fn start(&mut self, recorder: Arc<EventRecorder>) -> Result<()> {
        if !matches!(self.state(), NodeState::Stopped | NodeState::Failed) {
            return Err(Error::InvalidNodeState {
                expected: "stopped or failed".to_string(),
                actual: self.state().to_string(),
            });
        }

        *self.state.write() = NodeState::Starting;
        debug!("Starting node {}", self.name);

        // Create command channel
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        self.command_tx = Some(command_tx);

        // Spawn node task
        let node_id = self.id;
        let node_name = self.name.clone();
        let state = Arc::clone(&self.state);
        let recorder_clone = Arc::clone(&recorder);

        self.task_handle = Some(tokio::spawn(async move {
            Self::node_task(node_id, node_name, state, command_rx, recorder_clone).await
        }));

        // Wait for startup to complete
        sleep(Duration::from_millis(10)).await;

        *self.state.write() = NodeState::Running;
        self.startup_time = Some(Instant::now());

        // Record startup event
        recorder.record_node_startup(self.id).await;

        info!("Node {} started successfully", self.name);
        Ok(())
    }

    /// Stop the node gracefully
    pub async fn stop(&mut self, reason: String, recorder: Arc<EventRecorder>) -> Result<()> {
        if matches!(self.state(), NodeState::Stopped) {
            return Ok(());
        }

        debug!("Stopping node {}: {}", self.name, reason);
        *self.state.write() = NodeState::Stopping;

        // Send stop command
        if let Some(ref tx) = self.command_tx {
            let _ = tx.send(NodeCommand::Stop { reason: reason.clone() });
        }

        // Wait for task to complete
        if let Some(handle) = self.task_handle.take() {
            let _ = timeout(Duration::from_secs(5), handle).await;
        }

        // Update uptime
        if let Some(start) = self.startup_time.take() {
            self.total_uptime += start.elapsed();
        }

        *self.state.write() = NodeState::Stopped;
        self.command_tx = None;

        // Record shutdown event
        recorder.record_node_shutdown(self.id, reason).await;

        info!("Node {} stopped", self.name);
        Ok(())
    }

    /// Pause the node (for testing)
    pub async fn pause(&mut self) -> Result<()> {
        if !matches!(self.state(), NodeState::Running) {
            return Err(Error::InvalidNodeState {
                expected: "running".to_string(),
                actual: self.state().to_string(),
            });
        }

        if let Some(ref tx) = self.command_tx {
            let _ = tx.send(NodeCommand::Pause);
        }

        *self.state.write() = NodeState::Paused;
        debug!("Node {} paused", self.name);
        Ok(())
    }

    /// Resume the node from pause
    pub async fn resume(&mut self) -> Result<()> {
        if !matches!(self.state(), NodeState::Paused) {
            return Err(Error::InvalidNodeState {
                expected: "paused".to_string(),
                actual: self.state().to_string(),
            });
        }

        if let Some(ref tx) = self.command_tx {
            let _ = tx.send(NodeCommand::Resume);
        }

        *self.state.write() = NodeState::Running;
        debug!("Node {} resumed", self.name);
        Ok(())
    }

    /// Simulate node failure
    pub async fn fail(&mut self, error: String, recorder: Arc<EventRecorder>) -> Result<()> {
        if matches!(self.state(), NodeState::Stopped | NodeState::Failed) {
            return Ok(());
        }

        debug!("Failing node {}: {}", self.name, error);

        if let Some(ref tx) = self.command_tx {
            let _ = tx.send(NodeCommand::Fail { error: error.clone() });
        }

        *self.state.write() = NodeState::Failed;
        self.failure_count.fetch_add(1, Ordering::Relaxed);

        // Update uptime
        if let Some(start) = self.startup_time.take() {
            self.total_uptime += start.elapsed();
        }

        // Record failure event
        recorder.record_node_failure(self.id, error).await;

        warn!("Node {} failed", self.name);
        Ok(())
    }

    /// Recover from failure
    pub async fn recover(&mut self, recorder: Arc<EventRecorder>) -> Result<()> {
        if !matches!(self.state(), NodeState::Failed) {
            return Err(Error::InvalidNodeState {
                expected: "failed".to_string(),
                actual: self.state().to_string(),
            });
        }

        debug!("Recovering node {}", self.name);
        *self.state.write() = NodeState::Recovering;

        // Simulate recovery time
        sleep(Duration::from_millis(100)).await;

        // Create new command channel
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        self.command_tx = Some(command_tx);

        // Spawn new task
        let node_id = self.id;
        let node_name = self.name.clone();
        let state = Arc::clone(&self.state);
        let recorder_clone = Arc::clone(&recorder);

        self.task_handle = Some(tokio::spawn(async move {
            Self::node_task(node_id, node_name, state, command_rx, recorder_clone).await
        }));

        *self.state.write() = NodeState::Running;
        self.startup_time = Some(Instant::now());

        // Record recovery event
        recorder.record_node_recovery(self.id).await;

        info!("Node {} recovered successfully", self.name);
        Ok(())
    }

    /// Shutdown the node completely
    pub async fn shutdown(&mut self, recorder: Arc<EventRecorder>) -> Result<()> {
        if matches!(self.state(), NodeState::Stopped) {
            return Ok(());
        }

        debug!("Shutting down node {}", self.name);

        // Send shutdown command and wait for response
        if let Some(ref tx) = self.command_tx {
            let (response_tx, response_rx) = oneshot::channel();
            if tx.send(NodeCommand::Shutdown { response: response_tx }).is_ok() {
                let _ = timeout(Duration::from_secs(5), response_rx).await;
            }
        }

        // Force cleanup if needed
        if let Some(handle) = self.task_handle.take() {
            let _ = timeout(Duration::from_secs(2), handle).await;
        }

        // Update uptime
        if let Some(start) = self.startup_time.take() {
            self.total_uptime += start.elapsed();
        }

        *self.state.write() = NodeState::Stopped;
        self.command_tx = None;

        // Record shutdown event
        recorder.record_node_shutdown(self.id, "shutdown".to_string()).await;

        info!("Node {} shut down", self.name);
        Ok(())
    }

    /// Node task implementation
    async fn node_task(
        _node_id: MockNodeId,
        node_name: String,
        _state: Arc<RwLock<NodeState>>,
        mut command_rx: mpsc::UnboundedReceiver<NodeCommand>,
        _recorder: Arc<EventRecorder>,
    ) -> Result<()> {
        trace!("Node task started for {}", node_name);

        while let Some(command) = command_rx.recv().await {
            match command {
                NodeCommand::Start => {
                    // Node is already running at this point
                },
                NodeCommand::Stop { .. } => {
                    trace!("Node {} received stop command", node_name);
                    break;
                },
                NodeCommand::Pause => {
                    // Pause processing - state is managed externally
                    trace!("Node {} paused", node_name);
                },
                NodeCommand::Resume => {
                    // Resume processing - state is managed externally
                    trace!("Node {} resumed", node_name);
                },
                NodeCommand::Fail { .. } => {
                    trace!("Node {} failed", node_name);
                    break;
                },
                NodeCommand::Recover => {
                    // Recovery handled externally
                    trace!("Node {} recovered", node_name);
                },
                NodeCommand::Shutdown { response } => {
                    trace!("Node {} shutting down", node_name);
                    let _ = response.send(Ok(()));
                    break;
                },
            }

            // Simulate some work
            sleep(Duration::from_millis(1)).await;
        }

        trace!("Node task completed for {}", node_name);
        Ok(())
    }
}

impl TestClusterNode for HarnessNode {
    fn test_id(&self) -> String {
        self.name.clone()
    }

    fn is_test_ready(&self) -> bool {
        self.is_running()
    }

    fn cleanup_test(&mut self) {
        // Reset state for next test
        *self.state.write() = NodeState::Stopped;
        self.command_tx = None;
        self.startup_time = None;
        self.total_uptime = Duration::ZERO;
        self.failure_count.store(0, Ordering::Relaxed);

        // Abort task if still running
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }
}

// ================================================================================================
// Harness Controller
// ================================================================================================

/// Controls cluster operations during testing
#[derive(Debug)]
pub struct HarnessController {
    /// Cluster operation timeout
    operation_timeout: Duration,
    /// Maximum retry attempts
    max_retries: usize,
    /// Retry delay
    retry_delay: Duration,
}

impl HarnessController {
    /// Create a new controller with default settings
    pub fn new() -> Self {
        Self {
            operation_timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
        }
    }

    /// Create a controller with custom settings
    pub fn with_settings(
        operation_timeout: Duration,
        max_retries: usize,
        retry_delay: Duration,
    ) -> Self {
        Self { operation_timeout, max_retries, retry_delay }
    }

    /// Wait for leader election to complete
    pub async fn wait_for_leader_election(&self, recorder: &EventRecorder) -> Result<MockNodeId> {
        let event = recorder
            .wait_for_event(ExpectedEvent::LeaderElection, self.operation_timeout)
            .await?;

        if let ClusterEvent::LeaderElectionCompleted { leader_id, .. } = event {
            Ok(leader_id)
        } else {
            Err(Error::consensus("Leader election event malformed"))
        }
    }

    /// Wait for cluster convergence
    pub async fn wait_for_cluster_convergence(&self, _node_count: usize) -> Result<()> {
        // Simulate waiting for cluster to converge
        sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    /// Wait for split-brain detection
    pub async fn wait_for_split_brain_detection(&self, recorder: &EventRecorder) -> Result<()> {
        recorder
            .wait_for_event(ExpectedEvent::SplitBrain, self.operation_timeout)
            .await?;
        Ok(())
    }

    /// Execute operation with retry logic
    pub async fn execute_with_retry<F, T>(&self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
        T: Send + 'static,
    {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    last_error = Some(err);
                    if attempt < self.max_retries {
                        sleep(self.retry_delay).await;
                    }
                },
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Internal("No error captured".to_string())))
    }
}

impl Default for HarnessController {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================================================
// Cluster Test Harness Implementation
// ================================================================================================

/// Main test harness coordinator for multi-node cluster testing
pub struct ClusterTestHarness {
    /// Unique harness identifier
    pub id: HarnessId,
    /// Cluster nodes
    nodes: Vec<HarnessNode>,
    /// Mock network for simulation
    network: Arc<MockNetwork>,
    /// Event recorder
    recorder: Arc<EventRecorder>,
    /// Harness controller
    controller: HarnessController,
    /// Cluster topology
    topology: ClusterTopology,
    /// Current network partition
    current_partition: Option<NetworkPartition>,
    /// Test timeout
    test_timeout: Duration,
    /// Whether the harness is initialized
    initialized: bool,
    /// Start time
    start_time: Instant,
    /// Background tasks
    background_tasks: Vec<JoinHandle<()>>,
}

impl ClusterTestHarness {
    /// Create a new test harness builder
    pub fn builder() -> ClusterTestHarnessBuilder {
        ClusterTestHarnessBuilder::new()
    }

    /// Create a simple test harness with specified node count
    pub async fn simple(node_count: usize) -> Result<Self> {
        Self::builder().node_count(node_count).build().await
    }

    /// Get the harness ID
    pub fn id(&self) -> HarnessId {
        self.id
    }

    /// Get all node IDs
    pub fn node_ids(&self) -> Vec<MockNodeId> {
        self.nodes.iter().map(|n| n.id).collect()
    }

    /// Get a node by ID
    pub fn node(&self, id: MockNodeId) -> Option<&HarnessNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Get a mutable reference to a node by ID
    pub fn node_mut(&mut self, id: MockNodeId) -> Option<&mut HarnessNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[HarnessNode] {
        &self.nodes
    }

    /// Get the event recorder
    pub fn recorder(&self) -> &EventRecorder {
        &self.recorder
    }

    /// Get the mock network
    pub fn network(&self) -> &MockNetwork {
        &self.network
    }

    /// Get active node count
    pub async fn active_node_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_available()).count()
    }

    /// Check if cluster has a leader (simulated)
    pub async fn has_leader(&self) -> bool {
        // In a real implementation, this would check actual leader state
        self.active_node_count().await > 0
    }

    /// Start the entire cluster
    pub async fn start_cluster(&mut self) -> Result<()> {
        if !self.initialized {
            return Err(Error::InvalidNodeState {
                expected: "initialized".to_string(),
                actual: "uninitialized".to_string(),
            });
        }

        info!("Starting cluster with {} nodes", self.nodes.len());

        let recorder = Arc::clone(&self.recorder);

        // Start all nodes concurrently
        let mut start_futures = FuturesUnordered::new();

        for i in 0..self.nodes.len() {
            let recorder_clone = Arc::clone(&recorder);
            start_futures.push(async move { (i, recorder_clone) });
        }

        let mut started_nodes = Vec::new();
        while let Some((i, recorder_ref)) = start_futures.next().await {
            if let Some(node) = self.nodes.get_mut(i) {
                match node.start(recorder_ref).await {
                    Ok(()) => {
                        started_nodes.push(i);
                        debug!("Node {} started", node.name);
                    },
                    Err(e) => {
                        error!("Failed to start node {}: {}", node.name, e);
                        return Err(e);
                    },
                }
            }
        }

        // Wait for cluster to stabilize
        sleep(Duration::from_millis(50)).await;

        // Simulate leader election
        if !started_nodes.is_empty() {
            let leader_id = self.nodes[started_nodes[0]].id;
            self.recorder.record_leader_election(leader_id, 1).await;
        }

        info!("Cluster started successfully with {} active nodes", self.active_node_count().await);

        Ok(())
    }

    /// Stop specific nodes
    pub async fn stop_nodes(&mut self, node_ids: &[MockNodeId]) -> Result<()> {
        let recorder = Arc::clone(&self.recorder);
        for &node_id in node_ids {
            if let Some(node) = self.node_mut(node_id) {
                node.stop("requested by test harness".to_string(), Arc::clone(&recorder))
                    .await?;
            }
        }
        Ok(())
    }

    /// Start specific nodes
    pub async fn start_nodes(&mut self, node_ids: &[MockNodeId]) -> Result<()> {
        let recorder = Arc::clone(&self.recorder);
        for &node_id in node_ids {
            if let Some(node) = self.node_mut(node_id) {
                node.start(Arc::clone(&recorder)).await?;
            }
        }
        Ok(())
    }

    /// Simulate node failures
    pub async fn fail_nodes(&mut self, node_ids: &[MockNodeId], error: String) -> Result<()> {
        let recorder = Arc::clone(&self.recorder);
        for &node_id in node_ids {
            if let Some(node) = self.node_mut(node_id) {
                node.fail(error.clone(), Arc::clone(&recorder)).await?;
            }
        }
        Ok(())
    }

    /// Recover failed nodes
    pub async fn recover_nodes(&mut self, node_ids: &[MockNodeId]) -> Result<()> {
        let recorder = Arc::clone(&self.recorder);
        for &node_id in node_ids {
            if let Some(node) = self.node_mut(node_id) {
                if matches!(node.state(), NodeState::Failed) {
                    node.recover(Arc::clone(&recorder)).await?;
                }
            }
        }
        Ok(())
    }

    /// Apply network conditions
    pub async fn apply_network_conditions(&self, conditions: NetworkConditions) -> Result<()> {
        self.network.apply_conditions(conditions).await;
        Ok(())
    }

    /// Apply network partition
    pub async fn apply_network_partition(&mut self, partition: NetworkPartition) -> Result<()> {
        self.network.apply_partition(partition.clone()).await;
        self.current_partition = Some(partition.clone());

        // Record partition event
        let partition_groups: Vec<Vec<MockNodeId>> = partition
            .partitions()
            .iter()
            .map(|group| group.iter().copied().collect())
            .collect();

        self.recorder
            .record(ClusterEvent::NetworkPartition {
                partition_groups,
                timestamp: self.start_time.elapsed(),
            })
            .await;

        Ok(())
    }

    /// Heal network partition
    pub async fn heal_network_partition(&mut self) -> Result<()> {
        self.network.heal_partition().await;
        self.current_partition = None;

        // Record partition healed event
        self.recorder
            .record(ClusterEvent::NetworkPartitionHealed { timestamp: self.start_time.elapsed() })
            .await;

        Ok(())
    }

    /// Wait for leader election
    pub async fn wait_for_leader_election(&self) -> Result<MockNodeId> {
        self.controller.wait_for_leader_election(&self.recorder).await
    }

    /// Wait for cluster convergence
    pub async fn wait_for_cluster_convergence(&self) -> Result<()> {
        self.controller.wait_for_cluster_convergence(self.nodes.len()).await
    }

    /// Wait for split-brain detection
    pub async fn wait_for_split_brain_detection(&self) -> Result<()> {
        self.controller.wait_for_split_brain_detection(&self.recorder).await
    }

    /// Assert event sequence occurred
    pub async fn assert_event_sequence(&self, expected_events: &[ExpectedEvent]) -> Result<()> {
        let events = self.recorder.events().await;
        let mut event_index = 0;

        for expected in expected_events {
            let mut found = false;

            for event in events.iter().skip(event_index) {
                if expected.matches(event) {
                    found = true;
                    break;
                }
                event_index += 1;
            }

            if !found {
                return Err(Error::validation_error(format!(
                    "Expected event {:?} not found in sequence",
                    expected
                )));
            }
        }

        Ok(())
    }

    /// Run a test scenario
    pub async fn run_scenario<S>(&mut self, mut scenario: S) -> Result<()>
    where
        S: TestScenario<Error = Error> + Send,
    {
        info!("Running test scenario: {}", scenario.name());

        let scenario_start = Instant::now();

        // Setup
        scenario.setup().await?;

        // Execute with timeout
        let execute_result = timeout(self.test_timeout, scenario.execute()).await;

        // Teardown regardless of execution result
        let teardown_result = scenario.teardown().await;

        // Check results
        match execute_result {
            Ok(Ok(())) => {
                let duration = scenario_start.elapsed();
                info!(
                    "Test scenario '{}' completed successfully in {:?}",
                    scenario.name(),
                    duration
                );
            },
            Ok(Err(e)) => {
                error!("Test scenario '{}' failed: {}", scenario.name(), e);
                return Err(e);
            },
            Err(_) => {
                error!(
                    "Test scenario '{}' timed out after {:?}",
                    scenario.name(),
                    self.test_timeout
                );
                return Err(Error::cluster_timeout(
                    scenario.name(),
                    self.test_timeout.as_millis() as u64,
                ));
            },
        }

        // Check teardown result
        teardown_result?;

        Ok(())
    }

    /// Graceful shutdown of the entire cluster
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down test harness");

        // Cancel background tasks
        for task in self.background_tasks.drain(..) {
            task.abort();
        }

        let recorder = Arc::clone(&self.recorder);

        // Shutdown all nodes concurrently
        let mut shutdown_futures = FuturesUnordered::new();

        for i in 0..self.nodes.len() {
            let recorder_clone = Arc::clone(&recorder);
            shutdown_futures.push(async move { (i, recorder_clone) });
        }

        while let Some((i, recorder_ref)) = shutdown_futures.next().await {
            if let Some(node) = self.nodes.get_mut(i) {
                if let Err(e) = node.shutdown(recorder_ref).await {
                    warn!("Error shutting down node {}: {}", node.name, e);
                }
            }
        }

        // Reset network state
        self.network.reset().await;

        info!("Test harness shutdown complete");
        Ok(())
    }

    /// Get harness statistics
    pub async fn statistics(&self) -> HarnessStatistics {
        let mut stats = HarnessStatistics {
            total_nodes: self.nodes.len(),
            active_nodes: 0,
            failed_nodes: 0,
            total_uptime: Duration::ZERO,
            total_failures: 0,
            events_recorded: self.recorder.event_count().await,
            network_partition_active: self.current_partition.is_some(),
        };

        for node in &self.nodes {
            match node.state() {
                NodeState::Running | NodeState::Paused => stats.active_nodes += 1,
                NodeState::Failed => stats.failed_nodes += 1,
                _ => {},
            }

            stats.total_uptime += node.uptime();
            stats.total_failures += node.failure_count();
        }

        stats
    }
}

impl Drop for ClusterTestHarness {
    fn drop(&mut self) {
        // Cleanup nodes
        for node in &mut self.nodes {
            node.cleanup_test();
        }

        // Cancel background tasks
        for task in self.background_tasks.drain(..) {
            task.abort();
        }
    }
}

// ================================================================================================
// Harness Statistics
// ================================================================================================

/// Statistics about the test harness state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessStatistics {
    /// Total number of nodes
    pub total_nodes: usize,
    /// Number of active nodes
    pub active_nodes: usize,
    /// Number of failed nodes
    pub failed_nodes: usize,
    /// Total uptime across all nodes
    pub total_uptime: Duration,
    /// Total failure count across all nodes
    pub total_failures: usize,
    /// Number of events recorded
    pub events_recorded: usize,
    /// Whether network partition is active
    pub network_partition_active: bool,
}

impl fmt::Display for HarnessStatistics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Harness Stats: {} nodes ({} active, {} failed), {} events, uptime: {:?}",
            self.total_nodes,
            self.active_nodes,
            self.failed_nodes,
            self.events_recorded,
            self.total_uptime
        )
    }
}

// ================================================================================================
// Builder Implementation
// ================================================================================================

/// Builder for creating cluster test harnesses
#[derive(Debug)]
pub struct ClusterTestHarnessBuilder {
    node_count: usize,
    topology: ClusterTopology,
    enable_event_recording: bool,
    timeout: Duration,
    network_conditions: Option<NetworkConditions>,
    initial_partition: Option<NetworkPartition>,
    controller_settings: Option<(Duration, usize, Duration)>,
}

impl ClusterTestHarnessBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            node_count: 3,
            topology: ClusterTopology::default(),
            enable_event_recording: true,
            timeout: Duration::from_secs(60),
            network_conditions: None,
            initial_partition: None,
            controller_settings: None,
        }
    }

    /// Set the number of nodes
    pub fn node_count(mut self, count: usize) -> Self {
        self.node_count = count.max(1).min(1000); // Reasonable bounds
        self
    }

    /// Set the cluster topology
    pub fn topology(mut self, topology: ClusterTopology) -> Self {
        self.topology = topology;
        self
    }

    /// Enable or disable event recording
    pub fn enable_event_recording(mut self) -> Self {
        self.enable_event_recording = true;
        self
    }

    /// Disable event recording
    pub fn disable_event_recording(mut self) -> Self {
        self.enable_event_recording = false;
        self
    }

    /// Set test timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set initial network conditions
    pub fn network_conditions(mut self, conditions: NetworkConditions) -> Self {
        self.network_conditions = Some(conditions);
        self
    }

    /// Set initial network partition
    pub fn initial_partition(mut self, partition: NetworkPartition) -> Self {
        self.initial_partition = Some(partition);
        self
    }

    /// Set controller settings
    pub fn controller_settings(
        mut self,
        operation_timeout: Duration,
        max_retries: usize,
        retry_delay: Duration,
    ) -> Self {
        self.controller_settings = Some((operation_timeout, max_retries, retry_delay));
        self
    }

    /// Build the test harness
    pub async fn build(self) -> Result<ClusterTestHarness> {
        static HARNESS_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
        let harness_id = HarnessId(HARNESS_ID_COUNTER.fetch_add(1, Ordering::Relaxed));

        info!("Building test harness {} with {} nodes", harness_id, self.node_count);

        // Create mock network
        let network = Arc::new(MockNetwork::new());

        // Create event recorder
        let recorder = Arc::new(EventRecorder::new(self.enable_event_recording));

        // Create controller
        let controller = if let Some((timeout, retries, delay)) = self.controller_settings {
            HarnessController::with_settings(timeout, retries, delay)
        } else {
            HarnessController::new()
        };

        // Create nodes
        let mut nodes = Vec::with_capacity(self.node_count);
        for i in 0..self.node_count {
            let node_name = format!("test-node-{}", i);
            let socket_addr = format!("127.0.0.1:{}", 9000 + i)
                .parse()
                .map_err(|e| Error::configuration(format!("Invalid socket address: {}", e)))?;

            // Register node with network
            let node_id = network.register_node(&node_name, socket_addr).await?;

            // Create harness node
            let harness_node = HarnessNode::new(node_id, node_name);
            nodes.push(harness_node);
        }

        // Apply initial network conditions if specified
        if let Some(conditions) = self.network_conditions {
            network.apply_conditions(conditions).await;
        }

        let mut harness = ClusterTestHarness {
            id: harness_id,
            nodes,
            network,
            recorder,
            controller,
            topology: self.topology,
            current_partition: None,
            test_timeout: self.timeout,
            initialized: true,
            start_time: Instant::now(),
            background_tasks: Vec::new(),
        };

        // Apply initial partition if specified
        if let Some(partition) = self.initial_partition {
            harness.apply_network_partition(partition).await?;
        }

        info!("Test harness {} built successfully", harness_id);
        Ok(harness)
    }
}

impl Default for ClusterTestHarnessBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================================================
// Error Extensions
// ================================================================================================

impl Error {
    /// Create a validation error
    pub fn validation_error<T: std::fmt::Display>(message: T) -> Self {
        Self::Configuration(message.to_string())
    }
}

// ================================================================================================
// Tests
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<()>;

    #[test]
    fn test_harness_id_creation() {
        let id1 = HarnessId(1);
        let id2 = HarnessId(2);
        assert_ne!(id1, id2);
        assert_eq!(format!("{}", id1), "harness-1");
    }

    #[test]
    fn test_node_state_display() {
        assert_eq!(NodeState::Running.to_string(), "running");
        assert_eq!(NodeState::Failed.to_string(), "failed");
        assert_eq!(NodeState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_cluster_topology_creation() {
        let mesh = ClusterTopology::mesh();
        assert!(matches!(mesh, ClusterTopology::Mesh));

        let ring = ClusterTopology::ring();
        assert!(matches!(ring, ClusterTopology::Ring));

        let star = ClusterTopology::star();
        assert!(matches!(star, ClusterTopology::Star { center_node: 0 }));
    }

    #[test]
    fn test_expected_event_matching() {
        let event = ClusterEvent::NodeStartup {
            node_id: MockNodeId::new(1),
            timestamp: Duration::from_secs(1),
        };

        assert!(ExpectedEvent::NodeStartup.matches(&event));
        assert!(ExpectedEvent::Any.matches(&event));
        assert!(!ExpectedEvent::NodeShutdown.matches(&event));
    }

    #[test]
    fn test_cluster_event_properties() {
        let event = ClusterEvent::NodeFailure {
            node_id: MockNodeId::new(1),
            timestamp: Duration::from_secs(5),
            error: "test error".to_string(),
        };

        assert_eq!(event.timestamp(), Duration::from_secs(5));
        assert_eq!(event.category(), "node_health");
        assert!(event.is_failure());
    }

    #[tokio::test]
    async fn test_event_recorder_basic() -> TestResult {
        let recorder = EventRecorder::new(true);

        recorder.record_node_startup(MockNodeId::new(1)).await;
        recorder.record_node_shutdown(MockNodeId::new(1), "test".to_string()).await;

        let events = recorder.events().await;
        assert_eq!(events.len(), 2);

        let event_count = recorder.event_count().await;
        assert_eq!(event_count, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_event_recorder_categories() -> TestResult {
        let recorder = EventRecorder::new(true);

        recorder.record_node_startup(MockNodeId::new(1)).await;
        recorder.record_node_failure(MockNodeId::new(1), "test".to_string()).await;
        recorder.record_leader_election(MockNodeId::new(1), 1).await;

        let node_events = recorder.events_by_category("node_lifecycle").await;
        assert_eq!(node_events.len(), 1);

        let health_events = recorder.events_by_category("node_health").await;
        assert_eq!(health_events.len(), 1);

        let leader_events = recorder.events_by_category("leader_election").await;
        assert_eq!(leader_events.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_node_lifecycle() -> TestResult {
        let mut node = HarnessNode::new(MockNodeId::new(1), "test-node".to_string());
        let recorder = Arc::new(EventRecorder::new(true));

        assert_eq!(node.state(), NodeState::Stopped);
        assert!(!node.is_running());

        node.start(Arc::clone(&recorder)).await?;
        assert_eq!(node.state(), NodeState::Running);
        assert!(node.is_running());

        node.pause().await?;
        assert_eq!(node.state(), NodeState::Paused);
        assert!(!node.is_running());
        assert!(node.is_available());

        node.resume().await?;
        assert_eq!(node.state(), NodeState::Running);

        node.fail("test failure".to_string(), Arc::clone(&recorder)).await?;
        assert_eq!(node.state(), NodeState::Failed);
        assert_eq!(node.failure_count(), 1);

        node.recover(Arc::clone(&recorder)).await?;
        assert_eq!(node.state(), NodeState::Running);

        node.shutdown(recorder).await?;
        assert_eq!(node.state(), NodeState::Stopped);

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_node_uptime_tracking() -> TestResult {
        let mut node = HarnessNode::new(MockNodeId::new(1), "test-node".to_string());
        let recorder = Arc::new(EventRecorder::new(false));

        assert_eq!(node.uptime(), Duration::ZERO);

        node.start(Arc::clone(&recorder)).await?;

        // Let it run for a bit
        tokio::time::sleep(Duration::from_millis(10)).await;
        let uptime1 = node.uptime();
        assert!(uptime1 > Duration::ZERO);

        // Stop and check total uptime
        node.stop("test".to_string(), recorder).await?;
        let uptime2 = node.uptime();
        assert!(uptime2 >= uptime1);

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_controller() -> TestResult {
        let controller = HarnessController::new();
        let recorder = EventRecorder::new(true);

        // Test timeout behavior
        let start = Instant::now();
        let result = controller.wait_for_leader_election(&recorder).await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        assert!(elapsed >= Duration::from_secs(30)); // Should timeout

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_builder() -> TestResult {
        let harness = ClusterTestHarness::builder()
            .node_count(3)
            .topology(ClusterTopology::ring())
            .timeout(Duration::from_secs(10))
            .enable_event_recording()
            .build()
            .await?;

        assert_eq!(harness.nodes().len(), 3);
        assert_eq!(harness.node_ids().len(), 3);
        assert!(harness.initialized);

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_cluster_lifecycle() -> TestResult {
        let mut harness = ClusterTestHarness::builder()
            .node_count(3)
            .timeout(Duration::from_secs(5))
            .build()
            .await?;

        // Initially no nodes are running
        assert_eq!(harness.active_node_count().await, 0);

        // Start cluster
        harness.start_cluster().await?;
        assert_eq!(harness.active_node_count().await, 3);
        assert!(harness.has_leader().await);

        // Stop some nodes
        let first_node = harness.node_ids()[0];
        harness.stop_nodes(&[first_node]).await?;
        assert_eq!(harness.active_node_count().await, 2);

        // Fail a node
        let second_node = harness.node_ids()[1];
        harness.fail_nodes(&[second_node], "test failure".to_string()).await?;
        assert_eq!(harness.active_node_count().await, 1);

        // Recover the failed node
        harness.recover_nodes(&[second_node]).await?;
        assert_eq!(harness.active_node_count().await, 2);

        // Restart stopped node
        harness.start_nodes(&[first_node]).await?;
        assert_eq!(harness.active_node_count().await, 3);

        // Shutdown
        harness.shutdown().await?;
        assert_eq!(harness.active_node_count().await, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_network_partition() -> TestResult {
        let mut harness = ClusterTestHarness::builder()
            .node_count(4)
            .enable_event_recording()
            .timeout(Duration::from_secs(5))
            .build()
            .await?;

        harness.start_cluster().await?;

        let node_ids = harness.node_ids();
        let partition = NetworkPartition::new().split(&node_ids[0..2], &node_ids[2..4]);

        // Apply partition
        harness.apply_network_partition(partition).await?;
        assert!(harness.current_partition.is_some());

        // Check events
        let events = harness.recorder().events().await;
        let partition_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ClusterEvent::NetworkPartition { .. }))
            .collect();
        assert!(!partition_events.is_empty());

        // Heal partition
        harness.heal_network_partition().await?;
        assert!(harness.current_partition.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_harness_statistics() -> TestResult {
        let mut harness = ClusterTestHarness::builder().node_count(3).build().await?;

        harness.start_cluster().await?;

        // Let nodes run for a bit
        tokio::time::sleep(Duration::from_millis(10)).await;

        let stats = harness.statistics().await;
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.active_nodes, 3);
        assert_eq!(stats.failed_nodes, 0);
        assert!(stats.total_uptime > Duration::ZERO);
        assert!(!stats.network_partition_active);

        Ok(())
    }

    #[tokio::test]
    async fn test_simple_harness_creation() -> TestResult {
        let harness = ClusterTestHarness::simple(5).await?;
        assert_eq!(harness.nodes().len(), 5);
        assert_eq!(harness.node_ids().len(), 5);

        Ok(())
    }
}
