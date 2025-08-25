//! # Consistency Models Implementation
//!
//! Provides multiple consistency levels for replication operations, from eventual
//! consistency to strong linearizable consistency with configurable trade-offs.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use kaelix_cluster::types::NodeId;

use super::{ReplicationError, Result};

/// Consistency levels supported by the replication system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsistencyLevel {
    /// Best performance, eventual consistency
    /// - Writes acknowledged immediately from primary
    /// - Reads may return stale data
    /// - No ordering guarantees
    Eventual,

    /// Session consistency guarantees
    /// - Read your own writes within a session
    /// - Monotonic reads within a session
    /// - Writes from same session are ordered
    ReadYourWrites,

    /// Monotonic consistency
    /// - Monotonic read consistency (no going backwards)
    /// - Monotonic write consistency (writes are ordered)
    /// - Causal consistency (causally related operations are ordered)
    Monotonic,

    /// Strong linearizable consistency
    /// - All operations appear to take effect atomically
    /// - Operations respect real-time ordering
    /// - Quorum reads and writes required
    /// - Highest latency but strongest guarantees
    Strong,
}

impl Default for ConsistencyLevel {
    fn default() -> Self {
        Self::ReadYourWrites
    }
}

impl std::fmt::Display for ConsistencyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eventual => write!(f, "eventual"),
            Self::ReadYourWrites => write!(f, "read-your-writes"),
            Self::Monotonic => write!(f, "monotonic"),
            Self::Strong => write!(f, "strong"),
        }
    }
}

/// Session information for session consistency
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session identifier
    pub session_id: String,
    /// Last write sequence observed by this session
    pub last_write_sequence: u64,
    /// Last read timestamp for this session
    pub last_read_timestamp: SystemTime,
    /// Node ID where session was created
    pub origin_node: NodeId,
}

/// Read preferences for consistency control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPreference {
    /// Read from primary replica only
    Primary,
    /// Read from primary if available, otherwise any replica
    PrimaryPreferred,
    /// Read from any available replica
    Any,
    /// Read from nearest replica (lowest latency)
    Nearest,
}

/// Write preferences for consistency control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePreference {
    /// Write must be acknowledged by primary
    Primary,
    /// Write must be acknowledged by majority
    Majority,
    /// Write must be acknowledged by all replicas
    All,
}

/// Consistency manager that enforces consistency guarantees
pub struct ConsistencyManager {
    /// Active sessions for read-your-writes consistency
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    
    /// Vector clocks for causality tracking
    vector_clocks: Arc<RwLock<HashMap<NodeId, u64>>>,
    
    /// Last committed sequence per node for monotonic reads
    node_sequences: Arc<RwLock<HashMap<NodeId, u64>>>,
    
    /// Local node ID
    local_node_id: NodeId,
}

impl ConsistencyManager {
    /// Create a new consistency manager
    pub fn new(local_node_id: NodeId) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            vector_clocks: Arc::new(RwLock::new(HashMap::new())),
            node_sequences: Arc::new(RwLock::new(HashMap::new())),
            local_node_id,
        }
    }

    /// Create a new session for read-your-writes consistency
    pub async fn create_session(&self, session_id: String) -> Result<SessionInfo> {
        let session = SessionInfo {
            session_id: session_id.clone(),
            last_write_sequence: 0,
            last_read_timestamp: SystemTime::now(),
            origin_node: self.local_node_id,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, session.clone());

        debug!("Created consistency session: {}", session.session_id);
        Ok(session)
    }

    /// Update session information after a write
    pub async fn update_session_write(
        &self,
        session_id: &str,
        sequence: u64,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_write_sequence = std::cmp::max(session.last_write_sequence, sequence);
            debug!("Updated session {} write sequence to {}", session_id, sequence);
        }
        Ok(())
    }

    /// Check if a read satisfies consistency requirements
    pub async fn check_read_consistency(
        &self,
        level: ConsistencyLevel,
        session_id: Option<&str>,
        read_sequence: u64,
        read_node: NodeId,
    ) -> Result<bool> {
        match level {
            ConsistencyLevel::Eventual => {
                // Always satisfied for eventual consistency
                Ok(true)
            }
            ConsistencyLevel::ReadYourWrites => {
                if let Some(session_id) = session_id {
                    let sessions = self.sessions.read().await;
                    if let Some(session) = sessions.get(session_id) {
                        // Ensure we can read our own writes
                        Ok(read_sequence >= session.last_write_sequence)
                    } else {
                        warn!("Session {} not found for read-your-writes check", session_id);
                        Ok(false)
                    }
                } else {
                    // No session provided, cannot guarantee read-your-writes
                    Ok(false)
                }
            }
            ConsistencyLevel::Monotonic => {
                // Check monotonic read consistency
                let node_sequences = self.node_sequences.read().await;
                if let Some(&last_sequence) = node_sequences.get(&read_node) {
                    Ok(read_sequence >= last_sequence)
                } else {
                    // First read from this node
                    Ok(true)
                }
            }
            ConsistencyLevel::Strong => {
                // Strong consistency requires quorum reads with latest data
                // This is handled at the replication protocol level
                Ok(true)
            }
        }
    }

    /// Check if a write satisfies consistency requirements
    pub async fn check_write_consistency(
        &self,
        level: ConsistencyLevel,
        write_sequence: u64,
        replicated_nodes: &[NodeId],
        total_replicas: usize,
    ) -> Result<bool> {
        match level {
            ConsistencyLevel::Eventual => {
                // Write to primary is sufficient
                Ok(!replicated_nodes.is_empty())
            }
            ConsistencyLevel::ReadYourWrites | ConsistencyLevel::Monotonic => {
                // Require primary acknowledgment
                Ok(!replicated_nodes.is_empty())
            }
            ConsistencyLevel::Strong => {
                // Require majority acknowledgment for strong consistency
                let majority = (total_replicas / 2) + 1;
                Ok(replicated_nodes.len() >= majority)
            }
        }
    }

    /// Get read preferences for consistency level
    pub fn get_read_preference(&self, level: ConsistencyLevel) -> ReadPreference {
        match level {
            ConsistencyLevel::Eventual => ReadPreference::Any,
            ConsistencyLevel::ReadYourWrites => ReadPreference::PrimaryPreferred,
            ConsistencyLevel::Monotonic => ReadPreference::PrimaryPreferred,
            ConsistencyLevel::Strong => ReadPreference::Primary,
        }
    }

    /// Get write preferences for consistency level
    pub fn get_write_preference(&self, level: ConsistencyLevel) -> WritePreference {
        match level {
            ConsistencyLevel::Eventual => WritePreference::Primary,
            ConsistencyLevel::ReadYourWrites => WritePreference::Primary,
            ConsistencyLevel::Monotonic => WritePreference::Primary,
            ConsistencyLevel::Strong => WritePreference::Majority,
        }
    }

    /// Update vector clock for causality tracking
    pub async fn update_vector_clock(&self, node_id: NodeId, sequence: u64) -> Result<()> {
        let mut clocks = self.vector_clocks.write().await;
        let current = clocks.entry(node_id).or_insert(0);
        *current = std::cmp::max(*current, sequence);
        Ok(())
    }

    /// Update node sequence for monotonic consistency
    pub async fn update_node_sequence(&self, node_id: NodeId, sequence: u64) -> Result<()> {
        let mut sequences = self.node_sequences.write().await;
        let current = sequences.entry(node_id).or_insert(0);
        *current = std::cmp::max(*current, sequence);
        Ok(())
    }

    /// Wait for consistency to be satisfied
    pub async fn wait_for_consistency(
        &self,
        level: ConsistencyLevel,
        target_sequence: u64,
        timeout: Duration,
    ) -> Result<()> {
        let start = Instant::now();

        match level {
            ConsistencyLevel::Eventual => {
                // No waiting required for eventual consistency
                Ok(())
            }
            ConsistencyLevel::ReadYourWrites | ConsistencyLevel::Monotonic => {
                // Wait for local sequence to catch up
                while start.elapsed() < timeout {
                    let sequences = self.node_sequences.read().await;
                    if let Some(&current) = sequences.get(&self.local_node_id) {
                        if current >= target_sequence {
                            return Ok(());
                        }
                    }
                    
                    // Small delay before checking again
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                
                Err(ReplicationError::Timeout { timeout })
            }
            ConsistencyLevel::Strong => {
                // Strong consistency waits are handled by quorum operations
                Ok(())
            }
        }
    }

    /// Get session information
    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    /// Remove expired sessions
    pub async fn cleanup_expired_sessions(&self, max_age: Duration) -> Result<usize> {
        let cutoff = SystemTime::now() - max_age;
        let mut sessions = self.sessions.write().await;
        let initial_count = sessions.len();
        
        sessions.retain(|_, session| session.last_read_timestamp > cutoff);
        
        let removed = initial_count - sessions.len();
        if removed > 0 {
            debug!("Cleaned up {} expired consistency sessions", removed);
        }
        
        Ok(removed)
    }

    /// Get consistency statistics
    pub async fn get_consistency_stats(&self) -> ConsistencyStats {
        let sessions = self.sessions.read().await;
        let clocks = self.vector_clocks.read().await;
        let sequences = self.node_sequences.read().await;

        ConsistencyStats {
            active_sessions: sessions.len(),
            tracked_nodes: clocks.len(),
            sequence_nodes: sequences.len(),
        }
    }
}

/// Consistency statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyStats {
    /// Number of active sessions
    pub active_sessions: usize,
    /// Number of nodes being tracked
    pub tracked_nodes: usize,
    /// Number of nodes with sequence information
    pub sequence_nodes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consistency_manager_creation() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        let stats = manager.get_consistency_stats().await;
        assert_eq!(stats.active_sessions, 0);
        assert_eq!(stats.tracked_nodes, 0);
    }

    #[tokio::test]
    async fn test_session_creation() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        let session = manager.create_session("test-session".to_string()).await.unwrap();
        assert_eq!(session.session_id, "test-session");
        assert_eq!(session.origin_node, node_id);
        
        let stats = manager.get_consistency_stats().await;
        assert_eq!(stats.active_sessions, 1);
    }

    #[tokio::test]
    async fn test_session_write_update() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        let _session = manager.create_session("test-session".to_string()).await.unwrap();
        manager.update_session_write("test-session", 42).await.unwrap();
        
        let session = manager.get_session("test-session").await.unwrap().unwrap();
        assert_eq!(session.last_write_sequence, 42);
    }

    #[tokio::test]
    async fn test_read_consistency_eventual() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        let result = manager.check_read_consistency(
            ConsistencyLevel::Eventual,
            None,
            100,
            node_id
        ).await.unwrap();
        
        assert!(result); // Always satisfied for eventual consistency
    }

    #[tokio::test]
    async fn test_read_consistency_read_your_writes() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        // Create session and update write sequence
        let _session = manager.create_session("test-session".to_string()).await.unwrap();
        manager.update_session_write("test-session", 50).await.unwrap();
        
        // Read with higher sequence should satisfy
        let result = manager.check_read_consistency(
            ConsistencyLevel::ReadYourWrites,
            Some("test-session"),
            60,
            node_id
        ).await.unwrap();
        assert!(result);
        
        // Read with lower sequence should not satisfy
        let result = manager.check_read_consistency(
            ConsistencyLevel::ReadYourWrites,
            Some("test-session"),
            40,
            node_id
        ).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_write_consistency_check() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        let replicated_nodes = vec![node_id];
        
        // Eventual consistency satisfied with one replica
        let result = manager.check_write_consistency(
            ConsistencyLevel::Eventual,
            100,
            &replicated_nodes,
            3
        ).await.unwrap();
        assert!(result);
        
        // Strong consistency not satisfied with one replica out of 3
        let result = manager.check_write_consistency(
            ConsistencyLevel::Strong,
            100,
            &replicated_nodes,
            3
        ).await.unwrap();
        assert!(!result); // Need majority (2 out of 3)
    }

    #[test]
    fn test_consistency_level_display() {
        assert_eq!(ConsistencyLevel::Eventual.to_string(), "eventual");
        assert_eq!(ConsistencyLevel::ReadYourWrites.to_string(), "read-your-writes");
        assert_eq!(ConsistencyLevel::Monotonic.to_string(), "monotonic");
        assert_eq!(ConsistencyLevel::Strong.to_string(), "strong");
    }

    #[test]
    fn test_read_write_preferences() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        assert_eq!(manager.get_read_preference(ConsistencyLevel::Eventual), ReadPreference::Any);
        assert_eq!(manager.get_read_preference(ConsistencyLevel::Strong), ReadPreference::Primary);
        
        assert_eq!(manager.get_write_preference(ConsistencyLevel::Eventual), WritePreference::Primary);
        assert_eq!(manager.get_write_preference(ConsistencyLevel::Strong), WritePreference::Majority);
    }

    #[tokio::test]
    async fn test_session_cleanup() {
        let node_id = NodeId::generate();
        let manager = ConsistencyManager::new(node_id);
        
        let _session = manager.create_session("test-session".to_string()).await.unwrap();
        
        // Should not cleanup recent sessions
        let removed = manager.cleanup_expired_sessions(Duration::from_secs(3600)).await.unwrap();
        assert_eq!(removed, 0);
        
        let stats = manager.get_consistency_stats().await;
        assert_eq!(stats.active_sessions, 1);
    }
}