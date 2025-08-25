//! # Replication Manager
//!
//! Core replication coordination and topology management for the distributed
//! streaming system. Integrates WAL, consensus, and membership protocols.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use kaelix_cluster::{
    consensus::raft::RaftNode,
    types::NodeId,
};

use super::{
    consistency::{ConsistencyLevel, ConsistencyManager},
    protocol::{ReplicationEntry, ReplicationProtocol},
    quorum::QuorumManager,
    recovery::RecoveryManager,
    transport::ReplicationTransport,
    ReplicationConfig, ReplicationError, Result, ReadResult, WriteResult,
    WriteAheadLog,
};

/// Role of a replica in the replication topology
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicaRole {
    /// Primary replica (leader)
    Primary,
    /// Secondary replica (follower)  
    Secondary,
    /// Read-only replica
    ReadOnly,
    /// Catching up replica (recently added)
    CatchingUp,
    /// Failed replica (temporarily unavailable)
    Failed,
}

impl ReplicaRole {
    /// Check if replica can accept writes
    pub fn can_write(&self) -> bool {
        matches!(self, ReplicaRole::Primary)
    }
    
    /// Check if replica can serve reads
    pub fn can_read(&self) -> bool {
        matches!(
            self,
            ReplicaRole::Primary | ReplicaRole::Secondary | ReplicaRole::ReadOnly
        )
    }
    
    /// Check if replica participates in consensus
    pub fn participates_in_consensus(&self) -> bool {
        matches!(self, ReplicaRole::Primary | ReplicaRole::Secondary)
    }
}

/// Information about a replica node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaNode {
    /// Node identifier
    pub node_id: NodeId,
    
    /// Network address
    pub address: SocketAddr,
    
    /// Current role
    pub role: ReplicaRole,
    
    /// Last known sequence number
    pub last_sequence: u64,
    
    /// Last heartbeat timestamp
    pub last_heartbeat: SystemTime,
    
    /// Health status
    pub is_healthy: bool,
    
    /// Lag in milliseconds from primary
    pub lag_ms: u64,
    
    /// Number of consecutive failures
    pub failure_count: u32,
}

impl ReplicaNode {
    /// Create a new replica node
    pub fn new(node_id: NodeId, address: SocketAddr, role: ReplicaRole) -> Self {
        Self {
            node_id,
            address,
            role,
            last_sequence: 0,
            last_heartbeat: SystemTime::now(),
            is_healthy: true,
            lag_ms: 0,
            failure_count: 0,
        }
    }
    
    /// Update health status based on heartbeat
    pub fn update_health(&mut self, sequence: u64) {
        self.last_sequence = sequence;
        self.last_heartbeat = SystemTime::now();
        self.is_healthy = true;
        self.failure_count = 0;
    }
    
    /// Mark as failed
    pub fn mark_failed(&mut self) {
        self.is_healthy = false;
        self.failure_count += 1;
    }
    
    /// Check if replica is behind by more than threshold
    pub fn is_lagging(&self, primary_sequence: u64, threshold_ms: u64) -> bool {
        let sequence_lag = primary_sequence.saturating_sub(self.last_sequence);
        sequence_lag > 0 && self.lag_ms > threshold_ms
    }
    
    /// Get age of last heartbeat
    pub fn heartbeat_age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.last_heartbeat)
            .unwrap_or_default()
    }
}

/// Replication topology and health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationTopology {
    /// Primary replica (if any)
    pub primary: Option<NodeId>,
    
    /// All known replicas
    pub replicas: HashMap<NodeId, ReplicaNode>,
    
    /// Current replication factor
    pub replication_factor: usize,
    
    /// Healthy replica count
    pub healthy_count: usize,
    
    /// Last topology update
    pub last_updated: SystemTime,
}

impl ReplicationTopology {
    /// Create new topology
    pub fn new(replication_factor: usize) -> Self {
        Self {
            primary: None,
            replicas: HashMap::new(),
            replication_factor,
            healthy_count: 0,
            last_updated: SystemTime::now(),
        }
    }
    
    /// Add or update replica
    pub fn update_replica(&mut self, replica: ReplicaNode) {
        let is_primary = replica.role == ReplicaRole::Primary;
        let was_healthy = self.replicas.get(&replica.node_id)
            .map(|r| r.is_healthy)
            .unwrap_or(false);
        
        // Update primary if this replica is primary
        if is_primary {
            self.primary = Some(replica.node_id);
        }
        
        self.replicas.insert(replica.node_id, replica.clone());
        
        // Update healthy count
        if replica.is_healthy && !was_healthy {
            self.healthy_count += 1;
        } else if !replica.is_healthy && was_healthy {
            self.healthy_count = self.healthy_count.saturating_sub(1);
        }
        
        self.last_updated = SystemTime::now();
    }
    
    /// Remove replica
    pub fn remove_replica(&mut self, node_id: &NodeId) -> Option<ReplicaNode> {
        if let Some(replica) = self.replicas.remove(node_id) {
            if replica.is_healthy {
                self.healthy_count = self.healthy_count.saturating_sub(1);
            }
            
            // Clear primary if this was the primary
            if self.primary == Some(*node_id) {
                self.primary = None;
            }
            
            self.last_updated = SystemTime::now();
            Some(replica)
        } else {
            None
        }
    }
    
    /// Get healthy replicas
    pub fn healthy_replicas(&self) -> Vec<&ReplicaNode> {
        self.replicas
            .values()
            .filter(|r| r.is_healthy)
            .collect()
    }
    
    /// Get secondary replicas
    pub fn secondary_replicas(&self) -> Vec<&ReplicaNode> {
        self.replicas
            .values()
            .filter(|r| r.role == ReplicaRole::Secondary && r.is_healthy)
            .collect()
    }
    
    /// Check if we have quorum
    pub fn has_quorum(&self) -> bool {
        self.healthy_count > self.replication_factor / 2
    }
    
    /// Get primary replica
    pub fn get_primary(&self) -> Option<&ReplicaNode> {
        self.primary.and_then(|id| self.replicas.get(&id))
    }
}

/// Core replication manager
///
/// Coordinates replication across multiple replicas with configurable
/// consistency guarantees and automatic failure handling.
pub struct ReplicationManager {
    /// Local node ID
    local_node_id: NodeId,
    
    /// Replication configuration
    config: ReplicationConfig,
    
    /// Current replication topology
    topology: Arc<RwLock<ReplicationTopology>>,
    
    /// WAL for reading/writing entries
    wal: Arc<WriteAheadLog>,
    
    /// Raft consensus node
    consensus: Arc<RaftNode>,
    
    /// Replication protocol handler
    protocol: ReplicationProtocol,
    
    /// Network transport
    transport: ReplicationTransport,
    
    /// Consistency manager
    consistency_manager: ConsistencyManager,
    
    /// Quorum manager
    quorum_manager: QuorumManager,
    
    /// Recovery manager
    recovery_manager: RecoveryManager,
    
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub async fn new(
        local_node_id: NodeId,
        config: ReplicationConfig,
        wal: Arc<WriteAheadLog>,
        consensus: Arc<RaftNode>,
    ) -> Result<Self> {
        let topology = Arc::new(RwLock::new(ReplicationTopology::new(config.replication_factor)));
        let protocol = ReplicationProtocol::new(config.clone());
        let transport = ReplicationTransport::new(config.network_buffer_size);
        let consistency_manager = ConsistencyManager::new(config.consistency_level);
        let quorum_manager = QuorumManager::new(config.quorum_config.clone());
        let recovery_manager = RecoveryManager::new();
        
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        
        Ok(Self {
            local_node_id,
            config,
            topology,
            wal,
            consensus,
            protocol,
            transport,
            consistency_manager,
            quorum_manager,
            recovery_manager,
            shutdown_tx,
            shutdown_rx,
        })
    }
    
    /// Start the replication manager
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting replication manager for node {}", self.local_node_id);
        
        // Start background tasks
        self.start_heartbeat_task().await;
        self.start_replication_task().await;
        self.start_recovery_task().await;
        
        // Initialize local replica in topology
        let local_replica = ReplicaNode::new(
            self.local_node_id,
            "127.0.0.1:0".parse().unwrap(), // Will be updated with actual address
            ReplicaRole::Secondary, // Start as secondary
        );
        
        {
            let mut topology = self.topology.write().await;
            topology.update_replica(local_replica);
        }
        
        info!("Replication manager started successfully");
        Ok(())
    }
    
    /// Write data with specified consistency level
    pub async fn write_consistent(
        &mut self,
        data: Vec<u8>,
        consistency_level: ConsistencyLevel,
    ) -> Result<WriteResult> {
        let start_time = Instant::now();
        
        // Check if we can perform writes
        let topology = self.topology.read().await;
        if topology.primary != Some(self.local_node_id) {
            return Err(ReplicationError::InvalidReplicaState {
                state: "Not primary replica".to_string(),
            });
        }
        drop(topology);
        
        // Write to local WAL first
        let sequence = self.wal.write_entry(&data).await?;
        
        // Create replication entry
        let replication_entry = ReplicationEntry {
            offset: sequence,
            term: self.consensus.current_term(),
            data: data.clone(),
            checksum: crc32fast::hash(&data) as u64,
            timestamp: SystemTime::now(),
            metadata: HashMap::new(),
        };
        
        // Determine target replicas based on consistency level
        let target_replicas = self.select_target_replicas(&consistency_level).await?;
        
        // Replicate to target replicas
        let replication_result = self.protocol
            .replicate_entry(replication_entry, &target_replicas)
            .await?;
        
        let consistency_satisfied = self.consistency_manager
            .verify_write_consistency(
                &consistency_level,
                replication_result.replicated_nodes.len(),
                target_replicas.len(),
            );
        
        Ok(WriteResult {
            sequence,
            replicated_count: replication_result.replicated_nodes.len(),
            replication_latency: start_time.elapsed(),
            consistency_satisfied,
            timestamp: SystemTime::now(),
        })
    }
    
    /// Read data with specified consistency level
    pub async fn read_consistent(
        &self,
        key: &str,
        consistency_level: ConsistencyLevel,
        _session_id: Option<&str>,
    ) -> Result<ReadResult> {
        let start_time = Instant::now();
        
        // For strong consistency, always read from primary
        if consistency_level == ConsistencyLevel::Strong {
            return self.read_from_primary(key).await.map(|data| ReadResult {
                data,
                sequence: 0, // Would need to track per-key sequences
                replicas_consulted: 1,
                read_latency: start_time.elapsed(),
                consistency_satisfied: true,
                timestamp: SystemTime::now(),
            });
        }
        
        // For other consistency levels, use quorum reads
        let result = self.quorum_manager
            .execute_read(key, &consistency_level)
            .await?;
        
        Ok(ReadResult {
            data: result.data,
            sequence: result.sequence,
            replicas_consulted: result.replicas_consulted,
            read_latency: start_time.elapsed(),
            consistency_satisfied: result.consensus_achieved,
            timestamp: SystemTime::now(),
        })
    }
    
    /// Add a replica to the topology
    pub async fn add_replica(&self, node_id: NodeId, address: SocketAddr) -> Result<()> {
        let replica = ReplicaNode::new(node_id, address, ReplicaRole::CatchingUp);
        
        {
            let mut topology = self.topology.write().await;
            topology.update_replica(replica);
        }
        
        // Start catch-up process for the new replica
        self.start_catch_up_for_replica(node_id).await?;
        
        info!("Added replica {} at {}", node_id, address);
        Ok(())
    }
    
    /// Remove a replica from the topology
    pub async fn remove_replica(&self, node_id: &NodeId) -> Result<()> {
        let mut topology = self.topology.write().await;
        if let Some(replica) = topology.remove_replica(node_id) {
            info!("Removed replica {} (was {:?})", node_id, replica.role);
        }
        Ok(())
    }
    
    /// Get current replication status
    pub async fn get_status(&self) -> ReplicationStatus {
        let topology = self.topology.read().await;
        let wal_offset = self.wal.get_replication_offset();
        
        ReplicationStatus {
            local_node_id: self.local_node_id,
            role: topology.replicas
                .get(&self.local_node_id)
                .map(|r| r.role)
                .unwrap_or(ReplicaRole::Failed),
            current_offset: wal_offset,
            topology: topology.clone(),
            is_healthy: topology.has_quorum(),
            last_replication: SystemTime::now(), // Would track actual last replication
        }
    }
    
    /// Shutdown the replication manager
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down replication manager");
        
        // Send shutdown signal
        let _ = self.shutdown_tx.send(());
        
        // Clean shutdown of components
        self.recovery_manager.shutdown().await;
        
        info!("Replication manager shut down successfully");
        Ok(())
    }
    
    // ========================================
    // Private Implementation Methods
    // ========================================
    
    /// Select target replicas for replication based on consistency level
    async fn select_target_replicas(&self, consistency_level: &ConsistencyLevel) -> Result<Vec<NodeId>> {
        let topology = self.topology.read().await;
        
        let mut targets = Vec::new();
        
        // For strong consistency, replicate to all healthy secondaries
        if *consistency_level == ConsistencyLevel::Strong {
            for replica in topology.secondary_replicas() {
                targets.push(replica.node_id);
            }
        } else {
            // For other levels, use a subset based on configuration
            let target_count = (topology.replication_factor - 1).min(topology.healthy_count);
            for replica in topology.secondary_replicas().iter().take(target_count) {
                targets.push(replica.node_id);
            }
        }
        
        Ok(targets)
    }
    
    /// Read data from primary replica
    async fn read_from_primary(&self, _key: &str) -> Result<Vec<u8>> {
        // Placeholder implementation
        // In real implementation, would read from WAL or storage layer
        Ok(Vec::new())
    }
    
    /// Start catch-up process for a new replica
    async fn start_catch_up_for_replica(&self, node_id: NodeId) -> Result<()> {
        debug!("Starting catch-up for replica {}", node_id);
        
        // Get current WAL offset
        let _current_offset = self.wal.get_current_offset().await.unwrap_or(0);
        
        // Start catch-up from beginning (in real implementation, would be more sophisticated)
        let entries = self.wal.get_replication_entries(0, self.config.catch_up_batch_size).await?;
        
        if !entries.is_empty() {
            // Send catch-up batch to replica
            debug!("Sending {} catch-up entries to replica {}", entries.len(), node_id);
        }
        
        Ok(())
    }
    
    /// Start heartbeat task
    async fn start_heartbeat_task(&self) {
        let topology = Arc::clone(&self.topology);
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_rx.resubscribe();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.heartbeat_interval);
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Send heartbeats to all replicas
                        let topology = topology.read().await;
                        for replica in topology.replicas.values() {
                            if replica.node_id != topology.primary.unwrap_or_default() {
                                // Send heartbeat (placeholder)
                                debug!("Sending heartbeat to replica {}", replica.node_id);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("Heartbeat task shutting down");
                        break;
                    }
                }
            }
        });
    }
    
    /// Start replication task
    async fn start_replication_task(&self) {
        let wal = Arc::clone(&self.wal);
        let _protocol = self.protocol.clone();
        let mut shutdown_rx = self.shutdown_rx.resubscribe();
        
        tokio::spawn(async move {
            // Subscribe to WAL changes
            let mut change_rx = wal.subscribe_to_changes();
            
            loop {
                tokio::select! {
                    change_notification = change_rx.recv() => {
                        match change_notification {
                            Ok(notification) => {
                                debug!("WAL change notification: {:?}", notification);
                                // Process WAL changes for replication
                            }
                            Err(_) => {
                                warn!("WAL change notification channel closed");
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("Replication task shutting down");
                        break;
                    }
                }
            }
        });
    }
    
    /// Start recovery task
    async fn start_recovery_task(&self) {
        let topology = Arc::clone(&self.topology);
        let recovery_manager = self.recovery_manager.clone();
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_rx.resubscribe();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30)); // Check every 30s
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check for failed replicas and initiate recovery
                        let mut topology = topology.write().await;
                        let mut failed_replicas = Vec::new();
                        
                        for replica in topology.replicas.values_mut() {
                            let heartbeat_age = replica.heartbeat_age();
                            if heartbeat_age > config.heartbeat_interval * 3 {
                                replica.mark_failed();
                                failed_replicas.push(replica.node_id);
                            }
                        }
                        
                        // Start recovery for failed replicas
                        for node_id in failed_replicas {
                            info!("Starting recovery for failed replica {}", node_id);
                            if let Err(e) = recovery_manager.start_recovery(node_id).await {
                                warn!("Failed to start recovery for {}: {}", node_id, e);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("Recovery task shutting down");
                        break;
                    }
                }
            }
        });
    }
}

/// Current status of the replication system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// Local node ID
    pub local_node_id: NodeId,
    
    /// Current role of this node
    pub role: ReplicaRole,
    
    /// Current WAL offset
    pub current_offset: u64,
    
    /// Current topology
    pub topology: ReplicationTopology,
    
    /// Overall health status
    pub is_healthy: bool,
    
    /// Last replication timestamp
    pub last_replication: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replica_role_permissions() {
        assert!(ReplicaRole::Primary.can_write());
        assert!(ReplicaRole::Primary.can_read());
        assert!(ReplicaRole::Primary.participates_in_consensus());
        
        assert!(!ReplicaRole::Secondary.can_write());
        assert!(ReplicaRole::Secondary.can_read());
        assert!(ReplicaRole::Secondary.participates_in_consensus());
        
        assert!(!ReplicaRole::ReadOnly.can_write());
        assert!(ReplicaRole::ReadOnly.can_read());
        assert!(!ReplicaRole::ReadOnly.participates_in_consensus());
    }

    #[test]
    fn test_replica_node_health() {
        let node_id = NodeId::generate();
        let address = "127.0.0.1:8080".parse().unwrap();
        let mut replica = ReplicaNode::new(node_id, address, ReplicaRole::Secondary);
        
        assert!(replica.is_healthy);
        assert_eq!(replica.failure_count, 0);
        
        replica.mark_failed();
        assert!(!replica.is_healthy);
        assert_eq!(replica.failure_count, 1);
        
        replica.update_health(100);
        assert!(replica.is_healthy);
        assert_eq!(replica.failure_count, 0);
        assert_eq!(replica.last_sequence, 100);
    }

    #[test]
    fn test_replication_topology() {
        let mut topology = ReplicationTopology::new(3);
        
        let node1 = NodeId::generate();
        let node2 = NodeId::generate();
        
        let replica1 = ReplicaNode::new(node1, "127.0.0.1:8080".parse().unwrap(), ReplicaRole::Primary);
        let replica2 = ReplicaNode::new(node2, "127.0.0.1:8081".parse().unwrap(), ReplicaRole::Secondary);
        
        topology.update_replica(replica1);
        topology.update_replica(replica2);
        
        assert_eq!(topology.primary, Some(node1));
        assert_eq!(topology.healthy_count, 2);
        assert!(topology.has_quorum());
        
        topology.remove_replica(&node2);
        assert_eq!(topology.healthy_count, 1);
        assert!(!topology.has_quorum()); // 1 out of 3 is not majority
    }
}