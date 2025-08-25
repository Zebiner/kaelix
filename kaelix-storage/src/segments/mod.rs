use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}},
    time::Instant,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};

// Re-export segment types
pub mod entry;
pub mod segment;
pub mod segment_writer;

pub use entry::{StorageEntry, StorageEntryMetadata, CompressionType};
pub use segment::StorageSegment;
pub use segment_writer::{SegmentWriter, SegmentWriterConfig};

/// Unique identifier for a segment
pub type SegmentId = u64;

/// Main errors for segment storage
#[derive(Error, Debug)]
pub enum StorageError {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(String),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// Corruption detected
    #[error("Data corruption detected: {0}")]
    Corruption(String),
    
    /// Segment not found
    #[error("Segment not found: {0}")]
    SegmentNotFound(SegmentId),
    
    /// Entry not found
    #[error("Entry not found: {0}")]
    EntryNotFound(String),
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    /// Resource exhausted
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
}

/// Result type for segment operations
pub type Result<T> = std::result::Result<T, StorageError>;

/// Configuration for segment-based storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentConfig {
    /// Directory to store segment files
    pub data_dir: std::path::PathBuf,
    /// Maximum size for a single segment file (bytes)
    pub max_segment_size: u64,
    /// Maximum number of entries per segment
    pub max_entries_per_segment: u64,
    /// Enable compression for segment data
    pub enable_compression: bool,
    /// Compression level (0-9, higher = more compression)
    pub compression_level: u8,
    /// Write buffer size for segments
    pub write_buffer_size: usize,
    /// Enable synchronous writes
    pub sync_on_write: bool,
    /// Interval for automatic synchronization (milliseconds)
    pub sync_interval_ms: u64,
    /// Maximum number of concurrent readers
    pub max_concurrent_readers: usize,
    /// Enable background compaction
    pub enable_compaction: bool,
    /// Compaction trigger threshold (fraction of deleted entries)
    pub compaction_threshold: f64,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            data_dir: std::path::PathBuf::from("segments"),
            max_segment_size: 64 * 1024 * 1024, // 64MB
            max_entries_per_segment: 100_000,
            enable_compression: true,
            compression_level: 6,
            write_buffer_size: 64 * 1024, // 64KB
            sync_on_write: false,
            sync_interval_ms: 1000,
            max_concurrent_readers: 16,
            enable_compaction: true,
            compaction_threshold: 0.3,
        }
    }
}

impl SegmentConfig {
    /// Validate the segment configuration
    pub fn validate(&self) -> Result<()> {
        if self.max_segment_size == 0 {
            return Err(StorageError::Configuration(
                "max_segment_size must be greater than 0".to_string(),
            ));
        }
        
        if self.max_entries_per_segment == 0 {
            return Err(StorageError::Configuration(
                "max_entries_per_segment must be greater than 0".to_string(),
            ));
        }
        
        if self.compression_level > 9 {
            return Err(StorageError::Configuration(
                "compression_level must be between 0 and 9".to_string(),
            ));
        }
        
        if self.write_buffer_size == 0 {
            return Err(StorageError::Configuration(
                "write_buffer_size must be greater than 0".to_string(),
            ));
        }
        
        if self.max_concurrent_readers == 0 {
            return Err(StorageError::Configuration(
                "max_concurrent_readers must be greater than 0".to_string(),
            ));
        }
        
        if !(0.0..=1.0).contains(&self.compaction_threshold) {
            return Err(StorageError::Configuration(
                "compaction_threshold must be between 0.0 and 1.0".to_string(),
            ));
        }
        
        Ok(())
    }
}

/// Statistics for segment operations
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SegmentStats {
    /// Total number of segments
    pub total_segments: u64,
    /// Total entries written
    pub entries_written: u64,
    /// Total entries read
    pub entries_read: u64,
    /// Total bytes written
    pub bytes_written: u64,
    /// Total bytes read
    pub bytes_read: u64,
    /// Number of active segments
    pub active_segments: u64,
    /// Average write latency in microseconds
    pub avg_write_latency_us: u64,
    /// Average read latency in microseconds
    pub avg_read_latency_us: u64,
    /// Compression ratio (compressed/uncompressed)
    pub compression_ratio: f64,
    /// Number of compactions performed
    pub compactions_completed: u64,
    /// Last compaction timestamp
    pub last_compaction: Option<std::time::SystemTime>,
}

/// Segment-based storage manager
pub struct SegmentStorage {
    /// Configuration
    config: SegmentConfig,
    /// Active segments
    active_segments: Arc<RwLock<HashMap<SegmentId, Arc<Mutex<StorageSegment>>>>>,
    /// Segment writer for new entries
    writer: Arc<tokio::sync::Mutex<SegmentWriter>>,
    /// Statistics
    stats: Arc<RwLock<SegmentStats>>,
    /// Next segment ID
    next_segment_id: AtomicU64,
    /// Shutdown flag
    is_shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl SegmentStorage {
    /// Create a new segment storage instance
    pub async fn new(config: SegmentConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;
        
        info!("Initializing segment storage with config: {:?}", config);
        
        // Create data directory
        if !config.data_dir.exists() {
            tokio::fs::create_dir_all(&config.data_dir).await
                .map_err(|e| StorageError::Io(format!("Failed to create data directory: {}", e)))?;
        }
        
        // Create segment writer
        let segment_id = 0;
        let writer_config = SegmentWriterConfig {
            segment_size: config.max_segment_size as usize,
            buffer_size: config.write_buffer_size,
            compression_enabled: config.enable_compression,
            checksum_enabled: true,
        };

        let writer = SegmentWriter::new(segment_id, &config.data_dir, config.enable_compression, writer_config).await
            .map_err(|e| StorageError::Io(format!("Failed to create segment writer: {}", e)))?;

        Ok(Self {
            config,
            active_segments: Arc::new(RwLock::new(HashMap::new())),
            writer: Arc::new(tokio::sync::Mutex::new(writer)),
            stats: Arc::new(RwLock::new(SegmentStats::default())),
            next_segment_id: AtomicU64::new(1),
            is_shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
    
    /// Write an entry to storage
    pub async fn write_entry(&self, entry: StorageEntry) -> Result<SegmentId> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Err(StorageError::Configuration("Storage is shutting down".to_string()));
        }
        
        let start_time = Instant::now();
        let mut writer = self.writer.lock().await;
        
        // Write the entry - convert error types
        let write_result = writer.write_entry(&entry).await
            .map_err(|e| match e {
                crate::StorageError::Io(io_err) => StorageError::Io(io_err.to_string()),
                crate::StorageError::Serialization(msg) => StorageError::Serialization(msg),
                _ => StorageError::Io(format!("Writer error: {}", e)),
            })?;
        
        let segment_id = writer.segment_id();
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.entries_written += 1;
            stats.bytes_written += entry.size() as u64;
            
            let write_latency_us = start_time.elapsed().as_micros() as u64;
            let total_writes = stats.entries_written;
            if total_writes > 0 {
                stats.avg_write_latency_us = (stats.avg_write_latency_us * (total_writes - 1) + write_latency_us) / total_writes;
            }
        }
        
        debug!("Wrote entry to segment {} at offset {}", segment_id, write_result.offset);
        Ok(segment_id)
    }
    
    /// Read entries from a segment
    pub async fn read_entries(&self, _segment_id: SegmentId, _offset: u64, limit: usize) -> Result<Vec<StorageEntry>> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Err(StorageError::Configuration("Storage is shutting down".to_string()));
        }
        
        let start_time = Instant::now();
        
        // For now, return empty entries - would implement actual reading logic
        let entries = Vec::with_capacity(limit);
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.entries_read += entries.len() as u64;
            
            let read_latency_us = start_time.elapsed().as_micros() as u64;
            if stats.entries_read > 0 {
                stats.avg_read_latency_us = (stats.avg_read_latency_us * (stats.entries_read - entries.len() as u64) + read_latency_us) / stats.entries_read;
            }
        }
        
        Ok(entries)
    }
    
    /// Get all segment IDs
    pub async fn list_segments(&self) -> Result<Vec<SegmentId>> {
        let segments = self.active_segments.read().await;
        
        // Simulate reading from disk - would implement actual directory listing
        let segment_ids: Vec<SegmentId> = Vec::new();
        
        // Update active count from actual segments
        for _segment in segments.iter() {
            // Would check segment status here
        }
        
        Ok(segment_ids)
    }
    
    /// Create a new segment
    pub async fn create_segment(&self) -> Result<SegmentId> {
        let segment_id = self.next_segment_id.fetch_add(1, Ordering::Relaxed);
        
        let segment = StorageSegment::create_new(
            segment_id,
            &self.config.data_dir,
            self.config.enable_compression,
        ).await.map_err(|e| match e {
            crate::StorageError::Io(io_err) => StorageError::Io(io_err.to_string()),
            crate::StorageError::Serialization(msg) => StorageError::Serialization(msg),
            _ => StorageError::Io(format!("Segment creation error: {}", e)),
        })?;

        let segment_arc = Arc::new(Mutex::new(segment));

        // Add to active segments
        {
            let mut active_segments = self.active_segments.write().await;
            active_segments.insert(segment_id, Arc::clone(&segment_arc));
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_segments += 1;
            stats.active_segments = self.active_segments.read().await.len() as u64;
        }
        
        info!("Created new segment: {}", segment_id);
        Ok(segment_id)
    }
    
    /// Get current statistics
    pub fn stats(&self) -> SegmentStats {
        // Use try_read to avoid blocking if there's a write operation
        match self.stats.try_read() {
            Ok(stats) => stats.clone(),
            Err(_) => {
                // Return default stats if we can't acquire the lock
                SegmentStats::default()
            }
        }
    }
    
    /// Perform segment compaction
    pub async fn compact_segments(&self) -> Result<u64> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Err(StorageError::Configuration("Storage is shutting down".to_string()));
        }
        
        info!("Starting segment compaction");
        let start_time = Instant::now();
        
        // Placeholder compaction logic
        let bytes_reclaimed = 0u64;
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.compactions_completed += 1;
            stats.last_compaction = Some(std::time::SystemTime::now());
        }
        
        let duration = start_time.elapsed();
        info!("Compaction completed in {:?}, reclaimed {} bytes", duration, bytes_reclaimed);
        
        Ok(bytes_reclaimed)
    }
    
    /// Shutdown the storage system
    pub async fn shutdown(&self) -> Result<()> {
        self.is_shutdown.store(true, Ordering::Relaxed);
        
        info!("Shutting down segment storage");
        
        // Flush writer
        {
            let mut writer = self.writer.lock().await;
            writer.flush().await.map_err(|e| StorageError::Io(format!("Flush error: {}", e)))?;
        }
        
        // Clear active segments
        {
            let mut active_segments = self.active_segments.write().await;
            active_segments.clear();
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use bytes::Bytes;
    
    async fn create_test_storage() -> (SegmentStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = SegmentConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        
        let storage = SegmentStorage::new(config).await.unwrap();
        (storage, temp_dir)
    }
    
    #[tokio::test]
    async fn test_segment_storage_creation() {
        let (_storage, _temp_dir) = create_test_storage().await;
        // Storage should be created successfully
    }
    
    #[tokio::test]
    async fn test_segment_config_validation() {
        let mut config = SegmentConfig::default();
        assert!(config.validate().is_ok());
        
        config.max_segment_size = 0;
        assert!(config.validate().is_err());
        
        config.max_segment_size = 1024;
        config.compression_level = 10;
        assert!(config.validate().is_err());
    }
    
    #[tokio::test]
    async fn test_write_entry() {
        let (storage, _temp_dir) = create_test_storage().await;
        
        let entry = StorageEntry::new(
            1,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros() as u64,
            Bytes::from("test data"),
        );
        
        let segment_id = storage.write_entry(entry).await.unwrap();
        assert_eq!(segment_id, 0);
        
        let stats = storage.stats();
        assert_eq!(stats.entries_written, 1);
    }
    
    #[tokio::test]
    async fn test_segment_creation() {
        let (storage, _temp_dir) = create_test_storage().await;
        
        let segment_id = storage.create_segment().await.unwrap();
        assert_eq!(segment_id, 1);
        
        let stats = storage.stats();
        assert_eq!(stats.total_segments, 1);
    }
    
    #[tokio::test]
    async fn test_storage_shutdown() {
        let (storage, _temp_dir) = create_test_storage().await;
        
        storage.shutdown().await.unwrap();
        
        // Operations should fail after shutdown
        let entry = StorageEntry::new(
            1,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros() as u64,
            Bytes::from("test"),
        );
        
        assert!(storage.write_entry(entry).await.is_err());
    }
}