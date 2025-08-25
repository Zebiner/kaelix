//! # Integrated Raft Coordinator with WAL Integration
//!
//! This module provides a complete Raft consensus system with integrated
//! Write-Ahead Log (WAL) support for high-performance distributed coordination.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{RwLock, Mutex};
use tracing::{debug, error, info, trace, warn};

use crate::{
    communication::{NetworkMessage, MessageRouter, TcpTransport},
    consensus::{ConsensusError, ConsensusResult, ConsensusState},
    types::{NodeId, Term},
};

use super::{
    AppendEntriesRequest, AppendEntriesResponse,
    RequestVoteRequest, RequestVoteResponse,
    LogIndex, RaftNode, RaftConfig, RaftMetrics,
    log_replication::{LogReplicationManager, ReplicationMetrics, LogStats},
};

use kaelix_storage::wal::{Wal, WalConfig};

/// Integrated Raft coordinator combining consensus and log replication
#[derive(Debug)]
pub struct IntegratedRaftCoordinator {
    /// Node identifier
    node_id: NodeId,
    
    /// Core Raft consensus node
    raft_node: Arc<RwLock<RaftNode>>,
    
    /// Log replication manager with WAL integration
    log_manager: Arc<LogReplicationManager>,
    
    /// Network message router
    message_router: Arc<MessageRouter>,
    
    /// Cluster membership (peer nodes)
    cluster_members: Arc<RwLock<HashMap<NodeId, PeerInfo>>>,
    
    /// Configuration
    config: Arc<RaftConfig>,
    
    /// Coordinator metrics
    metrics: Arc<RwLock<CoordinatorMetrics>>,
    
    /// Shutdown coordination
    shutdown_sender: Option<tokio::sync::watch::Sender<bool>>,
    shutdown_receiver: tokio::sync::watch::Receiver<bool>,
    
    /// Background task handles
    task_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    
    /// Request timeout tracking
    request_timeouts: Arc<RwLock<HashMap<u64, Instant>>>,
}

/// Information about cluster peers
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Peer node ID
    pub node_id: NodeId,
    /// Network address
    pub address: std::net::SocketAddr,
    /// Last contact time
    pub last_contact: Option<Instant>,
    /// Connection status
    pub connected: bool,
    /// Round-trip time measurements
    pub rtt_samples: Vec<Duration>,
}

/// Comprehensive coordinator metrics
#[derive(Debug, Default, Clone)]
pub struct CoordinatorMetrics {
    /// Total messages processed
    pub messages_processed: u64,
    /// Raft messages sent
    pub raft_messages_sent: u64,
    /// Raft messages received
    pub raft_messages_received: u64,
    /// Elections initiated
    pub elections_initiated: u64,
    /// Elections completed
    pub elections_completed: u64,
    /// Average message processing latency
    pub avg_message_latency: Duration,
    /// Network round-trip times
    pub network_rtt_samples: Vec<Duration>,
    /// Leader changes
    pub leader_changes: u64,
    /// Last leadership change
    pub last_leader_change: Option<Instant>,
    /// Total log entries replicated
    pub log_entries_replicated: u64,
    /// Commit operations performed
    pub commit_operations: u64,
    /// Error count
    pub error_count: u64,
}

/// Coordinator configuration
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Base Raft configuration
    pub raft_config: RaftConfig,
    /// WAL configuration
    pub wal_config: WalConfig,
    /// Network bind address
    pub bind_address: std::net::SocketAddr,
    /// Request timeout duration
    pub request_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Metrics reporting interval
    pub metrics_interval: Duration,
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    /// Enable performance optimizations
    pub enable_optimizations: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            raft_config: RaftConfig::default(),
            wal_config: WalConfig::default(),
            bind_address: "127.0.0.1:0".parse().unwrap(),
            request_timeout: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(10),
            metrics_interval: Duration::from_secs(30),
            max_concurrent_requests: 1000,
            enable_optimizations: true,
        }
    }
}

impl IntegratedRaftCoordinator {
    /// Create a new integrated Raft coordinator
    pub async fn new(
        node_id: NodeId,
        cluster_size: usize,
        config: CoordinatorConfig,
    ) -> ConsensusResult<Self> {
        info!("Creating integrated Raft coordinator for node {} (cluster size: {})", 
            node_id, cluster_size);

        // Create WAL instance
        let wal = Arc::new(Wal::new(config.wal_config.clone()).await
            .map_err(|e| ConsensusError::Generic(format!("Failed to create WAL: {}", e)))?);

        // Create Raft node
        let raft_node = Arc::new(RwLock::new(
            RaftNode::new(node_id, cluster_size, config.raft_config.clone())
        ));

        // Create log replication manager
        let log_manager = Arc::new(
            LogReplicationManager::new(node_id, wal, Arc::new(config.raft_config.clone()))
                .await?
        );

        // Create network transport and router
        let transport = TcpTransport::new(config.bind_address)
            .await
            .map_err(|e| ConsensusError::Generic(format!("Failed to create transport: {}", e)))?;
        let message_router = Arc::new(MessageRouter::new(node_id, transport));

        // Create shutdown coordination
        let (shutdown_sender, shutdown_receiver) = tokio::sync::watch::channel(false);

        let coordinator = Self {
            node_id,
            raft_node,
            log_manager,
            message_router,
            cluster_members: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config.raft_config),
            metrics: Arc::new(RwLock::new(CoordinatorMetrics::default())),
            shutdown_sender: Some(shutdown_sender),
            shutdown_receiver,
            task_handles: Arc::new(Mutex::new(Vec::new())),
            request_timeouts: Arc::new(RwLock::new(HashMap::new())),
        };

        Ok(coordinator)
    }

    /// Start the coordinator with all background tasks
    pub async fn start(&mut self) -> ConsensusResult<()> {
        info!("Starting integrated Raft coordinator for node {}", self.node_id);

        // Start network transport listening
        self.message_router.transport().start_listening()
            .await
            .map_err(|e| ConsensusError::Generic(format!("Failed to start listening: {}", e)))?;

        // Start background tasks
        self.start_background_tasks().await?;

        info!("Integrated Raft coordinator started successfully for node {}", self.node_id);
        Ok(())
    }

    /// Start all background tasks
    async fn start_background_tasks(&mut self) -> ConsensusResult<()> {
        let mut task_handles = self.task_handles.lock().await;
        
        // Start consensus loop
        task_handles.push(self.start_consensus_loop().await);
        
        // Start message processing loop
        task_handles.push(self.start_message_processing().await);
        
        // Start health monitoring
        task_handles.push(self.start_health_monitoring().await);
        
        // Start metrics reporting
        task_handles.push(self.start_metrics_reporting().await);
        
        // Start request timeout cleanup
        task_handles.push(self.start_timeout_cleanup().await);

        Ok(())
    }

    /// Start the main consensus loop
    async fn start_consensus_loop(&self) -> tokio::task::JoinHandle<()> {
        let raft_node = Arc::clone(&self.raft_node);
        let config = Arc::clone(&self.config);
        let shutdown_receiver = self.shutdown_receiver.clone();
        let node_id = self.node_id;

        tokio::spawn(async move {
            let mut heartbeat_interval = tokio::time::interval(config.heartbeat_interval);
            let mut shutdown_receiver = shutdown_receiver;

            loop {
                tokio::select! {
                    _ = heartbeat_interval.tick() => {
                        if let Err(e) = Self::consensus_tick(&raft_node, node_id).await {
                            error!("Consensus tick error for node {}: {}", node_id, e);
                        }
                    }
                    _ = shutdown_receiver.changed() => {
                        if *shutdown_receiver.borrow() {
                            info!("Shutting down consensus loop for node {}", node_id);
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Perform a single consensus tick
    async fn consensus_tick(
        raft_node: &Arc<RwLock<RaftNode>>,
        node_id: NodeId,
    ) -> ConsensusResult<()> {
        let mut node = raft_node.write().await;
        
        match node.state() {
            ConsensusState::Leader => {
                // Leader tasks: send heartbeats, check lease
                if !node.is_leadership_lease_valid() {
                    warn!("Leadership lease expired for node {}, stepping down", node_id);
                    let current_term = node.current_term();
                    node.become_follower(current_term)?;
                }
                trace!("Leader {} maintaining leadership", node_id);
            }
            ConsensusState::Follower | ConsensusState::Candidate => {
                // Check for election timeout
                if node.is_election_timeout() {
                    debug!("Node {} detected election timeout, starting election", node_id);
                    let _vote_request = node.start_election().await?;
                    // TODO: Broadcast vote request via message router
                }
            }
        }
        
        Ok(())
    }

    /// Start message processing loop
    async fn start_message_processing(&self) -> tokio::task::JoinHandle<()> {
        let coordinator = self.clone_for_task();
        
        tokio::spawn(async move {
            // This would integrate with actual network message receiving
            // For now, it's a placeholder that could handle incoming messages
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                // Check for shutdown
                if *coordinator.shutdown_receiver.borrow() {
                    info!("Shutting down message processing for node {}", coordinator.node_id);
                    break;
                }
                
                // TODO: Process incoming network messages
                // This would receive messages from the network transport and route them
                // to the appropriate handlers (vote requests, append entries, etc.)
            }
        })
    }

    /// Start health monitoring
    async fn start_health_monitoring(&self) -> tokio::task::JoinHandle<()> {
        let coordinator = self.clone_for_task();
        
        tokio::spawn(async move {
            let mut health_interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                tokio::select! {
                    _ = health_interval.tick() => {
                        if let Err(e) = coordinator.perform_health_check().await {
                            error!("Health check failed for node {}: {}", coordinator.node_id, e);
                        }
                    }
                    _ = coordinator.shutdown_receiver.changed() => {
                        if *coordinator.shutdown_receiver.borrow() {
                            info!("Shutting down health monitoring for node {}", coordinator.node_id);
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Start metrics reporting
    async fn start_metrics_reporting(&self) -> tokio::task::JoinHandle<()> {
        let coordinator = self.clone_for_task();
        
        tokio::spawn(async move {
            let mut metrics_interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = metrics_interval.tick() => {
                        coordinator.report_metrics().await;
                    }
                    _ = coordinator.shutdown_receiver.changed() => {
                        if *coordinator.shutdown_receiver.borrow() {
                            info!("Shutting down metrics reporting for node {}", coordinator.node_id);
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Start request timeout cleanup
    async fn start_timeout_cleanup(&self) -> tokio::task::JoinHandle<()> {
        let coordinator = self.clone_for_task();
        
        tokio::spawn(async move {
            let mut cleanup_interval = tokio::time::interval(Duration::from_secs(5));
            
            loop {
                tokio::select! {
                    _ = cleanup_interval.tick() => {
                        coordinator.cleanup_expired_requests().await;
                    }
                    _ = coordinator.shutdown_receiver.changed() => {
                        if *coordinator.shutdown_receiver.borrow() {
                            info!("Shutting down timeout cleanup for node {}", coordinator.node_id);
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Perform health check on cluster members
    async fn perform_health_check(&self) -> ConsensusResult<()> {
        let cluster_members = self.cluster_members.read().await;
        
        for (peer_id, peer_info) in cluster_members.iter() {
            // Check if peer is responsive
            if let Some(last_contact) = peer_info.last_contact {
                if last_contact.elapsed() > Duration::from_secs(30) {
                    warn!("Peer {} has not responded for {}s", 
                        peer_id, last_contact.elapsed().as_secs());
                    
                    // TODO: Implement peer health recovery
                    // This could involve:
                    // 1. Sending ping messages
                    // 2. Attempting reconnection
                    // 3. Updating cluster membership
                }
            }
        }
        
        Ok(())
    }

    /// Report comprehensive metrics
    async fn report_metrics(&self) {
        let metrics = self.metrics.read().await;
        let raft_metrics = {
            let node = self.raft_node.read().await;
            node.metrics().clone()
        };
        let log_stats = self.log_manager.get_log_stats().await;
        let replication_metrics = self.log_manager.get_metrics().await;

        info!(
            "Coordinator metrics for node {}: messages={}, elections={}/{}, leader_changes={}, log_entries={}, avg_latency={}μs",
            self.node_id,
            metrics.messages_processed,
            raft_metrics.elections_started,
            raft_metrics.elections_won,
            metrics.leader_changes,
            log_stats.total_entries,
            metrics.avg_message_latency.as_micros()
        );

        debug!(
            "Detailed metrics - Raft: elections_won={}, fast_elections={}, replication_latency={}μs, WAL: writes={}, commit_ops={}",
            raft_metrics.elections_won,
            raft_metrics.fast_elections_completed,
            replication_metrics.avg_replication_latency.as_micros(),
            replication_metrics.wal_writes,
            metrics.commit_operations
        );
    }

    /// Clean up expired requests
    async fn cleanup_expired_requests(&self) {
        let mut timeouts = self.request_timeouts.write().await;
        let now = Instant::now();
        let timeout_duration = Duration::from_secs(30);
        
        timeouts.retain(|request_id, start_time| {
            if now.duration_since(*start_time) > timeout_duration {
                debug!("Cleaning up expired request {}", request_id);
                false
            } else {
                true
            }
        });
        
        // Also cleanup WAL-level timeouts
        self.log_manager.cleanup_pending_requests(timeout_duration).await;
    }

    /// Clone coordinator for use in async tasks
    fn clone_for_task(&self) -> CoordinatorForTask {
        CoordinatorForTask {
            node_id: self.node_id,
            raft_node: Arc::clone(&self.raft_node),
            log_manager: Arc::clone(&self.log_manager),
            message_router: Arc::clone(&self.message_router),
            cluster_members: Arc::clone(&self.cluster_members),
            metrics: Arc::clone(&self.metrics),
            shutdown_receiver: self.shutdown_receiver.clone(),
            request_timeouts: Arc::clone(&self.request_timeouts),
        }
    }

    /// Add a peer to the cluster
    pub async fn add_peer(&self, node_id: NodeId, address: std::net::SocketAddr) -> ConsensusResult<()> {
        info!("Adding peer {} at {}", node_id, address);

        // Add to cluster members
        let peer_info = PeerInfo {
            node_id,
            address,
            last_contact: None,
            connected: false,
            rtt_samples: Vec::new(),
        };
        self.cluster_members.write().await.insert(node_id, peer_info);

        // Connect via transport
        self.message_router.transport().connect(node_id, address)
            .await
            .map_err(|e| ConsensusError::Generic(format!("Failed to connect to peer: {}", e)))?;

        // Initialize follower state in log manager
        self.log_manager.initialize_followers(vec![node_id]).await;

        Ok(())
    }

    /// Remove a peer from the cluster
    pub async fn remove_peer(&self, node_id: NodeId) -> ConsensusResult<()> {
        info!("Removing peer {}", node_id);

        self.cluster_members.write().await.remove(&node_id);
        
        self.message_router.transport().disconnect(&node_id)
            .await
            .map_err(|e| ConsensusError::Generic(format!("Failed to disconnect from peer: {}", e)))?;

        Ok(())
    }

    /// Handle incoming vote request
    pub async fn handle_vote_request(&self, request: RequestVoteRequest) -> ConsensusResult<RequestVoteResponse> {
        let processing_start = Instant::now();
        
        trace!("Handling vote request from {} for term {}", 
            request.candidate_id, request.term);

        let response = {
            let mut node = self.raft_node.write().await;
            node.handle_vote_request(request).await?
        };

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.raft_messages_received += 1;
            metrics.messages_processed += 1;
            
            let latency = processing_start.elapsed();
            let current_avg = metrics.avg_message_latency.as_nanos() as f64;
            let new_latency = latency.as_nanos() as f64;
            let new_avg = (current_avg * 0.9) + (new_latency * 0.1);
            metrics.avg_message_latency = Duration::from_nanos(new_avg as u64);
        }

        Ok(response)
    }

    /// Handle incoming vote response
    pub async fn handle_vote_response(&self, response: RequestVoteResponse) -> ConsensusResult<bool> {
        trace!("Handling vote response: granted={}, term={}", 
            response.vote_granted, response.term);

        let became_leader = {
            let mut node = self.raft_node.write().await;
            node.handle_vote_response(response).await?
        };

        if became_leader {
            self.on_became_leader().await?;
        }

        Ok(became_leader)
    }

    /// Handle incoming append entries request
    pub async fn handle_append_entries(&self, request: AppendEntriesRequest) -> ConsensusResult<AppendEntriesResponse> {
        let processing_start = Instant::now();
        
        trace!("Handling append entries from {} (entries: {}, commit: {})", 
            request.leader_id, request.entries.len(), request.leader_commit);

        // Handle via both Raft node and log manager for consistency
        let raft_response = {
            let mut node = self.raft_node.write().await;
            node.handle_append_entries(request.clone()).await?
        };

        // If successful, also process through log manager for WAL integration
        if raft_response.success && !request.entries.is_empty() {
            let log_response = self.log_manager.handle_append_entries_request(&request).await?;
            
            // Verify consistency between responses
            if log_response.success != raft_response.success {
                warn!("Inconsistency between Raft node and log manager responses");
            }
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.raft_messages_received += 1;
            metrics.messages_processed += 1;
            
            if !request.entries.is_empty() {
                metrics.log_entries_replicated += request.entries.len() as u64;
            }
        }

        Ok(raft_response)
    }

    /// Handle append entries response from follower
    pub async fn handle_append_entries_response(
        &self,
        follower_id: NodeId,
        response: AppendEntriesResponse,
    ) -> ConsensusResult<()> {
        trace!("Handling append entries response from {}: success={}", 
            follower_id, response.success);

        // Process through both Raft node and log manager
        {
            let mut node = self.raft_node.write().await;
            node.handle_append_entries_response(follower_id, response.clone()).await?;
        }

        self.log_manager.handle_append_entries_response(follower_id, response).await?;

        Ok(())
    }

    /// Append a new entry to the log (leader only)
    pub async fn append_entry(&self, command: Vec<u8>) -> ConsensusResult<LogIndex> {
        // Verify leadership
        {
            let node = self.raft_node.read().await;
            if !node.is_leader() {
                return Err(ConsensusError::NotLeader { leader: node.current_leader() });
            }
        }

        let current_term = {
            let node = self.raft_node.read().await;
            node.current_term()
        };

        // Append through log manager (which handles WAL integration)
        let index = self.log_manager.append_entry(current_term, command).await?;

        // Update coordinator metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.log_entries_replicated += 1;
        }

        // Trigger replication to followers
        self.replicate_to_followers().await?;

        debug!("Successfully appended entry {} to log", index);
        Ok(index)
    }

    /// Replicate entries to all followers
    async fn replicate_to_followers(&self) -> ConsensusResult<()> {
        let (current_term, commit_index) = {
            let node = self.raft_node.read().await;
            (node.current_term(), self.log_manager.commit_index())
        };

        let cluster_members = self.cluster_members.read().await;
        
        for &follower_id in cluster_members.keys() {
            if follower_id != self.node_id {
                if let Ok(request) = self.log_manager.create_append_entries_request(
                    follower_id, 
                    current_term, 
                    commit_index
                ).await {
                    
                    // Send via message router
                    let network_message = NetworkMessage::RaftAppendEntries { request };
                    
                    if let Err(e) = self.message_router.send_raft_message(&follower_id, network_message).await {
                        warn!("Failed to send append entries to {}: {}", follower_id, e);
                    } else {
                        let mut metrics = self.metrics.write().await;
                        metrics.raft_messages_sent += 1;
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle becoming leader
    async fn on_became_leader(&self) -> ConsensusResult<()> {
        info!("Node {} became leader", self.node_id);

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.leader_changes += 1;
            metrics.last_leader_change = Some(Instant::now());
        }

        // Initialize followers in log manager
        let follower_ids: Vec<NodeId> = self.cluster_members.read().await
            .keys()
            .filter(|&&id| id != self.node_id)
            .copied()
            .collect();

        self.log_manager.initialize_followers(follower_ids).await;

        // Send initial heartbeats to all followers
        self.replicate_to_followers().await?;

        Ok(())
    }

    /// Apply committed entries to state machine
    pub async fn apply_committed_entries<F>(&self, apply_fn: F) -> ConsensusResult<u64>
    where
        F: FnMut(&super::log_replication::RaftWalEntry) -> ConsensusResult<()> + Send,
    {
        // Apply through log manager (which tracks both Raft and WAL state)
        let applied_count = self.log_manager.apply_committed_entries(|raft_wal_entry| {
            // Apply the entry
            let mut apply_fn = apply_fn;
            apply_fn(raft_wal_entry)
        }).await?;

        if applied_count > 0 {
            let mut metrics = self.metrics.write().await;
            metrics.commit_operations += applied_count;
        }

        Ok(applied_count)
    }

    /// Get comprehensive coordinator status
    pub async fn get_status(&self) -> CoordinatorStatus {
        let raft_node = self.raft_node.read().await;
        let metrics = self.metrics.read().await;
        let log_stats = self.log_manager.get_log_stats().await;
        let replication_metrics = self.log_manager.get_metrics().await;

        CoordinatorStatus {
            node_id: self.node_id,
            state: raft_node.state(),
            current_term: raft_node.current_term(),
            current_leader: raft_node.current_leader(),
            is_leader: raft_node.is_leader(),
            cluster_size: raft_node.cluster_size,
            log_stats,
            replication_metrics,
            coordinator_metrics: metrics.clone(),
            connected_peers: self.cluster_members.read().await.len(),
        }
    }

    /// Shutdown the coordinator gracefully
    pub async fn shutdown(&mut self) -> ConsensusResult<()> {
        info!("Shutting down integrated Raft coordinator for node {}", self.node_id);

        // Signal shutdown to all tasks
        if let Some(sender) = self.shutdown_sender.take() {
            let _ = sender.send(true);
        }

        // Wait for all tasks to complete
        let mut task_handles = self.task_handles.lock().await;
        for handle in task_handles.drain(..) {
            if let Err(e) = handle.await {
                error!("Task shutdown error: {}", e);
            }
        }

        info!("Integrated Raft coordinator shutdown complete for node {}", self.node_id);
        Ok(())
    }
}

/// Simplified coordinator reference for async tasks
#[derive(Debug)]
struct CoordinatorForTask {
    node_id: NodeId,
    raft_node: Arc<RwLock<RaftNode>>,
    log_manager: Arc<LogReplicationManager>,
    message_router: Arc<MessageRouter>,
    cluster_members: Arc<RwLock<HashMap<NodeId, PeerInfo>>>,
    metrics: Arc<RwLock<CoordinatorMetrics>>,
    shutdown_receiver: tokio::sync::watch::Receiver<bool>,
    request_timeouts: Arc<RwLock<HashMap<u64, Instant>>>,
}

/// Comprehensive coordinator status
#[derive(Debug, Clone)]
pub struct CoordinatorStatus {
    /// Node identifier
    pub node_id: NodeId,
    /// Current consensus state
    pub state: ConsensusState,
    /// Current term
    pub current_term: Term,
    /// Current leader (if known)
    pub current_leader: Option<NodeId>,
    /// Whether this node is the leader
    pub is_leader: bool,
    /// Cluster size
    pub cluster_size: usize,
    /// Log statistics
    pub log_stats: LogStats,
    /// Replication metrics
    pub replication_metrics: ReplicationMetrics,
    /// Coordinator metrics
    pub coordinator_metrics: CoordinatorMetrics,
    /// Number of connected peers
    pub connected_peers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_coordinator() -> (IntegratedRaftCoordinator, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = CoordinatorConfig::default();
        config.wal_config.wal_dir = temp_dir.path().to_path_buf();

        let coordinator = IntegratedRaftCoordinator::new(
            NodeId::generate(),
            3,
            config,
        ).await.unwrap();

        (coordinator, temp_dir)
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let (_coordinator, _temp_dir) = create_test_coordinator().await;
        // Coordinator should be created successfully
    }

    #[tokio::test]
    async fn test_peer_management() {
        let (coordinator, _temp_dir) = create_test_coordinator().await;
        
        let peer_id = NodeId::generate();
        let peer_address: std::net::SocketAddr = "127.0.0.1:8080".parse().unwrap();
        
        coordinator.add_peer(peer_id, peer_address).await.unwrap();
        
        let members = coordinator.cluster_members.read().await;
        assert!(members.contains_key(&peer_id));
        assert_eq!(members[&peer_id].address, peer_address);
    }

    #[tokio::test]
    async fn test_coordinator_status() {
        let (coordinator, _temp_dir) = create_test_coordinator().await;
        
        let status = coordinator.get_status().await;
        assert_eq!(status.node_id, coordinator.node_id);
        assert_eq!(status.state, ConsensusState::Follower);
        assert_eq!(status.cluster_size, 3);
    }

    #[tokio::test]
    async fn test_vote_request_handling() {
        let (coordinator, _temp_dir) = create_test_coordinator().await;
        
        let request = RequestVoteRequest {
            term: Term::new(1),
            candidate_id: NodeId::generate(),
            last_log_index: 0,
            last_log_term: Term::default(),
            pre_vote: false,
            request_timestamp: 0,
            fast_election: false,
        };
        
        let response = coordinator.handle_vote_request(request).await.unwrap();
        assert!(response.vote_granted);
    }

    #[tokio::test]
    async fn test_entry_append_not_leader() {
        let (coordinator, _temp_dir) = create_test_coordinator().await;
        
        let command = b"test command".to_vec();
        let result = coordinator.append_entry(command).await;
        
        // Should fail since we're not the leader
        assert!(matches!(result, Err(ConsensusError::NotLeader { .. })));
    }
}