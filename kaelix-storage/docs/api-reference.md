# WAL (Write-Ahead Log) API Reference

## Overview
This document provides a comprehensive API reference for the Write-Ahead Log (WAL) system in the Kaelix Storage Engine.

## Configuration

### `WalConfig`
```rust
pub struct WalConfig {
    /// Maximum segment size in bytes
    pub max_segment_size: usize,

    /// Compression level (0-9)
    pub compression_level: u8,

    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,

    /// Maximum concurrent writes
    pub max_concurrent_writes: usize,

    /// Data directory for WAL segments
    pub data_dir: PathBuf,
}
```

#### Configuration Methods
- `fn default() -> Self`: Creates a default WAL configuration
- `fn validate(&self) -> Result<()>`: Validates configuration parameters

### Initialization
```rust
impl Wal {
    /// Create a new WAL instance
    pub async fn new(config: WalConfig) -> Result<Self>

    /// Shutdown the WAL system
    pub async fn shutdown(&self) -> Result<()>
}
```

## Write Operations

### Writing Data
```rust
impl Wal {
    /// Write a message to the WAL
    /// Returns the sequence number of the written entry
    pub async fn write(&self, message: ClusterMessage) -> Result<u64>

    /// Batch write multiple messages
    pub async fn write_batch(&self, messages: Vec<ClusterMessage>) -> Result<Vec<u64>>
}
```

## Read Operations

### Reading Entries
```rust
impl Wal {
    /// Read an entry by sequence number
    pub async fn read(&self, sequence: u64) -> Result<Option<ClusterMessage>>

    /// Read a range of entries
    pub async fn read_range(&self, start: u64, end: u64) -> Result<Vec<ClusterMessage>>
}
```

## Statistics and Monitoring

### Performance Metrics
```rust
pub struct WalStats {
    /// Total number of entries written
    pub entries_written: u64,

    /// Total number of entries read
    pub entries_read: u64,

    /// Average write latency in microseconds
    pub write_latency_us: f64,

    /// Current memory usage
    pub memory_usage_bytes: usize,

    /// Number of active segments
    pub active_segments: usize,
}

impl Wal {
    /// Retrieve current WAL statistics
    pub async fn stats(&self) -> WalStats
}
```

## Error Handling

### Error Types
```rust
pub enum WalError {
    /// Configuration validation failed
    ConfigurationError(String),

    /// I/O error during WAL operations
    IoError(std::io::Error),

    /// Serialization/deserialization error
    SerializationError(String),

    /// Segment-related errors
    SegmentError(String),

    /// Write operation failed
    WriteError(String),

    /// Read operation failed
    ReadError(String),
}
```

## Advanced Usage Patterns

### Typical Initialization
```rust
let config = WalConfig {
    max_segment_size: 1_024 * 1_024 * 128,  // 128MB segments
    compression_level: 3,
    flush_interval_ms: 10,
    max_concurrent_writes: 1024,
    data_dir: PathBuf::from("/path/to/wal/storage"),
};

let wal = Wal::new(config).await?;

// Write a message
let message = ClusterMessage::new(...);
let sequence = wal.write(message).await?;

// Retrieve statistics
let stats = wal.stats().await;
```

## Best Practices
1. Always validate WAL configuration before initialization
2. Handle potential errors from write and read operations
3. Monitor WAL statistics regularly
4. Configure segment sizes and flush intervals based on your performance requirements
5. Use batch writes for improved throughput

## Performance Considerations
- Minimize serialization overhead
- Choose appropriate compression levels
- Tune flush intervals based on durability needs
- Monitor and adjust concurrent write limits

## Compatibility
- Supports Rust 1.75+
- Async runtime: Tokio recommended
- Compatible with kaelix-cluster message types