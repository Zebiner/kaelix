use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{debug, info};

use crate::wal::{LogEntry, WalError};

use serde::{Deserialize, Serialize};

/// Configuration for batch operations
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum entries per batch
    pub max_batch_size: usize,
    /// Maximum time to wait for batch completion
    pub max_batch_duration: Duration,
    /// Maximum memory usage for batching
    pub max_memory_usage: usize,
    /// Enable adaptive batching
    pub adaptive_batching: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            max_batch_duration: Duration::from_millis(10),
            max_memory_usage: 16 * 1024 * 1024, // 16MB
            adaptive_batching: true,
        }
    }
}

/// Result of a batch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    /// Number of entries in the batch
    pub batch_size: usize,
    /// Time taken to create the batch
    pub batch_duration: Duration,
    /// Memory used by the batch
    pub memory_used: usize,
    /// Starting sequence number for the batch
    pub start_sequence: u64,
    /// Ending sequence number for the batch
    pub end_sequence: u64,
}

/// Batch coordinator for Write-Ahead Log operations
///
/// Manages batching of WAL entries to improve write throughput by reducing
/// the number of individual I/O operations. Uses adaptive algorithms to
/// balance latency and throughput based on system conditions.
pub struct BatchCoordinator {
    /// Configuration
    config: BatchConfig,
    
    /// Current batch being accumulated
    current_batch: Arc<RwLock<Vec<LogEntry>>>,
    
    /// Batch completion notifier
    batch_ready_tx: mpsc::UnboundedSender<Vec<LogEntry>>,
    
    /// Batch completion receiver
    batch_ready_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<LogEntry>>>>,
    
    /// Shutdown flag
    is_shutdown: Arc<AtomicBool>,
    
    /// Backpressure semaphore
    backpressure_semaphore: Arc<Semaphore>,
    
    /// Performance metrics
    metrics: Arc<RwLock<BatchMetrics>>,
}

/// Performance metrics for batch operations
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BatchMetrics {
    /// Total batches processed
    pub total_batches: u64,
    /// Total entries processed
    pub total_entries: u64,
    /// Average batch size
    pub avg_batch_size: f64,
    /// Average batch duration in microseconds
    pub avg_batch_duration_us: u64,
    /// Maximum batch size seen
    pub max_batch_size: usize,
    /// Minimum batch size seen
    pub min_batch_size: usize,
    /// Total memory used by batches
    pub total_memory_used: u64,
    /// Number of batches that triggered backpressure
    pub backpressure_events: u64,
    /// Last batch timestamp
    pub last_batch_timestamp: Option<std::time::SystemTime>,
}

impl BatchCoordinator {
    /// Create a new batch coordinator
    pub fn new(config: BatchConfig) -> Result<Self, WalError> {
        let (batch_ready_tx, batch_ready_rx) = mpsc::unbounded_channel();
        
        // Create semaphore for backpressure with permits equal to max memory / avg entry size
        let max_permits = (config.max_memory_usage / 1024).max(100); // At least 100 permits
        
        Ok(Self {
            config,
            current_batch: Arc::new(RwLock::new(Vec::new())),
            batch_ready_tx,
            batch_ready_rx: Arc::new(tokio::sync::Mutex::new(batch_ready_rx)),
            is_shutdown: Arc::new(AtomicBool::new(false)),
            backpressure_semaphore: Arc::new(Semaphore::new(max_permits)),
            metrics: Arc::new(RwLock::new(BatchMetrics::default())),
        })
    }
    
    /// Add an entry to the current batch
    pub async fn add_entry(&self, entry: LogEntry) -> Result<(), WalError> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Err(WalError::Shutdown);
        }
        
        // Acquire permit for backpressure control
        let _permit = self.backpressure_semaphore
            .acquire()
            .await
            .map_err(|_| WalError::BackpressureTimeout)?;
        
        let should_flush = {
            let mut batch = self.current_batch.write().await;
            batch.push(entry);
            
            // Check if we should flush based on size
            batch.len() >= self.config.max_batch_size
        };
        
        if should_flush {
            self.flush_batch().await?;
        }
        
        Ok(())
    }
    
    /// Flush the current batch
    pub async fn flush_batch(&self) -> Result<Option<BatchResult>, WalError> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Ok(None);
        }
        
        let start_time = Instant::now();
        
        let batch = {
            let mut current_batch = self.current_batch.write().await;
            if current_batch.is_empty() {
                return Ok(None);
            }
            
            std::mem::take(&mut *current_batch)
        };
        
        if batch.is_empty() {
            return Ok(None);
        }
        
        let batch_size = batch.len();
        let memory_used = batch.iter().map(|e| e.serialized_size()).sum::<usize>();
        
        // Extract sequence numbers for result
        let start_sequence = batch.first().map(|e| e.sequence_number).unwrap_or(0);
        let end_sequence = batch.last().map(|e| e.sequence_number).unwrap_or(0);
        
        // Send batch for processing
        self.batch_ready_tx
            .send(batch)
            .map_err(|_| WalError::ChannelClosed)?;
        
        let batch_duration = start_time.elapsed();
        
        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_batches += 1;
            metrics.total_entries += batch_size as u64;
            
            // Update averages
            let total_batches_f = metrics.total_batches as f64;
            metrics.avg_batch_size = (metrics.avg_batch_size * (total_batches_f - 1.0) + batch_size as f64) / total_batches_f;
            
            let batch_duration_us = batch_duration.as_micros() as u64;
            metrics.avg_batch_duration_us = (metrics.avg_batch_duration_us * (total_batches_f - 1.0) as u64 + batch_duration_us) / total_batches_f as u64;
            
            // Update min/max
            if batch_size > metrics.max_batch_size {
                metrics.max_batch_size = batch_size;
            }
            if batch_size < metrics.min_batch_size || metrics.min_batch_size == 0 {
                metrics.min_batch_size = batch_size;
            }
            
            metrics.total_memory_used += memory_used as u64;
            metrics.last_batch_timestamp = Some(std::time::SystemTime::now());
        }
        
        debug!(
            "Flushed batch: {} entries, {} bytes, {:?}",
            batch_size, memory_used, batch_duration
        );
        
        Ok(Some(BatchResult {
            batch_size,
            batch_duration,
            memory_used,
            start_sequence,
            end_sequence,
        }))
    }
    
    /// Get the next ready batch
    pub async fn next_batch(&self) -> Result<Option<Vec<LogEntry>>, WalError> {
        if self.is_shutdown.load(Ordering::Relaxed) {
            return Ok(None);
        }
        
        let mut rx = self.batch_ready_rx.lock().await;
        match rx.recv().await {
            Some(batch) => Ok(Some(batch)),
            None => Ok(None),
        }
    }
    
    /// Start periodic batch flushing based on time
    pub async fn start_periodic_flush(&self) {
        let current_batch = Arc::clone(&self.current_batch);
        let batch_ready_tx = self.batch_ready_tx.clone();
        let is_shutdown = Arc::clone(&self.is_shutdown);
        let config = self.config.clone();
        let metrics = Arc::clone(&self.metrics);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.max_batch_duration);
            
            while !is_shutdown.load(Ordering::Relaxed) {
                interval.tick().await;
                
                let batch = {
                    let mut current_batch = current_batch.write().await;
                    if current_batch.is_empty() {
                        continue;
                    }
                    std::mem::take(&mut *current_batch)
                };
                
                if !batch.is_empty() {
                    let batch_size = batch.len();
                    info!("Time-triggered batch flush: {} entries", batch_size);
                    
                    if batch_ready_tx.send(batch).is_err() {
                        break; // Channel closed
                    }
                    
                    // Update metrics
                    {
                        let mut metrics = metrics.write().await;
                        metrics.total_batches += 1;
                        metrics.total_entries += batch_size as u64;
                    }
                }
            }
        });
    }
    
    /// Get current batch metrics
    pub async fn metrics(&self) -> BatchMetrics {
        self.metrics.read().await.clone()
    }
    
    /// Check if coordinator has pending entries
    pub async fn has_pending_entries(&self) -> bool {
        let batch = self.current_batch.read().await;
        !batch.is_empty()
    }
    
    /// Get current batch size
    pub async fn current_batch_size(&self) -> usize {
        let batch = self.current_batch.read().await;
        batch.len()
    }
    
    /// Shutdown the batch coordinator
    pub async fn shutdown(&self) -> Result<(), WalError> {
        info!("Shutting down batch coordinator");
        
        self.is_shutdown.store(true, Ordering::Relaxed);
        
        // Flush any remaining entries
        if let Some(result) = self.flush_batch().await? {
            info!("Final batch flush: {} entries", result.batch_size);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaelix_cluster::messages::ClusterMessage;

    fn create_test_entry(sequence: u64) -> LogEntry {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        LogEntry::new(sequence, timestamp, ClusterMessage::default()).unwrap()
    }

    #[tokio::test]
    async fn test_batch_coordinator_creation() {
        let config = BatchConfig::default();
        let coordinator = BatchCoordinator::new(config).unwrap();
        
        assert_eq!(coordinator.current_batch_size().await, 0);
        assert!(!coordinator.has_pending_entries().await);
    }

    #[tokio::test]
    async fn test_batch_entry_addition() {
        let config = BatchConfig::default();
        let coordinator = BatchCoordinator::new(config).unwrap();
        
        let entry = create_test_entry(1);
        coordinator.add_entry(entry).await.unwrap();
        
        assert_eq!(coordinator.current_batch_size().await, 1);
        assert!(coordinator.has_pending_entries().await);
    }

    #[tokio::test]
    async fn test_batch_flushing() {
        let mut config = BatchConfig::default();
        config.max_batch_size = 2;
        
        let coordinator = BatchCoordinator::new(config).unwrap();
        
        // Add first entry
        let entry1 = create_test_entry(1);
        coordinator.add_entry(entry1).await.unwrap();
        assert_eq!(coordinator.current_batch_size().await, 1);
        
        // Add second entry - should trigger flush
        let entry2 = create_test_entry(2);
        coordinator.add_entry(entry2).await.unwrap();
        
        // After flush, batch should be empty
        assert_eq!(coordinator.current_batch_size().await, 0);
        
        // Should have a batch ready for processing
        let batch = coordinator.next_batch().await.unwrap();
        assert!(batch.is_some());
        assert_eq!(batch.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_manual_flush() {
        let config = BatchConfig::default();
        let coordinator = BatchCoordinator::new(config).unwrap();
        
        let entry = create_test_entry(1);
        coordinator.add_entry(entry).await.unwrap();
        
        let result = coordinator.flush_batch().await.unwrap();
        assert!(result.is_some());
        
        let result = result.unwrap();
        assert_eq!(result.batch_size, 1);
        assert_eq!(result.start_sequence, 1);
        assert_eq!(result.end_sequence, 1);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let mut config = BatchConfig::default();
        config.max_batch_size = 1;
        
        let coordinator = BatchCoordinator::new(config).unwrap();
        
        // Process a few entries
        for i in 1..=3 {
            let entry = create_test_entry(i);
            coordinator.add_entry(entry).await.unwrap();
        }
        
        let metrics = coordinator.metrics().await;
        assert_eq!(metrics.total_batches, 3);
        assert_eq!(metrics.total_entries, 3);
        assert_eq!(metrics.avg_batch_size, 1.0);
    }

    #[tokio::test]
    async fn test_coordinator_shutdown() {
        let config = BatchConfig::default();
        let coordinator = BatchCoordinator::new(config).unwrap();
        
        let entry = create_test_entry(1);
        coordinator.add_entry(entry).await.unwrap();
        
        coordinator.shutdown().await.unwrap();
        
        // Should not be able to add entries after shutdown
        let entry2 = create_test_entry(2);
        let result = coordinator.add_entry(entry2).await;
        assert!(result.is_err());
    }
}