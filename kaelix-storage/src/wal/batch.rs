use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time;
use crossbeam::queue::SegQueue;

use super::{LogEntry, WalError};
use crate::wal::segment::SegmentWriter;

/// Request to write entries to WAL
#[derive(Debug)]
pub(crate) struct BatchWriteRequest {
    pub entries: Vec<LogEntry>,
    pub response_tx: tokio::sync::oneshot::Sender<Result<Vec<u64>, WalError>>,
}

/// Batch coordinator for efficient batch writing
#[derive(Debug)]
pub(crate) struct BatchCoordinator {
    /// Queue for requests waiting to be batched
    request_queue: Arc<SegQueue<BatchWriteRequest>>,
    
    /// Queue for individual entries (for fire-and-forget writes)
    pending_queue: Arc<SegQueue<LogEntry>>,
    
    /// Notification for new requests
    notify: Arc<Notify>,
    
    /// Configuration
    max_batch_size: usize,
    batch_timeout: Duration,
    
    /// Metrics
    metrics: Arc<BatchMetrics>,
}

impl BatchCoordinator {
    /// Create new batch coordinator
    pub fn new(max_batch_size: usize, batch_timeout: Duration) -> Self {
        Self {
            request_queue: Arc::new(SegQueue::new()),
            pending_queue: Arc::new(SegQueue::new()),
            notify: Arc::new(Notify::new()),
            max_batch_size,
            batch_timeout,
            metrics: Arc::new(BatchMetrics::new()),
        }
    }
    
    /// Add entry to pending queue (fire-and-forget)
    pub fn add_entry(&self, entry: LogEntry) {
        self.pending_queue.push(entry);
        self.notify.notify_one();
    }
    
    /// Add batch write request
    pub fn add_request(&self, request: BatchWriteRequest) {
        self.request_queue.push(request);
        self.notify.notify_one();
    }
    
    /// Start batch processing loop
    pub async fn start_processing(
        &self,
        segment_writer: Arc<Mutex<SegmentWriter>>,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        let mut interval = time::interval(self.batch_timeout);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.process_batches(&segment_writer).await;
                    Self::flush_pending_queue(
                        &self.pending_queue,
                        &segment_writer,
                        &self.metrics,
                        self.max_batch_size,
                    ).await;
                }
                _ = self.notify.notified() => {
                    self.process_batches(&segment_writer).await;
                }
                _ = shutdown_rx.recv() => {
                    // Final flush on shutdown
                    self.process_batches(&segment_writer).await;
                    Self::flush_pending_queue(
                        &self.pending_queue,
                        &segment_writer,
                        &self.metrics,
                        self.max_batch_size,
                    ).await;
                    break;
                }
            }
        }
    }
    
    /// Process accumulated batches
    async fn process_batches(&self, segment_writer: &Arc<Mutex<SegmentWriter>>) {
        // Drain requests from queue
        let mut requests = VecDeque::new();
        let mut total_entries = 0;
        
        while let Some(request) = self.request_queue.pop() {
            total_entries += request.entries.len();
            requests.push_back(request);
            
            if total_entries >= self.max_batch_size {
                break;
            }
        }
        
        if requests.is_empty() {
            return;
        }
        
        self.process_request_batch(requests, segment_writer).await;
    }
    
    /// Process a batch of write requests
    async fn process_request_batch(
        &self,
        mut requests: VecDeque<BatchWriteRequest>,
        segment_writer: &Arc<Mutex<SegmentWriter>>,
    ) {
        let start_time = Instant::now();
        let metrics = Arc::clone(&self.metrics);
        
        // Collect all entries from requests
        let mut all_entries = Vec::new();
        let mut response_channels = Vec::new();
        let mut entry_counts = Vec::new();
        
        for request in requests.drain(..) {
            entry_counts.push(request.entries.len());
            all_entries.extend(request.entries);
            response_channels.push(request.response_tx);
        }
        
        // Write batch to segment
        let result = {
            let mut writer = segment_writer.lock().await;
            writer.write_batch(&all_entries).await
        };
        
        let duration = start_time.elapsed();
        
        match result {
            Ok(positions) => {
                // Distribute positions back to requesters
                let mut pos_offset = 0;
                for (i, count) in entry_counts.iter().enumerate() {
                    let end_offset = pos_offset + count;
                    let request_positions = positions[pos_offset..end_offset].to_vec();
                    pos_offset = end_offset;
                    
                    // Take ownership of the sender to avoid borrow issues
                    if i < response_channels.len() {
                        if let Some(response_tx) = response_channels.get(i) {
                            // We need to take the sender out, but we can't move from a shared reference
                            // So we'll iterate differently to take ownership
                        }
                    }
                }
                
                // Instead, iterate by consuming the vector
                let mut pos_offset = 0;
                for (response_tx, count) in response_channels.into_iter().zip(entry_counts.iter()) {
                    let end_offset = pos_offset + count;
                    let request_positions = positions[pos_offset..end_offset].to_vec();
                    pos_offset = end_offset;
                    
                    let _ = response_tx.send(Ok(request_positions));
                }
                
                metrics.record_batch(all_entries.len(), duration);
            }
            Err(e) => {
                // Send error to all requesters
                for response_tx in response_channels {
                    let _ = response_tx.send(Err(e.clone()));
                }
                
                tracing::error!("Batch write failed: {}", e);
            }
        }
    }
    
    /// Flush entries from the pending queue
    async fn flush_pending_queue(
        pending_queue: &SegQueue<LogEntry>,
        segment_writer: &Arc<Mutex<SegmentWriter>>,
        metrics: &Arc<BatchMetrics>,
        max_batch_size: usize,
    ) {
        if pending_queue.is_empty() {
            return;
        }
        
        let start_time = Instant::now();
        
        // Drain entries from queue
        let mut entries = Vec::with_capacity(max_batch_size.min(1024));
        while let Some(entry) = pending_queue.pop() {
            entries.push(entry);
            if entries.len() >= max_batch_size {
                break;
            }
        }
        
        if entries.is_empty() {
            return;
        }
        
        // Write batch to segment
        let result = {
            let mut writer = segment_writer.lock().await;
            writer.write_batch(&entries).await
        };
        
        let duration = start_time.elapsed();
        
        match result {
            Ok(_positions) => {
                metrics.record_batch(entries.len(), duration);
                tracing::trace!("Flushed {} pending entries", entries.len());
            }
            Err(e) => {
                tracing::error!("Failed to flush pending entries: {}", e);
                // Re-queue entries for retry (could be improved with exponential backoff)
                for entry in entries {
                    pending_queue.push(entry);
                }
            }
        }
    }
    
    /// Get batch metrics
    pub fn metrics(&self) -> Arc<BatchMetrics> {
        Arc::clone(&self.metrics)
    }
    
    /// Check if coordinator has pending work
    pub fn has_pending_work(&self) -> bool {
        !self.request_queue.is_empty() || !self.pending_queue.is_empty()
    }
}

/// Metrics for batch operations
#[derive(Debug)]
pub(crate) struct BatchMetrics {
    /// Total number of batches processed
    pub batches_processed: std::sync::atomic::AtomicU64,
    
    /// Total number of entries processed
    pub entries_processed: std::sync::atomic::AtomicU64,
    
    /// Total processing time
    pub total_processing_time: std::sync::Mutex<Duration>,
    
    /// Average batch size
    pub average_batch_size: std::sync::atomic::AtomicU64,
    
    /// Last batch timestamp
    pub last_batch_time: std::sync::Mutex<Option<Instant>>,
}

impl BatchMetrics {
    /// Create new batch metrics
    pub fn new() -> Self {
        Self {
            batches_processed: std::sync::atomic::AtomicU64::new(0),
            entries_processed: std::sync::atomic::AtomicU64::new(0),
            total_processing_time: std::sync::Mutex::new(Duration::ZERO),
            average_batch_size: std::sync::atomic::AtomicU64::new(0),
            last_batch_time: std::sync::Mutex::new(None),
        }
    }
    
    /// Record batch processing
    pub fn record_batch(&self, entry_count: usize, duration: Duration) {
        use std::sync::atomic::Ordering;
        
        let old_batches = self.batches_processed.fetch_add(1, Ordering::Relaxed);
        let old_entries = self.entries_processed.fetch_add(entry_count as u64, Ordering::Relaxed);
        
        // Update average batch size
        let new_total_entries = old_entries + entry_count as u64;
        let new_total_batches = old_batches + 1;
        self.average_batch_size.store(
            new_total_entries / new_total_batches.max(1),
            Ordering::Relaxed,
        );
        
        // Update timing
        if let Ok(mut total_time) = self.total_processing_time.lock() {
            *total_time += duration;
        }
        
        if let Ok(mut last_time) = self.last_batch_time.lock() {
            *last_time = Some(Instant::now());
        }
    }
    
    /// Get current statistics
    pub fn stats(&self) -> BatchStats {
        use std::sync::atomic::Ordering;
        
        let batches = self.batches_processed.load(Ordering::Relaxed);
        let entries = self.entries_processed.load(Ordering::Relaxed);
        let avg_size = self.average_batch_size.load(Ordering::Relaxed);
        
        let total_time = self.total_processing_time.lock()
            .map(|t| *t)
            .unwrap_or(Duration::ZERO);
        
        let last_batch = self.last_batch_time.lock()
            .map(|t| *t)
            .unwrap_or(None);
        
        BatchStats {
            batches_processed: batches,
            entries_processed: entries,
            average_batch_size: avg_size,
            total_processing_time: total_time,
            last_batch_time: last_batch,
            average_processing_time: if batches > 0 {
                total_time / batches as u32
            } else {
                Duration::ZERO
            },
        }
    }
}

impl Default for BatchMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of batch statistics
#[derive(Debug, Clone)]
pub struct BatchStats {
    /// Total number of batches processed
    pub batches_processed: u64,
    
    /// Total number of entries processed
    pub entries_processed: u64,
    
    /// Average batch size
    pub average_batch_size: u64,
    
    /// Total processing time across all batches
    pub total_processing_time: Duration,
    
    /// Average processing time per batch
    pub average_processing_time: Duration,
    
    /// Timestamp of last batch
    pub last_batch_time: Option<Instant>,
}

impl BatchStats {
    /// Calculate throughput in entries per second
    pub fn throughput(&self) -> f64 {
        if self.total_processing_time.is_zero() {
            0.0
        } else {
            self.entries_processed as f64 / self.total_processing_time.as_secs_f64()
        }
    }
    
    /// Calculate batch frequency in batches per second
    pub fn batch_frequency(&self) -> f64 {
        if self.total_processing_time.is_zero() {
            0.0
        } else {
            self.batches_processed as f64 / self.total_processing_time.as_secs_f64()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    
    #[test]
    fn test_batch_metrics() {
        let metrics = BatchMetrics::new();
        
        // Record some batches
        metrics.record_batch(10, Duration::from_millis(5));
        metrics.record_batch(20, Duration::from_millis(8));
        metrics.record_batch(15, Duration::from_millis(6));
        
        let stats = metrics.stats();
        assert_eq!(stats.batches_processed, 3);
        assert_eq!(stats.entries_processed, 45);
        assert_eq!(stats.average_batch_size, 15); // 45 / 3
        assert!(stats.total_processing_time >= Duration::from_millis(19));
    }
    
    #[test]
    fn test_batch_stats_throughput() {
        let stats = BatchStats {
            batches_processed: 100,
            entries_processed: 1000,
            average_batch_size: 10,
            total_processing_time: Duration::from_secs(1),
            average_processing_time: Duration::from_millis(10),
            last_batch_time: None,
        };
        
        assert_eq!(stats.throughput(), 1000.0); // 1000 entries / 1 second
        assert_eq!(stats.batch_frequency(), 100.0); // 100 batches / 1 second
    }
    
    #[tokio::test]
    async fn test_batch_coordinator_creation() {
        let coordinator = BatchCoordinator::new(100, Duration::from_millis(10));
        
        assert!(!coordinator.has_pending_work());
        assert_eq!(coordinator.max_batch_size, 100);
        assert_eq!(coordinator.batch_timeout, Duration::from_millis(10));
    }
}