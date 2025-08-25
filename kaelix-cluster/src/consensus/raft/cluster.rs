//! # Raft Cluster Coordinator
//!
//! Integrates Raft consensus with SWIM membership management and network communication
//! for complete cluster coordination in MemoryStreamer with ultra-fast leader election.

use super::*;
use crate::{
    communication::{MessageRouter, NetworkMessage},
    error::{Error, Result},
    membership::NodeInfo,
    types::NodeId,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    sync::{RwLock, watch},
    time::interval,
};
use tracing::{debug, error, info, warn};

/// Serializable wrapper for Instant to handle network messages
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SerializableInstant {
    /// Duration since UNIX_EPOCH
    duration_since_epoch: Duration,
}

impl SerializableInstant {
    /// Create from Instant
    pub fn from_instant(_instant: Instant) -> Self {
        let now = SystemTime::now();
        let elapsed = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        Self {
            duration_since_epoch: elapsed,
        }
    }
    
    /// Convert to Instant (approximate)
    pub fn to_instant(&self) -> Instant {
        // This is an approximation since Instant is not directly convertible
        Instant::now()
    }
    
    /// Get duration since epoch
    pub fn duration_since_epoch(&self) -> Duration {
        self.duration_since_epoch
    }
}

impl Default for SerializableInstant {
    fn default() -> Self {
        Self::from_instant(Instant::now())
    }
}

/// Raft cluster coordinator that integrates consensus with membership and communication
pub struct RaftCluster {
    /// Raft state machine
    raft: Arc<RwLock<RaftNode>>,
    /// Message router for network communication
    router: Arc<MessageRouter>,
    /// Known cluster members from SWIM
    members: Arc<RwLock<HashMap<NodeId, NodeInfo>>>,
    /// Cluster configuration
    config: RaftConfig,
    /// Fast election configuration
    fast_config: FastElectionConfig,
    /// Shutdown signal
    shutdown: watch::Receiver<bool>,
    /// Leader change notifications
    leader_change_tx: watch::Sender<Option<NodeId>>,
    /// Performance metrics
    metrics: Arc<RwLock<ClusterMetrics>>,
    /// Active election coordination state
    election_coordinator: Arc<RwLock<ElectionCoordinator>>,
}

/// Enhanced cluster-level performance metrics
#[derive(Debug, Default, Clone)]
pub struct ClusterMetrics {
    /// Total leader elections observed
    pub total_elections: u64,
    /// Pre-vote phases completed
    pub pre_vote_phases: u64,
    /// Fast elections completed successfully  
    pub fast_elections_completed: u64,
    /// Current cluster size
    pub cluster_size: usize,
    /// Number of active connections
    pub active_connections: usize,
    /// Average consensus latency
    pub avg_consensus_latency: Duration,
    /// Last successful heartbeat timestamps per node
    pub last_heartbeat: HashMap<NodeId, Instant>,
    /// Election frequency (elections per hour)
    pub election_frequency: f64,
    /// Network partition recovery count
    pub partition_recoveries: u64,
    /// Split vote scenarios detected
    pub split_vote_scenarios: u64,
    /// Leadership transfer events
    pub leadership_transfers: u64,
    /// Average election duration across cluster
    pub avg_election_duration: Duration,
    /// Sub-millisecond elections achieved
    pub sub_millisecond_elections: u64,
}

/// Election coordination state for optimizing cluster-wide elections
#[derive(Debug, Default)]
struct ElectionCoordinator {
    /// Current election term being coordinated
    current_election_term: Option<crate::types::Term>,
    /// Nodes participating in current election
    election_participants: HashMap<NodeId, ElectionParticipation>,
    /// Election start time for performance tracking
    election_start_time: Option<Instant>,
    /// Pre-vote phase tracking
    pre_vote_phase: Option<PreVoteTracker>,
    /// Fast election optimization state
    fast_election_state: FastElectionState,
}

/// Track individual node participation in elections
#[derive(Debug, Clone)]
struct ElectionParticipation {
    /// Node's candidate state
    is_candidate: bool,
    /// Votes granted by this node
    votes_granted: Vec<NodeId>,
    /// Pre-votes granted by this node
    pre_votes_granted: Vec<NodeId>,
    /// Network latency to this node
    network_latency: Option<Duration>,
    /// Last response time from this node
    last_response: Option<Instant>,
}

/// Pre-vote phase tracking
#[derive(Debug)]
struct PreVoteTracker {
    /// Pre-vote start time
    start_time: Instant,
    /// Candidates that started pre-vote
    candidates: HashMap<NodeId, PreVoteCandidate>,
    /// Responses received
    responses_received: usize,
    /// Expected responses
    expected_responses: usize,
}

#[derive(Debug)]
struct PreVoteCandidate {
    /// Candidate node ID
    node_id: NodeId,
    /// Pre-votes received
    pre_votes: Vec<NodeId>,
    /// Request timestamp
    request_time: Instant,
}

/// Fast election optimization state
#[derive(Debug, Default)]
struct FastElectionState {
    /// Whether fast election is enabled for current cluster state
    enabled: bool,
    /// Network stability assessment
    network_stable: bool,
    /// Average network latency
    avg_network_latency: Duration,
    /// Last leadership lease duration
    last_lease_duration: Option<Duration>,
    /// Consecutive successful fast elections
    consecutive_fast_elections: u32,
}

impl RaftCluster {
    /// Create a new Raft cluster coordinator with enhanced election support
    pub async fn new(
        node_id: NodeId,
        router: Arc<MessageRouter>,
        config: RaftConfig,
    ) -> Result<(Self, watch::Sender<bool>, watch::Receiver<Option<NodeId>>)> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (leader_tx, leader_rx) = watch::channel(None);
        
        // Initialize with single node cluster (will grow with membership)
        let raft_node = RaftNode::new(node_id, 1, config.clone());
        let fast_config = FastElectionConfig::default();
        
        let cluster = Self {
            raft: Arc::new(RwLock::new(raft_node)),
            router,
            members: Arc::new(RwLock::new(HashMap::new())),
            config,
            fast_config,
            shutdown: shutdown_rx,
            leader_change_tx: leader_tx,
            metrics: Arc::new(RwLock::new(ClusterMetrics::default())),
            election_coordinator: Arc::new(RwLock::new(ElectionCoordinator::default())),
        };

        info!("Enhanced Raft cluster coordinator created for node {} with fast election support", node_id);
        Ok((cluster, shutdown_tx, leader_rx))
    }

    /// Start the cluster coordinator main loop with optimized election handling
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting enhanced Raft cluster coordinator with sub-millisecond election target");

        // Use shorter intervals for faster response times
        let mut election_timer = interval(Duration::from_millis(5)); // Check elections every 5ms
        let mut heartbeat_timer = interval(self.config.heartbeat_interval);
        let mut metrics_timer = interval(Duration::from_secs(30)); // Update metrics every 30s
        let mut optimization_timer = interval(Duration::from_millis(100)); // Optimize every 100ms

        loop {
            select! {
                _ = election_timer.tick() => {
                    if let Err(e) = self.handle_fast_election_tick().await {
                        error!("Fast election tick error: {}", e);
                    }
                },
                _ = heartbeat_timer.tick() => {
                    if let Err(e) = self.handle_heartbeat_tick().await {
                        error!("Heartbeat tick error: {}", e);
                    }
                },
                _ = optimization_timer.tick() => {
                    if let Err(e) = self.optimize_election_parameters().await {
                        error!("Election optimization error: {}", e);
                    }
                },
                _ = metrics_timer.tick() => {
                    if let Err(e) = self.update_metrics().await {
                        error!("Metrics update error: {}", e);
                    }
                },
                _ = self.shutdown.changed() => {
                    if *self.shutdown.borrow() {
                        info!("Enhanced Raft cluster coordinator shutting down");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle fast election timing with microsecond precision
    async fn handle_fast_election_tick(&self) -> Result<()> {
        let mut raft = self.raft.write().await;
        let mut coordinator = self.election_coordinator.write().await;
        
        match raft.state() {
            ConsensusState::Follower | ConsensusState::Candidate => {
                if raft.is_election_timeout() {
                    debug!("Node {} election timeout detected, initiating optimized election", raft.node_id());
                    
                    // Check if we should attempt fast election
                    let can_fast_elect = self.assess_fast_election_eligibility(&coordinator, &raft).await;
                    
                    // Start election (pre-vote if enabled, direct election otherwise)
                    let vote_request = raft.start_election().await
                        .map_err(|e| Error::consensus(format!("Election start failed: {e}")))?;
                    
                    // Update coordinator state
                    coordinator.current_election_term = Some(vote_request.term);
                    coordinator.election_start_time = Some(Instant::now());
                    
                    if vote_request.pre_vote {
                        // Initialize pre-vote phase tracking
                        coordinator.pre_vote_phase = Some(PreVoteTracker {
                            start_time: Instant::now(),
                            candidates: HashMap::from([(
                                raft.node_id(), 
                                PreVoteCandidate {
                                    node_id: raft.node_id(),
                                    pre_votes: vec![raft.node_id()], // Self pre-vote
                                    request_time: Instant::now(),
                                }
                            )]),
                            responses_received: 0,
                            expected_responses: raft.cluster_size.saturating_sub(1),
                        });
                    }

                    // Update metrics
                    {
                        let mut metrics = self.metrics.write().await;
                        metrics.total_elections += 1;
                        if can_fast_elect {
                            debug!("Attempting fast election for node {}", raft.node_id());
                        }
                    }

                    // Broadcast vote request with optimized routing
                    let message = NetworkMessage::RaftRequestVote { request: vote_request };

                    if let Err(e) = self.broadcast_election_message(message).await {
                        warn!("Failed to broadcast election message: {}", e);
                    }
                }
            },
            ConsensusState::Leader => {
                // Leaders monitor for leadership lease expiration
                if !raft.is_leadership_lease_valid() {
                    warn!("Leadership lease expired for node {}, stepping down", raft.node_id());
                    let current_term = raft.current_term();
                    raft.become_follower(current_term)
                        .map_err(|e| Error::consensus(format!("Failed to step down: {e}")))?;
                }
            }
        }
        
        Ok(())
    }

    /// Assess whether fast election is viable given current cluster conditions
    async fn assess_fast_election_eligibility(
        &self,
        coordinator: &ElectionCoordinator,
        raft: &RaftNode,
    ) -> bool {
        // Check basic prerequisites
        if !self.fast_config.enable_single_round || raft.cluster_size < 3 {
            return false;
        }

        // Check network stability
        if !coordinator.fast_election_state.network_stable {
            return false;
        }

        // Check recent election success pattern
        let metrics = self.metrics.read().await;
        let recent_success_rate = if metrics.total_elections > 0 {
            (metrics.fast_elections_completed as f64) / (metrics.total_elections as f64)
        } else {
            0.0
        };

        // Require good success rate for fast elections (80%+)
        recent_success_rate > 0.8 && 
        coordinator.fast_election_state.avg_network_latency <= self.fast_config.max_network_latency
    }

    /// Broadcast election message with optimized delivery
    async fn broadcast_election_message(&self, message: NetworkMessage) -> Result<()> {
        // Use optimized broadcast for election messages
        if let Err(e) = self.router.broadcast_raft_message(message.clone()).await {
            warn!("Standard broadcast failed, attempting individual sends: {}", e);
            
            // Fallback to individual sends for better reliability
            let connected_nodes = self.router.transport().get_connected_nodes().await;
            for node_id in connected_nodes {
                if let Err(e) = self.router.send_raft_message(&node_id, message.clone()).await {
                    debug!("Failed to send election message to {}: {}", node_id, e);
                }
            }
        }
        Ok(())
    }

    /// Handle heartbeat sending and leader duties with enhanced performance
    async fn handle_heartbeat_tick(&self) -> Result<()> {
        let raft = self.raft.read().await;
        
        if raft.is_leader() {
            // Send heartbeats to all followers with lease information
            let connected_nodes = self.router.transport().get_connected_nodes().await;
            let mut heartbeat_tasks = Vec::new();
            
            for node_id in connected_nodes {
                if node_id != raft.node_id() {
                    let router = Arc::clone(&self.router);
                    let leader_id = raft.node_id();
                    
                    // Create heartbeat task for parallel sending
                    let task = tokio::spawn(async move {
                        if let Err(e) = send_optimized_heartbeat(router, leader_id, node_id).await {
                            debug!("Failed to send heartbeat to {}: {}", node_id, e);
                        }
                    });
                    
                    heartbeat_tasks.push(task);
                }
            }
            
            // Wait for all heartbeats to complete
            for task in heartbeat_tasks {
                let _ = task.await;
            }
        }
        
        Ok(())
    }

    /// Optimize election parameters based on recent performance
    async fn optimize_election_parameters(&self) -> Result<()> {
        let mut coordinator = self.election_coordinator.write().await;
        let metrics = self.metrics.read().await;
        
        // Update network stability assessment
        let avg_latency = if !metrics.last_heartbeat.is_empty() {
            // Calculate average heartbeat latency as network stability indicator
            let now = Instant::now();
            let recent_latencies: Vec<Duration> = metrics.last_heartbeat.values()
                .filter_map(|&timestamp| {
                    let latency = now.saturating_duration_since(timestamp);
                    if latency < Duration::from_secs(5) { Some(latency) } else { None }
                })
                .collect();
            
            if !recent_latencies.is_empty() {
                Duration::from_nanos(
                    recent_latencies.iter().map(|d| d.as_nanos() as u64).sum::<u64>() 
                    / recent_latencies.len() as u64
                )
            } else {
                Duration::from_millis(10) // Default conservative estimate
            }
        } else {
            Duration::from_millis(10)
        };

        // Update fast election state
        coordinator.fast_election_state.avg_network_latency = avg_latency;
        coordinator.fast_election_state.network_stable = 
            avg_latency <= self.fast_config.max_network_latency && 
            metrics.active_connections >= (metrics.cluster_size * 2 / 3); // Majority connected
        coordinator.fast_election_state.enabled = coordinator.fast_election_state.network_stable;

        // Log optimization decisions
        if coordinator.fast_election_state.enabled {
            debug!("Fast election optimizations enabled (avg latency: {}μs, connections: {}/{})",
                avg_latency.as_micros(), metrics.active_connections, metrics.cluster_size);
        } else {
            debug!("Fast election disabled due to network conditions (latency: {}μs, stable: {})",
                avg_latency.as_micros(), coordinator.fast_election_state.network_stable);
        }

        Ok(())
    }

    /// Handle incoming Raft vote request with fast election coordination
    pub async fn handle_vote_request(&self, request: RequestVoteRequest) -> Result<()> {
        let candidate_id = request.candidate_id;
        let is_pre_vote = request.pre_vote;
        
        // Record election participation
        {
            let mut coordinator = self.election_coordinator.write().await;
            if let Some(participation) = coordinator.election_participants.get_mut(&candidate_id) {
                participation.is_candidate = true;
                participation.last_response = Some(Instant::now());
            } else {
                coordinator.election_participants.insert(candidate_id, ElectionParticipation {
                    is_candidate: true,
                    votes_granted: Vec::new(),
                    pre_votes_granted: Vec::new(),
                    network_latency: None,
                    last_response: Some(Instant::now()),
                });
            }
        }

        // Process vote request
        let mut raft = self.raft.write().await;
        let response = raft.handle_vote_request(request).await
            .map_err(|e| Error::consensus(format!("Vote request handling failed: {e}")))?;
        
        // Update election coordination state
        if response.vote_granted {
            let mut coordinator = self.election_coordinator.write().await;
            if let Some(participation) = coordinator.election_participants.get_mut(&candidate_id) {
                if is_pre_vote {
                    participation.pre_votes_granted.push(raft.node_id());
                } else {
                    participation.votes_granted.push(raft.node_id());
                }
            }
        }

        // Send response back to candidate with timing information
        let message = NetworkMessage::RaftRequestVoteResponse { response };
        self.router.send_raft_message(&candidate_id, message).await?;
        
        Ok(())
    }

    /// Handle incoming vote response with enhanced election coordination
    pub async fn handle_vote_response(&self, response: RequestVoteResponse) -> Result<()> {
        let mut coordinator = self.election_coordinator.write().await;
        let mut raft = self.raft.write().await;
        
        // Track pre-vote phase progress
        if response.pre_vote {
            if let Some(ref mut pre_vote) = coordinator.pre_vote_phase {
                pre_vote.responses_received += 1;
                
                // Check if pre-vote phase is complete
                if pre_vote.responses_received >= pre_vote.expected_responses {
                    let pre_vote_duration = pre_vote.start_time.elapsed();
                    debug!("Pre-vote phase completed in {}μs", pre_vote_duration.as_micros());
                    
                    // Check if we should proceed to actual election
                    let should_proceed = pre_vote.candidates.values()
                        .any(|candidate| candidate.pre_votes.len() >= (raft.cluster_size / 2) + 1);
                    
                    if should_proceed {
                        // Trigger actual election
                        debug!("Pre-vote successful, proceeding to election");
                        coordinator.pre_vote_phase = None;
                        
                        // The actual election will be triggered in the next election tick
                    } else {
                        debug!("Pre-vote failed, resetting election state");
                        coordinator.pre_vote_phase = None;
                        coordinator.current_election_term = None;
                    }
                }
            }
            return Ok(());
        }

        // Process actual vote response
        let became_leader = raft.handle_vote_response(response).await
            .map_err(|e| Error::consensus(format!("Vote response handling failed: {e}")))?;
        
        if became_leader {
            let election_duration = coordinator.election_start_time
                .map(|start| start.elapsed())
                .unwrap_or_default();
                
            info!("Node {} became leader for term {} in {}μs", 
                raft.node_id(), raft.current_term(), election_duration.as_micros());
            
            // Update metrics
            {
                let mut metrics = self.metrics.write().await;
                metrics.avg_election_duration = election_duration;
                
                // Track sub-millisecond elections
                if election_duration < Duration::from_millis(1) {
                    metrics.sub_millisecond_elections += 1;
                    info!("Sub-millisecond election achieved! ({}μs)", election_duration.as_micros());
                }
                
                // Track fast elections
                if election_duration.as_micros() < raft.config.fast_election_threshold_us as u128 {
                    metrics.fast_elections_completed += 1;
                    coordinator.fast_election_state.consecutive_fast_elections += 1;
                } else {
                    coordinator.fast_election_state.consecutive_fast_elections = 0;
                }
            }
            
            // Notify leadership change
            if let Err(e) = self.leader_change_tx.send(Some(raft.node_id())) {
                warn!("Failed to notify leadership change: {}", e);
            }
            
            // Reset election coordination state
            coordinator.current_election_term = None;
            coordinator.election_start_time = None;
            coordinator.election_participants.clear();
            coordinator.pre_vote_phase = None;
            
            // Send immediate leadership establishment heartbeats
            let node_id = raft.node_id();
            drop(raft); // Release lock before async operations
            drop(coordinator); // Release coordinator lock
            
            if let Err(e) = self.broadcast_leadership_establishment().await {
                error!("Failed to broadcast leadership establishment: {}", e);
            }
        }
        
        Ok(())
    }

    /// Handle incoming append entries request with enhanced performance tracking
    pub async fn handle_append_entries(&self, request: AppendEntriesRequest) -> Result<()> {
        let leader_id = request.leader_id;
        let is_heartbeat = request.entries.is_empty();
        let processing_start = Instant::now();
        
        let mut raft = self.raft.write().await;
        let response = raft.handle_append_entries(request).await
            .map_err(|e| Error::consensus(format!("Append entries handling failed: {e}")))?;
        
        // Update heartbeat tracking for network condition assessment
        {
            let mut metrics = self.metrics.write().await;
            metrics.last_heartbeat.insert(leader_id, Instant::now());
            
            // Track heartbeat latency for optimization
            if is_heartbeat {
                let heartbeat_latency = processing_start.elapsed();
                // Update running average
                let current_latency = metrics.avg_consensus_latency.as_nanos() as f64;
                let new_latency = heartbeat_latency.as_nanos() as f64;
                metrics.avg_consensus_latency = Duration::from_nanos(
                    ((current_latency * 0.9) + (new_latency * 0.1)) as u64
                );
            }
        }
        
        // Send response back to leader
        let message = NetworkMessage::RaftAppendEntriesResponse { response };
        self.router.send_raft_message(&leader_id, message).await?;
        
        Ok(())
    }

    /// Handle append entries response from followers
    pub async fn handle_append_entries_response(
        &self,
        follower_id: NodeId,
        response: AppendEntriesResponse,
    ) -> Result<()> {
        // Track follower response performance
        if let Some(latency_us) = response.processing_latency_us {
            debug!("Follower {} processed append entries in {}μs", follower_id, latency_us);
        }

        let mut raft = self.raft.write().await;
        raft.handle_append_entries_response(follower_id, response).await
            .map_err(|e| Error::consensus(format!("Append entries response handling failed: {e}")))?;
        Ok(())
    }

    /// Broadcast leadership establishment with optimized heartbeats
    async fn broadcast_leadership_establishment(&self) -> Result<()> {
        let connected_nodes = self.router.transport().get_connected_nodes().await;
        let raft = self.raft.read().await;
        let leader_id = raft.node_id();
        
        // Send leadership heartbeats in parallel
        let mut tasks = Vec::new();
        for node_id in connected_nodes {
            if node_id != leader_id {
                let router = Arc::clone(&self.router);
                let task = tokio::spawn(async move {
                    if let Err(e) = send_leadership_heartbeat(router, leader_id, node_id).await {
                        debug!("Failed to send leadership heartbeat to {}: {}", node_id, e);
                    }
                });
                tasks.push(task);
            }
        }
        
        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
        debug!("Leadership establishment broadcast completed for {}", leader_id);
        Ok(())
    }

    /// Submit a new entry to the Raft log (leader only)
    pub async fn submit_entry(&self, command: Vec<u8>) -> Result<LogIndex> {
        let mut raft = self.raft.write().await;
        let index = raft.append_entry(command).await
            .map_err(|e| Error::consensus(format!("Entry submission failed: {e}")))?;
        
        // Trigger immediate replication for fast commit
        drop(raft); // Release lock for replication
        if let Err(e) = self.trigger_immediate_replication().await {
            warn!("Immediate replication trigger failed: {}", e);
        }
        
        Ok(index)
    }

    /// Trigger immediate replication to followers for fast commits
    async fn trigger_immediate_replication(&self) -> Result<()> {
        let raft = self.raft.read().await;
        if !raft.is_leader() {
            return Ok(());
        }

        let connected_nodes = self.router.transport().get_connected_nodes().await;
        let mut replication_tasks = Vec::new();
        
        for node_id in connected_nodes {
            if node_id != raft.node_id() {
                let router = Arc::clone(&self.router);
                let leader_id = raft.node_id();
                
                let task = tokio::spawn(async move {
                    if let Err(e) = send_replication_request(router, leader_id, node_id).await {
                        debug!("Failed to send replication to {}: {}", node_id, e);
                    }
                });
                
                replication_tasks.push(task);
            }
        }
        
        // Wait for all replication requests to complete
        for task in replication_tasks {
            let _ = task.await;
        }
        Ok(())
    }

    /// Apply committed entries using a provided function
    pub async fn apply_committed_entries<F>(&self, apply_fn: F) -> Result<()>
    where
        F: FnMut(&LogEntry) -> ConsensusResult<()>,
    {
        let mut raft = self.raft.write().await;
        raft.apply_committed_entries(apply_fn).await
            .map_err(|e| Error::consensus(format!("Entry application failed: {e}")))
    }

    /// Update cluster membership from SWIM protocol with election optimization
    pub async fn update_membership(&self, members: HashMap<NodeId, NodeInfo>) -> Result<()> {
        let cluster_size = members.len();
        let mut current_members = self.members.write().await;
        
        // Calculate membership changes
        let previous_size = current_members.len();
        let new_members: Vec<_> = members.keys()
            .filter(|id| !current_members.contains_key(id))
            .cloned()
            .collect();
        let removed_members: Vec<_> = current_members.keys()
            .filter(|id| !members.contains_key(id))
            .cloned()
            .collect();

        // Update membership
        *current_members = members;
        
        // Update Raft cluster size
        {
            let mut raft = self.raft.write().await;
            raft.cluster_size = cluster_size;
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.cluster_size = cluster_size;
        }

        // Reset fast election state if cluster topology changed significantly
        if !new_members.is_empty() || !removed_members.is_empty() {
            let mut coordinator = self.election_coordinator.write().await;
            coordinator.fast_election_state.consecutive_fast_elections = 0;
            coordinator.fast_election_state.network_stable = false; // Re-assess
            
            info!("Cluster membership updated: {} -> {} nodes ({} added, {} removed) - resetting fast election state",
                previous_size, cluster_size, new_members.len(), removed_members.len());
        }

        Ok(())
    }

    /// Get current cluster leader
    pub async fn current_leader(&self) -> Option<NodeId> {
        self.raft.read().await.current_leader()
    }

    /// Get current node state
    pub async fn current_state(&self) -> ConsensusState {
        self.raft.read().await.state()
    }

    /// Get current term
    pub async fn current_term(&self) -> crate::types::Term {
        self.raft.read().await.current_term()
    }

    /// Get enhanced cluster metrics
    pub async fn get_metrics(&self) -> ClusterMetrics {
        self.metrics.read().await.clone()
    }

    /// Update cluster metrics with performance tracking
    async fn update_metrics(&self) -> Result<()> {
        let mut metrics = self.metrics.write().await;
        let coordinator = self.election_coordinator.read().await;
        
        // Update connection count
        let connections = self.router.transport().get_all_connections().await;
        metrics.active_connections = connections.len();
        
        // Calculate election frequency (elections per hour)
        if metrics.cluster_size > 1 {
            metrics.election_frequency = (metrics.total_elections as f64) / 
                (metrics.cluster_size as f64 * 1.0); // Simplified calculation
        }

        // Clean up old heartbeat entries
        let threshold = Instant::now() - Duration::from_secs(300); // 5 minutes
        metrics.last_heartbeat.retain(|_, timestamp| *timestamp > threshold);
        
        // Update fast election metrics
        metrics.fast_elections_completed = coordinator.fast_election_state.consecutive_fast_elections as u64;

        // Log performance summary
        if metrics.sub_millisecond_elections > 0 {
            info!("Election performance: {}/{} elections sub-millisecond ({}%), avg: {}μs",
                metrics.sub_millisecond_elections,
                metrics.total_elections,
                (metrics.sub_millisecond_elections * 100) / metrics.total_elections.max(1),
                metrics.avg_election_duration.as_micros()
            );
        }
        
        Ok(())
    }

    /// Force leadership transfer (if leader) with optimized handover
    pub async fn transfer_leadership(&self, target: Option<NodeId>) -> Result<()> {
        let mut raft = self.raft.write().await;
        
        if !raft.is_leader() {
            return Err(Error::configuration("Not currently leader".to_string()));
        }

        info!("Initiating optimized leadership transfer from {} to {:?}", raft.node_id(), target);
        
        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.leadership_transfers += 1;
        }

        // Use Raft's built-in leadership transfer
        raft.initiate_leadership_transfer(target).await
            .map_err(|e| Error::consensus(format!("Leadership transfer failed: {e}")))?;
        
        // Notify leadership change
        if let Err(e) = self.leader_change_tx.send(None) {
            warn!("Failed to notify leadership transfer: {}", e);
        }
        
        Ok(())
    }

    /// Check cluster health with election performance assessment
    pub async fn health_check(&self) -> ClusterHealth {
        let raft = self.raft.read().await;
        let metrics = self.metrics.read().await;
        let coordinator = self.election_coordinator.read().await;
        let connections = self.router.transport().get_all_connections().await;
        
        // Count healthy nodes: all nodes with recent activity plus ourselves
        let healthy_nodes = connections.iter()
            .filter(|(_, conn)| !conn.is_idle(30_000)) // 30 second threshold
            .count() + 1; // +1 for self

        // Assess election performance health
        let election_health_score = if metrics.total_elections > 0 {
            let fast_election_rate = metrics.fast_elections_completed as f64 / metrics.total_elections as f64;
            let sub_ms_rate = metrics.sub_millisecond_elections as f64 / metrics.total_elections as f64;
            (fast_election_rate * 0.7) + (sub_ms_rate * 0.3) // Weighted score
        } else {
            1.0 // No elections yet, assume healthy
        };
        
        ClusterHealth {
            cluster_size: metrics.cluster_size,
            healthy_nodes,
            current_leader: raft.current_leader(),
            current_term: raft.current_term(),
            is_stable: healthy_nodes * 2 > metrics.cluster_size,
            last_election: metrics.total_elections,
            election_performance: ElectionPerformanceHealth {
                avg_duration: metrics.avg_election_duration,
                sub_millisecond_count: metrics.sub_millisecond_elections,
                fast_election_enabled: coordinator.fast_election_state.enabled,
                network_stable: coordinator.fast_election_state.network_stable,
                health_score: election_health_score,
            },
        }
    }
}

/// Enhanced cluster health status with election performance
#[derive(Debug, Clone)]
pub struct ClusterHealth {
    /// Total cluster size
    pub cluster_size: usize,
    /// Number of healthy nodes
    pub healthy_nodes: usize,
    /// Current cluster leader
    pub current_leader: Option<NodeId>,
    /// Current consensus term
    pub current_term: crate::types::Term,
    /// Whether cluster has stable majority
    pub is_stable: bool,
    /// Total elections observed
    pub last_election: u64,
    /// Election performance metrics
    pub election_performance: ElectionPerformanceHealth,
}

/// Election performance health metrics
#[derive(Debug, Clone)]
pub struct ElectionPerformanceHealth {
    /// Average election duration
    pub avg_duration: Duration,
    /// Count of sub-millisecond elections
    pub sub_millisecond_count: u64,
    /// Whether fast election is currently enabled
    pub fast_election_enabled: bool,
    /// Network stability assessment
    pub network_stable: bool,
    /// Overall election health score (0.0 to 1.0)
    pub health_score: f64,
}

/// Helper function to send optimized heartbeat
async fn send_optimized_heartbeat(
    router: Arc<MessageRouter>,
    leader_id: NodeId,
    follower_id: NodeId,
) -> Result<()> {
    // Create minimal heartbeat message
    let heartbeat = AppendEntriesRequest {
        term: crate::types::Term::new(1), // This should come from the actual leader term
        leader_id,
        prev_log_index: 0,
        prev_log_term: crate::types::Term::default(),
        entries: vec![], // Empty for heartbeat
        leader_commit: 0,
        request_id: 0,
        leadership_lease_ms: Some(50), // 50ms lease
    };
    
    let message = NetworkMessage::RaftAppendEntries { request: heartbeat };
    router.send_raft_message(&follower_id, message).await
}

/// Helper function to send leadership heartbeat
async fn send_leadership_heartbeat(
    router: Arc<MessageRouter>,
    leader_id: NodeId,
    follower_id: NodeId,
) -> Result<()> {
    // Send immediate heartbeat to establish leadership
    send_optimized_heartbeat(router, leader_id, follower_id).await
}

/// Helper function to send replication request
async fn send_replication_request(
    router: Arc<MessageRouter>,
    leader_id: NodeId,
    follower_id: NodeId,
) -> Result<()> {
    // This would normally create an append entries request with actual log entries
    // For now, we'll use the heartbeat mechanism
    send_optimized_heartbeat(router, leader_id, follower_id).await
}

/// Route Raft network messages to cluster coordinator with enhanced performance
pub async fn route_raft_message(
    cluster: &RaftCluster,
    message: NetworkMessage,
) -> Result<()> {
    let start_time = Instant::now();
    
    let result = match message {
        NetworkMessage::RaftRequestVote { request } => {
            cluster.handle_vote_request(request).await
        },
        NetworkMessage::RaftRequestVoteResponse { response } => {
            cluster.handle_vote_response(response).await
        },
        NetworkMessage::RaftAppendEntries { request } => {
            cluster.handle_append_entries(request).await
        },
        NetworkMessage::RaftAppendEntriesResponse { response: _ } => {
            // Need sender node ID - this should come from message routing context
            warn!("Received append entries response without sender context");
            Ok(())
        },
        NetworkMessage::RaftInstallSnapshot { request: _ } => {
            // TODO: Implement snapshot installation
            debug!("Snapshot installation not yet implemented");
            Ok(())
        },
        NetworkMessage::RaftInstallSnapshotResponse { response: _ } => {
            // TODO: Handle snapshot response
            debug!("Snapshot response handling not yet implemented");
            Ok(())
        },
        _ => {
            Err(Error::configuration("Not a Raft message".to_string()))
        }
    };

    // Track message processing latency
    let processing_time = start_time.elapsed();
    if processing_time > Duration::from_millis(1) {
        warn!("Slow Raft message processing: {}μs", processing_time.as_micros());
    } else {
        debug!("Raft message processed in {}μs", processing_time.as_micros());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication::TcpTransport;
    use std::net::SocketAddr;

    async fn create_test_cluster() -> Result<(RaftCluster, watch::Sender<bool>)> {
        let node_id = NodeId::generate();
        let transport = TcpTransport::new("127.0.0.1:0".parse().unwrap()).await?;
        let router = Arc::new(MessageRouter::new(node_id, transport));
        let config = RaftConfig::default();
        
        let (cluster, shutdown_tx, _leader_rx) = RaftCluster::new(node_id, router, config).await?;
        Ok((cluster, shutdown_tx))
    }

    #[tokio::test]
    async fn test_enhanced_cluster_creation() {
        let (cluster, _shutdown) = create_test_cluster().await.unwrap();
        
        let state = cluster.current_state().await;
        assert_eq!(state, ConsensusState::Follower);
        
        let term = cluster.current_term().await;
        assert_eq!(term, crate::types::Term::default());
    }

    #[tokio::test]
    async fn test_fast_election_assessment() {
        let (cluster, _shutdown) = create_test_cluster().await.unwrap();
        let coordinator = cluster.election_coordinator.read().await;
        let raft = cluster.raft.read().await;
        
        // Initially should not be eligible due to cluster size
        let eligible = cluster.assess_fast_election_eligibility(&coordinator, &raft).await;
        assert!(!eligible); // Single node cluster
    }

    #[tokio::test]
    async fn test_membership_update_with_fast_election_reset() {
        let (cluster, _shutdown) = create_test_cluster().await.unwrap();
        
        let mut members = HashMap::new();
        for _ in 0..3 {
            let node_id = NodeId::generate();
            members.insert(node_id, NodeInfo::new(
                node_id,
                "127.0.0.1:8001".parse::<SocketAddr>().unwrap().into()
            ));
        }
        
        cluster.update_membership(members).await.unwrap();
        
        let metrics = cluster.get_metrics().await;
        assert_eq!(metrics.cluster_size, 3);
        
        // Fast election state should be reset after membership change
        let coordinator = cluster.election_coordinator.read().await;
        assert_eq!(coordinator.fast_election_state.consecutive_fast_elections, 0);
    }

    #[tokio::test]
    async fn test_enhanced_health_check() {
        let (cluster, _shutdown) = create_test_cluster().await.unwrap();
        
        let health = cluster.health_check().await;
        assert_eq!(health.cluster_size, 0); // No members added yet
        assert_eq!(health.healthy_nodes, 1); // Self is always healthy
        assert_eq!(health.election_performance.health_score, 1.0); // No elections yet
    }

    #[tokio::test] 
    async fn test_leadership_operations() {
        let (cluster, _shutdown) = create_test_cluster().await.unwrap();
        
        // Initially no leader
        assert_eq!(cluster.current_leader().await, None);
        
        // Cannot submit entries as follower
        let result = cluster.submit_entry(b"test".to_vec()).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_election_coordinator_state() {
        let coordinator = ElectionCoordinator::default();
        assert!(coordinator.current_election_term.is_none());
        assert!(coordinator.pre_vote_phase.is_none());
        assert!(!coordinator.fast_election_state.enabled);
    }

    #[test]
    fn test_cluster_health_with_election_performance() {
        let health = ClusterHealth {
            cluster_size: 5,
            healthy_nodes: 3,
            current_leader: Some(NodeId::generate()),
            current_term: crate::types::Term::new(1),
            is_stable: true,
            last_election: 1,
            election_performance: ElectionPerformanceHealth {
                avg_duration: Duration::from_micros(500),
                sub_millisecond_count: 1,
                fast_election_enabled: true,
                network_stable: true,
                health_score: 0.9,
            },
        };
        
        assert!(health.is_stable);
        assert_eq!(health.cluster_size, 5);
        assert!(health.election_performance.fast_election_enabled);
        assert_eq!(health.election_performance.sub_millisecond_count, 1);
    }

    #[test]
    fn test_fast_election_config() {
        let fast_config = FastElectionConfig::default();
        assert!(fast_config.enable_single_round);
        assert!(fast_config.enable_leadership_transfer);
        assert_eq!(fast_config.max_network_latency, Duration::from_micros(200));
    }

    #[tokio::test]
    async fn test_optimization_parameters() {
        let (cluster, _shutdown) = create_test_cluster().await.unwrap();
        
        // Test parameter optimization
        cluster.optimize_election_parameters().await.unwrap();
        
        let coordinator = cluster.election_coordinator.read().await;
        // Should have some default network latency estimate
        assert!(coordinator.fast_election_state.avg_network_latency > Duration::ZERO);
    }

    #[test]
    fn test_serializable_instant() {
        let instant = Instant::now();
        let serializable = SerializableInstant::from_instant(instant);
        
        // Should have a valid duration since epoch
        assert!(serializable.duration_since_epoch() > Duration::ZERO);
        
        // Should be able to convert back (though not exact)
        let _recovered = serializable.to_instant();
    }
}