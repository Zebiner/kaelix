//! # Membership Module
//!
//! Node discovery, failure detection, and cluster membership management using 
//! the SWIM (Scalable Weakly-consistent Infection-style Membership) protocol.
//!
//! This module provides ultra-fast failure detection and membership updates optimized
//! for MemoryStreamer's <1ms consensus targets.

use crate::{
    communication::{MessageRouter, NetworkMessage},
    error::{Error, Result}, 
    types::NodeId,
};
use rand::{seq::SliceRandom, thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
    net::SocketAddr,
};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// Node addressing information for cluster communication
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeAddress {
    /// TCP address (host, port)
    Tcp(SocketAddr),
    /// UDP address (host, port) 
    Udp(SocketAddr),
    /// Unix domain socket path
    #[cfg(unix)]
    Unix(std::path::PathBuf),
}

impl NodeAddress {
    /// Create TCP address
    pub fn tcp(addr: SocketAddr) -> Self {
        Self::Tcp(addr)
    }

    /// Create UDP address
    pub fn udp(addr: SocketAddr) -> Self {
        Self::Udp(addr)
    }

    /// Get the underlying socket address if TCP or UDP
    pub fn socket_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::Tcp(addr) | Self::Udp(addr) => Some(*addr),
            #[cfg(unix)]
            Self::Unix(_) => None,
        }
    }
}

impl std::fmt::Display for NodeAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "tcp://{addr}"),
            Self::Udp(addr) => write!(f, "udp://{addr}"),
            #[cfg(unix)]
            Self::Unix(path) => write!(f, "unix://{}", path.display()),
        }
    }
}

impl From<SocketAddr> for NodeAddress {
    fn from(addr: SocketAddr) -> Self {
        Self::Tcp(addr)
    }
}

/// Node status in the cluster membership
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is alive and responsive
    Alive,
    /// Node is suspected to have failed
    Suspect,
    /// Node has been confirmed as failed
    Failed,
    /// Node has left the cluster gracefully
    Left,
}

impl NodeStatus {
    /// Check if node is considered active (alive or suspect)
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Alive | Self::Suspect)
    }

    /// Check if node is alive
    pub const fn is_alive(&self) -> bool {
        matches!(self, Self::Alive)
    }

    /// Check if node has failed or left
    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::Failed | Self::Left)
    }
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Self::Alive => "alive",
            Self::Suspect => "suspect", 
            Self::Failed => "failed",
            Self::Left => "left",
        };
        write!(f, "{status}")
    }
}

/// Information about a cluster node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier
    pub id: NodeId,
    /// Node network address
    pub address: NodeAddress,
    /// Current node status
    pub status: NodeStatus,
    /// Node incarnation number (for conflict resolution)
    pub incarnation: u64,
    /// Last seen timestamp
    pub last_seen: Option<u64>, // Unix timestamp in milliseconds
    /// Node metadata (optional)
    pub metadata: HashMap<String, String>,
}

impl NodeInfo {
    /// Create new node info
    pub fn new(id: NodeId, address: NodeAddress) -> Self {
        Self {
            id,
            address,
            status: NodeStatus::Alive,
            incarnation: 0,
            last_seen: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            ),
            metadata: HashMap::new(),
        }
    }

    /// Update node status with new incarnation
    pub fn update_status(&mut self, status: NodeStatus, incarnation: u64) {
        if incarnation >= self.incarnation {
            self.status = status;
            self.incarnation = incarnation;
            self.last_seen = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            );
        }
    }

    /// Check if node is alive
    pub const fn is_alive(&self) -> bool {
        self.status.is_alive()
    }

    /// Check if node is active (alive or suspect)
    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// Get time since last seen (if available)
    pub fn time_since_last_seen(&self) -> Option<Duration> {
        self.last_seen.map(|timestamp| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            Duration::from_millis(now.saturating_sub(timestamp))
        })
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

/// Phi Accrual Failure Detector for adaptive failure detection
#[derive(Debug, Clone)]
pub struct PhiFailureDetector {
    /// History of heartbeat intervals
    intervals: Vec<f64>,
    /// Maximum history size
    max_history: usize,
    /// Phi threshold for failure detection
    threshold: f64,
    /// Last heartbeat time
    last_heartbeat: Option<Instant>,
}

impl PhiFailureDetector {
    /// Create new failure detector
    pub fn new(max_history: usize, threshold: f64) -> Self {
        Self {
            intervals: Vec::with_capacity(max_history),
            max_history,
            threshold,
            last_heartbeat: None,
        }
    }

    /// Record heartbeat
    pub fn heartbeat(&mut self, _expected_interval: Duration) {
        let now = Instant::now();
        
        if let Some(last) = self.last_heartbeat {
            let actual_interval = now.duration_since(last).as_secs_f64();
            
            // Add to history
            if self.intervals.len() >= self.max_history {
                self.intervals.remove(0);
            }
            self.intervals.push(actual_interval);
        }
        
        self.last_heartbeat = Some(now);
    }

    /// Calculate phi value (suspicion level)
    pub fn phi(&self) -> f64 {
        if self.intervals.len() < 2 {
            return 0.0;
        }

        let last_heartbeat = match self.last_heartbeat {
            Some(time) => time,
            None => return 0.0,
        };

        let elapsed = last_heartbeat.elapsed().as_secs_f64();
        let mean = self.intervals.iter().sum::<f64>() / self.intervals.len() as f64;
        
        if mean <= 0.0 {
            return 0.0;
        }

        // Calculate standard deviation
        let variance = self.intervals.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / self.intervals.len() as f64;
        let std_dev = variance.sqrt();

        if std_dev <= 0.0 {
            return if elapsed > mean { 100.0 } else { 0.0 };
        }

        // Phi = -log10(P(X > elapsed))
        let p = 1.0 - normal_cdf((elapsed - mean) / std_dev);
        if p > 0.0 {
            -p.log10()
        } else {
            100.0 // Very high phi for extreme cases
        }
    }

    /// Check if node is suspected failed
    pub fn is_suspected(&self) -> bool {
        self.phi() > self.threshold
    }
}

/// Approximation of cumulative distribution function for standard normal distribution
fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// Error function approximation
fn erf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * (-x * x).exp();

    sign * y
}

/// SWIM membership protocol implementation
#[derive(Debug)]
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
        let _now = Instant::now();
        let round_interval = _now.duration_since(self.last_protocol_round);
        
        debug!("Executing SWIM protocol round after {:?}", round_interval);

        // Check if it's time for next protocol round
        let protocol_period = Duration::from_millis(self.config.protocol_period_ms);
        if round_interval < protocol_period {
            return Ok(());
        }

        self.last_protocol_round = _now;

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

        // Check if we got successful responses
        if let Some(request) = self.pending_indirect_pings.get(&request_id) {
            let success = request.responses_received > 0;
            debug!("Indirect ping to {} completed: {} responses received (success: {})", 
                   target.id, request.responses_received, success);
            success
        } else {
            false
        }
    }

    /// Select proxy nodes for indirect ping
    fn select_proxy_nodes(&self, target_id: &NodeId, count: usize) -> Vec<NodeInfo> {
        let mut candidates: Vec<&NodeInfo> = self
            .members
            .values()
            .filter(|node| {
                node.id != self.local_node.id 
                && node.id != *target_id 
                && node.is_alive()
            })
            .collect();

        let mut rng = thread_rng();
        candidates.shuffle(&mut rng);
        
        candidates
            .into_iter()
            .take(count)
            .cloned()
            .collect()
    }

    /// Mark node as suspect
    pub fn mark_suspect(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(node) = self.members.get_mut(node_id) {
            if node.status == NodeStatus::Alive {
                debug!("Marking node {} as suspect", node_id);
                node.update_status(NodeStatus::Suspect, node.incarnation + 1);
                self.suspects.insert(*node_id);
            }
        }
        Ok(())
    }

    /// Mark node as failed
    pub fn mark_failed(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(node) = self.members.get_mut(node_id) {
            warn!("Marking node {} as failed", node_id);
            node.update_status(NodeStatus::Failed, node.incarnation + 1);
            self.suspects.remove(node_id);
            self.failure_detectors.remove(node_id);
        }
        Ok(())
    }

    /// Process suspicion timeouts and mark suspected nodes as failed
    fn process_suspicion_timeouts(&mut self) {
        let suspicion_timeout = Duration::from_millis(
            self.config.failure_timeout_ms * self.config.suspicion_multiplier as u64
        );

        let expired_suspects: Vec<NodeId> = self.suspects.iter()
            .filter_map(|node_id| {
                self.members.get(node_id).and_then(|node| {
                    node.time_since_last_seen().map(|elapsed| {
                        if elapsed > suspicion_timeout {
                            Some(*node_id)
                        } else {
                            None
                        }
                    }).unwrap_or(None)
                })
            })
            .collect();

        for node_id in expired_suspects {
            if let Err(e) = self.mark_failed(&node_id) {
                error!("Failed to mark node {} as failed: {}", node_id, e);
            }
        }
    }

    /// Generate gossip updates for membership information
    fn get_gossip_updates(&self) -> Vec<NodeInfo> {
        let mut updates: Vec<NodeInfo> = self.members.values().cloned().collect();
        
        // Shuffle for randomness and limit to fanout size
        let mut rng = thread_rng();
        updates.shuffle(&mut rng);
        updates.truncate(self.config.gossip_fanout);
        
        updates
    }

    /// Gossip membership updates to random nodes
    pub async fn gossip_updates(&self, router: &MessageRouter) -> Result<()> {
        let targets = self.select_gossip_targets();
        let updates = self.get_gossip_updates();
        
        if targets.is_empty() || updates.is_empty() {
            return Ok(());
        }

        let gossip_message = SwimMessage::Gossip {
            source: self.local_node.id,
            updates,
        };

        let network_message = NetworkMessage::MembershipUpdate {
            updates: bincode::serialize(&gossip_message)
                .map_err(|e| Error::membership(format!("Failed to serialize gossip: {e}")))?,
        };

        // Send to selected targets
        for target in targets {
            if let Err(e) = router.send_to_node(&target.id, network_message.clone()).await {
                debug!("Failed to send gossip to {}: {}", target.id, e);
            }
        }

        Ok(())
    }

    /// Select nodes for gossip
    fn select_gossip_targets(&self) -> Vec<NodeInfo> {
        let mut candidates: Vec<&NodeInfo> = self
            .members
            .values()
            .filter(|node| node.id != self.local_node.id && node.is_alive())
            .collect();

        let mut rng = thread_rng();
        candidates.shuffle(&mut rng);
        
        candidates
            .into_iter()
            .take(self.config.gossip_fanout)
            .cloned()
            .collect()
    }

    /// Clean up expired indirect ping requests
    fn cleanup_expired_indirect_pings(&mut self) {
        let timeout = Duration::from_millis(self.config.failure_timeout_ms * 2);
        let now = Instant::now();
        
        self.pending_indirect_pings.retain(|_, request| {
            now.duration_since(request.timestamp) < timeout
        });
    }

    /// Handle incoming SWIM message
    pub async fn handle_swim_message(&mut self, message: SwimMessage, router: &MessageRouter) -> Result<()> {
        match message {
            SwimMessage::Ping { source, incarnation, updates } => {
                self.handle_ping(source, incarnation, updates, router).await
            },
            SwimMessage::Ack { source, incarnation, updates } => {
                self.handle_ack(source, incarnation, updates).await
            },
            SwimMessage::IndirectPing { source, target, request_id } => {
                self.handle_indirect_ping(source, target, request_id, router).await
            },
            SwimMessage::IndirectAck { source, target, request_id, success } => {
                self.handle_indirect_ack(source, target, request_id, success).await
            },
            SwimMessage::Gossip { source: _, updates } => {
                self.handle_gossip(updates).await
            },
        }
    }

    /// Handle ping message
    async fn handle_ping(&mut self, source: NodeId, incarnation: u64, updates: Vec<NodeInfo>, router: &MessageRouter) -> Result<()> {
        debug!("Received ping from {} (incarnation: {})", source, incarnation);

        // Update source node information
        self.update_node_info(source, NodeStatus::Alive, incarnation);

        // Process gossip updates
        self.process_membership_updates(updates);

        // Send ACK response
        let ack_message = SwimMessage::Ack {
            source: self.local_node.id,
            incarnation: self.local_node.incarnation,
            updates: self.get_gossip_updates(),
        };

        let network_message = NetworkMessage::MembershipUpdate {
            updates: bincode::serialize(&ack_message)
                .map_err(|e| Error::membership(format!("Failed to serialize ack: {e}")))?,
        };

        if let Err(e) = router.send_to_node(&source, network_message).await {
            warn!("Failed to send ack to {}: {}", source, e);
        }

        Ok(())
    }

    /// Handle ACK message
    async fn handle_ack(&mut self, source: NodeId, incarnation: u64, updates: Vec<NodeInfo>) -> Result<()> {
        debug!("Received ack from {} (incarnation: {})", source, incarnation);
        
        // Update source node information
        self.update_node_info(source, NodeStatus::Alive, incarnation);
        
        // Process gossip updates
        self.process_membership_updates(updates);
        
        Ok(())
    }

    /// Handle indirect ping request
    async fn handle_indirect_ping(&mut self, source: NodeId, target: NodeId, request_id: u64, router: &MessageRouter) -> Result<()> {
        debug!("Received indirect ping request from {} for target {}", source, target);

        // Try to ping the target
        let success = if let Some(target_node) = self.members.get(&target).cloned() {
            match self.send_ping(&target_node, router).await {
                Ok(_) => true,
                Err(_) => false,
            }
        } else {
            false
        };

        // Send response back to requester
        let response_message = SwimMessage::IndirectAck {
            source: self.local_node.id,
            target,
            request_id,
            success,
        };

        let network_message = NetworkMessage::MembershipUpdate {
            updates: bincode::serialize(&response_message)
                .map_err(|e| Error::membership(format!("Failed to serialize indirect ack: {e}")))?,
        };

        if let Err(e) = router.send_to_node(&source, network_message).await {
            warn!("Failed to send indirect ack to {}: {}", source, e);
        }

        Ok(())
    }

    /// Handle indirect ACK response
    async fn handle_indirect_ack(&mut self, _source: NodeId, target: NodeId, request_id: u64, success: bool) -> Result<()> {
        debug!("Received indirect ack for target {} (request_id: {}, success: {})", target, request_id, success);

        // Update pending request
        if let Some(request) = self.pending_indirect_pings.get_mut(&request_id) {
            request.responses_received += 1;
            
            if success {
                // Mark target as alive if indirect ping succeeded
                self.update_node_info(target, NodeStatus::Alive, 0);
                self.suspects.remove(&target);
            }
        }

        Ok(())
    }

    /// Handle gossip updates
    async fn handle_gossip(&mut self, updates: Vec<NodeInfo>) -> Result<()> {
        debug!("Received gossip updates: {} nodes", updates.len());
        self.process_membership_updates(updates);
        Ok(())
    }

    /// Process membership updates from gossip
    fn process_membership_updates(&mut self, updates: Vec<NodeInfo>) {
        for update in updates {
            // Skip self updates
            if update.id == self.local_node.id {
                continue;
            }

            // Update node information
            self.update_node_info(update.id, update.status, update.incarnation);
        }
    }

    /// Update node information
    fn update_node_info(&mut self, node_id: NodeId, status: NodeStatus, incarnation: u64) {
        match self.members.get_mut(&node_id) {
            Some(existing_node) => {
                // Only update if incarnation is higher or equal
                if incarnation >= existing_node.incarnation {
                    existing_node.update_status(status, incarnation);
                    
                    // Remove from suspects if marked alive
                    if status == NodeStatus::Alive {
                        self.suspects.remove(&node_id);
                    }
                }
            },
            None => {
                // Add new node if we don't know about it
                warn!("Discovered new node {} with status {}", node_id, status);
                // Note: We would need address information to create a complete NodeInfo
                // This is a simplified implementation
            }
        }
    }

    /// Add new member to cluster
    pub fn add_member(&mut self, node_info: NodeInfo) -> Result<()> {
        info!("Adding new member: {} at {}", node_info.id, node_info.address);
        self.members.insert(node_info.id, node_info);
        Ok(())
    }

    /// Remove member from cluster
    pub fn remove_member(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(node) = self.members.remove(node_id) {
            info!("Removed member: {} at {}", node.id, node.address);
            self.suspects.remove(node_id);
            self.failure_detectors.remove(node_id);
            Ok(())
        } else {
            Err(Error::membership(format!("Node {} not found in membership", node_id)))
        }
    }

    /// Get current membership as vector
    pub fn get_membership(&self) -> Vec<NodeInfo> {
        self.members.values().cloned().collect()
    }

    /// Check if node is suspected
    pub fn is_suspected(&self, node_id: &NodeId) -> bool {
        self.suspects.contains(node_id)
    }

    /// Get failure detector phi value for node
    pub fn get_phi(&self, node_id: &NodeId) -> Option<f64> {
        self.failure_detectors.get(node_id).map(|detector| detector.phi())
    }
}

/// SWIM protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwimMessage {
    /// Ping message with membership updates
    Ping {
        source: NodeId,
        incarnation: u64,
        updates: Vec<NodeInfo>,
    },
    /// Acknowledgment message
    Ack {
        source: NodeId,
        incarnation: u64,
        updates: Vec<NodeInfo>,
    },
    /// Indirect ping request
    IndirectPing {
        source: NodeId,
        target: NodeId,
        request_id: u64,
    },
    /// Indirect ping acknowledgment
    IndirectAck {
        source: NodeId,
        target: NodeId,
        request_id: u64,
        success: bool,
    },
    /// Pure gossip message
    Gossip {
        source: NodeId,
        updates: Vec<NodeInfo>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication::{TcpTransport};

    fn create_test_node(id: NodeId, port: u16) -> NodeInfo {
        NodeInfo::new(
            id,
            NodeAddress::tcp(format!("127.0.0.1:{}", port).parse().unwrap())
        )
    }

    async fn create_test_router(port: u16) -> MessageRouter {
        let transport = TcpTransport::new(format!("127.0.0.1:{}", port).parse().unwrap()).await.unwrap();
        MessageRouter::new(NodeId::generate(), transport)
    }

    #[test]
    fn test_node_info_creation() {
        let node_id = NodeId::generate();
        let address = NodeAddress::tcp("127.0.0.1:8080".parse().unwrap());
        
        let node = NodeInfo::new(node_id, address);
        assert_eq!(node.id, node_id);
        assert_eq!(node.status, NodeStatus::Alive);
        assert!(node.is_alive());
        assert!(node.is_active());
    }

    #[test]
    fn test_node_status_transitions() {
        let mut node = create_test_node(NodeId::generate(), 8080);
        
        assert_eq!(node.status, NodeStatus::Alive);
        
        node.update_status(NodeStatus::Suspect, 1);
        assert_eq!(node.status, NodeStatus::Suspect);
        assert_eq!(node.incarnation, 1);
        
        node.update_status(NodeStatus::Failed, 2);
        assert_eq!(node.status, NodeStatus::Failed);
        assert_eq!(node.incarnation, 2);
    }

    #[test]
    fn test_phi_failure_detector() {
        let mut detector = PhiFailureDetector::new(10, 8.0);
        let interval = Duration::from_millis(1000);
        
        // Initially no suspicion
        assert_eq!(detector.phi(), 0.0);
        assert!(!detector.is_suspected());
        
        // Add some heartbeats
        for _ in 0..5 {
            detector.heartbeat(interval);
            std::thread::sleep(Duration::from_millis(100));
        }
        
        // Should still be healthy
        assert!(!detector.is_suspected());
    }

    #[tokio::test]
    async fn test_swim_membership_creation() {
        let node = create_test_node(NodeId::generate(), 8080);
        let config = SwimConfig::default();
        
        let membership = SwimMembership::new(node.clone(), config);
        
        assert_eq!(membership.member_count(), 1);
        assert_eq!(membership.alive_member_count(), 1);
        assert_eq!(membership.local_node().id, node.id);
    }

    #[tokio::test]
    async fn test_member_addition_removal() {
        let local_node = create_test_node(NodeId::generate(), 8080);
        let config = SwimConfig::default();
        let mut membership = SwimMembership::new(local_node, config);
        
        let new_node = create_test_node(NodeId::generate(), 8081);
        membership.add_member(new_node.clone()).unwrap();
        
        assert_eq!(membership.member_count(), 2);
        
        membership.remove_member(&new_node.id).unwrap();
        assert_eq!(membership.member_count(), 1);
    }

    #[tokio::test]
    async fn test_ping_target_selection() {
        let local_node = create_test_node(NodeId::generate(), 8080);
        let config = SwimConfig::default();
        let mut membership = SwimMembership::new(local_node, config);
        
        // No targets initially (only self)
        assert!(membership.select_ping_target().is_none());
        
        // Add another node
        let target_node = create_test_node(NodeId::generate(), 8081);
        membership.add_member(target_node.clone()).unwrap();
        
        // Should select the target
        let selected = membership.select_ping_target().unwrap();
        assert_eq!(selected.id, target_node.id);
    }

    #[tokio::test]
    async fn test_suspect_marking() {
        let local_node = create_test_node(NodeId::generate(), 8080);
        let config = SwimConfig::default();
        let mut membership = SwimMembership::new(local_node, config);
        
        let target_node = create_test_node(NodeId::generate(), 8081);
        membership.add_member(target_node.clone()).unwrap();
        
        // Mark as suspect
        membership.mark_suspect(&target_node.id).unwrap();
        
        assert!(membership.is_suspected(&target_node.id));
        assert_eq!(membership.alive_member_count(), 1); // Only local node is alive
    }

    #[test]
    fn test_node_address_display() {
        let tcp_addr = NodeAddress::tcp("127.0.0.1:8080".parse().unwrap());
        let udp_addr = NodeAddress::udp("127.0.0.1:8081".parse().unwrap());
        
        assert_eq!(tcp_addr.to_string(), "tcp://127.0.0.1:8080");
        assert_eq!(udp_addr.to_string(), "udp://127.0.0.1:8081");
    }

    #[test]
    fn test_gossip_update_generation() {
        let local_node = create_test_node(NodeId::generate(), 8080);
        let config = SwimConfig {
            gossip_fanout: 2,
            ..SwimConfig::default()
        };
        let mut membership = SwimMembership::new(local_node, config);
        
        // Add multiple nodes
        for i in 8081..8085 {
            let node = create_test_node(NodeId::generate(), i);
            membership.add_member(node).unwrap();
        }
        
        let updates = membership.get_gossip_updates();
        assert!(updates.len() <= 2); // Limited by fanout
    }
}