//! # Membership Management
//!
//! Provides cluster membership protocols including SWIM, failure detection,
//! and node discovery for distributed systems.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::Instant,
};
use tracing::{debug, error, info, warn};

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
}

impl Default for SwimConfig {
    fn default() -> Self {
        Self {
            protocol_period_ms: 1000,
            failure_timeout_ms: 5000,
            indirect_ping_count: 3,
            gossip_fanout: 3,
            suspicion_multiplier: 3,
        }
    }
}

impl SwimMembership {
    /// Create a new SWIM membership instance
    pub fn new(local_node: NodeInfo, config: SwimConfig) -> Self {
        let mut members = HashMap::new();
        members.insert(local_node.id, local_node.clone());

        Self { local_node, members, config, suspects: HashSet::new() }
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
            Ok(())
        } else {
            Err(Error::membership(format!("Member {node_id} not found")))
        }
    }

    /// Mark a member as suspect
    pub fn mark_suspect(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(member) = self.members.get_mut(node_id) {
            if member.is_alive() {
                member.update_status(NodeStatus::Suspect);
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
        use rand::seq::SliceRandom;
        use rand::thread_rng;

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
                    // Update if incarnation is newer
                    if update.incarnation > existing.incarnation {
                        *existing = update;
                        debug!(
                            "Updated member {} with incarnation {}",
                            existing.id, existing.incarnation
                        );
                    }
                },
                None => {
                    // New member
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
}
