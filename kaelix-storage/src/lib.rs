#[warn(missing_docs, unused)]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

// Re-export all submodules
pub mod segments;
pub mod transactions;
pub mod wal;

use kaelix_cluster::{
    messages::{ClusterMessage, MessagePayload},
    types::NodeId,
};

use crate::{
    segments::{SegmentStorage, SegmentStats},
    transactions::{TransactionManager, TransactionStats},
    wal::{Wal, WalConfig, WalStats},
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
            .map_err(StorageError::Configuration)?;

        // Validate segment config
        self.segment_storage
            .validate()
            .map_err(StorageError::Configuration)?;

        // Validate transaction config
        self.transaction_manager
            .validate()
            .map_err(StorageError::Configuration)?;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    /// WAL statistics
    pub wal_stats: WalStats,

    /// Transaction statistics
    pub transaction_stats: TransactionStats,

    /// Segment statistics
    pub segment_stats: SegmentStats,
}

/// The main storage engine
///
/// Provides a unified interface to all storage subsystems:
/// - Write-Ahead Log (WAL) for durability
/// - Transaction management for ACID properties
/// - Segment storage for efficient data organization
pub struct StorageEngine {
    /// Engine configuration
    config: Arc<StorageConfig>,

    /// Write-Ahead Log
    wal: Wal,

    /// Transaction manager
    transaction_manager: TransactionManager,

    /// Segment storage
    segment_storage: SegmentStorage,

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

        info!("Storage engine initialized successfully");

        Ok(Self {
            config,
            wal,
            transaction_manager,
            segment_storage,
            is_shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Write data to the storage engine
    pub async fn write(&self, data: Vec<u8>) -> Result<u64> {
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

        Ok(sequence)
    }

    /// Shutdown the storage engine gracefully
    pub async fn shutdown(&self) -> Result<()> {
        self.is_shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // Shutdown components
        self.wal.shutdown().await?;
        self.transaction_manager.shutdown().await?;
        self.segment_storage.shutdown().await?;

        Ok(())
    }

    /// Get engine statistics
    pub async fn stats(&self) -> EngineStats {
        EngineStats {
            wal_stats: self.wal.stats().await,
            transaction_stats: self.transaction_manager.stats().await,
            segment_stats: self.segment_storage.stats(),
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

    #[tokio::test]
    async fn test_storage_engine_creation() {
        let (_engine, _temp_dir) = create_test_engine().await;
        // Engine should be created successfully
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
    }

    #[tokio::test]
    async fn test_storage_engine_shutdown() {
        let (engine, _temp_dir) = create_test_engine().await;
        
        // Shutdown should succeed
        engine.shutdown().await.unwrap();

        // Further operations should fail
        let data = b"test data".to_vec();
        assert!(engine.write(data).await.is_err());
    }
}