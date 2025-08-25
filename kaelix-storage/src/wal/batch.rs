use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{debug, info};

use crate::segments::StorageEntry;

/// Convert LogEntry to StorageEntry
fn log_entry_to_storage_entry(log_entry: &LogEntry) -> StorageEntry {
    StorageEntry::new(
        log_entry.sequence_number,
        log_entry.timestamp / 1000, // Convert nanoseconds to microseconds
        log_entry.payload.clone().into(), // Convert Vec<u8> to Bytes
    )
}

use super::{LogEntry, WalError, WalResult};

/// Batch processing configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum entries per batch
    pub max_batch_size: usize,

    /// Maximum time to wait for a batch to fill (milliseconds)
    pub max_wait_time_ms: u64,

    /// Maximum memory usage for batching (bytes)
    pub max_memory_usage: u64,

    /// Number of concurrent batch writers
    pub writer_threads: usize,

    /// Enable adaptive batching based on load
    pub adaptive_batching: bool,

    /// Minimum batch size for adaptive mode
    pub min_adaptive_batch_size: usize,

    /// Maximum concurrent batches in flight
    pub max_concurrent_batches: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            max_wait_time_ms: 10,           // 10ms
            max_memory_usage: 64 * 1024 * 1024, // 64MB
            writer_threads: 4,
            adaptive_batching: true,
            min_adaptive_batch_size: 10,
            max_concurrent_batches: 16,
        }
    }
}

impl BatchConfig {
    /// Create BatchConfig from WalConfig
    pub fn from_wal_config(wal_config: &super::WalConfig) -> Self {
        let mut config = Self::default();
        
        // Derive batch settings from WAL config
        config.max_batch_size = (wal_config.segment_size / 1024).min(10000) as usize; // Rough estimate
        config.writer_threads = num_cpus::get().min(8);
        
        config
    }

    /// Validate the batch configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.max_batch_size == 0 {
            return Err("max_batch_size must be greater than 0".to_string());
        }

        if self.max_wait_time_ms == 0 {
            return Err("max_wait_time_ms must be greater than 0".to_string());
        }

        if self.writer_threads == 0 {
            return Err("writer_threads must be greater than 0".to_string());
        }

        if self.max_concurrent_batches == 0 {
            return Err("max_concurrent_batches must be greater than 0".to_string());
        }

        if self.adaptive_batching && self.min_adaptive_batch_size > self.max_batch_size {
            return Err("min_adaptive_batch_size cannot be greater than max_batch_size".to_string());
        }

        Ok(())
    }
}

/// A batch of log entries ready for writing
#[derive(Debug)]
pub struct LogEntryBatch {
    /// The entries in this batch
    pub entries: Vec<LogEntry>,
    
    /// Total size of the batch in bytes
    pub size_bytes: usize,
    
    /// When this batch was created
    pub created_at: Instant,
    
    /// Sequence number of the first entry
    pub first_sequence: u64,
    
    /// Sequence number of the last entry
    pub last_sequence: u64,
}

impl LogEntryBatch {
    /// Create a new batch
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            size_bytes: 0,
            created_at: Instant::now(),
            first_sequence: 0,
            last_sequence: 0,
        }
    }

    /// Add an entry to the batch
    pub fn add_entry(&mut self, entry: LogEntry) -> WalResult<()> {
        let entry_size = entry.serialized_size();
        
        if self.entries.is_empty() {
            self.first_sequence = entry.sequence_number;
        }
        
        self.last_sequence = entry.sequence_number;
        self.size_bytes += entry_size;
        self.entries.push(entry);
        
        Ok(())
    }

    /// Check if batch is ready (full or timed out)
    pub fn is_ready(&self, config: &BatchConfig) -> bool {
        // Check size limit
        if self.entries.len() >= config.max_batch_size {
            return true;
        }

        // Check time limit
        if self.created_at.elapsed().as_millis() as u64 >= config.max_wait_time_ms {
            return true;
        }

        false
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get batch age
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Convert to storage entries
    pub fn to_storage_entries(&self) -> Vec<StorageEntry> {
        self.entries.iter().map(log_entry_to_storage_entry).collect()
    }
}

impl Default for LogEntryBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch processing statistics
#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    /// Total batches processed
    pub batches_processed: u64,
    
    /// Total entries processed
    pub entries_processed: u64,
    
    /// Average batch size
    pub avg_batch_size: f64,
    
    /// Average batch processing time (microseconds)
    pub avg_batch_time_us: u64,
    
    /// Peak batch processing time (microseconds)
    pub peak_batch_time_us: u64,
    
    /// Current pending entries
    pub pending_entries: usize,
    
    /// Current active batches
    pub active_batches: usize,
    
    /// Batches dropped due to errors
    pub batches_failed: u64,
    
    /// Total processing time (microseconds)
    pub total_processing_time_us: u64,
}

impl BatchStats {
    /// Update stats after processing a batch
    pub fn update_batch_processed(&mut self, batch: &LogEntryBatch, processing_time: Duration) {
        self.batches_processed += 1;
        self.entries_processed += batch.entries.len() as u64;
        
        let processing_time_us = processing_time.as_micros() as u64;
        self.total_processing_time_us += processing_time_us;
        
        if processing_time_us > self.peak_batch_time_us {
            self.peak_batch_time_us = processing_time_us;
        }
        
        // Update average batch size
        if self.batches_processed > 0 {
            self.avg_batch_size = self.entries_processed as f64 / self.batches_processed as f64;
            self.avg_batch_time_us = self.total_processing_time_us / self.batches_processed;
        }
    }
}

/// Coordinates batch processing for WAL writes
#[derive(Debug)]
pub struct BatchCoordinator {
    /// Configuration
    config: BatchConfig,
    
    /// Current batch being assembled
    current_batch: Arc<RwLock<LogEntryBatch>>,
    
    /// Channel for sending completed batches
    batch_sender: mpsc::UnboundedSender<LogEntryBatch>,
    
    /// Channel for receiving completed batches
    batch_receiver: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<LogEntryBatch>>>,
    
    /// Statistics
    stats: Arc<RwLock<BatchStats>>,
    
    /// Semaphore to limit concurrent batches
    batch_semaphore: Arc<Semaphore>,
    
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    
    /// Background task handles
    _handles: Vec<tokio::task::JoinHandle<()>>,
}

impl BatchCoordinator {
    /// Create a new batch coordinator
    pub async fn new(config: BatchConfig) -> WalResult<Self> {
        config.validate().map_err(|e| WalError::Configuration(e))?;
        
        let (batch_sender, batch_receiver) = mpsc::unbounded_channel();
        let batch_semaphore = Arc::new(Semaphore::new(config.max_concurrent_batches));
        
        let coordinator = Self {
            config,
            current_batch: Arc::new(RwLock::new(LogEntryBatch::new())),
            batch_sender,
            batch_receiver: Arc::new(tokio::sync::Mutex::new(batch_receiver)),
            stats: Arc::new(RwLock::new(BatchStats::default())),
            batch_semaphore,
            shutdown: Arc::new(AtomicBool::new(false)),
            _handles: Vec::new(),
        };
        
        Ok(coordinator)
    }

    /// Add an entry to the current batch
    pub async fn add_entry(&self, entry: LogEntry) -> WalResult<()> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err(WalError::Shutdown);
        }

        let mut batch = self.current_batch.write().await;
        batch.add_entry(entry)?;
        
        // Check if batch is ready
        if batch.is_ready(&self.config) {
            let completed_batch = std::mem::replace(&mut *batch, LogEntryBatch::new());
            drop(batch); // Release lock before sending
            
            self.send_batch(completed_batch).await?;
        }
        
        Ok(())
    }

    /// Send a completed batch for processing
    async fn send_batch(&self, batch: LogEntryBatch) -> WalResult<()> {
        if batch.is_empty() {
            return Ok(());
        }
        
        debug!("Sending batch with {} entries", batch.entries.len());
        
        self.batch_sender.send(batch)
            .map_err(|_| WalError::Batch("Failed to send batch".to_string()))?;
        
        Ok(())
    }

    /// Get the next batch for processing
    pub async fn next_batch(&self) -> Option<LogEntryBatch> {
        let mut receiver = self.batch_receiver.lock().await;
        receiver.recv().await
    }

    /// Force flush current batch
    pub async fn flush(&self) -> WalResult<()> {
        let mut batch = self.current_batch.write().await;
        if !batch.is_empty() {
            let completed_batch = std::mem::replace(&mut *batch, LogEntryBatch::new());
            drop(batch);
            self.send_batch(completed_batch).await?;
        }
        Ok(())
    }

    /// Get batch processing statistics
    pub async fn stats(&self) -> BatchStats {
        let stats = self.stats.read().await;
        let current_batch = self.current_batch.read().await;
        
        let mut result = stats.clone();
        result.pending_entries = current_batch.entries.len();
        result.active_batches = 1; // Simplified for now
        
        result
    }

    /// Update statistics after batch processing
    pub async fn update_stats(&self, batch: &LogEntryBatch, processing_time: Duration) {
        let mut stats = self.stats.write().await;
        stats.update_batch_processed(batch, processing_time);
    }

    /// Shutdown the batch coordinator
    pub async fn shutdown(&self) -> WalResult<()> {
        info!("Shutting down batch coordinator");
        self.shutdown.store(true, Ordering::Relaxed);
        
        // Flush any remaining batch
        self.flush().await?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_config_validation() {
        let config = BatchConfig::default();
        assert!(config.validate().is_ok());

        let mut invalid_config = config.clone();
        invalid_config.max_batch_size = 0;
        assert!(invalid_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_batch_coordinator_creation() {
        let config = BatchConfig::default();
        let coordinator = BatchCoordinator::new(config).await.unwrap();
        
        let stats = coordinator.stats().await;
        assert_eq!(stats.batches_processed, 0);
    }

    #[test]
    fn test_batch_operations() {
        let mut batch = LogEntryBatch::new();
        assert!(batch.is_empty());
        
        // Create a test entry
        let entry = LogEntry::new(
            1,
            12345,
            kaelix_cluster::messages::ClusterMessage::new(
                kaelix_cluster::NodeId::generate(),
                kaelix_cluster::NodeId::generate(),
                kaelix_cluster::messages::MessagePayload::Ping { 
                    node_id: kaelix_cluster::NodeId::generate() 
                }
            )
        ).unwrap();
        
        batch.add_entry(entry).unwrap();
        assert!(!batch.is_empty());
        assert_eq!(batch.entries.len(), 1);
    }
}