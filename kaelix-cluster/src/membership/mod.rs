//! # Membership Management
//!
//! Provides cluster membership protocols including SWIM, failure detection,
//! and node discovery for distributed systems.

use crate::error::{Error, Result};
use crate::communication::{MessageRouter, NetworkMessage};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};
use rand::seq::SliceRandom;
use rand::thread_rng;

// Re-export NodeId from types module
pub use crate::types::NodeId;

/// Node status in the cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is alive and reachable
    Alive,
    /// Node is suspected to be down
    Suspect,
    /// Node is confirmed dead
    Dead,
    /// Node voluntarily left the cluster
    Left,
}

impl Default for NodeStatus {
    fn default() -> Self {
        Self::Alive
    }
}

/// Node information in the cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier
    pub id: NodeId,
    /// Network address for communication
    pub address: SocketAddr,
    /// Current node status
    pub status: NodeStatus,
    /// Metadata associated with the node
    pub metadata: HashMap<String, String>,
    /// Last seen timestamp (as milliseconds since epoch, for serialization)
    pub last_seen_ms: Option<i64>,
    /// Incarnation number for SWIM protocol
    pub incarnation: u64,
    /// Suspicion timestamp for failure detection (not serialized)
    #[serde(skip)]
    pub suspected_at: Option<Instant>,
}

impl NodeInfo {
    /// Create new node information
    pub fn new(id: NodeId, address: SocketAddr) -> Self {
        Self {
            id,
            address,
            status: NodeStatus::Alive,
            metadata: HashMap::new(),
            last_seen_ms: Some(chrono::Utc::now().timestamp_millis()),
            incarnation: 0,
            suspected_at: None,
        }
    }

    /// Check if node is considered alive
    pub fn is_alive(&self) -> bool {
        matches!(self.status, NodeStatus::Alive)
    }

    /// Check if node is suspected
    pub fn is_suspect(&self) -> bool {
        matches!(self.status, NodeStatus::Suspect)
    }

    /// Check if node is dead
    pub fn is_dead(&self) -> bool {
        matches!(self.status, NodeStatus::Dead)
    }

    /// Update node status
    pub fn update_status(&mut self, status: NodeStatus) {
        self.status = status;
        self.last_seen_ms = Some(chrono::Utc::now().timestamp_millis());
        
        // Reset suspicion timestamp when status changes
        if !matches!(status, NodeStatus::Suspect) {
            self.suspected_at = None;
        }
    }

    /// Increment incarnation number
    pub fn increment_incarnation(&mut self) {
        self.incarnation += 1;
    }

    /// Get last seen as Instant (for internal use)
    pub fn last_seen(&self) -> Option<Instant> {
        // Note: This is an approximation since we can't perfectly convert back
        // For production use, consider storing the instant separately or using a different approach
        self.last_seen_ms.map(|_| Instant::now())
    }

    /// Mark node as suspected
    pub fn mark_suspected(&mut self) {
        if self.is_alive() {
            self.status = NodeStatus::Suspect;
            self.suspected_at = Some(Instant::now());
            self.last_seen_ms = Some(chrono::Utc::now().timestamp_millis());
        }
    }

    /// Check if suspicion has timed out
    pub fn is_suspicion_timed_out(&self, timeout_duration: Duration) -> bool {
        if let Some(suspected_at) = self.suspected_at {
            suspected_at.elapsed() >= timeout_duration
        } else {
            false
        }
    }
}

/// SWIM protocol message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwimMessage {
    /// Direct ping message
    Ping {
        /// Source node ID
        source: NodeId,
        /// Incarnation number
        incarnation: u64,
        /// Membership updates to piggyback
        updates: Vec<NodeInfo>,
    },
    /// Ping acknowledgment
    Ack {
        /// Source node ID
        source: NodeId,
        /// Incarnation number
        incarnation: u64,
        /// Membership updates to piggyback
        updates: Vec<NodeInfo>,
    },
    /// Indirect ping request
    IndirectPing {
        /// Source node requesting indirect ping
        source: NodeId,
        /// Target node to ping indirectly
        target: NodeId,
        /// Request ID for tracking
        request_id: u64,
    },
    /// Indirect ping acknowledgment
    IndirectAck {
        /// Source node that received the ping
        source: NodeId,
        /// Target node that was pinged
        target: NodeId,
        /// Request ID for tracking
        request_id: u64,
        /// Whether the target responded
        target_responded: bool,
    },
    /// Membership update gossip
    Gossip {
        /// Source node ID
        source: NodeId,
        /// Membership updates
        updates: Vec<NodeInfo>,
    },
}

/// Phi-accrual failure detector for adaptive timeout calculation
#[derive(Debug, Clone)]
pub struct PhiFailureDetector {
    /// History of arrival intervals
    intervals: Vec<Duration>,
    /// Maximum history size
    max_history: usize,
    /// Phi threshold for failure detection
    phi_threshold: f64,
}

impl PhiFailureDetector {
    /// Create new phi-accrual failure detector
    pub fn new(max_history: usize, phi_threshold: f64) -> Self {
        Self {
            intervals: Vec::new(),
            max_history,
            phi_threshold,
        }
    }

    /// Record arrival of heartbeat
    pub fn heartbeat(&mut self, interval: Duration) {
        self.intervals.push(interval);
        if self.intervals.len() > self.max_history {
            self.intervals.remove(0);
        }
    }

    /// Calculate phi value for current interval
    pub fn phi(&self, current_interval: Duration) -> f64 {
        if self.intervals.is_empty() {
            return 0.0;
        }

        let mean = self.mean_interval();
        let variance = self.variance_interval(mean);
        let std_dev = variance.sqrt();

        if std_dev == 0.0 {
            return 0.0;
        }

        let z_score = (current_interval.as_millis() as f64 - mean) / std_dev;
        z_score.abs()
    }

    /// Check if phi value indicates failure
    pub fn is_failed(&self, current_interval: Duration) -> bool {
        self.phi(current_interval) > self.phi_threshold
    }

    /// Calculate mean interval
    fn mean_interval(&self) -> f64 {
        let sum: u128 = self.intervals.iter().map(|d| d.as_millis()).sum();
        sum as f64 / self.intervals.len() as f64
    }

    /// Calculate variance of intervals
    fn variance_interval(&self, mean: f64) -> f64 {
        let sum_squared_diffs: f64 = self.intervals
            .iter()
            .map(|d| {
                let diff = d.as_millis() as f64 - mean;
                diff * diff
            })
            .sum();
        sum_squared_diffs / self.intervals.len() as f64
    }
}

/// SWIM membership protocol implementation
pub struct SwimMembership {
    /// Local node information
    local_node: NodeInfo,
    /// Known cluster members
    members: HashMap<NodeId, NodeInfo>,
    /// Configuration
    config: SwimConfig,
    /// Suspect members awaiting confirmation
    suspects: HashSet<NodeId>,
    /// Failure detectors per node
    failure_detectors: HashMap<NodeId, PhiFailureDetector>,
    /// Last protocol round timestamp
    last_protocol_round: Instant,
    /// Pending indirect ping requests
    pending_indirect_pings: HashMap<u64, IndirectPingRequest>,
    /// Request ID counter
    request_id_counter: std::sync::atomic::AtomicU64,
}

/// Indirect ping request tracking
#[derive(Debug, Clone)]
struct IndirectPingRequest {
    /// Target node being pinged
    target: NodeId,
    /// Proxy nodes asked to ping
    proxies: Vec<NodeId>,
    /// Request timestamp
    timestamp: Instant,
    /// Number of responses received
    responses_received: usize,
}

/// SWIM protocol configuration
#[derive(Debug, Clone)]
pub struct SwimConfig {
    /// Protocol period in milliseconds
    pub protocol_period_ms: u64,
    /// Failure detection timeout
    pub failure_timeout_ms: u64,
    /// Number of indirect pings
    pub indirect_ping_count: usize,
    /// Gossip fanout factor
    pub gossip_fanout: usize,
    /// Suspicion timeout multiplier
    pub suspicion_multiplier: u32,
    /// Phi threshold for failure detection
    pub phi_threshold: f64,
    /// Maximum failure detector history
    pub max_failure_detector_history: usize,
}

impl Default for SwimConfig {
    fn default() -> Self {
        Self {
            protocol_period_ms: 1000,
            failure_timeout_ms: 5000,
            indirect_ping_count: 3,
            gossip_fanout: 3,
            suspicion_multiplier: 3,
            phi_threshold: 8.0,
            max_failure_detector_history: 100,
        }
    }
}

impl SwimMembership {
    /// Create a new SWIM membership instance
    pub fn new(local_node: NodeInfo, config: SwimConfig) -> Self {
        let mut members = HashMap::new();
        members.insert(local_node.id, local_node.clone());

        Self { 
            local_node, 
            members, 
            config, 
            suspects: HashSet::new(),
            failure_detectors: HashMap::new(),
            last_protocol_round: Instant::now(),
            pending_indirect_pings: HashMap::new(),
            request_id_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get local node information
    pub fn local_node(&self) -> &NodeInfo {
        &self.local_node
    }

    /// Get all known members
    pub fn members(&self) -> &HashMap<NodeId, NodeInfo> {
        &self.members
    }

    /// Get member count
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Get alive member count
    pub fn alive_member_count(&self) -> usize {
        self.members.values().filter(|m| m.is_alive()).count()
    }

    /// Execute periodic SWIM protocol round
    pub async fn execute_protocol_round(&mut self, router: &MessageRouter) -> Result<()> {
        let now = Instant::now();
        let round_interval = now.duration_since(self.last_protocol_round);
        
        debug!("Executing SWIM protocol round after {:?}", round_interval);

        // Check if it's time for next protocol round
        let protocol_period = Duration::from_millis(self.config.protocol_period_ms);
        if round_interval < protocol_period {
            return Ok(());
        }

        self.last_protocol_round = now;

        // 1. Select target for ping
        if let Some(target) = self.select_ping_target() {
            let target_clone = target.clone();
            debug!("Selected ping target: {}", target_clone.id);
            
            // Send direct ping
            let ping_result = self.send_ping(&target_clone, router).await;
            
            match ping_result {
                Ok(_) => {
                    debug!("Successfully sent ping to {}", target_clone.id);
                },
                Err(_) => {
                    warn!("Failed to ping {}, initiating indirect ping", target_clone.id);
                    self.handle_ping_timeout(&target_clone, router).await?;
                }
            }
        }

        // 2. Process suspicion timeouts
        self.process_suspicion_timeouts();

        // 3. Gossip membership updates
        self.gossip_updates(router).await?;

        // 4. Clean up expired indirect ping requests
        self.cleanup_expired_indirect_pings();

        Ok(())
    }

    /// Select random alive node for ping (public for benchmarking)
    pub fn select_ping_target(&self) -> Option<NodeInfo> {
        let candidates: Vec<&NodeInfo> = self
            .members
            .values()
            .filter(|node| node.id != self.local_node.id && node.is_alive())
            .collect();

        if candidates.is_empty() {
            return None;
        }

        let mut rng = thread_rng();
        candidates.choose(&mut rng).map(|&node| node.clone())
    }

    /// Send ping message to target node
    async fn send_ping(&mut self, target: &NodeInfo, router: &MessageRouter) -> Result<()> {
        let membership_updates = self.get_gossip_updates();
        
        let ping_message = SwimMessage::Ping {
            source: self.local_node.id,
            incarnation: self.local_node.incarnation,
            updates: membership_updates,
        };

        let network_message = NetworkMessage::MembershipUpdate {
            updates: bincode::serialize(&ping_message)
                .map_err(|e| Error::membership(format!("Failed to serialize ping: {e}")))?,
        };

        // Set timeout for ping
        let ping_timeout = Duration::from_millis(self.config.failure_timeout_ms);
        let send_result = timeout(
            ping_timeout,
            router.send_to_node(&target.id, network_message)
        ).await;

        match send_result {
            Ok(Ok(_)) => {
                debug!("Ping sent to {}", target.id);
                
                // Update failure detector
                let detector = self.failure_detectors
                    .entry(target.id)
                    .or_insert_with(|| PhiFailureDetector::new(
                        self.config.max_failure_detector_history,
                        self.config.phi_threshold,
                    ));
                
                detector.heartbeat(ping_timeout);
                Ok(())
            },
            Ok(Err(e)) => {
                warn!("Failed to send ping to {}: {}", target.id, e);
                Err(e)
            },
            Err(_) => {
                warn!("Ping to {} timed out", target.id);
                Err(Error::cluster_timeout("ping", ping_timeout.as_millis() as u64))
            }
        }
    }

    /// Handle ping timeout - initiate indirect ping
    pub async fn handle_ping_timeout(&mut self, target: &NodeInfo, router: &MessageRouter) -> Result<()> {
        warn!("Ping timeout for {}, initiating indirect ping", target.id);

        // Select proxy nodes for indirect ping
        let proxies = self.select_proxy_nodes(&target.id, self.config.indirect_ping_count);
        
        if proxies.is_empty() {
            warn!("No proxy nodes available for indirect ping to {}", target.id);
            self.mark_suspect(&target.id)?;
            return Ok(());
        }

        let success = self.indirect_ping(target, proxies, router).await?;
        
        if !success {
            warn!("Indirect ping to {} failed, marking as suspect", target.id);
            self.mark_suspect(&target.id)?;
        }

        Ok(())
    }

    /// Execute indirect ping through proxy nodes
    pub async fn indirect_ping(&mut self, target: &NodeInfo, proxies: Vec<NodeInfo>, router: &MessageRouter) -> Result<bool> {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        debug!("Starting indirect ping to {} via {} proxies (request_id: {})", 
               target.id, proxies.len(), request_id);

        // Track the request
        let proxy_ids: Vec<NodeId> = proxies.iter().map(|p| p.id).collect();
        let request = IndirectPingRequest {
            target: target.id,
            proxies: proxy_ids.clone(),
            timestamp: Instant::now(),
            responses_received: 0,
        };
        
        self.pending_indirect_pings.insert(request_id, request);

        // Send indirect ping requests to proxy nodes
        for proxy in proxies {
            let indirect_ping_msg = SwimMessage::IndirectPing {
                source: self.local_node.id,
                target: target.id,
                request_id,
            };

            let network_message = NetworkMessage::MembershipUpdate {
                updates: bincode::serialize(&indirect_ping_msg)
                    .map_err(|e| Error::membership(format!("Failed to serialize indirect ping: {e}")))?,
            };

            if let Err(e) = router.send_to_node(&proxy.id, network_message).await {
                warn!("Failed to send indirect ping request to proxy {}: {}", proxy.id, e);
            }
        }

        // Wait for responses with timeout
        let response_timeout = Duration::from_millis(self.config.failure_timeout_ms);
        sleep(response_timeout).await;

        // Check if any proxy confirmed target is alive
        let request = self.pending_indirect_pings.remove(&request_id);
        let success = request
            .map(|req| req.responses_received > 0)
            .unwrap_or(false);

        if success {
            debug!("Indirect ping to {} succeeded", target.id);
            self.confirm_alive(&target.id)?;
        } else {
            debug!("Indirect ping to {} failed", target.id);
        }

        Ok(success)
    }

    /// Select proxy nodes for indirect ping (public for benchmarking)
    pub fn select_proxy_nodes(&self, exclude_id: &NodeId, count: usize) -> Vec<NodeInfo> {
        let candidates: Vec<NodeInfo> = self
            .members
            .values()
            .filter(|node| {
                node.id != self.local_node.id 
                    && node.id != *exclude_id 
                    && node.is_alive()
            })
            .cloned()
            .collect();

        let mut rng = thread_rng();
        let mut proxies = candidates;
        proxies.shuffle(&mut rng);
        proxies.truncate(count);
        proxies
    }

    /// Process incoming SWIM protocol messages
    pub async fn handle_protocol_message(&mut self, message: NetworkMessage, _sender: SocketAddr, router: &MessageRouter) -> Result<()> {
        match message {
            NetworkMessage::MembershipUpdate { updates } => {
                let swim_message: SwimMessage = bincode::deserialize(&updates)
                    .map_err(|e| Error::membership(format!("Failed to deserialize SWIM message: {e}")))?;

                self.handle_swim_message(swim_message, router).await
            },
            _ => {
                debug!("Ignoring non-membership message from {}", _sender);
                Ok(())
            }
        }
    }

    /// Handle specific SWIM message types
    async fn handle_swim_message(&mut self, message: SwimMessage, router: &MessageRouter) -> Result<()> {
        match message {
            SwimMessage::Ping { source, incarnation, updates } => {
                debug!("Received ping from {}", source);
                
                // Process membership updates
                self.process_membership_update(updates)?;
                
                // Send acknowledgment
                let ack_updates = self.get_gossip_updates();
                let ack_message = SwimMessage::Ack {
                    source: self.local_node.id,
                    incarnation: self.local_node.incarnation,
                    updates: ack_updates,
                };

                let network_message = NetworkMessage::MembershipUpdate {
                    updates: bincode::serialize(&ack_message)
                        .map_err(|e| Error::membership(format!("Failed to serialize ack: {e}")))?,
                };

                router.send_to_node(&source, network_message).await?;
                
                // Update node as alive if it was suspect
                if let Some(node) = self.members.get_mut(&source) {
                    if node.is_suspect() || node.is_dead() {
                        node.update_status(NodeStatus::Alive);
                        info!("Node {} confirmed alive via ping", source);
                    }
                    node.incarnation = std::cmp::max(node.incarnation, incarnation);
                }
            },

            SwimMessage::Ack { source, incarnation, updates } => {
                debug!("Received ack from {}", source);
                
                // Process membership updates
                self.process_membership_update(updates)?;
                
                // Update node as alive
                if let Some(node) = self.members.get_mut(&source) {
                    if !node.is_alive() {
                        node.update_status(NodeStatus::Alive);
                        self.suspects.remove(&source);
                        info!("Node {} confirmed alive via ack", source);
                    }
                    node.incarnation = std::cmp::max(node.incarnation, incarnation);
                }
            },

            SwimMessage::IndirectPing { source, target, request_id } => {
                debug!("Received indirect ping request from {} for target {}", source, target);
                
                // Try to ping the target on behalf of the source
                let target_alive = if let Some(target_node) = self.members.get(&target).cloned() {
                    self.send_ping(&target_node, router).await.is_ok()
                } else {
                    false
                };

                // Send response back to source
                let response = SwimMessage::IndirectAck {
                    source: self.local_node.id,
                    target,
                    request_id,
                    target_responded: target_alive,
                };

                let network_message = NetworkMessage::MembershipUpdate {
                    updates: bincode::serialize(&response)
                        .map_err(|e| Error::membership(format!("Failed to serialize indirect ack: {e}")))?,
                };

                router.send_to_node(&source, network_message).await?;
            },

            SwimMessage::IndirectAck { source: _, target, request_id, target_responded } => {
                debug!("Received indirect ack for request {} target {} responded: {}", 
                       request_id, target, target_responded);
                
                // Update pending request
                if let Some(request) = self.pending_indirect_pings.get_mut(&request_id) {
                    request.responses_received += 1;
                    
                    if target_responded {
                        // Target is alive, update its status
                        self.confirm_alive(&target)?;
                    }
                }
            },

            SwimMessage::Gossip { source, updates } => {
                debug!("Received gossip from {} with {} updates", source, updates.len());
                self.process_membership_update(updates)?;
            },
        }

        Ok(())
    }

    /// Get membership updates to gossip
    fn get_gossip_updates(&self) -> Vec<NodeInfo> {
        // Return recent updates, prioritizing status changes
        self.members
            .values()
            .filter(|node| node.id != self.local_node.id)
            .take(self.config.gossip_fanout)
            .cloned()
            .collect()
    }

    /// Gossip membership updates to random nodes
    async fn gossip_updates(&self, router: &MessageRouter) -> Result<()> {
        let updates = self.get_gossip_updates();
        if updates.is_empty() {
            return Ok(());
        }

        let targets = self.select_gossip_targets(&self.local_node.id);
        
        for target_id in targets {
            let gossip_msg = SwimMessage::Gossip {
                source: self.local_node.id,
                updates: updates.clone(),
            };

            let network_message = NetworkMessage::MembershipUpdate {
                updates: bincode::serialize(&gossip_msg)
                    .map_err(|e| Error::membership(format!("Failed to serialize gossip: {e}")))?,
            };

            if let Err(e) = router.send_to_node(&target_id, network_message).await {
                warn!("Failed to gossip to {}: {}", target_id, e);
            }
        }

        Ok(())
    }

    /// Process suspicion timeouts (public for benchmarking)
    pub fn process_suspicion_timeouts(&mut self) {
        let suspicion_timeout = Duration::from_millis(
            self.config.failure_timeout_ms * self.config.suspicion_multiplier as u64
        );

        let expired_suspects: Vec<NodeId> = self.suspects
            .iter()
            .filter_map(|node_id| {
                self.members.get(node_id).and_then(|node| {
                    if node.is_suspicion_timed_out(suspicion_timeout) {
                        Some(*node_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        for node_id in expired_suspects {
            warn!("Suspicion timeout for {}, marking as dead", node_id);
            if let Err(e) = self.mark_dead(&node_id) {
                error!("Failed to mark {} as dead: {}", node_id, e);
            }
        }
    }

    /// Clean up expired indirect ping requests
    fn cleanup_expired_indirect_pings(&mut self) {
        let timeout = Duration::from_millis(self.config.failure_timeout_ms * 2);
        let now = Instant::now();

        self.pending_indirect_pings.retain(|_, request| {
            now.duration_since(request.timestamp) < timeout
        });
    }

    /// Add a new member
    pub fn add_member(&mut self, node: NodeInfo) -> Result<()> {
        if self.members.contains_key(&node.id) {
            debug!("Member {} already exists, updating", node.id);
        } else {
            info!("Adding new member {} at {}", node.id, node.address);
        }

        self.members.insert(node.id, node);
        Ok(())
    }

    /// Remove a member
    pub fn remove_member(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(node) = self.members.remove(node_id) {
            info!("Removed member {} from cluster", node.id);
            self.suspects.remove(node_id);
            self.failure_detectors.remove(node_id);
            Ok(())
        } else {
            Err(Error::membership(format!("Member {node_id} not found")))
        }
    }

    /// Mark a member as suspect
    pub fn mark_suspect(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(member) = self.members.get_mut(node_id) {
            if member.is_alive() {
                member.mark_suspected();
                self.suspects.insert(*node_id);
                warn!("Member {} marked as suspect", node_id);
            }
            Ok(())
        } else {
            Err(Error::membership(format!("Member {node_id} not found")))
        }
    }

    /// Mark a member as dead
    pub fn mark_dead(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(member) = self.members.get_mut(node_id) {
            member.update_status(NodeStatus::Dead);
            self.suspects.remove(node_id);
            self.failure_detectors.remove(node_id);
            error!("Member {} marked as dead", node_id);
            Ok(())
        } else {
            Err(Error::membership(format!("Member {node_id} not found")))
        }
    }

    /// Confirm a member is alive
    pub fn confirm_alive(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(member) = self.members.get_mut(node_id) {
            if !member.is_alive() {
                member.update_status(NodeStatus::Alive);
                self.suspects.remove(node_id);
                info!("Member {} confirmed alive", node_id);
            }
            Ok(())
        } else {
            Err(Error::membership(format!("Member {node_id} not found")))
        }
    }

    /// Select random members for gossip
    pub fn select_gossip_targets(&self, exclude: &NodeId) -> Vec<NodeId> {
        let candidates: Vec<NodeId> = self
            .members
            .iter()
            .filter(|(id, member)| *id != exclude && member.is_alive())
            .map(|(id, _)| *id)
            .collect();

        let mut rng = thread_rng();
        let mut targets = candidates;
        targets.shuffle(&mut rng);
        targets.truncate(self.config.gossip_fanout);
        targets
    }

    /// Process membership update
    pub fn process_membership_update(&mut self, updates: Vec<NodeInfo>) -> Result<()> {
        for update in updates {
            match self.members.get_mut(&update.id) {
                Some(existing) => {
                    // Update if incarnation is newer or status is different
                    if update.incarnation > existing.incarnation || 
                       update.status != existing.status {
                        let old_status = existing.status;
                        *existing = update;
                        
                        // Handle status changes
                        if old_status != existing.status {
                            match existing.status {
                                NodeStatus::Alive => {
                                    self.suspects.remove(&existing.id);
                                    info!("Node {} status changed to alive", existing.id);
                                },
                                NodeStatus::Suspect => {
                                    self.suspects.insert(existing.id);
                                    warn!("Node {} status changed to suspect", existing.id);
                                },
                                NodeStatus::Dead => {
                                    self.suspects.remove(&existing.id);
                                    error!("Node {} status changed to dead", existing.id);
                                },
                                NodeStatus::Left => {
                                    self.suspects.remove(&existing.id);
                                    info!("Node {} left the cluster", existing.id);
                                },
                            }
                        }
                        
                        debug!(
                            "Updated member {} with incarnation {} and status {:?}",
                            existing.id, existing.incarnation, existing.status
                        );
                    }
                },
                None => {
                    // New member
                    info!("Discovered new member {} with status {:?}", update.id, update.status);
                    if update.status == NodeStatus::Suspect {
                        self.suspects.insert(update.id);
                    }
                    self.add_member(update)?;
                },
            }
        }
        Ok(())
    }

    /// Get membership statistics
    pub fn stats(&self) -> MembershipStats {
        let alive_count = self.alive_member_count();
        let suspect_count = self.suspects.len();
        let dead_count = self.members.values().filter(|m| m.is_dead()).count();

        MembershipStats {
            total_members: self.member_count(),
            alive_members: alive_count,
            suspect_members: suspect_count,
            dead_members: dead_count,
        }
    }

    /// Start background protocol execution
    pub async fn start_background_protocol(&mut self, router: MessageRouter) -> Result<()> {
        info!("Starting SWIM protocol background execution");
        
        let protocol_period = Duration::from_millis(self.config.protocol_period_ms);
        
        loop {
            if let Err(e) = self.execute_protocol_round(&router).await {
                error!("SWIM protocol round failed: {}", e);
            }
            
            sleep(protocol_period).await;
        }
    }
}

/// Membership statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipStats {
    /// Total number of known members
    pub total_members: usize,
    /// Number of alive members
    pub alive_members: usize,
    /// Number of suspect members
    pub suspect_members: usize,
    /// Number of dead members
    pub dead_members: usize,
}

/// Membership manager coordinating different membership protocols
pub struct MembershipManager {
    /// SWIM membership instance
    swim: Option<SwimMembership>,
}

impl MembershipManager {
    /// Create a new membership manager
    pub fn new() -> Self {
        Self { swim: None }
    }

    /// Initialize SWIM membership
    pub fn init_swim(&mut self, local_node: NodeInfo, config: SwimConfig) -> Result<()> {
        let swim = SwimMembership::new(local_node, config);
        self.swim = Some(swim);
        info!("SWIM membership initialized");
        Ok(())
    }

    /// Get SWIM membership instance
    pub fn swim(&self) -> Option<&SwimMembership> {
        self.swim.as_ref()
    }

    /// Get mutable SWIM membership instance
    pub fn swim_mut(&mut self) -> Option<&mut SwimMembership> {
        self.swim.as_mut()
    }

    /// Start SWIM protocol background execution
    pub async fn start_swim_protocol(&mut self, router: MessageRouter) -> Result<()> {
        if let Some(swim) = self.swim.as_mut() {
            swim.start_background_protocol(router).await
        } else {
            Err(Error::membership("SWIM not initialized"))
        }
    }

    /// Shutdown membership manager
    pub fn shutdown(&mut self) -> Result<()> {
        self.swim = None;
        info!("Membership manager shutdown complete");
        Ok(())
    }
}

impl Default for MembershipManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn test_node_info_creation() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let node = NodeInfo::new(node_id, address);

        assert_eq!(node.id, node_id);
        assert_eq!(node.address, address);
        assert_eq!(node.status, NodeStatus::Alive);
        assert_eq!(node.incarnation, 0);
        assert!(node.is_alive());
    }

    #[test]
    fn test_phi_failure_detector() {
        let mut detector = PhiFailureDetector::new(10, 8.0);
        
        // Add some normal intervals
        detector.heartbeat(Duration::from_millis(1000));
        detector.heartbeat(Duration::from_millis(1100));
        detector.heartbeat(Duration::from_millis(900));
        
        // Test normal interval
        let phi_normal = detector.phi(Duration::from_millis(1000));
        assert!(phi_normal < 8.0);
        assert!(!detector.is_failed(Duration::from_millis(1000)));
        
        // Test abnormal interval
        let phi_abnormal = detector.phi(Duration::from_millis(10000));
        assert!(phi_abnormal > 8.0);
        assert!(detector.is_failed(Duration::from_millis(10000)));
    }

    #[test]
    fn test_swim_membership_creation() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let node = NodeInfo::new(node_id, address);
        let config = SwimConfig::default();
        let swim = SwimMembership::new(node, config);

        assert_eq!(swim.member_count(), 1);
        assert_eq!(swim.alive_member_count(), 1);
        assert!(swim.members().contains_key(&node_id));
    }

    #[test]
    fn test_membership_manager_creation() {
        let manager = MembershipManager::new();
        assert!(manager.swim().is_none());
    }

    #[test]
    fn test_node_metadata() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let mut node = NodeInfo::new(node_id, address);

        node.metadata.insert("role".to_string(), "leader".to_string());
        node.metadata.insert("version".to_string(), "1.0.0".to_string());

        assert_eq!(node.metadata.get("role"), Some(&"leader".to_string()));
        assert_eq!(node.metadata.get("version"), Some(&"1.0.0".to_string()));
    }

    #[test]
    fn test_member_status_transitions() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let node = NodeInfo::new(node_id, address);
        let config = SwimConfig::default();
        let mut swim = SwimMembership::new(node, config);

        // Add a member
        let member_id = NodeId::generate();
        let member_address: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        let member = NodeInfo::new(member_id, member_address);
        swim.add_member(member).unwrap();

        // Mark as suspect
        swim.mark_suspect(&member_id).unwrap();
        assert!(swim.members().get(&member_id).unwrap().is_suspect());

        // Mark as dead
        swim.mark_dead(&member_id).unwrap();
        assert!(swim.members().get(&member_id).unwrap().is_dead());

        // Confirm alive
        swim.confirm_alive(&member_id).unwrap();
        assert!(swim.members().get(&member_id).unwrap().is_alive());
    }

    #[test]
    fn test_proxy_node_selection() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let node = NodeInfo::new(node_id, address);
        let config = SwimConfig::default();
        let mut swim = SwimMembership::new(node, config);

        // Add several members
        for i in 0..5 {
            let member_id = NodeId::generate();
            let member_address: SocketAddr = format!("127.0.0.1:808{}", i + 1).parse().unwrap();
            let member = NodeInfo::new(member_id, member_address);
            swim.add_member(member).unwrap();
        }

        let target_id = NodeId::generate();
        let proxies = swim.select_proxy_nodes(&target_id, 3);
        
        assert_eq!(proxies.len(), 3);
        for proxy in proxies {
            assert_ne!(proxy.id, swim.local_node.id);
            assert_ne!(proxy.id, target_id);
            assert!(proxy.is_alive());
        }
    }

    #[test]
    fn test_suspicion_timeout() {
        let node_id = NodeId::generate();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let mut node = NodeInfo::new(node_id, address);

        // Mark as suspected
        node.mark_suspected();
        assert!(node.is_suspect());
        assert!(node.suspected_at.is_some());

        // Check timeout immediately (should not be timed out)
        assert!(!node.is_suspicion_timed_out(Duration::from_millis(1000)));

        // Simulate time passing by manually setting the timestamp
        node.suspected_at = Some(Instant::now() - Duration::from_millis(2000));
        assert!(node.is_suspicion_timed_out(Duration::from_millis(1000)));
    }
}