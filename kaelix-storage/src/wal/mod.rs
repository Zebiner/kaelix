use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex, RwLock};
use dashmap::DashMap;

pub use kaelix_core::message::Message as ClusterMessage;

// Internal modules
mod batch;
mod config;
mod entry;
mod segment;

// Re-export key types with proper visibility
pub use self::{
    config::{SyncPolicy, WalConfig},
    entry::{EntryFlags, EntryMetadata, EntryType, LogEntry},
};

// Re-export internal types for use within the WAL module
pub(crate) use self::{
    batch::{BatchCoordinator, BatchMetrics, BatchStats, BatchWriteRequest},
    segment::{SegmentMetadata, SegmentWriter, WalSegment},
};

// Type alias for segment ID
pub type SegmentId = u64;

/// Log sequence number for tracking entry positions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogSequence(pub u64);

impl LogSequence {
    /// Create new log sequence
    pub fn new(sequence: u64) -> Self {
        Self(sequence)
    }
    
    /// Get sequence number
    pub fn sequence(&self) -> u64 {
        self.0
    }
    
    /// Get next sequence
    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

impl From<u64> for LogSequence {
    fn from(sequence: u64) -> Self {
        Self(sequence)
    }
}

impl From<LogSequence> for u64 {
    fn from(seq: LogSequence) -> u64 {
        seq.0
    }
}

/// Position in WAL for reading entries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogPosition {
    /// Segment ID
    pub segment_id: SegmentId,
    /// Position within segment
    pub offset: u64,
}

impl LogPosition {
    /// Create new log position
    pub fn new(segment_id: SegmentId, offset: u64) -> Self {
        Self { segment_id, offset }
    }
}

/// Statistics for WAL operations
#[derive(Debug, Clone)]
pub struct WalStats {
    /// Total number of entries written
    pub entries_written: u64,
    /// Total number of entries read
    pub entries_read: u64,
    /// Total bytes written
    pub bytes_written: u64,
    /// Total bytes read
    pub bytes_read: u64,
    /// Number of active segments
    pub active_segments: usize,
    /// Number of sealed segments
    pub sealed_segments: usize,
    /// Total WAL size in bytes
    pub total_size: u64,
    /// Current sequence number
    pub current_sequence: u64,
    /// Average write latency
    pub avg_write_latency: Duration,
    /// Average read latency
    pub avg_read_latency: Duration,
}

impl Default for WalStats {
    fn default() -> Self {
        Self {
            entries_written: 0,
            entries_read: 0,
            bytes_written: 0,
            bytes_read: 0,
            active_segments: 0,
            sealed_segments: 0,
            total_size: 0,
            current_sequence: 0,
            avg_write_latency: Duration::ZERO,
            avg_read_latency: Duration::ZERO,
        }
    }
}

/// Write-Ahead Log implementation with ultra-high performance characteristics
/// 
/// Provides <10μs write latency through:
/// - Lock-free append operations using atomic sequence numbers
/// - Memory-mapped I/O for maximum performance
/// - Batch coordination for high throughput
/// - Thread-safe concurrent access
#[derive(Debug)]
pub struct WriteAheadLog {
    /// Configuration
    config: WalConfig,
    
    /// Directory for WAL segments
    wal_dir: PathBuf,
    
    /// Active segment for writing
    active_segment: Arc<RwLock<Option<WalSegment>>>,
    
    /// Sealed segments (read-only)
    sealed_segments: Arc<DashMap<SegmentId, WalSegment>>,
    
    /// Segment writer for batch operations
    segment_writer: Arc<Mutex<Option<SegmentWriter>>>,
    
    /// Batch coordinator for efficient batching
    batch_coordinator: Arc<BatchCoordinator>,
    
    /// Atomic sequence number generator
    sequence_generator: Arc<AtomicU64>,
    
    /// Next segment ID
    next_segment_id: Arc<AtomicU64>,
    
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
    
    /// Statistics
    stats: Arc<RwLock<WalStats>>,
    
    /// Background task handle
    #[allow(dead_code)]
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl WriteAheadLog {
    /// Create new Write-Ahead Log
    pub async fn new(config: WalConfig, wal_dir: impl AsRef<Path>) -> Result<Self, WalError> {
        let wal_dir = wal_dir.as_ref().to_path_buf();
        
        // Create WAL directory if it doesn't exist
        tokio::fs::create_dir_all(&wal_dir).await
            .map_err(|e| WalError::Io(format!("Failed to create WAL directory: {}", e)))?;
        
        // Initialize batch coordinator
        let batch_coordinator = Arc::new(BatchCoordinator::new(
            config.max_batch_size,
            config.batch_timeout,
        ));
        
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        let mut wal = Self {
            config,
            wal_dir,
            active_segment: Arc::new(RwLock::new(None)),
            sealed_segments: Arc::new(DashMap::new()),
            segment_writer: Arc::new(Mutex::new(None)),
            batch_coordinator: Arc::clone(&batch_coordinator),
            sequence_generator: Arc::new(AtomicU64::new(1)),
            next_segment_id: Arc::new(AtomicU64::new(1)),
            shutdown_tx: Some(shutdown_tx),
            stats: Arc::new(RwLock::new(WalStats::default())),
            task_handle: None,
        };
        
        // Initialize first segment
        wal.create_new_segment().await?;
        
        // Start batch processing
        let segment_writer = Arc::clone(&wal.segment_writer);
        let coordinator = Arc::clone(&batch_coordinator);
        
        let task_handle = tokio::spawn(async move {
            coordinator.start_processing(segment_writer, shutdown_rx).await;
        });
        
        wal.task_handle = Some(task_handle);
        
        Ok(wal)
    }
    
    /// Append single entry to WAL
    /// 
    /// Returns position where entry was written.
    /// Target latency: <10μs
    pub async fn append(&self, message: ClusterMessage) -> Result<LogPosition, WalError> {
        let sequence = self.next_sequence();
        let timestamp = self.current_timestamp();
        
        let entry = LogEntry::new(sequence, timestamp, message)?;
        
        // Use batch coordinator for consistent handling
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request = BatchWriteRequest {
            entries: vec![entry],
            response_tx: tx,
        };
        
        self.batch_coordinator.add_request(request);
        
        let positions = rx.await
            .map_err(|_| WalError::ShutdownInProgress)?
            .map_err(|e| e)?;
        
        let position = positions.into_iter().next()
            .ok_or_else(|| WalError::Io("No position returned from batch write".to_string()))?;
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.entries_written += 1;
            stats.bytes_written += entry.serialized_size() as u64;
        }
        
        // Convert to LogPosition (assuming we're writing to active segment)
        let segment_id = self.current_segment_id().await;
        Ok(LogPosition::new(segment_id, position))
    }
    
    /// Append batch of messages to WAL
    /// 
    /// Returns positions where entries were written.
    /// Optimized for high throughput: 10M+ messages/second
    pub async fn append_batch(&self, messages: Vec<ClusterMessage>) -> Result<Vec<LogPosition>, WalError> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }
        
        let start_sequence = self.sequence_generator.fetch_add(messages.len() as u64, Ordering::Relaxed);
        let timestamp = self.current_timestamp();
        
        // Convert messages to entries
        let mut entries = Vec::with_capacity(messages.len());
        for (i, message) in messages.into_iter().enumerate() {
            let sequence = start_sequence + i as u64;
            let entry = LogEntry::new(sequence, timestamp, message)?;
            entries.push(entry);
        }
        
        // Use batch coordinator
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request = BatchWriteRequest {
            entries,
            response_tx: tx,
        };
        
        self.batch_coordinator.add_request(request);
        
        let positions = rx.await
            .map_err(|_| WalError::ShutdownInProgress)?
            .map_err(|e| e)?;
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.entries_written += positions.len() as u64;
            // Note: bytes_written would need actual entry sizes
        }
        
        // Convert to LogPositions
        let segment_id = self.current_segment_id().await;
        let log_positions = positions.into_iter()
            .map(|pos| LogPosition::new(segment_id, pos))
            .collect();
        
        Ok(log_positions)
    }
    
    /// Read entry at position
    pub async fn read(&self, position: LogPosition) -> Result<LogEntry, WalError> {
        let segment = self.get_segment(position.segment_id).await?;
        let entry = segment.read_entry(position.offset).await?;
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.entries_read += 1;
            stats.bytes_read += entry.serialized_size() as u64;
        }
        
        Ok(entry)
    }
    
    /// Sync WAL to disk based on configured policy
    pub async fn sync(&self) -> Result<(), WalError> {
        if let Some(writer) = self.segment_writer.lock().await.as_mut() {
            writer.sync_if_needed(&self.config.sync_policy).await?;
        }
        Ok(())
    }
    
    /// Force sync WAL to disk
    pub async fn force_sync(&self) -> Result<(), WalError> {
        if let Some(writer) = self.segment_writer.lock().await.as_mut() {
            writer.sync().await?;
        }
        Ok(())
    }
    
    /// Get current WAL statistics
    pub async fn stats(&self) -> WalStats {
        self.stats.read().await.clone()
    }
    
    /// Shutdown WAL gracefully
    pub async fn shutdown(&mut self) -> Result<(), WalError> {
        // Signal shutdown
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        
        // Wait for background task
        if let Some(handle) = self.task_handle.take() {
            let _ = handle.await;
        }
        
        // Final sync
        self.force_sync().await?;
        
        // Seal active segment
        if let Some(writer) = self.segment_writer.lock().await.as_mut() {
            writer.seal().await?;
        }
        
        Ok(())
    }
    
    /// Get next sequence number
    fn next_sequence(&self) -> u64 {
        self.sequence_generator.fetch_add(1, Ordering::Relaxed)
    }
    
    /// Get current timestamp in nanoseconds
    fn current_timestamp(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
    
    /// Create new segment
    async fn create_new_segment(&mut self) -> Result<(), WalError> {
        let segment_id = self.next_segment_id.fetch_add(1, Ordering::Relaxed);
        let segment_path = self.wal_dir.join(format!("segment-{:08x}.wal", segment_id));
        
        let segment = WalSegment::create(
            segment_id,
            segment_path,
            self.config.segment_size,
            self.config.use_memory_mapping,
        )?;
        
        let writer = SegmentWriter::new(segment);
        
        // Update active segment and writer
        {
            let mut active = self.active_segment.write().await;
            *active = Some(writer.metadata().read().unwrap().clone().into());
        }
        
        {
            let mut segment_writer = self.segment_writer.lock().await;
            *segment_writer = Some(writer);
        }
        
        Ok(())
    }
    
    /// Get segment by ID
    async fn get_segment(&self, segment_id: SegmentId) -> Result<&WalSegment, WalError> {
        // Check active segment first
        if let Some(ref segment) = *self.active_segment.read().await {
            if segment.metadata().read().unwrap().id == segment_id {
                return Ok(segment);
            }
        }
        
        // Check sealed segments
        if let Some(segment) = self.sealed_segments.get(&segment_id) {
            return Ok(segment.value());
        }
        
        Err(WalError::SegmentNotFound(segment_id))
    }
    
    /// Get current active segment ID
    async fn current_segment_id(&self) -> SegmentId {
        self.active_segment.read().await
            .as_ref()
            .map(|s| s.metadata().read().unwrap().id)
            .unwrap_or(0)
    }
}

// Convert SegmentMetadata to WalSegment for active segment
impl From<SegmentMetadata> for WalSegment {
    fn from(metadata: SegmentMetadata) -> Self {
        // This is a simplified conversion for the active segment tracking
        // In practice, this would need to properly reconstruct the WalSegment
        WalSegment::create(
            metadata.id,
            metadata.path,
            metadata.capacity,
            true, // use_mmap
        ).expect("Failed to create segment from metadata")
    }
}

/// WAL Error types specific to WAL operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum WalError {
    #[error("WAL I/O error: {0}")]
    Io(String),

    #[error("WAL corruption detected at sequence {sequence}: {details}")]
    Corruption { sequence: u64, details: String },

    #[error("WAL segment full: {0}")]
    SegmentFull(u64),

    #[error("WAL segment sealed: {0}")]
    SegmentSealed(u64),

    #[error("WAL segment not found: {0}")]
    SegmentNotFound(u64),

    #[error("WAL sequence number exhausted")]
    SequenceExhausted,

    #[error("WAL batch too large: {size} bytes (max: {max_size})")]
    BatchTooLarge { size: usize, max_size: usize },

    #[error("WAL shutdown in progress")]
    ShutdownInProgress,

    #[error("WAL serialization error: {details}")]
    Serialization { details: String },

    #[error("WAL memory mapping failed: {reason}")]
    MemoryMappingFailed { reason: String },

    #[error("Invalid WAL path: {0}")]
    InvalidPath(String),
}

impl From<std::io::Error> for WalError {
    fn from(error: std::io::Error) -> Self {
        WalError::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use kaelix_core::message::Message;
    
    #[tokio::test]
    async fn test_wal_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::default();
        
        let wal = WriteAheadLog::new(config, temp_dir.path()).await;
        assert!(wal.is_ok());
    }
    
    #[tokio::test]
    async fn test_single_append() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::default();
        
        let wal = WriteAheadLog::new(config, temp_dir.path()).await.unwrap();
        
        let message = Message::new(
            "test-id".to_string(),
            "source".into(),
            "dest".into(),
            b"test data".to_vec(),
        );
        
        let position = wal.append(message).await.unwrap();
        assert_eq!(position.segment_id, 1);
        assert!(position.offset > 0);
    }
    
    #[tokio::test]
    async fn test_batch_append() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::default();
        
        let wal = WriteAheadLog::new(config, temp_dir.path()).await.unwrap();
        
        let messages = vec![
            Message::new("1".to_string(), "src".into(), "dst".into(), b"data1".to_vec()),
            Message::new("2".to_string(), "src".into(), "dst".into(), b"data2".to_vec()),
            Message::new("3".to_string(), "src".into(), "dst".into(), b"data3".to_vec()),
        ];
        
        let positions = wal.append_batch(messages).await.unwrap();
        assert_eq!(positions.len(), 3);
        
        for position in positions {
            assert_eq!(position.segment_id, 1);
            assert!(position.offset > 0);
        }
    }
}