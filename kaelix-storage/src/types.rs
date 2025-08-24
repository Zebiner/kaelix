//! Storage-specific types and data structures.
//!
//! This module provides the core types used throughout the storage system,
//! including message representations, offsets, metrics, and result types.
//! All types are optimized for high-performance storage operations with
//! minimal serialization overhead.

use crate::error::{StorageError, StorageResult};
use kaelix_cluster::{
    messages::ClusterMessage, time::HybridLogicalClock, types::NodeId,
};
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};

/// Log offset type for efficient position tracking in storage
///
/// Represents a position within the distributed log with 64-bit precision.
/// Offsets are monotonically increasing and unique within a storage partition.
///
/// # Performance Characteristics
///
/// - **Size**: 8 bytes for cache-efficient operations
/// - **Ordering**: Total ordering for consistent reads
/// - **Range**: 0 to 2^64-1 positions supported
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LogOffset(u64);

impl LogOffset {
    /// Create a new log offset
    ///
    /// # Parameters
    ///
    /// * `value` - The offset value
    ///
    /// # Returns
    ///
    /// A new [`LogOffset`] with the specified value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the raw offset value
    ///
    /// # Returns
    ///
    /// The underlying u64 offset value.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.0
    }

    /// Get the next offset
    ///
    /// # Returns
    ///
    /// A new [`LogOffset`] with the next sequential value.
    #[must_use]
    pub const fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    /// Calculate distance between two offsets
    ///
    /// # Parameters
    ///
    /// * `other` - The other offset to compare with
    ///
    /// # Returns
    ///
    /// The absolute difference between the offsets.
    #[must_use]
    pub const fn distance(&self, other: &LogOffset) -> u64 {
        if self.0 >= other.0 { self.0 - other.0 } else { other.0 - self.0 }
    }

    /// Check if this offset is immediately after another
    ///
    /// # Parameters
    ///
    /// * `other` - The offset to check against
    ///
    /// # Returns
    ///
    /// `true` if this offset is exactly one position after the other.
    #[must_use]
    pub const fn follows(&self, other: &LogOffset) -> bool {
        self.0 == other.0 + 1
    }
}

impl fmt::Display for LogOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for LogOffset {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl From<LogOffset> for u64 {
    fn from(offset: LogOffset) -> Self {
        offset.value()
    }
}

/// Storage-optimized message representation
///
/// Provides an efficient storage representation of cluster messages with
/// additional metadata for storage operations, indexing, and retrieval.
///
/// # Design Principles
///
/// - **Zero-Copy Serialization**: Optimized for minimal serialization overhead
/// - **Compact Representation**: Minimal memory footprint for inactive messages
/// - **Rich Metadata**: Sufficient information for indexing and retrieval
/// - **Versioning Support**: Forward-compatible message format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageMessage {
    /// Storage protocol version for compatibility
    pub version: u32,
    /// Message offset within the log
    pub offset: LogOffset,
    /// Hybrid logical clock timestamp for ordering
    pub timestamp: HybridLogicalClock,
    /// Node that stored the message
    pub stored_by: NodeId,
    /// Size of the serialized message in bytes
    pub size_bytes: u32,
    /// Checksum for integrity verification
    pub checksum: u64,
    /// The original cluster message
    pub cluster_message: ClusterMessage,
    /// Storage-specific metadata
    pub metadata: StorageMetadata,
}

/// Storage-specific metadata for message management
///
/// Contains additional metadata required for storage operations,
/// garbage collection, and performance optimization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageMetadata {
    /// Compression algorithm used (if any)
    pub compression: Option<CompressionType>,
    /// Encryption algorithm used (if any)
    pub encryption: Option<EncryptionType>,
    /// Message priority for storage optimization
    pub priority: MessagePriority,
    /// Time-to-live for message expiration
    pub ttl: Option<Duration>,
    /// Number of replicas this message has been written to
    pub replica_count: u8,
    /// Custom tags for message classification
    pub tags: Vec<String>,
}

/// Message compression algorithms supported by storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    /// No compression
    None,
    /// LZ4 fast compression
    Lz4,
    /// Gzip compression
    Gzip,
    /// Snappy compression
    Snappy,
}

/// Message encryption algorithms supported by storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionType {
    /// No encryption
    None,
    /// AES-256-GCM encryption
    Aes256Gcm,
    /// ChaCha20-Poly1305 encryption
    ChaCha20Poly1305,
}

/// Message priority levels for storage optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    /// Low priority - can be processed in background
    Low = 0,
    /// Normal priority - standard processing
    Normal = 1,
    /// High priority - expedited processing
    High = 2,
    /// Critical priority - immediate processing required
    Critical = 3,
}

impl Default for MessagePriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Position specification for read operations
///
/// Provides flexible positioning options for reading from storage,
/// supporting both absolute and relative positioning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadPosition {
    /// Absolute offset position
    Offset(LogOffset),
    /// Beginning of the log
    Beginning,
    /// End of the log
    End,
    /// Relative to current position
    Relative(i64),
    /// Timestamp-based positioning
    Timestamp(HybridLogicalClock),
}

/// Result of a write operation
///
/// Contains the outcome of a storage write operation with performance
/// metrics and positioning information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteResult {
    /// Offset where message was written
    pub offset: LogOffset,
    /// Size of written data in bytes
    pub bytes_written: u32,
    /// Write operation latency
    pub latency: Duration,
    /// Whether data was immediately fsynced to disk
    pub fsync_performed: bool,
    /// Number of replicas the write was successfully sent to
    pub replicas_written: u8,
}

/// Result of a batch write operation
///
/// Contains the outcome of a batch storage operation with detailed
/// performance metrics and per-message results.
#[derive(Debug)]
pub struct BatchWriteResult {
    /// Results for individual messages in the batch
    pub message_results: Vec<WriteResult>,
    /// Total number of messages successfully written
    pub messages_written: usize,
    /// Total number of bytes written
    pub total_bytes_written: u64,
    /// Total batch operation latency
    pub total_latency: Duration,
    /// Average latency per message
    pub average_latency: Duration,
    /// Whether the entire batch was successfully written
    pub success: bool,
    /// First error encountered (if any)
    pub first_error: Option<String>,
}

/// Result of a flush operation
///
/// Contains information about a storage flush operation that ensures
/// data durability by syncing to persistent storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlushResult {
    /// Number of bytes flushed to storage
    pub bytes_flushed: u64,
    /// Number of messages flushed
    pub messages_flushed: usize,
    /// Flush operation latency
    pub latency: Duration,
    /// Whether flush included fsync to disk
    pub fsync_performed: bool,
}

/// Iterator for reading multiple messages
///
/// Provides efficient iteration over storage messages with configurable
/// batch sizes and filtering options.
pub trait MessageIterator: Iterator<Item = StorageResult<StorageMessage>> + Send + Sync {
    /// Get the current position of the iterator
    fn current_position(&self) -> LogOffset;

    /// Seek to a specific position
    fn seek(&mut self, position: ReadPosition) -> StorageResult<()>;

    /// Set batch size for reads
    fn set_batch_size(&mut self, size: usize);

    /// Get remaining message count estimate (if available)
    fn size_hint(&self) -> (usize, Option<usize>);
}

/// Comprehensive storage performance and operational metrics
///
/// Provides detailed metrics for monitoring storage system performance,
/// capacity utilization, and operational health.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageMetrics {
    // === Operation Counts ===
    /// Total number of write operations performed
    pub writes_total: u64,
    /// Total number of read operations performed
    pub reads_total: u64,
    /// Total number of batch operations performed
    pub batch_operations_total: u64,
    /// Total number of flush operations performed
    pub flush_operations_total: u64,

    // === Operation Latencies ===
    /// Average write operation latency
    pub write_latency_avg: Duration,
    /// 99th percentile write operation latency
    pub write_latency_p99: Duration,
    /// Average read operation latency
    pub read_latency_avg: Duration,
    /// 99th percentile read operation latency
    pub read_latency_p99: Duration,

    // === Throughput Metrics ===
    /// Messages written per second
    pub write_throughput_mps: f64,
    /// Messages read per second
    pub read_throughput_mps: f64,
    /// Bytes written per second
    pub write_throughput_bps: u64,
    /// Bytes read per second
    pub read_throughput_bps: u64,

    // === Capacity and Storage ===
    /// Total bytes stored
    pub total_bytes_stored: u64,
    /// Total number of messages stored
    pub total_messages_stored: u64,
    /// Available storage capacity in bytes
    pub available_capacity: u64,
    /// Storage utilization percentage (0.0 - 1.0)
    pub utilization_percentage: f64,

    // === Active Resources ===
    /// Number of active storage readers
    pub active_readers: u32,
    /// Number of active storage writers
    pub active_writers: u32,
    /// Number of pending operations
    pub pending_operations: u32,

    // === Error Metrics ===
    /// Total number of read errors
    pub read_errors_total: u64,
    /// Total number of write errors
    pub write_errors_total: u64,
    /// Total number of corruption errors detected
    pub corruption_errors_total: u64,

    // === Replication Metrics (for distributed backends) ===
    /// Number of active replicas
    pub active_replicas: u8,
    /// Replication lag in milliseconds
    pub replication_lag_ms: u64,
    /// Number of failed replication attempts
    pub replication_failures: u64,
}

impl Default for StorageMetadata {
    fn default() -> Self {
        Self {
            compression: None,
            encryption: None,
            priority: MessagePriority::Normal,
            ttl: None,
            replica_count: 1,
            tags: Vec::new(),
        }
    }
}

impl Default for StorageMetrics {
    fn default() -> Self {
        Self {
            writes_total: 0,
            reads_total: 0,
            batch_operations_total: 0,
            flush_operations_total: 0,
            write_latency_avg: Duration::ZERO,
            write_latency_p99: Duration::ZERO,
            read_latency_avg: Duration::ZERO,
            read_latency_p99: Duration::ZERO,
            write_throughput_mps: 0.0,
            read_throughput_mps: 0.0,
            write_throughput_bps: 0,
            read_throughput_bps: 0,
            total_bytes_stored: 0,
            total_messages_stored: 0,
            available_capacity: 0,
            utilization_percentage: 0.0,
            active_readers: 0,
            active_writers: 0,
            pending_operations: 0,
            read_errors_total: 0,
            write_errors_total: 0,
            corruption_errors_total: 0,
            active_replicas: 1,
            replication_lag_ms: 0,
            replication_failures: 0,
        }
    }
}

impl StorageMessage {
    /// Create a new storage message
    ///
    /// # Parameters
    ///
    /// * `cluster_message` - The cluster message to store
    /// * `stored_by` - Node that is storing the message
    /// * `offset` - Storage offset for the message
    ///
    /// # Returns
    ///
    /// A new [`StorageMessage`] with generated metadata.
    #[must_use]
    pub fn new(cluster_message: ClusterMessage, stored_by: NodeId, offset: LogOffset) -> Self {
        let timestamp = HybridLogicalClock::now();
        let serialized = bincode::serialize(&cluster_message).unwrap_or_default();
        let size_bytes = serialized.len() as u32;
        let checksum = Self::calculate_checksum(&serialized);

        Self {
            version: crate::STORAGE_PROTOCOL_VERSION,
            offset,
            timestamp,
            stored_by,
            size_bytes,
            checksum,
            cluster_message,
            metadata: StorageMetadata::default(),
        }
    }

    /// Calculate checksum for data integrity verification
    ///
    /// # Parameters
    ///
    /// * `data` - Data to calculate checksum for
    ///
    /// # Returns
    ///
    /// CRC32 checksum as u64.
    fn calculate_checksum(data: &[u8]) -> u64 {
        u64::from(crc32fast::hash(data))
    }

    /// Verify message integrity using stored checksum
    ///
    /// # Returns
    ///
    /// `true` if checksum verification passes.
    #[must_use]
    pub fn verify_integrity(&self) -> bool {
        if let Ok(serialized) = bincode::serialize(&self.cluster_message) {
            let calculated_checksum = Self::calculate_checksum(&serialized);
            calculated_checksum == self.checksum && serialized.len() == self.size_bytes as usize
        } else {
            false
        }
    }

    /// Get message age since storage
    ///
    /// # Returns
    ///
    /// Duration since the message was stored.
    #[must_use]
    pub fn age(&self) -> Duration {
        let current_time = HybridLogicalClock::now();
        Duration::from_millis(current_time.time_diff_millis(&self.timestamp))
    }

    /// Check if message has expired based on TTL
    ///
    /// # Returns
    ///
    /// `true` if message has exceeded its time-to-live.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.metadata.ttl.map_or(false, |ttl| self.age() > ttl)
    }
}

impl From<ClusterMessage> for StorageMessage {
    fn from(cluster_message: ClusterMessage) -> Self {
        // Use a default node ID when converting from cluster message
        // In practice, this should be provided by the storage engine
        let default_node_id = NodeId::generate();
        let default_offset = LogOffset::new(0);

        Self::new(cluster_message, default_node_id, default_offset)
    }
}

impl fmt::Display for CompressionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::None => "none",
            Self::Lz4 => "lz4",
            Self::Gzip => "gzip",
            Self::Snappy => "snappy",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for EncryptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::None => "none",
            Self::Aes256Gcm => "aes256-gcm",
            Self::ChaCha20Poly1305 => "chacha20-poly1305",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for MessagePriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Critical => "critical",
        };
        write!(f, "{name}")
    }
}

impl BatchWriteResult {
    /// Create a successful batch write result
    ///
    /// # Parameters
    ///
    /// * `message_results` - Individual message write results
    ///
    /// # Returns
    ///
    /// A new [`BatchWriteResult`] representing a successful batch operation.
    #[must_use]
    pub fn success(message_results: Vec<WriteResult>) -> Self {
        let messages_written = message_results.len();
        let total_bytes_written = message_results
            .iter()
            .map(|r| u64::from(r.bytes_written))
            .sum();
        let total_latency = message_results
            .iter()
            .map(|r| r.latency)
            .max()
            .unwrap_or(Duration::ZERO);
        let average_latency = if messages_written > 0 {
            Duration::from_nanos(
                message_results
                    .iter()
                    .map(|r| r.latency.as_nanos() as u64)
                    .sum::<u64>() / messages_written as u64,
            )
        } else {
            Duration::ZERO
        };

        Self {
            message_results,
            messages_written,
            total_bytes_written,
            total_latency,
            average_latency,
            success: true,
            first_error: None,
        }
    }

    /// Create a failed batch write result
    ///
    /// # Parameters
    ///
    /// * `partial_results` - Results for successfully processed messages
    /// * `error` - First error encountered
    ///
    /// # Returns
    ///
    /// A new [`BatchWriteResult`] representing a failed batch operation.
    #[must_use]
    pub fn failure(partial_results: Vec<WriteResult>, error: StorageError) -> Self {
        let messages_written = partial_results.len();
        let total_bytes_written = partial_results
            .iter()
            .map(|r| u64::from(r.bytes_written))
            .sum();

        Self {
            message_results: partial_results,
            messages_written,
            total_bytes_written,
            total_latency: Duration::ZERO,
            average_latency: Duration::ZERO,
            success: false,
            first_error: Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaelix_cluster::{messages::MessagePayload, types::NodeId};

    #[test]
    fn test_log_offset() {
        let offset1 = LogOffset::new(100);
        let offset2 = LogOffset::new(101);

        assert_eq!(offset1.value(), 100);
        assert_eq!(offset1.next(), offset2);
        assert!(offset2.follows(&offset1));
        assert_eq!(offset1.distance(&offset2), 1);
        assert_eq!(offset1.to_string(), "100");
    }

    #[test]
    fn test_storage_message_creation() {
        let node_id = NodeId::generate();
        let dest_id = NodeId::generate();
        let cluster_msg = ClusterMessage::new(
            node_id,
            dest_id,
            MessagePayload::HealthCheck,
        );

        let storage_msg = StorageMessage::new(
            cluster_msg.clone(),
            node_id,
            LogOffset::new(42),
        );

        assert_eq!(storage_msg.offset, LogOffset::new(42));
        assert_eq!(storage_msg.stored_by, node_id);
        assert_eq!(storage_msg.cluster_message, cluster_msg);
        assert!(storage_msg.verify_integrity());
    }

    #[test]
    fn test_message_priority_ordering() {
        assert!(MessagePriority::Low < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Critical);
    }

    #[test]
    fn test_batch_write_result() {
        let results = vec![
            WriteResult {
                offset: LogOffset::new(1),
                bytes_written: 100,
                latency: Duration::from_micros(50),
                fsync_performed: false,
                replicas_written: 1,
            },
            WriteResult {
                offset: LogOffset::new(2),
                bytes_written: 200,
                latency: Duration::from_micros(75),
                fsync_performed: false,
                replicas_written: 1,
            },
        ];

        let batch_result = BatchWriteResult::success(results);

        assert_eq!(batch_result.messages_written, 2);
        assert_eq!(batch_result.total_bytes_written, 300);
        assert!(batch_result.success);
        assert!(batch_result.first_error.is_none());
        assert_eq!(batch_result.average_latency, Duration::from_nanos(62_500));
    }

    #[test]
    fn test_read_position() {
        let offset_pos = ReadPosition::Offset(LogOffset::new(123));
        let beginning_pos = ReadPosition::Beginning;
        let end_pos = ReadPosition::End;
        let relative_pos = ReadPosition::Relative(-10);

        // Test that positions are different
        assert_ne!(offset_pos, beginning_pos);
        assert_ne!(beginning_pos, end_pos);
        assert_ne!(end_pos, relative_pos);
    }

    #[test]
    fn test_compression_and_encryption_display() {
        assert_eq!(CompressionType::Lz4.to_string(), "lz4");
        assert_eq!(EncryptionType::Aes256Gcm.to_string(), "aes256-gcm");
        assert_eq!(MessagePriority::High.to_string(), "high");
    }

    #[test]
    fn test_conversion_traits() {
        let offset_val = 12345u64;
        let log_offset = LogOffset::from(offset_val);
        assert_eq!(log_offset.value(), offset_val);

        let converted_back: u64 = log_offset.into();
        assert_eq!(converted_back, offset_val);
    }
}