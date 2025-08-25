//! In-memory storage backend implementation
//!
//! Provides a high-performance in-memory storage backend for development,
//! testing, and scenarios where data persistence is not required.

use crate::error::{StorageError, StorageResult};
use crate::types::LogOffset;
use bytes::Bytes;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Instant;

/// Message entry for in-memory storage
#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub sequence: u64,
    pub offset: u64,
    pub data: Bytes,
    pub timestamp: u64,
}

/// Range of messages for range reads
#[derive(Debug)]
pub struct MessageRange {
    pub start_offset: LogOffset,
    pub end_offset: LogOffset,
    pub messages: Vec<MessageEntry>,
    pub total_size: u64,
}

/// High-performance in-memory storage backend
///
/// Stores messages in memory using optimized data structures for maximum
/// throughput and minimal latency. Suitable for development, testing, and
/// scenarios where data persistence is not required.
#[derive(Debug)]
pub struct MemoryBackend {
    /// Messages indexed by offset for fast sequential access
    messages: Arc<RwLock<BTreeMap<u64, MessageEntry>>>,
    /// Message sequence index for fast lookup by sequence number
    sequence_index: Arc<DashMap<u64, u64>>, // sequence -> offset
    /// Current write offset
    write_offset: Arc<AtomicU64>,
    /// Total number of messages stored
    message_count: Arc<AtomicU64>,
    /// Total bytes stored
    total_bytes: Arc<AtomicU64>,
    /// Backend configuration
    config: MemoryBackendConfig,
}

/// Configuration for memory backend behavior
#[derive(Debug, Clone)]
pub struct MemoryBackendConfig {
    /// Maximum number of messages to retain in memory
    pub max_messages: Option<usize>,
    /// Maximum memory usage in bytes
    pub max_memory: Option<usize>,
    /// Enable compression for stored messages
    pub enable_compression: bool,
}

impl Default for MemoryBackendConfig {
    fn default() -> Self {
        Self {
            max_messages: Some(1_000_000),        // 1M messages
            max_memory: Some(1024 * 1024 * 1024), // 1GB
            enable_compression: false,
        }
    }
}

impl MemoryBackend {
    /// Create a new memory backend with default configuration
    pub fn new() -> Self {
        Self::with_config(MemoryBackendConfig::default())
    }

    /// Create a new memory backend with custom configuration
    pub fn with_config(config: MemoryBackendConfig) -> Self {
        Self {
            messages: Arc::new(RwLock::new(BTreeMap::new())),
            sequence_index: Arc::new(DashMap::new()),
            write_offset: Arc::new(AtomicU64::new(0)),
            message_count: Arc::new(AtomicU64::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    /// Get current memory usage statistics
    pub fn memory_stats(&self) -> MemoryStats {
        let messages = self.messages.read();
        let message_count = self.message_count.load(Ordering::Acquire);
        let total_bytes = self.total_bytes.load(Ordering::Acquire);
        let sequence_index_size = self.sequence_index.len();

        MemoryStats {
            message_count,
            total_bytes,
            sequence_index_size: sequence_index_size as u64,
            btree_size: messages.len() as u64,
            estimated_overhead: (sequence_index_size * 16 + messages.len() * 32) as u64,
        }
    }

    /// Clear all stored messages
    pub fn clear(&self) -> StorageResult<()> {
        let mut messages = self.messages.write();
        messages.clear();
        self.sequence_index.clear();
        self.write_offset.store(0, Ordering::Release);
        self.message_count.store(0, Ordering::Release);
        self.total_bytes.store(0, Ordering::Release);
        Ok(())
    }

    /// Check if storage limits are exceeded
    fn check_limits(&self) -> StorageResult<()> {
        let current_count = self.message_count.load(Ordering::Acquire);
        let current_memory = self.total_bytes.load(Ordering::Acquire);

        if let Some(max_messages) = self.config.max_messages {
            if current_count >= max_messages as u64 {
                return Err(StorageError::CapacityExceeded {
                    used: current_count,
                    capacity: max_messages as u64,
                });
            }
        }

        if let Some(max_memory) = self.config.max_memory {
            if current_memory >= max_memory as u64 {
                return Err(StorageError::CapacityExceeded {
                    used: current_memory,
                    capacity: max_memory as u64,
                });
            }
        }

        Ok(())
    }

    /// Write a message to memory storage
    pub async fn write_message(&self, sequence: u64, data: Bytes) -> StorageResult<LogOffset> {
        let start_time = Instant::now();

        // Check capacity limits
        self.check_limits()?;

        let offset = self.write_offset.fetch_add(1, Ordering::AcqRel);

        let message_entry = MessageEntry {
            sequence,
            offset,
            data: data.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
        };

        let message_size = data.len() as u64;

        // Store message
        {
            let mut messages = self.messages.write();
            messages.insert(offset, message_entry);
        }

        // Update sequence index
        self.sequence_index.insert(sequence, offset);

        // Update statistics
        self.message_count.fetch_add(1, Ordering::AcqRel);
        self.total_bytes.fetch_add(message_size, Ordering::AcqRel);

        tracing::debug!(
            sequence = sequence,
            offset = offset,
            size = message_size,
            duration_us = start_time.elapsed().as_micros(),
            "Message written to memory backend"
        );

        Ok(LogOffset::new(offset))
    }

    /// Write a batch of messages to memory storage
    pub async fn write_batch(&self, messages: Vec<(u64, Bytes)>) -> StorageResult<Vec<LogOffset>> {
        let start_time = Instant::now();

        if messages.is_empty() {
            return Ok(Vec::new());
        }

        // Check if batch would exceed limits
        let batch_size: u64 = messages.iter().map(|(_, data)| data.len() as u64).sum();
        let current_count = self.message_count.load(Ordering::Acquire);
        let current_memory = self.total_bytes.load(Ordering::Acquire);

        if let Some(max_messages) = self.config.max_messages {
            if current_count + messages.len() as u64 > max_messages as u64 {
                return Err(StorageError::CapacityExceeded {
                    used: current_count + messages.len() as u64,
                    capacity: max_messages as u64,
                });
            }
        }

        if let Some(max_memory) = self.config.max_memory {
            if current_memory + batch_size > max_memory as u64 {
                return Err(StorageError::CapacityExceeded {
                    used: current_memory + batch_size,
                    capacity: max_memory as u64,
                });
            }
        }

        let mut offsets = Vec::with_capacity(messages.len());
        let mut message_entries = Vec::with_capacity(messages.len());

        // Pre-allocate offsets
        let start_offset = self.write_offset.fetch_add(messages.len() as u64, Ordering::AcqRel);

        // Prepare message entries
        for (i, (sequence, data)) in messages.into_iter().enumerate() {
            let offset = start_offset + i as u64;
            let message_entry = MessageEntry {
                sequence,
                offset,
                data,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
            };

            offsets.push(LogOffset::new(offset));
            message_entries.push((sequence, offset, message_entry));
        }

        // Batch insert into storage
        {
            let mut storage_messages = self.messages.write();
            for (sequence, offset, message_entry) in message_entries {
                storage_messages.insert(offset, message_entry);
                self.sequence_index.insert(sequence, offset);
            }
        }

        // Update statistics
        self.message_count.fetch_add(offsets.len() as u64, Ordering::AcqRel);
        self.total_bytes.fetch_add(batch_size, Ordering::AcqRel);

        tracing::debug!(
            batch_size = offsets.len(),
            total_bytes = batch_size,
            duration_us = start_time.elapsed().as_micros(),
            "Batch written to memory backend"
        );

        Ok(offsets)
    }

    /// Read a message by offset
    pub async fn read_message(&self, offset: LogOffset) -> StorageResult<MessageEntry> {
        let start_time = Instant::now();

        let messages = self.messages.read();
        let message = messages.get(&offset.value()).ok_or_else(|| {
            StorageError::MessageNotFound { message_id: format!("offset-{}", offset.value()) }
        })?;

        tracing::debug!(
            offset = offset.value(),
            sequence = message.sequence,
            duration_us = start_time.elapsed().as_micros(),
            "Message read from memory backend"
        );

        Ok(message.clone())
    }

    /// Read a message by sequence number
    pub async fn read_message_by_sequence(&self, sequence: u64) -> StorageResult<MessageEntry> {
        let start_time = Instant::now();

        let offset = self
            .sequence_index
            .get(&sequence)
            .ok_or_else(|| StorageError::SequenceNotFound { sequence })?;

        let messages = self.messages.read();
        let message = messages.get(offset.value()).ok_or_else(|| {
            StorageError::MessageNotFound { message_id: format!("sequence-{}", sequence) }
        })?;

        tracing::debug!(
            sequence = sequence,
            offset = offset.value(),
            duration_us = start_time.elapsed().as_micros(),
            "Message read by sequence from memory backend"
        );

        Ok(message.clone())
    }

    /// Read a range of messages
    pub async fn read_range(
        &self,
        start: LogOffset,
        end: LogOffset,
    ) -> StorageResult<MessageRange> {
        let start_time = Instant::now();

        if start.value() > end.value() {
            return Err(StorageError::InvalidOffset {
                offset: start.value(),
                segment_size: end.value(),
            });
        }

        let messages = self.messages.read();

        let range_messages: Vec<MessageEntry> = messages
            .range(start.value()..=end.value())
            .map(|(_, message)| message.clone())
            .collect();

        let total_size: u64 = range_messages.iter().map(|m| m.data.len() as u64).sum();

        tracing::debug!(
            start_offset = start.value(),
            end_offset = end.value(),
            message_count = range_messages.len(),
            total_size = total_size,
            duration_us = start_time.elapsed().as_micros(),
            "Range read from memory backend"
        );

        Ok(MessageRange {
            start_offset: start,
            end_offset: end,
            messages: range_messages,
            total_size,
        })
    }

    /// Get the latest offset
    pub async fn get_latest_offset(&self) -> StorageResult<Option<LogOffset>> {
        let current_offset = self.write_offset.load(Ordering::Acquire);
        if current_offset > 0 {
            Ok(Some(LogOffset::new(current_offset - 1)))
        } else {
            Ok(None)
        }
    }

    /// Get message count
    pub async fn get_message_count(&self) -> StorageResult<u64> {
        Ok(self.message_count.load(Ordering::Acquire))
    }

    /// Get storage size
    pub async fn get_storage_size(&self) -> StorageResult<u64> {
        Ok(self.total_bytes.load(Ordering::Acquire))
    }

    /// Sync data (no-op for memory backend)
    pub async fn sync(&self) -> StorageResult<()> {
        Ok(())
    }

    /// Compact storage (no-op for memory backend)
    pub async fn compact(&self) -> StorageResult<()> {
        Ok(())
    }

    /// Initialize the backend
    pub async fn initialize(&self) -> StorageResult<()> {
        Ok(())
    }

    /// Shutdown the backend
    pub async fn shutdown(&self) -> StorageResult<()> {
        Ok(())
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory usage statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Number of messages stored
    pub message_count: u64,
    /// Total bytes stored in message data
    pub total_bytes: u64,
    /// Number of entries in sequence index
    pub sequence_index_size: u64,
    /// Number of entries in BTree
    pub btree_size: u64,
    /// Estimated overhead in bytes
    pub estimated_overhead: u64,
}

impl MemoryStats {
    /// Get total estimated memory usage
    pub fn total_memory_usage(&self) -> u64 {
        self.total_bytes + self.estimated_overhead
    }

    /// Get average message size
    pub fn average_message_size(&self) -> f64 {
        if self.message_count > 0 {
            self.total_bytes as f64 / self.message_count as f64
        } else {
            0.0
        }
    }

    /// Get memory efficiency ratio (data bytes / total memory)
    pub fn memory_efficiency(&self) -> f64 {
        let total = self.total_memory_usage();
        if total > 0 {
            self.total_bytes as f64 / total as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_backend_basic_operations() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        // Test write
        let data = Bytes::from_static(b"hello world");
        let offset = backend.write_message(1, data.clone()).await.unwrap();
        assert_eq!(offset.value(), 0);

        // Test read by offset
        let message = backend.read_message(offset).await.unwrap();
        assert_eq!(message.sequence, 1);
        assert_eq!(message.data, data);

        // Test read by sequence
        let message_by_seq = backend.read_message_by_sequence(1).await.unwrap();
        assert_eq!(message_by_seq.sequence, 1);
        assert_eq!(message_by_seq.data, data);

        // Test stats
        assert_eq!(backend.get_message_count().await.unwrap(), 1);
        assert_eq!(backend.get_storage_size().await.unwrap(), data.len() as u64);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        let messages = vec![
            (1, Bytes::from_static(b"message 1")),
            (2, Bytes::from_static(b"message 2")),
            (3, Bytes::from_static(b"message 3")),
        ];

        let offsets = backend.write_batch(messages.clone()).await.unwrap();
        assert_eq!(offsets.len(), 3);

        // Verify all messages can be read
        for (i, (sequence, expected_data)) in messages.iter().enumerate() {
            let message = backend.read_message(offsets[i]).await.unwrap();
            assert_eq!(message.sequence, *sequence);
            assert_eq!(message.data, *expected_data);

            let message_by_seq = backend.read_message_by_sequence(*sequence).await.unwrap();
            assert_eq!(message_by_seq.data, *expected_data);
        }
    }

    #[tokio::test]
    async fn test_range_read() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        // Write multiple messages
        let messages = vec![
            (1, Bytes::from_static(b"msg 1")),
            (2, Bytes::from_static(b"msg 2")),
            (3, Bytes::from_static(b"msg 3")),
            (4, Bytes::from_static(b"msg 4")),
            (5, Bytes::from_static(b"msg 5")),
        ];

        let offsets = backend.write_batch(messages).await.unwrap();

        // Read range
        let range = backend.read_range(offsets[1], offsets[3]).await.unwrap();

        assert_eq!(range.messages.len(), 3); // offsets[1], offsets[2], offsets[3]
        assert_eq!(range.messages[0].sequence, 2);
        assert_eq!(range.messages[1].sequence, 3);
        assert_eq!(range.messages[2].sequence, 4);
    }

    #[tokio::test]
    async fn test_capacity_limits() {
        let config = MemoryBackendConfig {
            max_messages: Some(2),
            max_memory: Some(100),
            enable_compression: false,
        };

        let backend = MemoryBackend::with_config(config);
        backend.initialize().await.unwrap();

        // Should succeed
        backend.write_message(1, Bytes::from_static(b"test")).await.unwrap();

        // Should succeed
        backend.write_message(2, Bytes::from_static(b"test")).await.unwrap();

        // Should fail due to message limit
        let result = backend.write_message(3, Bytes::from_static(b"test")).await;
        assert!(matches!(result, Err(StorageError::CapacityExceeded { .. })));
    }

    #[tokio::test]
    async fn test_memory_stats() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        let data = Bytes::from_static(b"test message");
        backend.write_message(1, data.clone()).await.unwrap();

        let stats = backend.memory_stats();
        assert_eq!(stats.message_count, 1);
        assert_eq!(stats.total_bytes, data.len() as u64);
        assert!(stats.estimated_overhead > 0);
        assert!(stats.total_memory_usage() > data.len() as u64);
        assert!(stats.memory_efficiency() < 1.0);
        assert!(stats.average_message_size() > 0.0);
    }

    #[tokio::test]
    async fn test_clear() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        // Add some data
        backend.write_message(1, Bytes::from_static(b"test")).await.unwrap();

        assert_eq!(backend.get_message_count().await.unwrap(), 1);

        // Clear
        backend.clear().unwrap();

        assert_eq!(backend.get_message_count().await.unwrap(), 0);
        assert_eq!(backend.get_storage_size().await.unwrap(), 0);

        // Should not find the message
        let result = backend.read_message_by_sequence(1).await;
        assert!(matches!(result, Err(StorageError::SequenceNotFound { .. })));
    }

    #[tokio::test]
    async fn test_error_conditions() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        // Test read non-existent message
        let result = backend.read_message(LogOffset::new(999)).await;
        assert!(matches!(result, Err(StorageError::MessageNotFound { .. })));

        // Test read by non-existent sequence
        let result = backend.read_message_by_sequence(999).await;
        assert!(matches!(result, Err(StorageError::SequenceNotFound { .. })));

        // Test invalid range
        let result = backend.read_range(LogOffset::new(10), LogOffset::new(5)).await;
        assert!(matches!(result, Err(StorageError::InvalidOffset { .. })));
    }
}
