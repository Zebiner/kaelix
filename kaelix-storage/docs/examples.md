# WAL (Write-Ahead Log) Usage Examples

## Basic WAL Operations

### 1. Simple Initialization and Writing
```rust
use kaelix_storage::wal::{Wal, WalConfig};
use kaelix_cluster::messages::ClusterMessage;

async fn basic_wal_example() -> Result<(), WalError> {
    // Create WAL configuration
    let config = WalConfig {
        max_segment_size: 1_024 * 1_024 * 128,  // 128MB segments
        compression_level: 3,
        flush_interval_ms: 10,
        ..Default::default()
    };

    // Initialize WAL
    let wal = Wal::new(config).await?;

    // Create a sample message
    let message = ClusterMessage::new(
        NodeId::generate(),
        NodeId::generate(),
        MessagePayload::Ping { node_id: NodeId::generate() }
    );

    // Write message to WAL
    let sequence = wal.write(message).await?;
    println!("Message written with sequence: {}", sequence);

    Ok(())
}
```

### 2. Batch Writing
```rust
async fn batch_write_example() -> Result<(), WalError> {
    let wal = Wal::new(WalConfig::default()).await?;

    // Prepare batch of messages
    let messages = vec![
        ClusterMessage::new(...),
        ClusterMessage::new(...),
        ClusterMessage::new(...),
    ];

    // Write batch of messages
    let sequences = wal.write_batch(messages).await?;
    println!("Batch written. Sequences: {:?}", sequences);

    Ok(())
}
```

### 3. Reading Entries
```rust
async fn read_entries_example() -> Result<(), WalError> {
    let wal = Wal::new(WalConfig::default()).await?;

    // Read a specific entry by sequence number
    let entry = wal.read(42).await?;
    if let Some(message) = entry {
        println!("Retrieved message: {:?}", message);
    }

    // Read a range of entries
    let entries = wal.read_range(10, 50).await?;
    for entry in entries {
        println!("Entry in range: {:?}", entry);
    }

    Ok(())
}
```

## Advanced Usage Patterns

### 4. Custom Configuration
```rust
async fn advanced_configuration_example() -> Result<(), WalError> {
    let config = WalConfig {
        // Large segment for high-throughput scenarios
        max_segment_size: 1_024 * 1_024 * 256,  // 256MB segments
        
        // Minimal compression for speed
        compression_level: 1,
        
        // Aggressive flushing for durability
        flush_interval_ms: 5,
        
        // High concurrency
        max_concurrent_writes: 2048,
        
        // Custom data directory
        data_dir: PathBuf::from("/custom/wal/storage"),
    };

    let wal = Wal::new(config).await?;
    // Perform WAL operations...

    Ok(())
}
```

### 5. Performance Monitoring
```rust
async fn monitoring_example() -> Result<(), WalError> {
    let wal = Wal::new(WalConfig::default()).await?;

    // Simulate some write operations
    for _ in 0..1000 {
        let message = ClusterMessage::new(...);
        wal.write(message).await?;
    }

    // Retrieve and log WAL statistics
    let stats = wal.stats().await;
    println!("WAL Statistics:");
    println!("Entries Written: {}", stats.entries_written);
    println!("Write Latency: {:.2}μs", stats.write_latency_us);
    println!("Memory Usage: {} bytes", stats.memory_usage_bytes);

    Ok(())
}
```

### 6. Error Handling and Recovery
```rust
async fn error_handling_example() -> Result<(), WalError> {
    let wal = Wal::new(WalConfig::default()).await?;

    // Wrap WAL operations with comprehensive error handling
    match wal.write(message).await {
        Ok(sequence) => {
            println!("Successfully wrote message. Sequence: {}", sequence);
        }
        Err(WalError::IoError(io_err)) => {
            // Handle I/O specific errors
            eprintln!("I/O Error during write: {}", io_err);
            // Potential retry or fallback mechanism
        }
        Err(WalError::SerializationError(ser_err)) => {
            // Handle serialization errors
            eprintln!("Serialization Error: {}", ser_err);
        }
        Err(other_err) => {
            // Catch-all for other error types
            eprintln!("Unexpected WAL error: {:?}", other_err);
        }
    }

    Ok(())
}
```

### 7. Integration with Storage Engine
```rust
async fn storage_engine_integration() -> Result<(), StorageError> {
    // Create storage configuration
    let config = StorageConfig {
        wal: WalConfig {
            max_segment_size: 1_024 * 1_024 * 128,
            compression_level: 3,
            ..Default::default()
        },
        ..Default::default()
    };

    // Initialize storage engine
    let storage_engine = StorageEngine::new(config).await?;

    // Write data through storage engine
    let data = b"example payload".to_vec();
    let sequence = storage_engine.write(data).await?;

    // Retrieve engine statistics
    let stats = storage_engine.stats().await;
    println!("WAL Entries: {}", stats.wal_stats.entries_written);

    Ok(())
}
```

## Best Practices
1. Always use configuration tuned to your workload
2. Monitor WAL statistics regularly
3. Implement robust error handling
4. Choose appropriate compression levels
5. Configure segment sizes based on expected message sizes

## Performance Tips
- Use batch writes for high-throughput scenarios
- Tune `flush_interval_ms` based on durability requirements
- Monitor and adjust `max_concurrent_writes`
- Choose compression levels carefully

## Compatibility
- Rust 1.75+
- Async runtime: Tokio recommended
- Compatible with kaelix-cluster message types

## Conclusion
These examples demonstrate the flexibility and power of the Kaelix Storage WAL system. Adapt these patterns to your specific use cases and performance requirements.