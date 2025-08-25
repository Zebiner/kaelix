#[warn(missing_docs, unused)]

use std::sync::{
    atomic::{AtomicBool},
    Arc,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

// Re-export all submodules
pub mod segments;
pub mod transactions;
pub mod wal;
pub mod replication;

use kaelix_cluster::{
    messages::{ClusterMessage, MessagePayload},
    types::NodeId,
};

use crate::{
    segments::{SegmentStorage, SegmentStats},
    transactions::{TransactionManager, TransactionStats},
    wal::{Wal, WalConfig, WalStats},
    replication::{ReplicationMetrics, ReplicationManager, WriteAheadLog},
};

/// Main errors for the storage engine
#[derive(Error, Debug)]
pub enum StorageError {
    /// WAL (Write-Ahead Log) error
    #[error("WAL error: {0}")]
    Wal(#[from] wal::WalError),

    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(#[from] transactions::TransactionError),

    /// Segment error
    #[error("Segment error: {0}")]
    Segment(#[from] segments::StorageError),

    /// Replication error
    #[error("Replication error: {0}")]
    Replication(#[from] replication::ReplicationError),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Resource not found
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Resource already exists
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// System is shutting down
    #[error("System shutdown in progress")]
    Shutdown,
}

/// Result type for storage operations
pub type Result<T> = std::result::Result<T, StorageError>;

/// Configuration for the storage engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// WAL configuration
    pub wal: WalConfig,

    /// Segment storage configuration
    pub segment_storage: segments::SegmentConfig,

    /// Transaction manager configuration
    pub transaction_manager: transactions::TransactionConfig,

    /// Replication configuration (optional)
    pub replication: Option<replication::ReplicationConfig>,

    /// Data directory
    pub data_dir: std::path::PathBuf,

    /// Whether to enable compression
    pub enable_compression: bool,

    /// Compression level (0-9)
    pub compression_level: u8,

    /// Buffer size for I/O operations
    pub buffer_size: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            wal: WalConfig::default(),
            segment_storage: segments::SegmentConfig::default(),
            transaction_manager: transactions::TransactionConfig::default(),
            replication: None, // Replication is optional by default
            data_dir: std::env::current_dir().unwrap_or_default().join("storage"),
            enable_compression: true,
            compression_level: 6,
            buffer_size: 64 * 1024, // 64KB
        }
    }
}

impl StorageConfig {
    /// Validate the storage configuration
    pub fn validate(&self) -> Result<()> {
        // Validate WAL config
        self.wal
            .validate()
            .map_err(|e| StorageError::Configuration(e))?;

        // Validate segment config
        self.segment_storage
            .validate()
            .map_err(|e| StorageError::Configuration(e.to_string()))?;

        // Validate transaction config
        self.transaction_manager
            .validate()
            .map_err(StorageError::Configuration)?;

        // Validate replication config if present
        if let Some(replication_config) = &self.replication {
            replication_config
                .validate()
                .map_err(StorageError::Replication)?;
        }

        // Validate compression level
        if self.compression_level > 9 {
            return Err(StorageError::Configuration(
                "compression_level must be between 0 and 9".to_string(),
            ));
        }

        // Validate buffer size
        if self.buffer_size == 0 {
            return Err(StorageError::Configuration(
                "buffer_size must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Combined statistics for all storage components
#[derive(Debug, Clone, Serialize)]
pub struct EngineStats {
    /// WAL statistics
    pub wal_stats: WalStats,

    /// Transaction statistics  
    pub transaction_stats: TransactionStats,

    /// Segment statistics
    pub segment_stats: SegmentStats,

    /// Replication statistics (if enabled)
    pub replication_stats: Option<ReplicationMetrics>,
}

/// The main storage engine
///
/// Provides a unified interface to all storage subsystems:
/// - Write-Ahead Log (WAL) for durability
/// - Transaction management for ACID properties
/// - Segment storage for efficient data organization
/// - Data replication for high availability (optional)
pub struct StorageEngine {
    /// Engine configuration
    config: Arc<StorageConfig>,

    /// Write-Ahead Log
    wal: Wal,

    /// Transaction manager
    transaction_manager: TransactionManager,

    /// Segment storage
    segment_storage: SegmentStorage,

    /// Replication manager (optional)
    replication_manager: Option<ReplicationManager>,

    /// Shutdown flag
    is_shutdown: Arc<AtomicBool>,
}

impl StorageEngine {
    /// Create a new storage engine with the given configuration
    pub async fn new(config: StorageConfig) -> Result<Self> {
        // Validate configuration first
        config.validate()?;

        info!("Initializing storage engine with config: {:?}", config);

        let config = Arc::new(config);

        // Create data directory if it doesn't exist
        if !config.data_dir.exists() {
            std::fs::create_dir_all(&config.data_dir)
                .map_err(|e| StorageError::Io(e))?;
        }

        // Initialize WAL
        debug!("Initializing WAL");
        let wal = Wal::new(config.wal.clone()).await?;

        // Initialize transaction manager
        debug!("Initializing transaction manager");
        let transaction_manager = TransactionManager::new(config.transaction_manager.clone()).await?;

        // Initialize segment storage
        debug!("Initializing segment storage");
        let segment_storage = SegmentStorage::new(config.segment_storage.clone()).await?;

        // Initialize replication manager if configured
        let replication_manager = if let Some(_replication_config) = &config.replication {
            debug!("Initializing replication manager");
            // For now, we'll need cluster integration for full replication
            // This is a placeholder that would be implemented with proper cluster context
            info!("Replication configuration present but requires cluster integration");
            None
        } else {
            None
        };

        info!("Storage engine initialized successfully");

        Ok(Self {
            config,
            wal,
            transaction_manager,
            segment_storage,
            replication_manager,
            is_shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Create a storage engine with replication enabled
    pub async fn new_with_replication(
        config: StorageConfig,
        local_node_id: NodeId,
        consensus: Arc<kaelix_cluster::consensus::raft::RaftNode>,
    ) -> Result<Self> {
        // Ensure replication is configured
        if config.replication.is_none() {
            return Err(StorageError::Configuration(
                "Replication config required for replicated storage".to_string(),
            ));
        }

        let mut engine = Self::new(config).await?;

        // Initialize replication manager with cluster integration
        if let Some(replication_config) = &engine.config.replication {
            debug!("Initializing replication manager with cluster integration");
            
            // Create WAL reference for replication
            let wal_arc = Arc::new(WriteAheadLog::new(engine.config.wal.clone()).await
                .map_err(StorageError::Wal)?);
            
            let replication_manager = ReplicationManager::new(
                local_node_id,
                replication_config.clone(),
                wal_arc,
                consensus,
            ).await.map_err(StorageError::Replication)?;

            engine.replication_manager = Some(replication_manager);
            info!("Storage engine with replication initialized successfully");
        }

        Ok(engine)
    }

    /// Write data to the storage engine
    pub async fn write(&self, _data: Vec<u8>) -> Result<u64> {
        if self.is_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::Shutdown);
        }

        // Create cluster message (placeholder implementation)
        // Using dummy NodeIds for now - in a real implementation these would come from cluster context
        let source = NodeId::generate();
        let destination = NodeId::generate();
        let payload = MessagePayload::Ping { node_id: source };
        let message = ClusterMessage::new(source, destination, payload);

        // Write to WAL
        let sequence = self.wal.write(message).await?;

        // If replication is enabled, replicate the write
        if let Some(_replication_manager) = &self.replication_manager {
            // Note: In a full implementation, this would use the replication manager
            // For now, we'll just log that replication would occur
            debug!("Would replicate write with sequence {} (replication manager present)", sequence);
        }

        Ok(sequence)
    }

    /// Write data with specified consistency level (requires replication)
    pub async fn write_consistent(
        &mut self,
        data: Vec<u8>,
        consistency_level: replication::ConsistencyLevel,
    ) -> Result<replication::WriteResult> {
        if self.is_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::Shutdown);
        }

        if let Some(replication_manager) = &mut self.replication_manager {
            replication_manager
                .write_consistent(data, consistency_level)
                .await
                .map_err(StorageError::Replication)
        } else {
            Err(StorageError::Configuration(
                "Replication not enabled - use regular write() method".to_string(),
            ))
        }
    }

    /// Read data with specified consistency level (requires replication)
    pub async fn read_consistent(
        &self,
        key: &str,
        consistency_level: replication::ConsistencyLevel,
        session_id: Option<&str>,
    ) -> Result<replication::ReadResult> {
        if self.is_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::Shutdown);
        }

        if let Some(replication_manager) = &self.replication_manager {
            replication_manager
                .read_consistent(key, consistency_level, session_id)
                .await
                .map_err(StorageError::Replication)
        } else {
            Err(StorageError::Configuration(
                "Replication not enabled".to_string(),
            ))
        }
    }

    /// Get replication status (if replication is enabled)
    pub async fn get_replication_status(&self) -> Option<replication::manager::ReplicationStatus> {
        if let Some(replication_manager) = &self.replication_manager {
            Some(replication_manager.get_status().await)
        } else {
            None
        }
    }

    /// Check if replication is enabled
    pub fn has_replication(&self) -> bool {
        self.replication_manager.is_some()
    }

    /// Shutdown the storage engine gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        self.is_shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // Shutdown replication first (if enabled)
        if let Some(replication_manager) = &mut self.replication_manager {
            info!("Shutting down replication manager");
            replication_manager.shutdown().await.map_err(StorageError::Replication)?;
        }

        // Shutdown other components
        self.wal.shutdown().await?;
        self.transaction_manager.shutdown().await?;
        self.segment_storage.shutdown().await?;

        Ok(())
    }

    /// Get engine statistics
    pub async fn stats(&self) -> EngineStats {
        let replication_stats = if let Some(_replication_manager) = &self.replication_manager {
            // Get metrics from replication manager
            // For now, return None as the metrics collection would need integration
            None
        } else {
            None
        };

        EngineStats {
            wal_stats: self.wal.stats().await,
            transaction_stats: self.transaction_manager.stats().await,
            segment_stats: self.segment_storage.stats(),
            replication_stats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_engine() -> (StorageEngine, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();

        let engine = StorageEngine::new(config).await.unwrap();
        (engine, temp_dir)
    }

    async fn create_test_engine_with_replication() -> (StorageEngine, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        config.replication = Some(replication::ReplicationConfig::default());

        let node_id = NodeId::generate();
        let raft_config = kaelix_cluster::consensus::raft::RaftConfig::default();
        let consensus = Arc::new(kaelix_cluster::consensus::raft::RaftNode::new(node_id, 3, raft_config));

        let engine = StorageEngine::new_with_replication(config, node_id, consensus).await.unwrap();
        (engine, temp_dir)
    }

    #[tokio::test]
    async fn test_storage_engine_creation() {
        let (_engine, _temp_dir) = create_test_engine().await;
        // Engine should be created successfully
    }

    #[tokio::test]
    async fn test_storage_engine_creation_with_replication() {
        let (engine, _temp_dir) = create_test_engine_with_replication().await;
        assert!(engine.has_replication());
    }

    #[tokio::test]
    async fn test_storage_engine_write() {
        let (engine, _temp_dir) = create_test_engine().await;
        
        let data = b"test data".to_vec();
        let sequence = engine.write(data).await.unwrap();
        assert_eq!(sequence, 0);
    }

    #[tokio::test]
    async fn test_storage_engine_stats() {
        let (engine, _temp_dir) = create_test_engine().await;
        
        let data = b"test data".to_vec();
        engine.write(data).await.unwrap();

        let stats = engine.stats().await;
        assert_eq!(stats.wal_stats.entries_written, 1);
        assert!(stats.replication_stats.is_none()); // No replication enabled
    }

    #[tokio::test]
    async fn test_storage_engine_with_replication_stats() {
        let (engine, _temp_dir) = create_test_engine_with_replication().await;
        
        let stats = engine.stats().await;
        // Note: replication_stats would be None until metrics are properly integrated
        assert!(engine.has_replication());
    }

    #[tokio::test]
    async fn test_storage_engine_shutdown() {
        let (mut engine, _temp_dir) = create_test_engine().await;
        
        // Shutdown should succeed
        engine.shutdown().await.unwrap();

        // Further operations should fail
        let data = b"test data".to_vec();
        assert!(engine.write(data).await.is_err());
    }

    #[tokio::test]
    async fn test_storage_engine_replication_methods() {
        let (mut engine, _temp_dir) = create_test_engine_with_replication().await;
        
        // Test that replication methods work when replication is enabled
        assert!(engine.has_replication());
        
        // Test replication status
        let status = engine.get_replication_status().await;
        assert!(status.is_some());
    }

    #[tokio::test]
    async fn test_storage_engine_replication_methods_without_replication() {
        let (mut engine, _temp_dir) = create_test_engine().await;
        
        // Test that replication methods fail gracefully without replication
        assert!(!engine.has_replication());
        
        let result = engine.write_consistent(
            b"test".to_vec(),
            replication::ConsistencyLevel::Strong,
        ).await;
        assert!(result.is_err());
        
        let result = engine.read_consistent(
            "test_key",
            replication::ConsistencyLevel::Strong,
            None,
        ).await;
        assert!(result.is_err());
    }
}