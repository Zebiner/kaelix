use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex, RwLock,
};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod entry;
pub mod segment;
pub mod segment_writer;

// Re-export types for public use
pub use self::entry::StorageEntry;
pub use self::segment::StorageSegment;
pub use self::segment_writer::{SegmentWriter, WriteResult};

/// Segment identifier
pub type SegmentId = u64;

/// Segment storage errors
#[derive(Error, Debug)]
pub enum StorageError {
    /// I/O operation failed
    #[error("I/O error: {0}")]
    Io(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Segment not found
    #[error("Segment {0} not found")]
    SegmentNotFound(SegmentId),

    /// Segment is full
    #[error("Segment {0} is full")]
    SegmentFull(SegmentId),

    /// Invalid segment ID
    #[error("Invalid segment ID: {0}")]
    InvalidSegmentId(SegmentId),

    /// System is shutting down
    #[error("System is shutting down")]
    Shutdown,

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),
}

/// Result type for segment operations
pub type Result<T> = std::result::Result<T, StorageError>;

/// Configuration for segment storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentConfig {
    /// Directory where segments are stored
    pub data_dir: PathBuf,

    /// Maximum size of a segment in bytes
    pub max_segment_size: u64,

    /// Maximum number of entries per segment
    pub max_entries_per_segment: u64,

    /// Compression settings
    pub enable_compression: bool,
    pub compression_level: u32,

    /// Sync behavior
    pub sync_on_write: bool,
    pub sync_interval_ms: u64,

    /// Buffer settings
    pub write_buffer_size: usize,
    pub read_buffer_size: usize,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./segments"),
            max_segment_size: 256 * 1024 * 1024, // 256 MB
            max_entries_per_segment: 1_000_000,   // 1M entries
            enable_compression: true,
            compression_level: 6,
            sync_on_write: false,
            sync_interval_ms: 1000, // 1 second
            write_buffer_size: 1024 * 1024, // 1 MB
            read_buffer_size: 1024 * 1024,  // 1 MB
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

        if self.write_buffer_size == 0 || self.read_buffer_size == 0 {
            return Err(StorageError::Configuration(
                "buffer sizes must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Statistics for segment storage operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentStats {
    /// Total number of segments created
    pub segments_created: u64,

    /// Total entries written across all segments
    pub entries_written: u64,

    /// Total entries read across all segments
    pub entries_read: u64,

    /// Total bytes written
    pub bytes_written: u64,

    /// Total bytes read
    pub bytes_read: u64,

    /// Current active segments
    pub active_segments: u64,

    /// Average write latency in microseconds
    pub avg_write_latency_us: u64,

    /// Average read latency in microseconds
    pub avg_read_latency_us: u64,

    /// Total compression ratio achieved
    pub compression_ratio: f64,

    /// Total errors encountered
    pub errors_total: u64,
}

/// Writer configuration
#[derive(Debug, Clone)]
pub struct WriterConfig {
    pub data_dir: PathBuf,
    pub max_segment_size: u64,
    pub max_entries_per_segment: u64,
    pub enable_compression: bool,
    pub compression_level: u32,
    pub buffer_size: usize,
    pub sync_on_write: bool,
    pub sync_interval_ms: u64,
}

/// Segment storage manager
///
/// Manages multiple segments and provides a unified interface for writing and reading entries.
/// Handles segment creation, rotation, and cleanup automatically.
#[derive(Debug)]
pub struct SegmentStorage {
    /// Configuration
    config: Arc<SegmentConfig>,

    /// Active segments (segment_id -> segment)
    active_segments: Arc<RwLock<HashMap<SegmentId, Arc<Mutex<StorageSegment>>>>>,

    /// Primary segment writer
    writer: Arc<tokio::sync::Mutex<SegmentWriter>>,

    /// Statistics
    stats: Arc<RwLock<SegmentStats>>,

    /// Shutdown flag
    is_shutdown: Arc<AtomicBool>,

    /// Next segment ID counter
    next_segment_id: Arc<AtomicU64>,
}

impl SegmentStorage {
    /// Create a new segment storage manager
    pub async fn new(config: SegmentConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        let config = Arc::new(config);

        // Create data directory if it doesn't exist
        if !config.data_dir.exists() {
            std::fs::create_dir_all(&config.data_dir)
                .map_err(|e| StorageError::Io(format!("Failed to create data directory: {}", e)))?;
        }

        // Initialize segment writer
        let writer_config = WriterConfig {
            data_dir: config.data_dir.clone(),
            max_segment_size: config.max_segment_size,
            max_entries_per_segment: config.max_entries_per_segment,
            enable_compression: config.enable_compression,
            compression_level: config.compression_level,
            buffer_size: config.write_buffer_size,
            sync_on_write: config.sync_on_write,
            sync_interval_ms: config.sync_interval_ms,
        };

        let writer = SegmentWriter::new(writer_config).await
            .map_err(|e| StorageError::Io(format!("Failed to create segment writer: {}", e)))?;

        Ok(Self {
            config,
            active_segments: Arc::new(RwLock::new(HashMap::new())),
            writer: Arc::new(tokio::sync::Mutex::new(writer)),
            stats: Arc::new(RwLock::new(SegmentStats::default())),
            is_shutdown: Arc::new(AtomicBool::new(false)),
            next_segment_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Write an entry to the segment storage
    pub async fn write_entry(&self, entry: &StorageEntry) -> Result<SegmentId> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Err(StorageError::Shutdown);
        }

        let writer = self.writer.lock().await;
        
        // Write entry using the segment writer
        let result = writer.write_entry(entry).await
            .map_err(|e| StorageError::Io(format!("Failed to write entry: {}", e)))?;

        // Update statistics
        {
            let mut stats = self.stats.write().unwrap();
            stats.entries_written += 1;
            stats.bytes_written += entry.payload.len() as u64;
        }

        Ok(result.segment_id)
    }

    /// Read entries from a segment
    pub async fn read_entries(&self, segment_id: SegmentId, offset: u64, limit: usize) -> Result<Vec<StorageEntry>> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Err(StorageError::Shutdown);
        }

        // For now, return empty vec as this is a placeholder implementation
        // In a real implementation, this would:
        // 1. Find the segment by ID
        // 2. Create a segment reader
        // 3. Read entries from the specified offset
        // 4. Apply the limit
        // 5. Update read statistics

        {
            let mut stats = self.stats.write().unwrap();
            stats.entries_read += limit as u64;
        }

        Ok(vec![])
    }

    /// Get storage statistics
    pub fn stats(&self) -> SegmentStats {
        self.stats.read().unwrap().clone()
    }

    /// Shutdown the segment storage
    pub async fn shutdown(&self) -> Result<()> {
        self.is_shutdown.store(true, Ordering::Relaxed);

        // Close all active segments
        let segments = self.active_segments.read().unwrap();
        for segment in segments.iter() {
            // In a full implementation, this would properly close each segment
        }

        // Shutdown the writer
        let writer = self.writer.lock().await;
        writer.flush().await
            .map_err(|e| StorageError::Io(format!("Failed to flush writer during shutdown: {}", e)))?;

        Ok(())
    }

    /// Create a new segment
    async fn create_segment(&self) -> Result<Arc<Mutex<StorageSegment>>> {
        let segment_id = self.next_segment_id.fetch_add(1, Ordering::SeqCst);
        let segment_path = self.config.data_dir.join(format!("segment-{}.log", segment_id));

        let segment = StorageSegment::create(
            segment_id,
            segment_path,
            self.config.max_segment_size,
            self.config.max_entries_per_segment,
        )?;

        let segment_arc = Arc::new(Mutex::new(segment));

        // Add to active segments
        {
            let mut active_segments = self.active_segments.write().unwrap();
            active_segments.insert(segment_id, Arc::clone(&segment_arc));
        }

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.segments_created += 1;
            stats.active_segments = self.active_segments.read().unwrap().len() as u64;
        }

        Ok(segment_arc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_segment_config_validation() {
        let config = SegmentConfig::default();
        assert!(config.validate().is_ok());

        let mut invalid_config = config.clone();
        invalid_config.max_segment_size = 0;
        assert!(invalid_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_segment_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = SegmentConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();

        let storage = SegmentStorage::new(config).await.unwrap();
        let stats = storage.stats();
        assert_eq!(stats.segments_created, 0);
    }

    #[tokio::test]
    async fn test_segment_storage_write() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = SegmentConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();

        let storage = SegmentStorage::new(config).await.unwrap();

        let entry = StorageEntry::new(1, 12345, b"test data".to_vec().into());
        let segment_id = storage.write_entry(&entry).await.unwrap();
        assert_eq!(segment_id, 1);

        let stats = storage.stats();
        assert_eq!(stats.entries_written, 1);
        assert!(stats.bytes_written > 0);
    }

    #[tokio::test]
    async fn test_segment_storage_shutdown() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = SegmentConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();

        let storage = SegmentStorage::new(config).await.unwrap();
        storage.shutdown().await.unwrap();

        let entry = StorageEntry::new(1, 12345, b"test data".to_vec().into());
        assert!(storage.write_entry(&entry).await.is_err());
    }
}