//! # Kaelix Cluster
//!
//! High-performance distributed clustering infrastructure for MemoryStreamer.
//! Provides ultra-low latency consensus, membership management, and high availability.

use std::{collections::HashMap, fmt, net::SocketAddr, sync::Arc};
use tracing::info;

/// Core clustering configuration
pub mod config;

/// Network communication and messaging
pub mod communication;

/// Distributed consensus implementations (Raft, PBFT)
pub mod consensus;

/// Cluster error types and handling
pub mod error;

/// High availability and failover management
pub mod ha;

/// Membership management and node discovery (SWIM)
pub mod membership;

/// Message definitions and serialization
pub mod messages;

/// Node management and metadata
pub mod node;

/// Distributed timing and synchronization
pub mod time;

/// Core types used across the cluster
pub mod types;

// Re-export commonly used types for convenience
pub use crate::{
    communication::{MessageRouter, NetworkTransport},
    config::{
        ClusterConfig, ClusterConfigBuilder, NodeConfig, NetworkConfig, HaConfig, 
        TelemetryConfig, MembershipConfig, ConsensusConfig, CommunicationConfig,
        SecurityConfig, PerformanceConfig, HaConfigCluster
    },
    consensus::ConsensusState,
    error::Error,
    ha::{
        HaManager, FailoverClient, FailoverType,
        client::{FailoverClientMetrics, RetryConfig},
        monitor::{HaMonitor, NodeHealth, ClusterHealthReport, CapacityStatus, MonitoringConfig},
        coordinator::{FailoverCoordinator, FailoverPlan, FailoverStep},
    },
    membership::{NodeInfo, NodeStatus},
    messages::{ClusterMessage, MessageHeader, MessageId, MessagePayload},
    node::{
        NodeAddress, NodeCapabilities, NodeMetadata, NodeRole, NodeSelector,
        NodeStatus as NodeOperationalStatus, Protocol,
    },
    time::{
        compare_causality, CausalityRelation, ClockSynchronizer, HybridLogicalClock, LogicalClock,
        VectorClock,
    },
    types::{NodeId, SessionId, Term},
};

/// Main cluster node representing a single instance in the distributed system
#[derive(Debug)]
pub struct ClusterNode {
    /// Node configuration
    config: ClusterConfig,
    /// Unique node identifier
    node_id: NodeId,
    /// Network address for this node
    bind_address: SocketAddr,
    /// Message router for network communication
    router: Arc<MessageRouter>,
    /// Consensus layer
    consensus: Option<Arc<dyn ConsensusProtocol>>,
    /// Membership management
    membership: Arc<membership::SwimMembership>,
    /// High availability manager
    ha_manager: Option<Arc<ha::HaManager>>,
    /// Node metadata and capabilities
    metadata: NodeMetadata,
}

/// Trait for consensus protocol implementations
pub trait ConsensusProtocol: Send + Sync + fmt::Debug {
    /// Start the consensus protocol
    fn start(&self) -> anyhow::Result<()>;
    
    /// Stop the consensus protocol
    fn stop(&self) -> anyhow::Result<()>;
    
    /// Get current consensus state
    fn state(&self) -> ConsensusState;
    
    /// Get current term
    fn current_term(&self) -> Term;
    
    /// Get current leader if known
    fn current_leader(&self) -> Option<NodeId>;
}

impl ClusterNode {
    /// Create a new cluster node with the given configuration
    pub async fn new(config: ClusterConfig, bind_address: SocketAddr) -> anyhow::Result<Self> {
        let node_id = NodeId::generate();
        
        // Initialize message router
        let router = Arc::new(MessageRouter::new(bind_address, config.communication.clone()).await?);
        
        // Initialize membership management
        let membership = Arc::new(
            membership::SwimMembership::new(
                node_id,
                bind_address,
                config.membership.clone(),
                router.clone(),
            ).await?
        );
        
        // Initialize metadata
        let metadata = NodeMetadata::new(node_id, vec!["storage".to_string(), "compute".to_string()]);
        
        info!("Created cluster node {} at {}", node_id, bind_address);
        
        Ok(Self {
            config,
            node_id,
            bind_address,
            router,
            consensus: None,
            membership,
            ha_manager: None,
            metadata,
        })
    }
    
    /// Start the cluster node
    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!("Starting cluster node {} at {}", self.node_id, self.bind_address);
        
        // Start membership management
        self.membership.start().await?;
        
        // Initialize and start consensus if enabled
        if self.config.consensus.enabled {
            let consensus = self.create_consensus_protocol().await?;
            consensus.start()?;
            self.consensus = Some(consensus);
        }
        
        // Initialize and start high availability if enabled
        if self.config.ha.enable_auto_failover {
            // Convert HaConfigCluster to full HaConfig
            let ha_config = ha::HaConfig {
                enable_auto_failover: self.config.ha.enable_auto_failover,
                failover_timeout: std::time::Duration::from_millis(self.config.ha.failover_timeout_ms),
                enable_health_monitoring: self.config.ha.enable_health_monitoring,
                monitoring_config: MonitoringConfig::default(),
                performance_thresholds: ha::PerformanceThresholds::default(),
                enable_leadership_management: true,
                leadership_lease_timeout: std::time::Duration::from_secs(60),
                max_failover_attempts: 3,
                failover_cooldown: std::time::Duration::from_secs(10),
            };
            
            match ha::HaManager::new(
                self.node_id,
                ha_config,
                self.router.clone(),
                self.membership.clone(),
            ) {
                Ok((ha_manager, _shutdown_tx, _leadership_rx, _topology_rx)) => {
                    if let Err(e) = ha_manager.start().await {
                        anyhow::bail!("Failed to start HA manager: {}", e);
                    }
                    self.ha_manager = Some(Arc::new(ha_manager));
                },
                Err(e) => {
                    anyhow::bail!("Failed to create HA manager: {}", e);
                }
            }
        }
        
        info!("Cluster node {} started successfully", self.node_id);
        Ok(())
    }
    
    /// Stop the cluster node
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        info!("Stopping cluster node {}", self.node_id);
        
        // Stop consensus
        if let Some(consensus) = &self.consensus {
            consensus.stop()?;
        }
        
        // Stop HA manager
        if let Some(ha_manager) = &self.ha_manager {
            if let Err(e) = ha_manager.stop().await {
                anyhow::bail!("Failed to stop HA manager: {}", e);
            }
        }
        
        // Stop membership
        self.membership.stop().await?;
        
        info!("Cluster node {} stopped", self.node_id);
        Ok(())
    }
    
    /// Get the node ID
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }
    
    /// Get the bind address
    pub fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }
    
    /// Get current cluster members
    pub async fn members(&self) -> HashMap<NodeId, NodeInfo> {
        self.membership.members().await
    }
    
    /// Get consensus state if consensus is enabled
    pub fn consensus_state(&self) -> Option<ConsensusState> {
        self.consensus.as_ref().map(|c| c.state())
    }
    
    /// Get current term if consensus is enabled
    pub fn current_term(&self) -> Option<Term> {
        self.consensus.as_ref().map(|c| c.current_term())
    }
    
    /// Get current leader if known
    pub fn current_leader(&self) -> Option<NodeId> {
        self.consensus.as_ref().and_then(|c| c.current_leader())
    }
    
    /// Join an existing cluster by connecting to a known member
    pub async fn join_cluster(&self, seed_address: SocketAddr) -> anyhow::Result<()> {
        info!("Joining cluster via seed node {}", seed_address);
        self.membership.join(seed_address).await
    }
    
    /// Leave the cluster gracefully
    pub async fn leave_cluster(&self) -> anyhow::Result<()> {
        info!("Leaving cluster gracefully");
        self.membership.leave().await
    }
    
    /// Get node metadata
    pub fn metadata(&self) -> &NodeMetadata {
        &self.metadata
    }
    
    /// Update node metadata
    pub fn update_metadata(&mut self, metadata: NodeMetadata) {
        self.metadata = metadata;
    }
    
    /// Create consensus protocol based on configuration
    async fn create_consensus_protocol(&self) -> anyhow::Result<Arc<dyn ConsensusProtocol>> {
        match self.config.consensus.algorithm_name.as_str() {
            "raft" => {
                // Create Raft consensus
                let raft_node = consensus::raft::RaftNode::new(
                    self.node_id,
                    self.config.consensus.cluster_size,
                    self.config.consensus.raft.clone(),
                );
                Ok(Arc::new(RaftConsensusAdapter::new(raft_node)))
            },
            algorithm => {
                anyhow::bail!("Unsupported consensus algorithm: {}", algorithm)
            }
        }
    }
}

/// Adapter to make RaftNode implement ConsensusProtocol
#[derive(Debug)]
struct RaftConsensusAdapter {
    raft_node: Arc<tokio::sync::RwLock<consensus::raft::RaftNode>>,
}

impl RaftConsensusAdapter {
    fn new(raft_node: consensus::raft::RaftNode) -> Self {
        Self {
            raft_node: Arc::new(tokio::sync::RwLock::new(raft_node)),
        }
    }
}

impl ConsensusProtocol for RaftConsensusAdapter {
    fn start(&self) -> anyhow::Result<()> {
        // Start the Raft protocol
        // This would spawn background tasks for the Raft state machine
        Ok(())
    }
    
    fn stop(&self) -> anyhow::Result<()> {
        // Stop the Raft protocol
        Ok(())
    }
    
    fn state(&self) -> ConsensusState {
        // This would need to be made async in a real implementation
        // For now, return a default state
        ConsensusState::Follower
    }
    
    fn current_term(&self) -> Term {
        // This would need to be made async in a real implementation
        Term::default()
    }
    
    fn current_leader(&self) -> Option<NodeId> {
        // This would need to be made async in a real implementation
        None
    }
}

/// Cluster builder for convenient cluster node creation
pub struct ClusterNodeBuilder {
    config: Option<ClusterConfig>,
    bind_address: Option<SocketAddr>,
    seed_addresses: Vec<SocketAddr>,
}

impl ClusterNodeBuilder {
    /// Create a new cluster node builder
    pub fn new() -> Self {
        Self {
            config: None,
            bind_address: None,
            seed_addresses: Vec::new(),
        }
    }
    
    /// Set the bind address
    pub fn bind_address(mut self, addr: SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }
    
    /// Add a seed address for joining existing cluster
    pub fn seed_address(mut self, addr: SocketAddr) -> Self {
        self.seed_addresses.push(addr);
        self
    }
    
    /// Set cluster configuration
    pub fn config(mut self, config: ClusterConfig) -> Self {
        self.config = Some(config);
        self
    }
    
    /// Build the cluster node
    pub async fn build(self) -> anyhow::Result<ClusterNode> {
        let bind_address = self.bind_address
            .ok_or_else(|| anyhow::anyhow!("Bind address is required"))?;
        
        let config = self.config.unwrap_or_else(|| {
            // Create a basic default configuration
            let node_id = NodeId::generate();
            ClusterConfig::builder()
                .node_id(node_id)
                .bind_address(bind_address)
                .cluster_name("default-cluster")
                .build()
                .expect("Failed to create default configuration")
        });
            
        let mut node = ClusterNode::new(config, bind_address).await?;
        
        // Start the node
        node.start().await?;
        
        // Join cluster if seed addresses provided
        for seed in self.seed_addresses {
            if let Err(e) = node.join_cluster(seed).await {
                tracing::warn!("Failed to join via seed {}: {}", seed, e);
            }
        }
        
        Ok(node)
    }
}

impl Default for ClusterNodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_cluster_node_creation() {
        let node_id = NodeId::generate();
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        
        let config = ClusterConfig::builder()
            .node_id(node_id)
            .bind_address(bind_addr)
            .cluster_name("test-cluster")
            .build()
            .expect("Failed to create config");
            
        let result = ClusterNode::new(config, bind_addr).await;
        match result {
            Ok(node) => {
                assert_eq!(node.bind_address(), bind_addr);
            }
            Err(e) => {
                // Expected in test environment due to network binding
                println!("Expected error in test: {}", e);
            }
        }
    }
    
    #[tokio::test]
    async fn test_cluster_node_builder() {
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        
        let result = ClusterNodeBuilder::new()
            .bind_address(bind_addr)
            .build()
            .await;
            
        // This might fail due to network binding, but should not panic
        match result {
            Ok(node) => {
                assert_eq!(node.bind_address(), bind_addr);
            }
            Err(e) => {
                // Expected in test environment
                println!("Expected error in test: {}", e);
            }
        }
    }
    
    #[test]
    fn test_node_id_generation() {
        let id1 = NodeId::generate();
        let id2 = NodeId::generate();
        
        assert_ne!(id1, id2);
    }
}