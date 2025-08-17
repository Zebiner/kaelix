//! Network simulation and fault injection for distributed testing
//!
//! This module provides comprehensive network simulation capabilities including
//! latency injection, packet loss, bandwidth throttling, and network partitions.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rand::prelude::*;
use rand_distr::{Distribution, Normal, Uniform};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::cluster::NodeId;

/// Network simulator for injecting network conditions
pub struct NetworkSimulator {
    /// Active network conditions
    conditions: Arc<RwLock<HashMap<NodeId, NetworkConditions>>>,
    /// Global network rules
    global_rules: Arc<RwLock<Vec<NetworkRule>>>,
    /// Partition state
    partitions: Arc<RwLock<HashMap<PartitionId, NetworkPartition>>>,
    /// Simulation state
    state: Arc<Mutex<SimulatorState>>,
    /// Random number generator
    rng: Arc<Mutex<StdRng>>,
}

/// Unique identifier for network partitions
pub type PartitionId = Uuid;

/// Network conditions that can be applied to nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConditions {
    /// Latency model
    pub latency: LatencyModel,
    /// Packet loss model
    pub loss: LossModel,
    /// Bandwidth model
    pub bandwidth: BandwidthModel,
    /// Whether the node is partitioned
    pub partitioned: bool,
    /// Custom jitter settings
    pub jitter: JitterModel,
}

/// Latency injection model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LatencyModel {
    /// Fixed latency
    Fixed {
        /// Base latency
        latency: Duration,
    },
    /// Normal distribution
    Normal {
        /// Mean latency
        mean: Duration,
        /// Standard deviation
        std_dev: Duration,
    },
    /// Uniform distribution
    Uniform {
        /// Minimum latency
        min: Duration,
        /// Maximum latency
        max: Duration,
    },
    /// Custom latency profile
    Custom {
        /// Latency percentiles
        percentiles: Vec<(f64, Duration)>,
    },
}

/// Packet loss injection model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LossModel {
    /// No packet loss
    None,
    /// Fixed loss rate
    Fixed {
        /// Loss rate (0.0 to 1.0)
        rate: f64,
    },
    /// Burst loss model
    Burst {
        /// Base loss rate
        base_rate: f64,
        /// Burst probability
        burst_probability: f64,
        /// Burst length (packets)
        burst_length: usize,
    },
    /// Gilbert-Elliott model for correlated loss
    GilbertElliott {
        /// Loss rate in good state
        good_loss_rate: f64,
        /// Loss rate in bad state
        bad_loss_rate: f64,
        /// Transition probability from good to bad
        good_to_bad: f64,
        /// Transition probability from bad to good
        bad_to_good: f64,
    },
}

/// Bandwidth throttling model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BandwidthModel {
    /// Unlimited bandwidth
    Unlimited,
    /// Fixed bandwidth limit
    Fixed {
        /// Bandwidth in bytes per second
        bytes_per_second: u64,
    },
    /// Variable bandwidth
    Variable {
        /// Minimum bandwidth
        min_bps: u64,
        /// Maximum bandwidth
        max_bps: u64,
        /// Variation period
        period: Duration,
    },
    /// Congestion-based model
    Congestion {
        /// Base bandwidth
        base_bps: u64,
        /// Congestion threshold
        congestion_threshold: f64,
        /// Bandwidth reduction factor during congestion
        reduction_factor: f64,
    },
}

/// Network jitter model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JitterModel {
    /// No jitter
    None,
    /// Fixed jitter amount
    Fixed {
        /// Jitter amount
        amount: Duration,
    },
    /// Normal distribution jitter
    Normal {
        /// Mean jitter
        mean: Duration,
        /// Standard deviation
        std_dev: Duration,
    },
    /// Custom jitter profile
    Custom {
        /// Jitter percentiles
        percentiles: Vec<(f64, Duration)>,
    },
}

/// Network partitioning model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionModel {
    /// Split-brain partition (two groups)
    SplitBrain {
        /// Nodes in first group
        group_a: Vec<NodeId>,
        /// Nodes in second group
        group_b: Vec<NodeId>,
    },
    /// Island partition (one node isolated)
    Island {
        /// Isolated node
        isolated_node: NodeId,
        /// Remaining nodes
        remaining_nodes: Vec<NodeId>,
    },
    /// Custom partition
    Custom {
        /// Groups of connected nodes
        groups: Vec<Vec<NodeId>>,
    },
    /// Asymmetric partition (unidirectional)
    Asymmetric {
        /// Source nodes (can send)
        sources: Vec<NodeId>,
        /// Destination nodes (can receive)
        destinations: Vec<NodeId>,
    },
}

/// Network rule for conditional application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRule {
    /// Rule identifier
    pub id: String,
    /// Condition for applying the rule
    pub condition: RuleCondition,
    /// Actions to take when condition is met
    pub actions: Vec<RuleAction>,
    /// Whether the rule is currently active
    pub active: bool,
}

/// Conditions for network rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    /// Apply to specific nodes
    Nodes(Vec<NodeId>),
    /// Apply based on node count
    NodeCount {
        /// Minimum node count
        min: usize,
        /// Maximum node count
        max: usize,
    },
    /// Apply based on time
    Time {
        /// Start time offset
        start: Duration,
        /// End time offset
        end: Duration,
    },
    /// Apply based on message count
    MessageCount {
        /// Minimum message count
        min: u64,
        /// Maximum message count
        max: u64,
    },
    /// Always apply
    Always,
}

/// Actions for network rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleAction {
    /// Add latency
    AddLatency(LatencyModel),
    /// Add packet loss
    AddLoss(LossModel),
    /// Throttle bandwidth
    ThrottleBandwidth(BandwidthModel),
    /// Create partition
    CreatePartition(PartitionModel),
    /// Heal partition
    HealPartition,
    /// Add jitter
    AddJitter(JitterModel),
}

/// Network partition representation
#[derive(Debug, Clone)]
pub struct NetworkPartition {
    /// Partition identifier
    pub id: PartitionId,
    /// Partition model
    pub model: PartitionModel,
    /// When the partition was created
    pub created_at: Instant,
    /// Whether the partition is active
    pub active: bool,
}

/// State of the network simulator
#[derive(Debug, Clone)]
pub enum SimulatorState {
    /// Simulator is inactive
    Inactive,
    /// Simulator is running
    Running,
    /// Simulator is paused
    Paused,
    /// Simulator encountered an error
    Error(String),
}

/// Statistics about network simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Total packets processed
    pub total_packets: u64,
    /// Packets dropped due to loss
    pub dropped_packets: u64,
    /// Total latency added (microseconds)
    pub total_latency_added: u64,
    /// Total bandwidth throttled (bytes)
    pub total_bandwidth_throttled: u64,
    /// Active partitions
    pub active_partitions: usize,
    /// Simulation duration
    pub simulation_duration: Duration,
}

impl NetworkSimulator {
    /// Create a new network simulator
    pub fn new() -> Self {
        Self {
            conditions: Arc::new(RwLock::new(HashMap::new())),
            global_rules: Arc::new(RwLock::new(Vec::new())),
            partitions: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(Mutex::new(SimulatorState::Inactive)),
            rng: Arc::new(Mutex::new(StdRng::from_entropy())),
        }
    }

    /// Start the network simulator
    pub async fn start(&self) -> Result<()> {
        info!("Starting network simulator");
        
        let mut state = self.state.lock().await;
        *state = SimulatorState::Running;
        
        info!("Network simulator started");
        Ok(())
    }

    /// Stop the network simulator
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping network simulator");
        
        let mut state = self.state.lock().await;
        *state = SimulatorState::Inactive;
        
        // Clear all conditions and partitions
        self.conditions.write().await.clear();
        self.partitions.write().await.clear();
        
        info!("Network simulator stopped");
        Ok(())
    }

    /// Apply network conditions to a specific node
    pub async fn apply_conditions(&self, node_id: NodeId, conditions: NetworkConditions) -> Result<()> {
        debug!("Applying network conditions to node: {}", node_id);
        
        let mut node_conditions = self.conditions.write().await;
        node_conditions.insert(node_id, conditions);
        
        Ok(())
    }

    /// Remove network conditions from a node
    pub async fn remove_conditions(&self, node_id: NodeId) -> Result<()> {
        debug!("Removing network conditions from node: {}", node_id);
        
        let mut node_conditions = self.conditions.write().await;
        node_conditions.remove(&node_id);
        
        Ok(())
    }

    /// Create a network partition
    pub async fn create_partition(&self, model: PartitionModel) -> Result<PartitionId> {
        let partition_id = Uuid::new_v4();
        info!("Creating network partition: {}", partition_id);
        
        let partition = NetworkPartition {
            id: partition_id,
            model: model.clone(),
            created_at: Instant::now(),
            active: true,
        };
        
        // Apply partition conditions to affected nodes
        match &model {
            PartitionModel::SplitBrain { group_a, group_b } => {
                self.apply_split_brain_partition(group_a, group_b).await?;
            }
            PartitionModel::Island { isolated_node, .. } => {
                self.apply_island_partition(*isolated_node).await?;
            }
            PartitionModel::Custom { groups } => {
                self.apply_custom_partition(groups).await?;
            }
            PartitionModel::Asymmetric { sources, destinations } => {
                self.apply_asymmetric_partition(sources, destinations).await?;
            }
        }
        
        let mut partitions = self.partitions.write().await;
        partitions.insert(partition_id, partition);
        
        info!("Network partition created: {}", partition_id);
        Ok(partition_id)
    }

    /// Heal a network partition
    pub async fn heal_partition(&self, partition_id: PartitionId) -> Result<()> {
        info!("Healing network partition: {}", partition_id);
        
        let mut partitions = self.partitions.write().await;
        if let Some(partition) = partitions.get_mut(&partition_id) {
            partition.active = false;
            
            // Remove partition conditions from affected nodes
            match &partition.model {
                PartitionModel::SplitBrain { group_a, group_b } => {
                    for node_id in group_a.iter().chain(group_b.iter()) {
                        self.heal_node_partition(*node_id).await?;
                    }
                }
                PartitionModel::Island { isolated_node, remaining_nodes } => {
                    self.heal_node_partition(*isolated_node).await?;
                    for node_id in remaining_nodes {
                        self.heal_node_partition(*node_id).await?;
                    }
                }
                PartitionModel::Custom { groups } => {
                    for group in groups {
                        for node_id in group {
                            self.heal_node_partition(*node_id).await?;
                        }
                    }
                }
                PartitionModel::Asymmetric { sources, destinations } => {
                    for node_id in sources.iter().chain(destinations.iter()) {
                        self.heal_node_partition(*node_id).await?;
                    }
                }
            }
        }
        
        info!("Network partition healed: {}", partition_id);
        Ok(())
    }

    /// Inject latency for a message
    pub async fn inject_latency(&self, node_id: NodeId) -> Result<Duration> {
        let conditions = self.conditions.read().await;
        if let Some(node_conditions) = conditions.get(&node_id) {
            let latency = self.calculate_latency(&node_conditions.latency).await?;
            let jitter = self.calculate_jitter(&node_conditions.jitter).await?;
            Ok(latency + jitter)
        } else {
            Ok(Duration::ZERO)
        }
    }

    /// Check if a packet should be dropped
    pub async fn should_drop_packet(&self, node_id: NodeId) -> Result<bool> {
        let conditions = self.conditions.read().await;
        if let Some(node_conditions) = conditions.get(&node_id) {
            if node_conditions.partitioned {
                return Ok(true);
            }
            
            self.should_drop_by_loss_model(&node_conditions.loss).await
        } else {
            Ok(false)
        }
    }

    /// Calculate bandwidth delay for a message
    pub async fn calculate_bandwidth_delay(&self, node_id: NodeId, message_size: usize) -> Result<Duration> {
        let conditions = self.conditions.read().await;
        if let Some(node_conditions) = conditions.get(&node_id) {
            self.calculate_bandwidth_delay_for_model(&node_conditions.bandwidth, message_size).await
        } else {
            Ok(Duration::ZERO)
        }
    }

    /// Get current network statistics
    pub async fn get_stats(&self) -> Result<NetworkStats> {
        // In a real implementation, these would be tracked throughout the simulation
        Ok(NetworkStats {
            total_packets: 0,
            dropped_packets: 0,
            total_latency_added: 0,
            total_bandwidth_throttled: 0,
            active_partitions: self.partitions.read().await.len(),
            simulation_duration: Duration::ZERO,
        })
    }

    /// Add a global network rule
    pub async fn add_rule(&self, rule: NetworkRule) -> Result<()> {
        info!("Adding network rule: {}", rule.id);
        
        let mut rules = self.global_rules.write().await;
        rules.push(rule);
        
        Ok(())
    }

    /// Remove a global network rule
    pub async fn remove_rule(&self, rule_id: &str) -> Result<()> {
        info!("Removing network rule: {}", rule_id);
        
        let mut rules = self.global_rules.write().await;
        rules.retain(|rule| rule.id != rule_id);
        
        Ok(())
    }

    // Private helper methods

    async fn apply_split_brain_partition(&self, group_a: &[NodeId], group_b: &[NodeId]) -> Result<()> {
        // Mark all nodes in group A as partitioned from group B and vice versa
        for node_id in group_a {
            let mut conditions = self.conditions.write().await;
            if let Some(node_conditions) = conditions.get_mut(node_id) {
                node_conditions.partitioned = true;
            } else {
                conditions.insert(*node_id, NetworkConditions {
                    latency: LatencyModel::Fixed { latency: Duration::ZERO },
                    loss: LossModel::None,
                    bandwidth: BandwidthModel::Unlimited,
                    partitioned: true,
                    jitter: JitterModel::None,
                });
            }
        }
        
        for node_id in group_b {
            let mut conditions = self.conditions.write().await;
            if let Some(node_conditions) = conditions.get_mut(node_id) {
                node_conditions.partitioned = true;
            } else {
                conditions.insert(*node_id, NetworkConditions {
                    latency: LatencyModel::Fixed { latency: Duration::ZERO },
                    loss: LossModel::None,
                    bandwidth: BandwidthModel::Unlimited,
                    partitioned: true,
                    jitter: JitterModel::None,
                });
            }
        }
        
        Ok(())
    }

    async fn apply_island_partition(&self, isolated_node: NodeId) -> Result<()> {
        let mut conditions = self.conditions.write().await;
        if let Some(node_conditions) = conditions.get_mut(&isolated_node) {
            node_conditions.partitioned = true;
        } else {
            conditions.insert(isolated_node, NetworkConditions {
                latency: LatencyModel::Fixed { latency: Duration::ZERO },
                loss: LossModel::None,
                bandwidth: BandwidthModel::Unlimited,
                partitioned: true,
                jitter: JitterModel::None,
            });
        }
        Ok(())
    }

    async fn apply_custom_partition(&self, groups: &[Vec<NodeId>]) -> Result<()> {
        // For custom partitions, mark all nodes as potentially partitioned
        // In a real implementation, this would be more sophisticated
        for group in groups {
            for node_id in group {
                let mut conditions = self.conditions.write().await;
                if let Some(node_conditions) = conditions.get_mut(node_id) {
                    node_conditions.partitioned = true;
                } else {
                    conditions.insert(*node_id, NetworkConditions {
                        latency: LatencyModel::Fixed { latency: Duration::ZERO },
                        loss: LossModel::None,
                        bandwidth: BandwidthModel::Unlimited,
                        partitioned: true,
                        jitter: JitterModel::None,
                    });
                }
            }
        }
        Ok(())
    }

    async fn apply_asymmetric_partition(&self, sources: &[NodeId], destinations: &[NodeId]) -> Result<()> {
        // In asymmetric partitions, sources can send but destinations cannot receive from sources
        for node_id in destinations {
            let mut conditions = self.conditions.write().await;
            if let Some(node_conditions) = conditions.get_mut(node_id) {
                node_conditions.partitioned = true;
            } else {
                conditions.insert(*node_id, NetworkConditions {
                    latency: LatencyModel::Fixed { latency: Duration::ZERO },
                    loss: LossModel::None,
                    bandwidth: BandwidthModel::Unlimited,
                    partitioned: true,
                    jitter: JitterModel::None,
                });
            }
        }
        Ok(())
    }

    async fn heal_node_partition(&self, node_id: NodeId) -> Result<()> {
        let mut conditions = self.conditions.write().await;
        if let Some(node_conditions) = conditions.get_mut(&node_id) {
            node_conditions.partitioned = false;
        }
        Ok(())
    }

    async fn calculate_latency(&self, model: &LatencyModel) -> Result<Duration> {
        let mut rng = self.rng.lock().await;
        
        match model {
            LatencyModel::Fixed { latency } => Ok(*latency),
            LatencyModel::Normal { mean, std_dev } => {
                let normal = Normal::new(mean.as_secs_f64(), std_dev.as_secs_f64())
                    .map_err(|e| anyhow::anyhow!("Invalid normal distribution: {}", e))?;
                let sample = normal.sample(&mut *rng).max(0.0);
                Ok(Duration::from_secs_f64(sample))
            }
            LatencyModel::Uniform { min, max } => {
                let uniform = Uniform::new(min.as_secs_f64(), max.as_secs_f64());
                let sample = uniform.sample(&mut *rng);
                Ok(Duration::from_secs_f64(sample))
            }
            LatencyModel::Custom { percentiles } => {
                let p = rng.gen::<f64>();
                for (percentile, latency) in percentiles {
                    if p <= *percentile {
                        return Ok(*latency);
                    }
                }
                Ok(Duration::ZERO)
            }
        }
    }

    async fn calculate_jitter(&self, model: &JitterModel) -> Result<Duration> {
        let mut rng = self.rng.lock().await;
        
        match model {
            JitterModel::None => Ok(Duration::ZERO),
            JitterModel::Fixed { amount } => Ok(*amount),
            JitterModel::Normal { mean, std_dev } => {
                let normal = Normal::new(mean.as_secs_f64(), std_dev.as_secs_f64())
                    .map_err(|e| anyhow::anyhow!("Invalid normal distribution: {}", e))?;
                let sample = normal.sample(&mut *rng).max(0.0);
                Ok(Duration::from_secs_f64(sample))
            }
            JitterModel::Custom { percentiles } => {
                let p = rng.gen::<f64>();
                for (percentile, jitter) in percentiles {
                    if p <= *percentile {
                        return Ok(*jitter);
                    }
                }
                Ok(Duration::ZERO)
            }
        }
    }

    async fn should_drop_by_loss_model(&self, model: &LossModel) -> Result<bool> {
        let mut rng = self.rng.lock().await;
        
        match model {
            LossModel::None => Ok(false),
            LossModel::Fixed { rate } => Ok(rng.gen::<f64>() < *rate),
            LossModel::Burst { base_rate, burst_probability, burst_length: _ } => {
                // Simplified burst model - in reality, this would track state
                if rng.gen::<f64>() < *burst_probability {
                    Ok(rng.gen::<f64>() < 0.9) // High loss during burst
                } else {
                    Ok(rng.gen::<f64>() < *base_rate)
                }
            }
            LossModel::GilbertElliott { good_loss_rate, bad_loss_rate, .. } => {
                // Simplified Gilbert-Elliott model - in reality, this would track state
                if rng.gen_bool(0.5) {
                    Ok(rng.gen::<f64>() < *good_loss_rate)
                } else {
                    Ok(rng.gen::<f64>() < *bad_loss_rate)
                }
            }
        }
    }

    async fn calculate_bandwidth_delay_for_model(&self, model: &BandwidthModel, message_size: usize) -> Result<Duration> {
        match model {
            BandwidthModel::Unlimited => Ok(Duration::ZERO),
            BandwidthModel::Fixed { bytes_per_second } => {
                let delay_secs = message_size as f64 / *bytes_per_second as f64;
                Ok(Duration::from_secs_f64(delay_secs))
            }
            BandwidthModel::Variable { min_bps, max_bps, .. } => {
                // Simplified variable bandwidth - in reality, this would vary over time
                let mut rng = self.rng.lock().await;
                let bps = rng.gen_range(*min_bps..=*max_bps);
                let delay_secs = message_size as f64 / bps as f64;
                Ok(Duration::from_secs_f64(delay_secs))
            }
            BandwidthModel::Congestion { base_bps, .. } => {
                // Simplified congestion model
                let delay_secs = message_size as f64 / *base_bps as f64;
                Ok(Duration::from_secs_f64(delay_secs))
            }
        }
    }
}

impl Default for NetworkConditions {
    fn default() -> Self {
        Self {
            latency: LatencyModel::Fixed { latency: Duration::ZERO },
            loss: LossModel::None,
            bandwidth: BandwidthModel::Unlimited,
            partitioned: false,
            jitter: JitterModel::None,
        }
    }
}

impl Default for NetworkSimulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience methods for common network scenarios
impl NetworkSimulator {
    /// Simulate a slow network
    pub async fn simulate_slow_network(&self, nodes: Vec<NodeId>) -> Result<()> {
        let conditions = NetworkConditions {
            latency: LatencyModel::Normal {
                mean: Duration::from_millis(100),
                std_dev: Duration::from_millis(20),
            },
            bandwidth: BandwidthModel::Fixed {
                bytes_per_second: 1024 * 1024, // 1 MB/s
            },
            ..Default::default()
        };

        for node_id in nodes {
            self.apply_conditions(node_id, conditions.clone()).await?;
        }

        Ok(())
    }

    /// Simulate a lossy network
    pub async fn simulate_lossy_network(&self, nodes: Vec<NodeId>, loss_rate: f64) -> Result<()> {
        let conditions = NetworkConditions {
            loss: LossModel::Fixed { rate: loss_rate },
            ..Default::default()
        };

        for node_id in nodes {
            self.apply_conditions(node_id, conditions.clone()).await?;
        }

        Ok(())
    }

    /// Simulate a high-latency network
    pub async fn simulate_high_latency(&self, nodes: Vec<NodeId>, latency: Duration) -> Result<()> {
        let conditions = NetworkConditions {
            latency: LatencyModel::Normal {
                mean: latency,
                std_dev: Duration::from_millis(latency.as_millis() as u64 / 10),
            },
            ..Default::default()
        };

        for node_id in nodes {
            self.apply_conditions(node_id, conditions.clone()).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_simulator_creation() {
        let simulator = NetworkSimulator::new();
        assert!(matches!(
            *simulator.state.lock().await,
            SimulatorState::Inactive
        ));
    }

    #[tokio::test]
    async fn test_network_simulator_lifecycle() {
        let simulator = NetworkSimulator::new();
        
        // Start simulator
        assert!(simulator.start().await.is_ok());
        assert!(matches!(
            *simulator.state.lock().await,
            SimulatorState::Running
        ));

        // Stop simulator
        assert!(simulator.stop().await.is_ok());
        assert!(matches!(
            *simulator.state.lock().await,
            SimulatorState::Inactive
        ));
    }

    #[tokio::test]
    async fn test_apply_network_conditions() {
        let simulator = NetworkSimulator::new();
        let node_id = Uuid::new_v4();
        
        let conditions = NetworkConditions {
            latency: LatencyModel::Fixed {
                latency: Duration::from_millis(10),
            },
            loss: LossModel::Fixed { rate: 0.1 },
            bandwidth: BandwidthModel::Fixed {
                bytes_per_second: 1024 * 1024,
            },
            partitioned: false,
            jitter: JitterModel::None,
        };

        assert!(simulator.apply_conditions(node_id, conditions).await.is_ok());
        
        // Verify conditions were applied
        let applied_conditions = simulator.conditions.read().await;
        assert!(applied_conditions.contains_key(&node_id));
    }

    #[tokio::test]
    async fn test_network_partition() {
        let simulator = NetworkSimulator::new();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let node3 = Uuid::new_v4();

        let partition_model = PartitionModel::SplitBrain {
            group_a: vec![node1],
            group_b: vec![node2, node3],
        };

        let partition_id = simulator.create_partition(partition_model).await.unwrap();
        
        // Verify partition was created
        let partitions = simulator.partitions.read().await;
        assert!(partitions.contains_key(&partition_id));
        assert!(partitions[&partition_id].active);

        // Heal partition
        assert!(simulator.heal_partition(partition_id).await.is_ok());
        
        let partitions = simulator.partitions.read().await;
        assert!(!partitions[&partition_id].active);
    }

    #[tokio::test]
    async fn test_latency_injection() {
        let simulator = NetworkSimulator::new();
        let node_id = Uuid::new_v4();

        let conditions = NetworkConditions {
            latency: LatencyModel::Fixed {
                latency: Duration::from_millis(50),
            },
            ..Default::default()
        };

        simulator.apply_conditions(node_id, conditions).await.unwrap();
        
        let injected_latency = simulator.inject_latency(node_id).await.unwrap();
        assert_eq!(injected_latency, Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_packet_loss() {
        let simulator = NetworkSimulator::new();
        let node_id = Uuid::new_v4();

        let conditions = NetworkConditions {
            loss: LossModel::Fixed { rate: 1.0 }, // 100% loss
            ..Default::default()
        };

        simulator.apply_conditions(node_id, conditions).await.unwrap();
        
        let should_drop = simulator.should_drop_packet(node_id).await.unwrap();
        assert!(should_drop);
    }

    #[tokio::test]
    async fn test_convenience_methods() {
        let simulator = NetworkSimulator::new();
        let nodes = vec![Uuid::new_v4(), Uuid::new_v4()];

        // Test slow network simulation
        assert!(simulator.simulate_slow_network(nodes.clone()).await.is_ok());
        
        // Test lossy network simulation
        assert!(simulator.simulate_lossy_network(nodes.clone(), 0.1).await.is_ok());
        
        // Test high latency simulation
        assert!(simulator.simulate_high_latency(nodes, Duration::from_millis(200)).await.is_ok());
    }
}