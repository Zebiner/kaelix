//! In-memory storage backend implementation
//!
//! Provides ultra-high-performance in-memory storage for development,
//! testing, and high-performance caching scenarios.

use crate::{
    error::{StorageError, StorageResult},
    traits::{StorageEngine, StorageReader},
    types::{
        BatchWriteResult, FlushResult, LogOffset, MessageIterator, ReadPosition, StorageMessage,
        StorageMetrics, WriteResult,
    },
};
use async_trait::async_trait;
use kaelix_cluster::HybridLogicalClock;
use parking_lot::RwLock;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// In-memory storage engine configuration
#[derive(Debug, Clone)]
pub struct MemoryStorageConfig {
    /// Maximum number of messages to store in memory
    pub max_messages: usize,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: usize,
    /// Enable compression for stored messages
    pub enable_compression: bool,
    /// Batch size for batch operations
    pub batch_size: usize,
    /// Maximum concurrent readers
    pub max_concurrent_readers: usize,
    /// Enable performance metrics collection
    pub enable_metrics: bool,
    /// Memory cleanup threshold (messages to clean when limit reached)
    pub cleanup_threshold: usize,
}

impl Default for MemoryStorageConfig {
    fn default() -> Self {
        Self {
            max_messages: 1_000_000,      // 1M messages
            max_memory_bytes: 1024 * 1024 * 1024, // 1GB
            enable_compression: false,     // Disabled for ultra-low latency
            batch_size: 1000,            // Reasonable batch size
            max_concurrent_readers: 100,  // Support many concurrent readers
            enable_metrics: true,         // Enable by default
            cleanup_threshold: 10_000,    // Clean 10K old messages when limit reached
        }
    }
}

impl MemoryStorageConfig {
    /// Create a new memory storage configuration
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum number of messages
    #[must_use]
    pub fn with_max_messages(mut self, max_messages: usize) -> Self {
        self.max_messages = max_messages;
        self
    }

    /// Set maximum memory usage in bytes
    #[must_use]
    pub fn with_max_memory_bytes(mut self, max_memory_bytes: usize) -> Self {
        self.max_memory_bytes = max_memory_bytes;
        self
    }

    /// Enable or disable compression
    #[must_use]
    pub fn with_compression(mut self, enable_compression: bool) -> Self {
        self.enable_compression = enable_compression;
        self
    }

    /// Set batch size for operations
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Set maximum concurrent readers
    #[must_use]
    pub fn with_max_concurrent_readers(mut self, max_concurrent_readers: usize) -> Self {
        self.max_concurrent_readers = max_concurrent_readers;
        self
    }

    /// Enable or disable metrics collection
    #[must_use]
    pub fn with_metrics(mut self, enable_metrics: bool) -> Self {
        self.enable_metrics = enable_metrics;
        self
    }

    /// Set cleanup threshold
    #[must_use]
    pub fn with_cleanup_threshold(mut self, cleanup_threshold: usize) -> Self {
        self.cleanup_threshold = cleanup_threshold;
        self
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns `StorageError::ConfigurationError` if configuration is invalid
    pub fn validate(&self) -> StorageResult<()> {
        if self.max_messages == 0 {
            return Err(StorageError::ConfigurationError {
                parameter: "max_messages".to_string(),
                value: self.max_messages.to_string(),
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.max_memory_bytes == 0 {
            return Err(StorageError::ConfigurationError {
                parameter: "max_memory_bytes".to_string(),
                value: self.max_memory_bytes.to_string(),
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.batch_size == 0 {
            return Err(StorageError::ConfigurationError {
                parameter: "batch_size".to_string(),
                value: self.batch_size.to_string(),
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.cleanup_threshold == 0 {
            return Err(StorageError::ConfigurationError {
                parameter: "cleanup_threshold".to_string(),
                value: self.cleanup_threshold.to_string(),
                reason: "must be greater than 0".to_string(),
            });
        }

        if self.cleanup_threshold >= self.max_messages {
            return Err(StorageError::ConfigurationError {
                parameter: "cleanup_threshold".to_string(),
                value: self.cleanup_threshold.to_string(),
                reason: "must be less than max_messages".to_string(),
            });
        }

        Ok(())
    }
}

/// Advanced performance metrics for memory storage operations
#[derive(Debug, Default)]
pub struct MemoryStorageMetrics {
    /// Total number of write operations
    pub total_writes: AtomicU64,
    /// Total number of read operations  
    pub total_reads: AtomicU64,
    /// Total number of messages stored
    pub total_messages: AtomicUsize,
    /// Total memory usage in bytes
    pub total_memory_bytes: AtomicUsize,
    /// Number of currently active readers
    pub active_readers: AtomicUsize,
    /// Total number of batch operations
    pub total_batch_operations: AtomicU64,
    /// Total number of flush operations
    pub total_flush_operations: AtomicU64,
    /// Total number of cleanup operations
    pub total_cleanup_operations: AtomicU64,
    /// Average write latency in nanoseconds
    pub write_latency_sum: AtomicU64,
    /// Average read latency in nanoseconds
    pub read_latency_sum: AtomicU64,
    /// Peak memory usage in bytes
    pub peak_memory_bytes: AtomicUsize,
    /// Number of messages cleaned up
    pub messages_cleaned: AtomicUsize,
}

impl MemoryStorageMetrics {
    /// Create a new metrics instance
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a write operation with latency
    pub fn record_write(&self, latency_ns: u64) {
        self.total_writes.fetch_add(1, Ordering::Relaxed);
        self.write_latency_sum.fetch_add(latency_ns, Ordering::Relaxed);
    }

    /// Record a read operation with latency
    pub fn record_read(&self, latency_ns: u64) {
        self.total_reads.fetch_add(1, Ordering::Relaxed);
        self.read_latency_sum.fetch_add(latency_ns, Ordering::Relaxed);
    }

    /// Record a batch operation
    pub fn record_batch_operation(&self) {
        self.total_batch_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a flush operation
    pub fn record_flush(&self) {
        self.total_flush_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cleanup operation
    pub fn record_cleanup(&self, messages_cleaned: usize) {
        self.total_cleanup_operations.fetch_add(1, Ordering::Relaxed);
        self.messages_cleaned.fetch_add(messages_cleaned, Ordering::Relaxed);
    }

    /// Update memory usage
    pub fn update_memory_usage(&self, bytes: usize) {
        self.total_memory_bytes.store(bytes, Ordering::Relaxed);
        let peak = self.peak_memory_bytes.load(Ordering::Relaxed);
        if bytes > peak {
            self.peak_memory_bytes.store(bytes, Ordering::Relaxed);
        }
    }

    /// Update message count
    pub fn update_message_count(&self, count: usize) {
        self.total_messages.store(count, Ordering::Relaxed);
    }

    /// Add an active reader
    pub fn add_reader(&self) {
        self.active_readers.fetch_add(1, Ordering::Relaxed);
    }

    /// Remove an active reader
    pub fn remove_reader(&self) {
        self.active_readers.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get average write latency in nanoseconds
    #[must_use]
    pub fn average_write_latency(&self) -> u64 {
        let total_writes = self.total_writes.load(Ordering::Relaxed);
        if total_writes == 0 {
            return 0;
        }
        self.write_latency_sum.load(Ordering::Relaxed) / total_writes
    }

    /// Get average read latency in nanoseconds
    #[must_use]
    pub fn average_read_latency(&self) -> u64 {
        let total_reads = self.total_reads.load(Ordering::Relaxed);
        if total_reads == 0 {
            return 0;
        }
        self.read_latency_sum.load(Ordering::Relaxed) / total_reads
    }
}

/// Ultra-high-performance in-memory storage engine
pub struct MemoryStorageEngine {
    /// Configuration for the storage engine
    pub config: MemoryStorageConfig,
    /// Thread-safe message storage using BTreeMap for efficient range queries
    messages: Arc<RwLock<BTreeMap<u64, StorageMessage>>>,
    /// Atomic counter for generating unique message offsets
    next_offset: AtomicU64,
    /// Performance metrics collection
    metrics: Arc<MemoryStorageMetrics>,
    /// Estimated memory usage in bytes
    memory_usage: AtomicUsize,
    /// Hybrid logical clock for timestamp ordering
    hlc: HybridLogicalClock,
}

impl MemoryStorageEngine {
    /// Create a new memory storage engine with default configuration
    ///
    /// # Errors
    ///
    /// Returns `StorageError::InitializationFailed` if configuration is invalid
    pub async fn new() -> StorageResult<Self> {
        Self::with_config(MemoryStorageConfig::default()).await
    }

    /// Create a new memory storage engine with custom configuration
    ///
    /// # Errors
    ///
    /// Returns `StorageError::InitializationFailed` if configuration is invalid
    pub async fn with_config(config: MemoryStorageConfig) -> StorageResult<Self> {
        config.validate().map_err(|e| StorageError::InitializationFailed {
            reason: format!("Invalid configuration: {}", e),
        })?;

        Ok(Self {
            config,
            messages: Arc::new(RwLock::new(BTreeMap::new())),
            next_offset: AtomicU64::new(1),
            metrics: Arc::new(MemoryStorageMetrics::new()),
            memory_usage: AtomicUsize::new(0),
            hlc: HybridLogicalClock::default(),
        })
    }

    /// Get current message count
    #[must_use]
    pub fn message_count(&self) -> usize {
        self.messages.read().len()
    }

    /// Get current memory usage in bytes
    #[must_use]
    pub fn memory_usage(&self) -> usize {
        self.memory_usage.load(Ordering::Relaxed)
    }

    /// Estimate the memory usage of a storage message
    fn estimate_message_size(&self, _message: &StorageMessage) -> usize {
        // Base size estimation: includes message metadata and payload
        // Since MessagePayload doesn't have estimate_size method, use fixed estimate
        let base_size = std::mem::size_of::<StorageMessage>();
        let payload_size = 256; // Reasonable estimate for message payload
        base_size + payload_size + 64 // Include BTreeMap overhead
    }

    /// Clean up old messages when capacity is exceeded
    async fn cleanup_if_needed(&self) -> StorageResult<()> {
        let message_count = self.message_count();
        if message_count <= self.config.max_messages {
            return Ok(());
        }

        let mut messages = self.messages.write();
        let target_count = self.config.max_messages - self.config.cleanup_threshold;
        let mut removed_count = 0;
        let mut removed_bytes = 0;

        // Remove oldest messages until we reach target count
        while messages.len() > target_count {
            if let Some((_, message)) = messages.pop_first() {
                removed_bytes += self.estimate_message_size(&message);
                removed_count += 1;
            } else {
                break;
            }
        }

        if removed_count > 0 {
            // Update memory usage
            let current_usage = self.memory_usage.load(Ordering::Relaxed);
            self.memory_usage.store(current_usage.saturating_sub(removed_bytes), Ordering::Relaxed);
            
            // Update metrics
            if self.config.enable_metrics {
                self.metrics.record_cleanup(removed_count);
                self.metrics.update_memory_usage(self.memory_usage.load(Ordering::Relaxed));
                self.metrics.update_message_count(messages.len());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl StorageEngine for MemoryStorageEngine {
    async fn append(&self, message: StorageMessage) -> StorageResult<WriteResult> {
        let start_time = Instant::now();
        
        // Check capacity before appending
        self.cleanup_if_needed().await?;

        let offset_value = self.next_offset.fetch_add(1, Ordering::Relaxed);
        let offset = LogOffset::new(offset_value);
        
        // Create new message with assigned offset
        let mut stored_message = message;
        stored_message.offset = offset;
        stored_message.timestamp = HybridLogicalClock::now();

        let message_size = self.estimate_message_size(&stored_message);

        // Check memory limits
        let current_usage = self.memory_usage.load(Ordering::Relaxed);
        if current_usage + message_size > self.config.max_memory_bytes {
            return Err(StorageError::CapacityExceeded {
                used: (current_usage + message_size) as u64,
                capacity: self.config.max_memory_bytes as u64,
            });
        }

        // Store the message
        {
            let mut messages = self.messages.write();
            messages.insert(offset_value, stored_message);
        }

        // Update memory usage and metrics
        self.memory_usage.fetch_add(message_size, Ordering::Relaxed);
        
        let latency = start_time.elapsed();
        if self.config.enable_metrics {
            self.metrics.record_write(latency.as_nanos() as u64);
            self.metrics.update_memory_usage(self.memory_usage.load(Ordering::Relaxed));
            self.metrics.update_message_count(self.message_count());
        }

        Ok(WriteResult {
            offset,
            bytes_written: message_size as u32,
            latency,
            fsync_performed: false, // Memory storage doesn't fsync
            replicas_written: 0, // Single-node memory storage
        })
    }

    async fn append_batch(&self, messages: Vec<StorageMessage>) -> StorageResult<BatchWriteResult> {
        let start_time = Instant::now();
        
        if messages.is_empty() {
            return Ok(BatchWriteResult {
                message_results: Vec::new(),
                messages_written: 0,
                total_bytes_written: 0,
                total_latency: Duration::from_nanos(0),
                average_latency: Duration::from_nanos(0),
                success: true,
                first_error: None,
            });
        }

        let mut message_results = Vec::with_capacity(messages.len());
        let mut total_bytes_written = 0;

        for message in messages {
            match self.append(message).await {
                Ok(write_result) => {
                    total_bytes_written += write_result.bytes_written as u64;
                    message_results.push(write_result);
                }
                Err(e) => {
                    let total_latency = start_time.elapsed();
                    let average_latency = if message_results.is_empty() {
                        Duration::from_nanos(0)
                    } else {
                        Duration::from_nanos(total_latency.as_nanos() as u64 / message_results.len() as u64)
                    };
                    let messages_written = message_results.len();
                    
                    return Ok(BatchWriteResult {
                        message_results,
                        messages_written,
                        total_bytes_written,
                        total_latency,
                        average_latency,
                        success: false,
                        first_error: Some(e.to_string()),
                    });
                }
            }
        }

        let total_latency = start_time.elapsed();
        let average_latency = Duration::from_nanos(total_latency.as_nanos() as u64 / message_results.len() as u64);
        let messages_written = message_results.len();
        
        if self.config.enable_metrics {
            self.metrics.record_batch_operation();
        }

        Ok(BatchWriteResult {
            message_results,
            messages_written,
            total_bytes_written,
            total_latency,
            average_latency,
            success: true,
            first_error: None,
        })
    }

    async fn read(&self, offset: LogOffset) -> StorageResult<StorageMessage> {
        let start_time = Instant::now();
        
        let messages = self.messages.read();
        let message = messages.get(&offset.value()).cloned().ok_or_else(|| {
            StorageError::MessageNotFound {
                offset: offset.value(),
            }
        })?;

        let latency = start_time.elapsed();
        if self.config.enable_metrics {
            self.metrics.record_read(latency.as_nanos() as u64);
        }

        Ok(message)
    }

    async fn read_range(
        &self,
        start: LogOffset,
        end: LogOffset,
    ) -> StorageResult<Box<dyn MessageIterator>> {
        let start_time = Instant::now();
        
        if start.value() > end.value() {
            return Err(StorageError::InvalidOffset {
                offset: start.value(),
                reason: "start offset must be less than or equal to end offset".to_string(),
            });
        }

        let messages = self.messages.read();
        let range_messages: Vec<StorageMessage> = messages
            .range(start.value()..end.value()) // end is exclusive in trait
            .map(|(_, message)| message.clone())
            .collect();

        let latency = start_time.elapsed();
        if self.config.enable_metrics {
            self.metrics.record_read(latency.as_nanos() as u64);
        }

        Ok(Box::new(MemoryMessageIterator::new(range_messages, start)))
    }

    async fn create_reader(&self, position: ReadPosition) -> StorageResult<Box<dyn StorageReader>> {
        // Check concurrent reader limit
        let current_readers = self.metrics.active_readers.load(Ordering::Relaxed);
        if current_readers >= self.config.max_concurrent_readers {
            return Err(StorageError::ResourceExhausted {
                resource: "concurrent_readers".to_string(),
                details: format!("Current: {}, Limit: {}", current_readers, self.config.max_concurrent_readers),
            });
        }

        let reader = MemoryStorageReader::new(
            Arc::clone(&self.messages),
            position,
            Arc::clone(&self.metrics),
        )?;
        
        Ok(Box::new(reader))
    }

    async fn flush(&self) -> StorageResult<FlushResult> {
        let start_time = Instant::now();
        
        // Memory storage doesn't need to flush to persistent storage
        // This is essentially a no-op but we maintain the interface
        
        let latency = start_time.elapsed();
        if self.config.enable_metrics {
            self.metrics.record_flush();
        }

        Ok(FlushResult {
            bytes_flushed: 0, // No actual flushing for memory storage
            messages_flushed: 0, // No actual flushing for memory storage
            latency,
            fsync_performed: false, // Memory storage doesn't fsync
        })
    }

    fn metrics(&self) -> StorageMetrics {
        let memory_usage = self.memory_usage.load(Ordering::Relaxed);
        let message_count = self.message_count();
        
        StorageMetrics {
            writes_total: self.metrics.total_writes.load(Ordering::Relaxed),
            reads_total: self.metrics.total_reads.load(Ordering::Relaxed),
            batch_operations_total: self.metrics.total_batch_operations.load(Ordering::Relaxed),
            flush_operations_total: self.metrics.total_flush_operations.load(Ordering::Relaxed),
            write_latency_avg: Duration::from_nanos(self.metrics.average_write_latency()),
            write_latency_p99: Duration::from_nanos(self.metrics.average_write_latency()),
            read_latency_avg: Duration::from_nanos(self.metrics.average_read_latency()),
            read_latency_p99: Duration::from_nanos(self.metrics.average_read_latency()),
            write_throughput_mps: 0.0, // Would need time window tracking
            read_throughput_mps: 0.0, // Would need time window tracking
            write_throughput_bps: 0,
            read_throughput_bps: 0,
            total_bytes_stored: memory_usage as u64,
            total_messages_stored: message_count as u64,
            available_capacity: self.config.max_memory_bytes.saturating_sub(memory_usage) as u64,
            utilization_percentage: if self.config.max_memory_bytes > 0 {
                memory_usage as f64 / self.config.max_memory_bytes as f64
            } else {
                0.0
            },
            active_readers: self.metrics.active_readers.load(Ordering::Relaxed) as u32,
            active_writers: 0, // Memory storage doesn't track writers
            pending_operations: 0, // Memory storage doesn't have pending operations
            read_errors_total: 0, // Would need error tracking
            write_errors_total: 0, // Would need error tracking
            corruption_errors_total: 0, // Memory storage doesn't corrupt
            active_replicas: 0, // Single-node memory storage
            replication_lag_ms: 0, // No replication
            replication_failures: 0, // No replication
        }
    }

    async fn shutdown(self) -> StorageResult<()> {
        // Memory storage cleanup is automatic via Drop
        // No additional cleanup needed
        Ok(())
    }
}

/// Message iterator implementation for memory storage
pub struct MemoryMessageIterator {
    messages: Vec<StorageMessage>,
    current_index: usize,
    current_position: LogOffset,
    batch_size: usize,
}

impl MemoryMessageIterator {
    /// Create a new iterator with messages
    pub fn new(messages: Vec<StorageMessage>, start_offset: LogOffset) -> Self {
        Self {
            messages,
            current_index: 0,
            current_position: start_offset,
            batch_size: 1000,
        }
    }
}

impl Iterator for MemoryMessageIterator {
    type Item = StorageResult<StorageMessage>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index < self.messages.len() {
            let message = self.messages[self.current_index].clone();
            self.current_index += 1;
            self.current_position = message.offset;
            Some(Ok(message))
        } else {
            None
        }
    }
}

impl MessageIterator for MemoryMessageIterator {
    fn current_position(&self) -> LogOffset {
        self.current_position
    }

    fn seek(&mut self, position: ReadPosition) -> StorageResult<()> {
        match position {
            ReadPosition::Beginning => {
                self.current_index = 0;
                if !self.messages.is_empty() {
                    self.current_position = self.messages[0].offset;
                }
            }
            ReadPosition::End => {
                self.current_index = self.messages.len();
                if !self.messages.is_empty() {
                    self.current_position = self.messages[self.messages.len() - 1].offset;
                }
            }
            ReadPosition::Offset(offset) => {
                // Find the message with the given offset
                if let Some(pos) = self.messages.iter().position(|msg| msg.offset >= offset) {
                    self.current_index = pos;
                    self.current_position = offset;
                } else {
                    self.current_index = self.messages.len();
                }
            }
            ReadPosition::Relative(delta) => {
                let new_index = if delta >= 0 {
                    self.current_index.saturating_add(delta as usize)
                } else {
                    self.current_index.saturating_sub((-delta) as usize)
                };
                self.current_index = new_index.min(self.messages.len());
                if self.current_index < self.messages.len() {
                    self.current_position = self.messages[self.current_index].offset;
                }
            }
            ReadPosition::Timestamp(hlc) => {
                // Find the first message at or after the given timestamp
                if let Some(pos) = self.messages.iter().position(|msg| msg.timestamp >= hlc) {
                    self.current_index = pos;
                    self.current_position = self.messages[pos].offset;
                } else {
                    self.current_index = self.messages.len();
                }
            }
        }
        Ok(())
    }

    fn set_batch_size(&mut self, size: usize) {
        self.batch_size = size;
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.messages.len().saturating_sub(self.current_index);
        (remaining, Some(remaining))
    }
}

/// Storage reader implementation for memory storage
pub struct MemoryStorageReader {
    messages: Arc<RwLock<BTreeMap<u64, StorageMessage>>>,
    current_offset: u64,
    end_offset: Option<u64>,
    metrics: Arc<MemoryStorageMetrics>,
    batch_size: usize,
}

impl MemoryStorageReader {
    /// Create a new memory storage reader
    ///
    /// # Errors
    ///
    /// Returns `StorageError` if the read position is invalid
    pub fn new(
        messages: Arc<RwLock<BTreeMap<u64, StorageMessage>>>,
        position: ReadPosition,
        metrics: Arc<MemoryStorageMetrics>,
    ) -> StorageResult<Self> {
        let current_offset = match position {
            ReadPosition::Beginning => 1,
            ReadPosition::End => {
                let messages_guard = messages.read();
                messages_guard.keys().last().copied().unwrap_or(0) + 1
            }
            ReadPosition::Offset(offset) => offset.value(),
            ReadPosition::Relative(_delta) => {
                // For new reader, relative positioning doesn't make sense
                // Start from beginning
                1
            }
            ReadPosition::Timestamp(hlc) => {
                // Find first message at or after timestamp
                let messages_guard = messages.read();
                messages_guard
                    .values()
                    .find(|msg| msg.timestamp >= hlc)
                    .map(|msg| msg.offset.value())
                    .unwrap_or_else(|| messages_guard.keys().last().copied().unwrap_or(0) + 1)
            }
        };

        metrics.add_reader();

        Ok(Self {
            messages,
            current_offset,
            end_offset: None,
            metrics,
            batch_size: 1000,
        })
    }
}

#[async_trait]
impl StorageReader for MemoryStorageReader {
    async fn read_next(&mut self) -> StorageResult<Option<StorageMessage>> {
        if let Some(end) = self.end_offset {
            if self.current_offset > end {
                return Ok(None);
            }
        }

        let messages_guard = self.messages.read();
        if let Some(message) = messages_guard.get(&self.current_offset) {
            let result = message.clone();
            self.current_offset += 1;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn seek(&mut self, position: ReadPosition) -> StorageResult<()> {
        match position {
            ReadPosition::Beginning => self.current_offset = 1,
            ReadPosition::End => {
                let messages_guard = self.messages.read();
                self.current_offset = messages_guard.keys().last().copied().unwrap_or(0) + 1;
            }
            ReadPosition::Offset(offset) => self.current_offset = offset.value(),
            ReadPosition::Relative(delta) => {
                if delta >= 0 {
                    self.current_offset = self.current_offset.saturating_add(delta as u64);
                } else {
                    self.current_offset = self.current_offset.saturating_sub((-delta) as u64);
                }
            }
            ReadPosition::Timestamp(hlc) => {
                let messages_guard = self.messages.read();
                self.current_offset = messages_guard
                    .values()
                    .find(|msg| msg.timestamp >= hlc)
                    .map(|msg| msg.offset.value())
                    .unwrap_or_else(|| messages_guard.keys().last().copied().unwrap_or(0) + 1);
            }
        }
        Ok(())
    }

    fn current_position(&self) -> LogOffset {
        LogOffset::new(self.current_offset)
    }

    fn has_more(&self) -> bool {
        if let Some(end) = self.end_offset {
            if self.current_offset > end {
                return false;
            }
        }

        let messages_guard = self.messages.read();
        messages_guard.contains_key(&self.current_offset)
    }

    fn set_batch_size(&mut self, batch_size: usize) {
        self.batch_size = batch_size;
    }

    fn batch_size(&self) -> usize {
        self.batch_size
    }

    async fn close(self: Box<Self>) -> StorageResult<()> {
        // Reader cleanup happens in Drop
        Ok(())
    }
}

impl Drop for MemoryStorageReader {
    fn drop(&mut self) {
        self.metrics.remove_reader();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaelix_cluster::messages::{ClusterMessage, MessagePayload};
    use kaelix_cluster::types::NodeId;

    async fn create_test_engine() -> MemoryStorageEngine {
        MemoryStorageEngine::new().await.unwrap()
    }

    async fn create_test_message(_data: &str) -> StorageMessage {
        let node_id = NodeId::generate();
        let cluster_msg = ClusterMessage::new(
            node_id,
            node_id,
            MessagePayload::HealthCheck,
        );
        let offset = LogOffset::new(0); // Placeholder offset
        StorageMessage::new(cluster_msg, node_id, offset)
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let engine = create_test_engine().await;
        assert_eq!(engine.message_count(), 0);
        assert_eq!(engine.memory_usage(), 0);
    }

    #[tokio::test]
    async fn test_engine_with_config() {
        let config = MemoryStorageConfig::new()
            .with_max_messages(500)
            .with_max_memory_bytes(1024)
            .with_cleanup_threshold(100) // Must be less than max_messages
            .with_compression(true);
        
        let engine = MemoryStorageEngine::with_config(config).await.unwrap();
        assert_eq!(engine.config.max_messages, 500);
        assert_eq!(engine.config.max_memory_bytes, 1024);
        assert_eq!(engine.config.cleanup_threshold, 100);
        assert!(engine.config.enable_compression);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let invalid_config = MemoryStorageConfig::new()
            .with_max_messages(0); // Invalid
        
        let result = MemoryStorageEngine::with_config(invalid_config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_basic_append_and_read() {
        let engine = create_test_engine().await;
        let message = create_test_message("test message").await;
        
        // Test append
        let write_result = engine.append(message.clone()).await.unwrap();
        assert_eq!(write_result.offset.value(), 1);
        assert!(write_result.bytes_written > 0);
        assert!(write_result.latency.as_nanos() > 0);
        
        // Test read
        let read_message = engine.read(write_result.offset).await.unwrap();
        assert_eq!(read_message.cluster_message.payload, message.cluster_message.payload);
    }

    #[tokio::test]
    async fn test_read_nonexistent_message() {
        let engine = create_test_engine().await;
        let result = engine.read(LogOffset::new(999)).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::MessageNotFound { .. }));
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let engine = create_test_engine().await;
        let messages = vec![
            create_test_message("message 1").await,
            create_test_message("message 2").await,
            create_test_message("message 3").await,
        ];
        
        // Test batch append
        let batch_result = engine.append_batch(messages.clone()).await.unwrap();
        assert_eq!(batch_result.message_results.len(), 3);
        assert!(batch_result.total_bytes_written > 0);
        assert!(batch_result.total_latency.as_nanos() > 0);
        assert!(batch_result.success);
        
        // Test read back all messages
        for (i, write_result) in batch_result.message_results.iter().enumerate() {
            let read_message = engine.read(write_result.offset).await.unwrap();
            assert_eq!(read_message.cluster_message.payload, messages[i].cluster_message.payload);
        }
    }

    #[tokio::test]
    async fn test_read_range() {
        let engine = create_test_engine().await;
        let messages = vec![
            create_test_message("message 1").await,
            create_test_message("message 2").await,
            create_test_message("message 3").await,
        ];
        
        // Add messages to the engine
        let batch_result = engine.append_batch(messages.clone()).await.unwrap();
        
        // Test read range
        let start_offset = batch_result.message_results[0].offset;
        let end_offset = LogOffset::new(batch_result.message_results[2].offset.value() + 1); // End is exclusive
        let range_iterator = engine.read_range(start_offset, end_offset).await.unwrap();
        
        let mut range_messages = Vec::new();
        for result in range_iterator {
            range_messages.push(result.unwrap());
        }
        
        assert_eq!(range_messages.len(), 3);
        for (i, message) in range_messages.iter().enumerate() {
            assert_eq!(message.cluster_message.payload, messages[i].cluster_message.payload);
        }
    }

    #[tokio::test]
    async fn test_capacity_limits() {
        let config = MemoryStorageConfig::new()
            .with_max_messages(2)
            .with_cleanup_threshold(1);
        
        let engine = MemoryStorageEngine::with_config(config).await.unwrap();
        let messages = vec![
            create_test_message("message 1").await,
            create_test_message("message 2").await,
            create_test_message("message 3").await,
        ];
        
        // Add messages that will trigger cleanup
        for message in messages {
            engine.append(message).await.unwrap();
        }
        
        // After cleanup, we should have at most max_messages - cleanup_threshold = 1 message
        assert!(engine.message_count() <= 1);
    }

    #[tokio::test]
    async fn test_concurrent_readers_limit() {
        let config = MemoryStorageConfig::new()
            .with_max_concurrent_readers(2);
        
        let engine = MemoryStorageEngine::with_config(config).await.unwrap();
        
        // Create maximum allowed readers
        let _reader1 = engine.create_reader(ReadPosition::Beginning).await.unwrap();
        let _reader2 = engine.create_reader(ReadPosition::Beginning).await.unwrap();
        
        // Third reader should fail due to limit
        let result = engine.create_reader(ReadPosition::Beginning).await;
        assert!(result.is_err());
        // Fix the compilation issue with the test
        match result {
            Err(StorageError::ResourceExhausted { .. }) => {
                // Test passes
            }
            _ => panic!("Expected ResourceExhausted error"),
        }
    }

    #[tokio::test]
    async fn test_reader_creation_and_iteration() {
        let engine = create_test_engine().await;
        let messages = vec![
            create_test_message("message 1").await,
            create_test_message("message 2").await,
        ];
        
        engine.append_batch(messages).await.unwrap();
        
        let mut reader = engine.create_reader(ReadPosition::Beginning).await.unwrap();
        let mut count = 0;
        
        while let Some(_message) = reader.read_next().await.unwrap() {
            count += 1;
            if count > 5 { // Safety break to avoid infinite loop
                break;
            }
        }
        
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_reader_positioning() {
        let engine = create_test_engine().await;
        let messages = vec![
            create_test_message("message 1").await,
            create_test_message("message 2").await,
        ];
        
        engine.append_batch(messages).await.unwrap();
        
        // Test different read positions
        let reader_beginning = engine.create_reader(ReadPosition::Beginning).await.unwrap();
        assert!(reader_beginning.has_more());
        
        let reader_end = engine.create_reader(ReadPosition::End).await.unwrap();
        assert!(!reader_end.has_more());
        
        let reader_timestamp = engine.create_reader(ReadPosition::Timestamp(HybridLogicalClock::default())).await.unwrap();
        assert!(reader_timestamp.has_more());
    }

    #[tokio::test]
    async fn test_flush_operations() {
        let engine = create_test_engine().await;
        let message = create_test_message("test message").await;
        
        engine.append(message).await.unwrap();
        
        let flush_result = engine.flush().await.unwrap();
        assert_eq!(flush_result.bytes_flushed, 0); // Memory storage doesn't flush to disk
        assert!(flush_result.latency.as_nanos() > 0);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let engine = create_test_engine().await;
        let message = create_test_message("test message").await;
        
        // Perform some operations
        engine.append(message.clone()).await.unwrap();
        let _read_result = engine.read(LogOffset::new(1)).await.unwrap();
        
        // Check metrics
        let storage_metrics = engine.metrics();
        assert_eq!(storage_metrics.total_messages_stored, 1);
        assert!(storage_metrics.total_bytes_stored > 0);
        assert!(storage_metrics.write_latency_p99.as_nanos() > 0);
        assert!(storage_metrics.read_latency_p99.as_nanos() > 0);
    }

    #[tokio::test]
    async fn test_memory_usage_tracking() {
        let engine = create_test_engine().await;
        let initial_usage = engine.memory_usage();
        
        let message = create_test_message("test message").await;
        engine.append(message).await.unwrap();
        
        assert!(engine.memory_usage() > initial_usage);
    }

    #[tokio::test]
    async fn test_reader_seek_operations() {
        let engine = create_test_engine().await;
        let messages = vec![
            create_test_message("message 1").await,
            create_test_message("message 2").await,
            create_test_message("message 3").await,
        ];
        
        let batch_result = engine.append_batch(messages).await.unwrap();
        
        let mut reader = engine.create_reader(ReadPosition::Beginning).await.unwrap();
        
        // Seek to specific offset
        let target_offset = batch_result.message_results[1].offset;
        reader.seek(ReadPosition::Offset(target_offset)).await.unwrap();
        
        // Should read message 2 and 3
        let mut count = 0;
        while reader.read_next().await.unwrap().is_some() {
            count += 1;
            if count > 5 { // Safety break
                break;
            }
        }
        
        assert_eq!(count, 2); // Should read 2 remaining messages
    }

    #[tokio::test]
    async fn test_performance_benchmarks() {
        let engine = create_test_engine().await;
        let message_count = 1000;
        
        // Benchmark write operations
        let start_time = Instant::now();
        for i in 0..message_count {
            let message = create_test_message(&format!("message {}", i)).await;
            engine.append(message).await.unwrap();
        }
        let write_duration = start_time.elapsed();
        
        // Benchmark read operations
        let start_time = Instant::now();
        for i in 1..=message_count {
            let _message = engine.read(LogOffset::new(i as u64)).await.unwrap();
        }
        let read_duration = start_time.elapsed();
        
        // Calculate average latencies
        let avg_write_latency = write_duration.as_nanos() as u64 / message_count;
        let avg_read_latency = read_duration.as_nanos() as u64 / message_count;
        
        println!("Average write latency: {}ns", avg_write_latency);
        println!("Average read latency: {}ns", avg_read_latency);
        
        // Verify performance targets (flexible for testing environment)
        assert!(avg_write_latency < 100_000, "Write latency {} > 100μs", avg_write_latency); // 100μs
        assert!(avg_read_latency < 50_000, "Read latency {} > 50μs", avg_read_latency); // 50μs
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        use std::sync::Arc;
        use tokio::task;
        
        let engine = Arc::new(create_test_engine().await);
        let mut handles = Vec::new();
        
        // Spawn multiple writers
        for i in 0..10 {
            let engine_clone = Arc::clone(&engine);
            let handle = task::spawn(async move {
                for j in 0..10 {
                    let message = create_test_message(&format!("message-{}-{}", i, j)).await;
                    engine_clone.append(message).await.unwrap();
                }
            });
            handles.push(handle);
        }
        
        // Spawn multiple readers
        for _i in 0..5 {
            let engine_clone = Arc::clone(&engine);
            let handle = task::spawn(async move {
                // Wait a bit for some writes to happen
                tokio::time::sleep(Duration::from_millis(10)).await;
                
                let mut reader = engine_clone.create_reader(ReadPosition::Beginning).await.unwrap();
                let mut count = 0;
                while reader.has_more() && count < 100 { // Limit iterations for safety
                    let _ = reader.read_next().await.unwrap();
                    count += 1;
                }
            });
            handles.push(handle);
        }
        
        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        // Verify final state
        assert!(engine.message_count() <= 100); // 10 writers * 10 messages each
        assert!(engine.memory_usage() > 0);
    }
}