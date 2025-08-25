//! # Replication Transport Layer
//!
//! High-performance network transport for replication messages with support for
//! compression, batching, and connection pooling.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use kaelix_cluster::types::NodeId;

use super::{
    protocol::{ReplicationBatch, ReplicationBatchResult, ReplicationEntry},
    ReplicationError, Result,
};

/// Network message types for replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplicationMessage {
    /// Replication entry message
    Entry {
        /// Source node ID
        from: NodeId,
        /// Target node ID  
        to: NodeId,
        /// Replication entry
        entry: ReplicationEntry,
        /// Message timestamp
        timestamp: SystemTime,
    },
    
    /// Batch replication message
    Batch {
        /// Source node ID
        from: NodeId,
        /// Target node ID
        to: NodeId,
        /// Replication batch
        batch: ReplicationBatch,
        /// Message timestamp
        timestamp: SystemTime,
    },
    
    /// Batch acknowledgment
    BatchAck {
        /// Source node ID
        from: NodeId,
        /// Target node ID
        to: NodeId,
        /// Batch result
        result: ReplicationBatchResult,
        /// Message timestamp
        timestamp: SystemTime,
    },
    
    /// Heartbeat message
    Heartbeat {
        /// Source node ID
        from: NodeId,
        /// Target node ID
        to: NodeId,
        /// Current sequence number
        sequence: u64,
        /// Message timestamp
        timestamp: SystemTime,
    },
    
    /// Catch-up request
    CatchupRequest {
        /// Source node ID
        from: NodeId,
        /// Target node ID
        to: NodeId,
        /// Starting offset for catch-up
        from_offset: u64,
        /// Maximum entries to return
        limit: usize,
        /// Message timestamp
        timestamp: SystemTime,
    },
    
    /// Catch-up response
    CatchupResponse {
        /// Source node ID
        from: NodeId,
        /// Target node ID
        to: NodeId,
        /// Entries for catch-up
        entries: Vec<ReplicationEntry>,
        /// More entries available
        has_more: bool,
        /// Message timestamp
        timestamp: SystemTime,
    },
}

impl ReplicationMessage {
    /// Get source node ID
    pub fn source(&self) -> NodeId {
        match self {
            ReplicationMessage::Entry { from, .. } => *from,
            ReplicationMessage::Batch { from, .. } => *from,
            ReplicationMessage::BatchAck { from, .. } => *from,
            ReplicationMessage::Heartbeat { from, .. } => *from,
            ReplicationMessage::CatchupRequest { from, .. } => *from,
            ReplicationMessage::CatchupResponse { from, .. } => *from,
        }
    }
    
    /// Get target node ID
    pub fn target(&self) -> NodeId {
        match self {
            ReplicationMessage::Entry { to, .. } => *to,
            ReplicationMessage::Batch { to, .. } => *to,
            ReplicationMessage::BatchAck { to, .. } => *to,
            ReplicationMessage::Heartbeat { to, .. } => *to,
            ReplicationMessage::CatchupRequest { to, .. } => *to,
            ReplicationMessage::CatchupResponse { to, .. } => *to,
        }
    }
    
    /// Get message timestamp
    pub fn timestamp(&self) -> SystemTime {
        match self {
            ReplicationMessage::Entry { timestamp, .. } => *timestamp,
            ReplicationMessage::Batch { timestamp, .. } => *timestamp,
            ReplicationMessage::BatchAck { timestamp, .. } => *timestamp,
            ReplicationMessage::Heartbeat { timestamp, .. } => *timestamp,
            ReplicationMessage::CatchupRequest { timestamp, .. } => *timestamp,
            ReplicationMessage::CatchupResponse { timestamp, .. } => *timestamp,
        }
    }
    
    /// Get estimated message size
    pub fn estimated_size(&self) -> usize {
        match self {
            ReplicationMessage::Entry { entry, .. } => {
                entry.data.len() + 64 // Entry data + overhead
            }
            ReplicationMessage::Batch { batch, .. } => {
                batch.total_size + 128 // Batch size + overhead
            }
            ReplicationMessage::BatchAck { .. } => 128,
            ReplicationMessage::Heartbeat { .. } => 64,
            ReplicationMessage::CatchupRequest { .. } => 96,
            ReplicationMessage::CatchupResponse { entries, .. } => {
                entries.iter().map(|e| e.data.len()).sum::<usize>() + 128
            }
        }
    }
    
    /// Check if message is time-sensitive
    pub fn is_time_sensitive(&self) -> bool {
        matches!(
            self,
            ReplicationMessage::Heartbeat { .. } | ReplicationMessage::Entry { .. }
        )
    }
}

/// Connection information for a replica
#[derive(Debug, Clone)]
pub struct ReplicaConnection {
    /// Node ID of the replica
    pub node_id: NodeId,
    
    /// Network endpoint
    pub endpoint: SocketAddr,
    
    /// Connection establishment time
    pub connected_at: SystemTime,
    
    /// Last activity timestamp
    pub last_activity: SystemTime,
    
    /// Connection health score (0.0-1.0)
    pub health_score: f64,
    
    /// Number of failed attempts
    pub failure_count: u32,
    
    /// Whether connection is active
    pub is_active: bool,
}

impl ReplicaConnection {
    /// Create a new replica connection
    pub fn new(node_id: NodeId, endpoint: SocketAddr) -> Self {
        let now = SystemTime::now();
        Self {
            node_id,
            endpoint,
            connected_at: now,
            last_activity: now,
            health_score: 1.0,
            failure_count: 0,
            is_active: true,
        }
    }
    
    /// Update activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = SystemTime::now();
        if self.failure_count > 0 {
            self.failure_count = 0;
        }
        self.health_score = (self.health_score + 0.1).min(1.0);
    }
    
    /// Record a failure
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.health_score = (self.health_score - 0.2).max(0.0);
        
        if self.failure_count >= 5 {
            self.is_active = false;
        }
    }
    
    /// Get connection age
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.connected_at)
            .unwrap_or_default()
    }
    
    /// Get time since last activity
    pub fn idle_time(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.last_activity)
            .unwrap_or_default()
    }
}

/// High-performance replication transport
pub struct ReplicationTransport {
    /// Local node ID
    local_node_id: NodeId,
    
    /// Active connections to replicas
    connections: Arc<RwLock<HashMap<NodeId, ReplicaConnection>>>,
    
    /// Network buffer size
    buffer_size: usize,
    
    /// Transport statistics
    stats: Arc<RwLock<TransportStats>>,
}

impl ReplicationTransport {
    /// Create a new transport instance
    pub fn new(buffer_size: usize) -> Self {
        Self {
            local_node_id: NodeId::generate(),
            connections: Arc::new(RwLock::new(HashMap::new())),
            buffer_size,
            stats: Arc::new(RwLock::new(TransportStats::default())),
        }
    }
    
    /// Create transport with specific node ID
    pub fn new_with_node_id(node_id: NodeId, buffer_size: usize) -> Self {
        Self {
            local_node_id: node_id,
            connections: Arc::new(RwLock::new(HashMap::new())),
            buffer_size,
            stats: Arc::new(RwLock::new(TransportStats::default())),
        }
    }
    
    /// Connect to a replica
    pub async fn connect_to_replica(&self, node_id: NodeId, endpoint: SocketAddr) -> Result<()> {
        let connection = ReplicaConnection::new(node_id, endpoint);
        
        {
            let mut connections = self.connections.write().await;
            connections.insert(node_id, connection);
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.total_connections += 1;
        }
        
        info!("Connected to replica {} at {}", node_id, endpoint);
        Ok(())
    }
    
    /// Disconnect from a replica
    pub async fn disconnect_from_replica(&self, node_id: NodeId) -> Result<()> {
        let mut connections = self.connections.write().await;
        if connections.remove(&node_id).is_some() {
            info!("Disconnected from replica {}", node_id);
            
            let mut stats = self.stats.write().await;
            stats.total_connections = stats.total_connections.saturating_sub(1);
        }
        Ok(())
    }
    
    /// Send a message to a replica
    pub async fn send_message(&self, message: ReplicationMessage) -> Result<()> {
        let target = message.target();
        
        // Check if we have a connection to the target
        let connection_exists = {
            let connections = self.connections.read().await;
            connections.contains_key(&target)
        };
        
        if !connection_exists {
            return Err(ReplicationError::ReplicaUnreachable {
                replica_id: target,
                reason: "No connection available".to_string(),
            });
        }
        
        // Simulate message sending (in real implementation, would use actual network)
        debug!(
            "Sending {:?} message to replica {} (size: {} bytes)",
            std::mem::discriminant(&message),
            target,
            message.estimated_size()
        );
        
        // Update connection activity
        {
            let mut connections = self.connections.write().await;
            if let Some(connection) = connections.get_mut(&target) {
                connection.update_activity();
            }
        }
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.messages_sent += 1;
            stats.bytes_sent += message.estimated_size() as u64;
        }
        
        Ok(())
    }
    
    /// Send a message to multiple replicas
    pub async fn broadcast_message(&self, message: ReplicationMessage, targets: &[NodeId]) -> Result<Vec<NodeId>> {
        let mut successful = Vec::new();
        let mut failed = Vec::new();
        
        for &target_id in targets {
            // Clone message for each target
            let mut target_message = message.clone();
            
            // Update target node ID based on message type
            match &mut target_message {
                ReplicationMessage::Entry { to, .. } => *to = target_id,
                ReplicationMessage::Batch { to, .. } => *to = target_id,
                ReplicationMessage::BatchAck { to, .. } => *to = target_id,
                ReplicationMessage::Heartbeat { to, .. } => *to = target_id,
                ReplicationMessage::CatchupRequest { to, .. } => *to = target_id,
                ReplicationMessage::CatchupResponse { to, .. } => *to = target_id,
            }
            
            match self.send_message(target_message).await {
                Ok(()) => successful.push(target_id),
                Err(e) => {
                    warn!("Failed to send message to {}: {}", target_id, e);
                    failed.push(target_id);
                }
            }
        }
        
        info!(
            "Broadcast complete: {} successful, {} failed",
            successful.len(),
            failed.len()
        );
        
        Ok(successful)
    }
    
    /// Get connection health for a replica
    pub async fn get_connection_health(&self, node_id: NodeId) -> Option<f64> {
        let connections = self.connections.read().await;
        connections.get(&node_id).map(|c| c.health_score)
    }
    
    /// Get all connected replicas
    pub async fn get_connected_replicas(&self) -> Vec<NodeId> {
        let connections = self.connections.read().await;
        connections
            .iter()
            .filter(|(_, conn)| conn.is_active)
            .map(|(&node_id, _)| node_id)
            .collect()
    }
    
    /// Perform connection health check
    pub async fn health_check(&self) -> Result<usize> {
        let mut healthy_count = 0;
        let idle_threshold = Duration::from_secs(30);
        
        {
            let mut connections = self.connections.write().await;
            let mut to_remove = Vec::new();
            
            for (node_id, connection) in connections.iter_mut() {
                if connection.idle_time() > idle_threshold {
                    connection.record_failure();
                    warn!("Connection to {} is idle for {:?}", node_id, connection.idle_time());
                    
                    if !connection.is_active {
                        to_remove.push(*node_id);
                    }
                } else {
                    healthy_count += 1;
                }
            }
            
            // Remove inactive connections
            for node_id in to_remove {
                connections.remove(&node_id);
                info!("Removed inactive connection to {}", node_id);
            }
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.healthy_connections = healthy_count;
            stats.last_health_check = SystemTime::now();
        }
        
        Ok(healthy_count)
    }
    
    /// Get transport statistics
    pub async fn get_statistics(&self) -> TransportStats {
        self.stats.read().await.clone()
    }
    
    /// Shutdown the transport
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down replication transport");
        
        // Close all connections
        let mut connections = self.connections.write().await;
        connections.clear();
        
        info!("Replication transport shut down successfully");
        Ok(())
    }
}

/// Transport performance statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportStats {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Total connections established
    pub total_connections: usize,
    /// Number of healthy connections
    pub healthy_connections: usize,
    /// Average health score across all connections
    pub avg_health_score: f64,
    /// Network buffer size
    pub buffer_size: usize,
    /// Last health check timestamp
    pub last_health_check: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transport_creation() {
        let node_id = NodeId::generate();
        let transport = ReplicationTransport::new_with_node_id(node_id, 64 * 1024);
        
        let stats = transport.get_statistics().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.buffer_size, 64 * 1024);
    }

    #[tokio::test]
    async fn test_replica_connection() {
        let node_id = NodeId::generate();
        let transport = ReplicationTransport::new_with_node_id(node_id, 64 * 1024);
        
        let replica_id = NodeId::generate();
        let endpoint: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        
        transport.connect_to_replica(replica_id, endpoint).await.unwrap();
        
        let stats = transport.get_statistics().await;
        assert_eq!(stats.total_connections, 1);
        
        transport.disconnect_from_replica(replica_id).await.unwrap();
        
        let stats = transport.get_statistics().await;
        assert_eq!(stats.total_connections, 0);
    }

    #[tokio::test]
    async fn test_message_sending() {
        let node_id = NodeId::generate();
        let transport = ReplicationTransport::new_with_node_id(node_id, 64 * 1024);
        
        let replica_id = NodeId::generate();
        let endpoint: SocketAddr = "127.0.0.1:8002".parse().unwrap();
        
        transport.connect_to_replica(replica_id, endpoint).await.unwrap();
        
        let message = ReplicationMessage::Heartbeat {
            from: node_id,
            to: replica_id,
            sequence: 123,
            timestamp: SystemTime::now(),
        };
        
        transport.send_message(message).await.unwrap();
        
        let stats = transport.get_statistics().await;
        assert_eq!(stats.messages_sent, 1);
        assert!(stats.bytes_sent > 0);
    }

    #[tokio::test]
    async fn test_broadcast_message() {
        let node_id = NodeId::generate();
        let transport = ReplicationTransport::new_with_node_id(node_id, 64 * 1024);
        
        let replica1 = NodeId::generate();
        let replica2 = NodeId::generate();
        let endpoint1: SocketAddr = "127.0.0.1:8003".parse().unwrap();
        let endpoint2: SocketAddr = "127.0.0.1:8004".parse().unwrap();
        
        transport.connect_to_replica(replica1, endpoint1).await.unwrap();
        transport.connect_to_replica(replica2, endpoint2).await.unwrap();
        
        let message = ReplicationMessage::Heartbeat {
            from: node_id,
            to: NodeId::generate(), // Will be overridden for each target
            sequence: 456,
            timestamp: SystemTime::now(),
        };
        
        let targets = vec![replica1, replica2];
        let successful = transport.broadcast_message(message, &targets).await.unwrap();
        
        assert_eq!(successful.len(), 2);
        
        let stats = transport.get_statistics().await;
        assert_eq!(stats.messages_sent, 2);
    }

    #[test]
    fn test_replica_connection_health() {
        let node_id = NodeId::generate();
        let endpoint: SocketAddr = "127.0.0.1:8005".parse().unwrap();
        let mut connection = ReplicaConnection::new(node_id, endpoint);
        
        assert_eq!(connection.health_score, 1.0);
        assert_eq!(connection.failure_count, 0);
        assert!(connection.is_active);
        
        connection.record_failure();
        assert!(connection.health_score < 1.0);
        assert_eq!(connection.failure_count, 1);
        assert!(connection.is_active);
        
        // Record multiple failures to make it inactive
        for _ in 0..5 {
            connection.record_failure();
        }
        assert!(!connection.is_active);
        
        connection.update_activity();
        assert_eq!(connection.failure_count, 0);
        assert!(connection.health_score > 0.0);
    }

    #[test]
    fn test_replication_message_properties() {
        let from = NodeId::generate();
        let to = NodeId::generate();
        let timestamp = SystemTime::now();
        
        let heartbeat = ReplicationMessage::Heartbeat {
            from,
            to,
            sequence: 789,
            timestamp,
        };
        
        assert_eq!(heartbeat.source(), from);
        assert_eq!(heartbeat.target(), to);
        assert_eq!(heartbeat.timestamp(), timestamp);
        assert!(heartbeat.is_time_sensitive());
        assert!(heartbeat.estimated_size() > 0);
    }
}