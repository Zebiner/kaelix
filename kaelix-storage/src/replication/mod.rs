//! # Distributed Data Replication System
//!
//! Provides comprehensive multi-replica replication with configurable consistency levels,
//! automatic failover, and efficient conflict resolution for the MemoryStreamer storage engine.

use std::{
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use kaelix_cluster::{
    messages::ClusterMessage,
    types::NodeId,
};

use crate::wal::{WalConfig, WalError};

// Sub-modules
pub mod consistency;
pub mod manager;
pub mod metrics;
pub mod protocol;
pub mod quorum;
pub mod recovery;
pub mod transport;

// Re-exports
pub use consistency::{ConsistencyLevel, ConsistencyManager};
pub use manager::{ReplicationManager, ReplicaNode, ReplicaRole};
pub use metrics::{ReplicationMetrics, ReplicationStats};
pub use protocol::{ReplicationProtocol, ReplicationEntry, ReplicationResult as ProtocolResult};
pub use quorum::{QuorumConfig, QuorumManager, QuorumResult, ReadResponse, WriteResponse};
pub use recovery::{RecoveryManager, RecoveryStrategy};
pub use transport::{ReplicationTransport, ReplicationMessage};

/// Replication system errors
#[derive(Error, Debug, Clone)]
pub enum ReplicationError {
    /// WAL integration error
    #[error("WAL error: {0}")]
    Wal(#[from] WalError),
    
    /// Network communication error
    #[error("Network error: {0}")]
    Network(String),
    
    /// Consensus error
    #[error("Consensus error: {0}")]
    Consensus(String),
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    /// Replica not available
    #[error("Replica not available: {0}")]
    ReplicaNotAvailable(String),
    
    /// Replica not found
    #[error("Replica not found: {0}")]
    ReplicaNotFound(String),
    
    /// Quorum not available
    #[error("Quorum not available: {0}")]
    QuorumNotAvailable(String),
    
    /// Quorum not achieved
    #[error("Quorum not achieved: {0}")]
    QuorumNotAchieved(String),
    
    /// Consistency violation
    #[error("Consistency violation: {0}")]
    ConsistencyViolation(String),
    
    /// Session not found
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    
    /// Operation timeout
    #[error("Operation timeout")]
    OperationTimeout,
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// System shutdown
    #[error("System shutdown")]
    Shutdown,
}

/// Result type for replication operations
pub type Result<T> = std::result::Result<T, ReplicationError>;

/// Configuration for the replication system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// Number of replicas to maintain
    pub replica_count: usize,
    /// Heartbeat interval for health checking
    pub heartbeat_interval: Duration,
    /// Timeout for replication operations
    pub operation_timeout: Duration,
    /// Maximum batch size for replication
    pub max_batch_size: usize,
    /// Enable compression for replication traffic
    pub enable_compression: bool,
    /// Buffer size for replication queues
    pub buffer_size: usize,
    /// Maximum number of retries for failed operations
    pub max_retries: usize,
    /// Backoff strategy for retries
    pub retry_backoff: Duration,
    /// Enable automatic failover
    pub enable_auto_failover: bool,
    /// Quorum configuration
    pub quorum: QuorumConfig,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            replica_count: 3,
            heartbeat_interval: Duration::from_secs(1),
            operation_timeout: Duration::from_secs(10),
            max_batch_size: 100,
            enable_compression: true,
            buffer_size: 10000,
            max_retries: 3,
            retry_backoff: Duration::from_millis(100),
            enable_auto_failover: true,
            quorum: QuorumConfig::default(),
        }
    }
}

impl ReplicationConfig {
    /// Validate the replication configuration
    pub fn validate(&self) -> Result<()> {
        if self.replica_count == 0 {
            return Err(ReplicationError::ConfigurationError(
                "replica_count must be greater than 0".to_string(),
            ));
        }
        
        if self.replica_count < 3 {
            warn!("replica_count < 3 may not provide adequate fault tolerance");
        }
        
        if self.heartbeat_interval.is_zero() {
            return Err(ReplicationError::ConfigurationError(
                "heartbeat_interval must be greater than 0".to_string(),
            ));
        }
        
        if self.operation_timeout < self.heartbeat_interval {
            return Err(ReplicationError::ConfigurationError(
                "operation_timeout should be greater than heartbeat_interval".to_string(),
            ));
        }
        
        if self.max_batch_size == 0 {
            return Err(ReplicationError::ConfigurationError(
                "max_batch_size must be greater than 0".to_string(),
            ));
        }
        
        if self.buffer_size == 0 {
            return Err(ReplicationError::ConfigurationError(
                "buffer_size must be greater than 0".to_string(),
            ));
        }
        
        // Validate quorum configuration
        self.quorum.validate()?;
        
        Ok(())
    }
}

/// Write operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    /// Sequence number assigned to the write
    pub sequence: u64,
    /// Number of replicas that acknowledged the write
    pub replicas_acknowledged: usize,
    /// Time taken for the operation
    pub latency: Duration,
    /// Whether consistency requirements were met
    pub consistency_achieved: bool,
    /// List of replicas that acknowledged
    pub acknowledging_replicas: Vec<NodeId>,
}

/// Read operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    /// The data read (None if not found)
    pub data: Option<Vec<u8>>,
    /// Version/timestamp of the data
    pub version: u64,
    /// Number of replicas consulted
    pub replicas_consulted: usize,
    /// Time taken for the operation
    pub latency: Duration,
    /// Whether consistency requirements were met
    pub consistency_achieved: bool,
    /// Source replica that provided the data
    pub source_replica: Option<NodeId>,
}

/// Placeholder WAL implementation for integration with replication
/// This will be replaced with proper WAL integration
#[derive(Debug, Clone)]
pub struct WriteAheadLog {
    /// WAL configuration
    config: WalConfig,
    /// Mock entries for testing
    entries: Arc<RwLock<Vec<ClusterMessage>>>,
}

impl WriteAheadLog {
    /// Create a new WAL instance
    pub async fn new(config: WalConfig) -> std::result::Result<Self, WalError> {
        // Validate configuration
        config.validate()?;
        
        debug!("Initializing WriteAheadLog with config: {:?}", config);
        
        Ok(Self {
            config,
            entries: Arc::new(RwLock::new(Vec::new())),
        })
    }
    
    /// Append an entry to the WAL
    pub async fn append(&self, entry: ClusterMessage) -> std::result::Result<u64, WalError> {
        let mut entries = self.entries.write().await;
        let sequence = entries.len() as u64;
        entries.push(entry);
        
        debug!("Appended entry to WAL at sequence {}", sequence);
        Ok(sequence)
    }
    
    /// Read entries from a given sequence number
    pub async fn read_from(&self, sequence: u64) -> std::result::Result<Vec<(u64, ClusterMessage)>, WalError> {
        let entries = self.entries.read().await;
        let start_idx = sequence as usize;
        
        if start_idx > entries.len() {
            return Ok(Vec::new());
        }
        
        let result: Vec<(u64, ClusterMessage)> = entries[start_idx..]
            .iter()
            .enumerate()
            .map(|(i, entry)| (start_idx as u64 + i as u64, entry.clone()))
            .collect();
        
        debug!("Read {} entries from sequence {}", result.len(), sequence);
        Ok(result)
    }
    
    /// Get the current log length
    pub async fn len(&self) -> u64 {
        let entries = self.entries.read().await;
        entries.len() as u64
    }
    
    /// Truncate the log at the specified sequence number
    pub async fn truncate(&self, sequence: u64) -> std::result::Result<(), WalError> {
        let mut entries = self.entries.write().await;
        let new_len = sequence as usize;
        
        if new_len <= entries.len() {
            entries.truncate(new_len);
            info!("Truncated WAL to sequence {}", sequence);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_replication_config_validation() {
        let mut config = ReplicationConfig::default();
        assert!(config.validate().is_ok());
        
        config.replica_count = 0;
        assert!(config.validate().is_err());
        
        config.replica_count = 3;
        config.heartbeat_interval = Duration::from_secs(0);
        assert!(config.validate().is_err());
        
        config.heartbeat_interval = Duration::from_secs(10);
        config.operation_timeout = Duration::from_secs(1);
        assert!(config.validate().is_err());
    }
    
    #[tokio::test]
    async fn test_wal_integration() {
        let config = WalConfig::default();
        let wal = WriteAheadLog::new(config).await.unwrap();
        
        let message = ClusterMessage::default();
        let sequence = wal.append(message.clone()).await.unwrap();
        assert_eq!(sequence, 0);
        
        let entries = wal.read_from(0).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 0);
        assert_eq!(entries[0].1, message);
    }
    
    #[test]
    fn test_error_types() {
        let err = ReplicationError::ConfigurationError("test".to_string());
        assert!(err.to_string().contains("Configuration error"));
        
        let err = ReplicationError::QuorumNotAvailable("test".to_string());
        assert!(err.to_string().contains("Quorum not available"));
    }
}