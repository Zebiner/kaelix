//! Storage engine traits and interfaces.
//!
//! This module defines the core traits that all storage implementations must
//! implement, providing a unified interface for different storage backends
//! while maintaining high performance and flexibility.

use crate::{
    error::StorageResult,
    types::{
        BatchWriteResult, FlushResult, LogOffset, MessageIterator, ReadPosition, StorageMessage,
        StorageMetrics, WriteResult,
    },
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};

/// Core storage engine trait for high-performance message storage
///
/// Defines the primary interface for all storage operations in the MemoryStreamer
/// system. Implementations must provide async/await support throughout and maintain
/// the performance characteristics specified in each method.
///
/// # Performance Requirements
///
/// - **Write Operations**: <10μs P99 latency target
/// - **Read Operations**: <5μs P99 latency target  
/// - **Batch Operations**: >100K messages/second throughput
/// - **Concurrent Access**: Thread-safe for multiple readers/writers
///
/// # Safety and Durability
///
/// - **Thread Safety**: All methods must be `Send + Sync + 'static`
/// - **Durability**: Configurable durability guarantees via flush operations
/// - **Consistency**: ACID compliant within storage backend capabilities
/// - **Recovery**: Support for crash recovery and consistency repair
///
/// # Example Implementation Pattern
///
/// ```rust
/// use kaelix_storage::{StorageEngine, StorageMessage, WriteResult};
/// use async_trait::async_trait;
///
/// pub struct MyStorageEngine {
///     // Implementation-specific fields
/// }
///
/// #[async_trait]
/// impl StorageEngine for MyStorageEngine {
///     async fn append(&self, message: StorageMessage) -> kaelix_storage::StorageResult<WriteResult> {
///         // High-performance append implementation
///         todo!()
///     }
///
///     // ... other required methods
/// }
/// ```
#[async_trait]
pub trait StorageEngine: Send + Sync + 'static {
    /// Append a single message to the storage log
    ///
    /// This is the primary write operation for the storage engine. Implementations
    /// must ensure atomic append operations with optional durability guarantees.
    ///
    /// # Performance Target
    ///
    /// - **Latency**: <10μs P99 for in-memory backends, <100μs P99 for disk backends
    /// - **Throughput**: >1M messages/second for sustained workloads
    ///
    /// # Parameters
    ///
    /// * `message` - The storage message to append
    ///
    /// # Returns
    ///
    /// A [`WriteResult`] containing the assigned offset and performance metrics,
    /// or a [`StorageError`] if the operation fails.
    ///
    /// # Errors
    ///
    /// - [`StorageError::WriteFailed`] - Write operation failed
    /// - [`StorageError::CapacityExceeded`] - Storage capacity exceeded  
    /// - [`StorageError::BackendUnavailable`] - Storage backend unavailable
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_storage::{StorageEngine, StorageMessage};
    /// 
    /// async fn append_message(engine: &dyn StorageEngine, msg: StorageMessage) {
    ///     match engine.append(msg).await {
    ///         Ok(result) => {
    ///             println!("Message written to offset: {}", result.offset);
    ///             println!("Write latency: {:?}", result.latency);
    ///         }
    ///         Err(e) => eprintln!("Write failed: {}", e),
    ///     }
    /// }
    /// ```
    async fn append(&self, message: StorageMessage) -> StorageResult<WriteResult>;

    /// Append multiple messages in a single batch operation
    ///
    /// Provides efficient batch processing for high-throughput scenarios.
    /// Implementations should optimize for batch processing to minimize
    /// per-message overhead.
    ///
    /// # Performance Target
    ///
    /// - **Latency**: <50μs P99 for batches up to 1000 messages
    /// - **Throughput**: >10M messages/second for batch operations
    /// - **Efficiency**: <10% overhead compared to individual appends
    ///
    /// # Parameters
    ///
    /// * `messages` - Vector of messages to append atomically
    ///
    /// # Returns
    ///
    /// A [`BatchWriteResult`] with individual results and aggregate metrics,
    /// or a [`StorageError`] for fatal batch failures.
    ///
    /// # Atomicity Guarantees
    ///
    /// - **All-or-Nothing**: Either all messages succeed or none are written
    /// - **Ordering**: Messages maintain relative ordering within the batch
    /// - **Offset Sequence**: Assigned offsets are consecutive for the batch
    ///
    /// # Errors
    ///
    /// - [`StorageError::BatchOperationFailed`] - Partial or complete batch failure
    /// - [`StorageError::CapacityExceeded`] - Insufficient capacity for batch
    /// - [`StorageError::WriteFailed`] - Individual message write failures
    async fn append_batch(&self, messages: Vec<StorageMessage>) -> StorageResult<BatchWriteResult>;

    /// Read a single message by offset
    ///
    /// Retrieves a message at a specific log offset with high performance.
    /// Implementations should optimize for random access patterns.
    ///
    /// # Performance Target
    ///
    /// - **Latency**: <5μs P99 for in-memory backends, <50μs P99 for disk backends
    /// - **Cache Efficiency**: Leverage CPU cache for frequently accessed messages
    ///
    /// # Parameters
    ///
    /// * `offset` - Log offset of the message to read
    ///
    /// # Returns
    ///
    /// The [`StorageMessage`] at the specified offset, or a [`StorageError`]
    /// if the message cannot be retrieved.
    ///
    /// # Errors
    ///
    /// - [`StorageError::MessageNotFound`] - No message exists at offset
    /// - [`StorageError::ReadFailed`] - Read operation failed
    /// - [`StorageError::MessageCorrupted`] - Message corruption detected
    /// - [`StorageError::InvalidOffset`] - Invalid offset specified
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_storage::{StorageEngine, LogOffset};
    /// 
    /// async fn read_message(engine: &dyn StorageEngine, offset: LogOffset) {
    ///     match engine.read(offset).await {
    ///         Ok(message) => {
    ///             println!("Read message from offset {}: {:?}", offset, message.cluster_message);
    ///             if !message.verify_integrity() {
    ///                 eprintln!("Warning: Message integrity check failed");
    ///             }
    ///         }
    ///         Err(e) => eprintln!("Read failed: {}", e),
    ///     }
    /// }
    /// ```
    async fn read(&self, offset: LogOffset) -> StorageResult<StorageMessage>;

    /// Read a range of messages between two offsets
    ///
    /// Efficiently retrieves multiple consecutive messages for batch processing
    /// or range queries. Implementations should optimize for sequential access.
    ///
    /// # Performance Target
    ///
    /// - **Throughput**: >5M messages/second for sequential reads
    /// - **Memory Efficiency**: Streaming reads without excessive memory usage
    ///
    /// # Parameters
    ///
    /// * `start` - Starting offset (inclusive)
    /// * `end` - Ending offset (exclusive)
    ///
    /// # Returns
    ///
    /// A [`MessageIterator`] for efficient iteration over the message range,
    /// or a [`StorageError`] if the range is invalid.
    ///
    /// # Behavior
    ///
    /// - **Range Semantics**: `[start, end)` - start inclusive, end exclusive
    /// - **Empty Ranges**: Returns empty iterator for `start >= end`
    /// - **Partial Ranges**: Iterator stops at first missing message
    /// - **Streaming**: Results are streamed to minimize memory usage
    ///
    /// # Errors
    ///
    /// - [`StorageError::InvalidOffset`] - Invalid start or end offset
    /// - [`StorageError::ReadFailed`] - Range read operation failed
    async fn read_range(
        &self,
        start: LogOffset,
        end: LogOffset,
    ) -> StorageResult<Box<dyn MessageIterator>>;

    /// Create a new storage reader at a specified position
    ///
    /// Creates a stateful reader for efficient sequential access patterns.
    /// Readers maintain position state and can be used for streaming operations.
    ///
    /// # Performance Target
    ///
    /// - **Creation Time**: <1μs reader creation overhead
    /// - **Read Throughput**: >5M messages/second sequential reads
    /// - **Memory Usage**: <1KB per reader for inactive readers
    ///
    /// # Parameters
    ///
    /// * `position` - Initial position for the reader
    ///
    /// # Returns
    ///
    /// A [`StorageReader`] positioned at the specified location,
    /// or a [`StorageError`] if reader creation fails.
    ///
    /// # Reader Lifecycle
    ///
    /// - **Creation**: Reader is positioned but not yet reading
    /// - **Active**: Reader is actively processing messages
    /// - **Inactive**: Reader is paused but retains position
    /// - **Closed**: Reader resources are released
    async fn create_reader(&self, position: ReadPosition) -> StorageResult<Box<dyn StorageReader>>;

    /// Force flush of pending writes to durable storage
    ///
    /// Ensures that all pending write operations are committed to durable
    /// storage. Critical for durability guarantees and consistent snapshots.
    ///
    /// # Performance Target
    ///
    /// - **Latency**: <1ms P99 for memory backends, <10ms P99 for disk backends
    /// - **Throughput Impact**: <5% reduction during flush operations
    ///
    /// # Durability Guarantees
    ///
    /// - **Atomic Flush**: All pending writes are flushed atomically
    /// - **Ordering**: Write ordering is preserved during flush
    /// - **Consistency**: Storage state is consistent after flush completion
    ///
    /// # Returns
    ///
    /// A [`FlushResult`] with flush operation metrics and durability confirmation,
    /// or a [`StorageError`] if the flush operation fails.
    ///
    /// # Errors
    ///
    /// - [`StorageError::IoError`] - I/O operation failed during flush
    /// - [`StorageError::BackendUnavailable`] - Storage backend unavailable
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_storage::StorageEngine;
    /// 
    /// async fn ensure_durability(engine: &dyn StorageEngine) {
    ///     match engine.flush().await {
    ///         Ok(result) => {
    ///             println!("Flushed {} messages ({} bytes) in {:?}",
    ///                 result.messages_flushed, result.bytes_flushed, result.latency);
    ///         }
    ///         Err(e) => eprintln!("Flush failed: {}", e),
    ///     }
    /// }
    /// ```
    async fn flush(&self) -> StorageResult<FlushResult>;

    /// Get current storage engine metrics
    ///
    /// Provides comprehensive performance and operational metrics for monitoring
    /// and alerting purposes. Metrics should be efficiently accessible.
    ///
    /// # Performance Target
    ///
    /// - **Access Time**: <1μs metrics access overhead
    /// - **Update Frequency**: Metrics updated at least every 100ms
    /// - **Memory Usage**: <10KB for complete metrics structure
    ///
    /// # Returns
    ///
    /// Current [`StorageMetrics`] snapshot with all available metrics.
    ///
    /// # Metric Categories
    ///
    /// - **Performance**: Latency, throughput, and operation counts
    /// - **Capacity**: Storage utilization and available space
    /// - **Health**: Error rates and availability metrics
    /// - **Replication**: Distributed storage specific metrics
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kaelix_storage::StorageEngine;
    /// 
    /// async fn monitor_storage(engine: &dyn StorageEngine) {
    ///     let metrics = engine.metrics();
    ///     println!("Write throughput: {:.2} msg/s", metrics.write_throughput_mps);
    ///     println!("Storage utilization: {:.1}%", metrics.utilization_percentage * 100.0);
    ///     
    ///     if metrics.write_latency_p99.as_micros() > 100 {
    ///         println!("Warning: High write latency detected");
    ///     }
    /// }
    /// ```
    fn metrics(&self) -> StorageMetrics;

    /// Shutdown the storage engine gracefully
    ///
    /// Performs graceful shutdown of the storage engine, ensuring all pending
    /// operations complete and resources are properly cleaned up.
    ///
    /// # Shutdown Process
    ///
    /// 1. **Stop Accepting New Operations**: New requests are rejected
    /// 2. **Complete Pending Operations**: All in-flight operations finish
    /// 3. **Flush Pending Data**: All data is committed to durable storage
    /// 4. **Release Resources**: Connections, memory, and handles are cleaned up
    ///
    /// # Performance Target
    ///
    /// - **Shutdown Time**: <1s for graceful shutdown under normal load
    /// - **Data Safety**: Zero data loss during graceful shutdown
    ///
    /// # Returns
    ///
    /// `Ok(())` if shutdown completed successfully, or a [`StorageError`]
    /// if shutdown encountered issues.
    ///
    /// # Errors
    ///
    /// - [`StorageError::ShutdownFailed`] - Shutdown process failed
    /// - [`StorageError::OperationTimeout`] - Shutdown timed out
    async fn shutdown(self) -> StorageResult<()>
    where
        Self: Sized;
}

/// Storage backend trait for pluggable storage implementations
///
/// Provides a lower-level interface for different storage backend implementations.
/// This trait allows for pluggable storage systems while maintaining a consistent
/// interface for the storage engine.
///
/// # Backend Types
///
/// - **Memory Backend**: In-memory storage for development and high-performance caching
/// - **File Backend**: Single-node persistent file-based storage
/// - **Distributed Backend**: Multi-node distributed storage with replication
///
/// # Implementation Requirements
///
/// - **Thread Safety**: Must support concurrent access from multiple threads
/// - **Performance**: Must meet or exceed performance targets for the storage engine
/// - **Reliability**: Must handle failures gracefully and provide recovery mechanisms
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    /// Backend-specific configuration type
    type Config: Send + Sync + 'static;

    /// Initialize the storage backend with configuration
    ///
    /// # Parameters
    ///
    /// * `config` - Backend-specific configuration
    ///
    /// # Returns
    ///
    /// `Ok(())` if initialization succeeded, or a [`StorageError`] if it failed.
    async fn initialize(&mut self, config: Self::Config) -> StorageResult<()>;

    /// Write raw data to the backend at a specific offset
    ///
    /// # Parameters
    ///
    /// * `offset` - Target offset for the write
    /// * `data` - Raw data to write
    ///
    /// # Returns
    ///
    /// Number of bytes written, or a [`StorageError`] if the write failed.
    async fn write_at(&self, offset: LogOffset, data: &[u8]) -> StorageResult<usize>;

    /// Read raw data from the backend at a specific offset
    ///
    /// # Parameters
    ///
    /// * `offset` - Offset to read from
    /// * `length` - Number of bytes to read
    ///
    /// # Returns
    ///
    /// The read data as a byte vector, or a [`StorageError`] if the read failed.
    async fn read_at(&self, offset: LogOffset, length: usize) -> StorageResult<Vec<u8>>;

    /// Sync backend data to durable storage
    ///
    /// # Returns
    ///
    /// `Ok(())` if sync succeeded, or a [`StorageError`] if it failed.
    async fn sync(&self) -> StorageResult<()>;

    /// Get backend-specific storage statistics
    ///
    /// # Returns
    ///
    /// [`StorageMetrics`] with backend-specific information.
    fn backend_metrics(&self) -> StorageMetrics;

    /// Close the storage backend
    ///
    /// # Returns
    ///
    /// `Ok(())` if close succeeded, or a [`StorageError`] if it failed.
    async fn close(self) -> StorageResult<()>
    where
        Self: Sized;
}

/// Storage reader trait for sequential access patterns
///
/// Provides efficient sequential reading capabilities with state management.
/// Readers are designed for streaming operations and long-running read workloads.
///
/// # Use Cases
///
/// - **Log Tailing**: Continuously reading new messages as they arrive
/// - **Batch Processing**: Processing large ranges of messages efficiently  
/// - **Replication**: Streaming messages to replica nodes
/// - **Analytics**: Sequential processing for analytics workloads
#[async_trait]
pub trait StorageReader: Send + Sync + 'static {
    /// Read the next message from the current position
    ///
    /// # Performance Target
    ///
    /// - **Latency**: <5μs P99 for sequential reads
    /// - **Throughput**: >5M messages/second for streaming workloads
    ///
    /// # Returns
    ///
    /// The next [`StorageMessage`] in sequence, or [`None`] if at end of log.
    /// Returns [`StorageError`] if read operation fails.
    ///
    /// # State Management
    ///
    /// - **Position Tracking**: Reader position is automatically advanced
    /// - **End of Log**: Returns `None` when no more messages are available
    /// - **Error Recovery**: Position is not advanced on read errors
    async fn read_next(&mut self) -> StorageResult<Option<StorageMessage>>;

    /// Seek to a specific position in the log
    ///
    /// # Parameters
    ///
    /// * `position` - Target position to seek to
    ///
    /// # Returns
    ///
    /// `Ok(())` if seek succeeded, or a [`StorageError`] if it failed.
    async fn seek(&mut self, position: ReadPosition) -> StorageResult<()>;

    /// Get the current reader position
    ///
    /// # Returns
    ///
    /// Current [`LogOffset`] position of the reader.
    fn current_position(&self) -> LogOffset;

    /// Check if more messages are available
    ///
    /// # Returns
    ///
    /// `true` if more messages can be read from current position.
    fn has_more(&self) -> bool;

    /// Set read batch size for optimization
    ///
    /// # Parameters
    ///
    /// * `batch_size` - Preferred number of messages to read in each batch
    fn set_batch_size(&mut self, batch_size: usize);

    /// Get current batch size setting
    ///
    /// # Returns
    ///
    /// Current batch size configuration.
    fn batch_size(&self) -> usize;

    /// Close the reader and release resources
    ///
    /// # Returns
    ///
    /// `Ok(())` if close succeeded, or a [`StorageError`] if it failed.
    async fn close(self: Box<Self>) -> StorageResult<()>;
}

/// Trait for storage engines that support streaming writes
///
/// Provides streaming write capabilities for high-throughput scenarios
/// where messages need to be written as they arrive without buffering.
#[async_trait]
pub trait StreamingWriter: AsyncWrite + Send + Sync + Unpin {
    /// Flush the streaming writer buffer
    ///
    /// # Returns
    ///
    /// Number of bytes flushed, or a [`StorageError`] if flush failed.
    async fn flush_stream(&mut self) -> StorageResult<usize>;

    /// Get the current write position
    ///
    /// # Returns
    ///
    /// Current [`LogOffset`] position in the stream.
    fn current_offset(&self) -> LogOffset;
}

/// Trait for storage engines that support streaming reads  
///
/// Provides streaming read capabilities for efficient sequential access
/// to large ranges of messages without loading everything into memory.
#[async_trait]
pub trait StreamingReader: AsyncRead + AsyncSeek + Send + Sync + Unpin {
    /// Get the current read position
    ///
    /// # Returns
    ///
    /// Current [`LogOffset`] position in the stream.
    fn current_position(&self) -> LogOffset;

    /// Check if end of stream has been reached
    ///
    /// # Returns
    ///
    /// `true` if at end of stream.
    fn is_at_end(&self) -> bool;
}

/// Factory trait for creating storage engine instances
///
/// Provides a standardized interface for creating and configuring
/// storage engine instances with different backends and configurations.
pub trait StorageEngineFactory: Send + Sync + 'static {
    /// Storage engine type produced by this factory
    type Engine: StorageEngine;

    /// Configuration type required by this factory
    type Config: Send + Sync + 'static;

    /// Create a new storage engine instance
    ///
    /// # Parameters
    ///
    /// * `config` - Engine configuration
    ///
    /// # Returns
    ///
    /// A new storage engine instance, or a [`StorageError`] if creation failed.
    fn create_engine(&self, config: Self::Config) -> StorageResult<Arc<Self::Engine>>;

    /// Validate configuration before engine creation
    ///
    /// # Parameters
    ///
    /// * `config` - Configuration to validate
    ///
    /// # Returns
    ///
    /// `Ok(())` if configuration is valid, or a [`StorageError`] describing issues.
    fn validate_config(config: &Self::Config) -> StorageResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test that our traits have the required bounds
    #[test]
    fn test_trait_bounds() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        fn assert_static<T: 'static>() {}

        // These functions won't compile if the bounds aren't satisfied
        assert_send::<Box<dyn StorageEngine>>();
        assert_sync::<Box<dyn StorageEngine>>();
        assert_static::<Box<dyn StorageEngine>>();

        assert_send::<Box<dyn StorageBackend<Config = ()>>>();
        assert_sync::<Box<dyn StorageBackend<Config = ()>>>();
        assert_static::<Box<dyn StorageBackend<Config = ()>>>();

        assert_send::<Box<dyn StorageReader>>();
        assert_sync::<Box<dyn StorageReader>>();
        assert_static::<Box<dyn StorageReader>>();
    }
}