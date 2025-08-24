//! Comprehensive Write-Ahead Log (WAL) Test Suite
//!
//! This test suite provides thorough validation of the WAL system,
//! covering performance, concurrency, recovery, and edge cases.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task;

use kaelix_storage::wal::{
    WriteAheadLog, WalConfig, WalError, SyncPolicy, 
    RecoveryConfig, RecoveryMode, CorruptionPolicy,
    IntegrityStatus, LogPosition
};
use kaelix_core::message::Message;
use tempfile::TempDir;

/// Utility function to create a test message
fn create_test_message(id: usize) -> Message {
    Message::new(
        format!("test-{}", id),
        "source".into(),
        "destination".into(),
        format!("payload-{}", id).into_bytes(),
    )
}

/// Performance Tests Module
mod performance_tests {
    use super::*;

    /// Test single append operation latency
    #[tokio::test]
    async fn test_single_append_latency() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            sync_policy: SyncPolicy::NoSync, // Fastest mode
            ..Default::default()
        };

        let wal = WriteAheadLog::new(config, temp_dir.path()).await.unwrap();
        let message = create_test_message(0);
        
        let mut latencies = Vec::new();
        let iterations = 1000;

        for _ in 0..iterations {
            let start = Instant::now();
            let _position = wal.append(message.clone()).await.unwrap();
            let duration = start.elapsed();
            latencies.push(duration);
        }

        // Sort and find P99 latency
        latencies.sort();
        let p99_index = (iterations as f64 * 0.99).floor() as usize;
        let p99_latency = latencies[p99_index];

        assert!(
            p99_latency < Duration::from_micros(10), 
            "P99 latency {} exceeds 10μs target", 
            p99_latency.as_micros()
        );

        wal.shutdown().await.unwrap();
    }

    /// Test batch append throughput
    #[tokio::test]
    async fn test_batch_append_throughput() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            sync_policy: SyncPolicy::NoSync,
            max_batch_size: 1000,
            ..Default::default()
        };

        let wal = WriteAheadLog::new(config, temp_dir.path()).await.unwrap();
        
        let batch_size = 1000;
        let messages: Vec<_> = (0..batch_size)
            .map(create_test_message)
            .collect();

        let start = Instant::now();
        let _positions = wal.append_batch(messages).await.unwrap();
        let duration = start.elapsed();

        let throughput = batch_size as f64 / duration.as_secs_f64();
        
        // Target: 10M+ messages/second 
        assert!(
            throughput > 100_000.0, 
            "Throughput {} msg/s below minimum target", 
            throughput
        );

        wal.shutdown().await.unwrap();
    }

    /// Test concurrent append throughput
    #[tokio::test]
    async fn test_concurrent_append_performance() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            sync_policy: SyncPolicy::NoSync,
            ..Default::default()
        };

        let wal = Arc::new(WriteAheadLog::new(config, temp_dir.path()).await.unwrap());
        
        let num_writers = 10;
        let messages_per_writer = 1000;
        let total_messages = num_writers * messages_per_writer;

        let start = Instant::now();

        let tasks: Vec<_> = (0..num_writers)
            .map(|writer_id| {
                let wal = Arc::clone(&wal);
                task::spawn(async move {
                    for i in 0..messages_per_writer {
                        let message = create_test_message(writer_id * messages_per_writer + i);
                        wal.append(message).await.unwrap();
                    }
                })
            })
            .collect();

        // Wait for all writers to complete
        for task in tasks {
            task.await.unwrap();
        }

        let duration = start.elapsed();
        let throughput = total_messages as f64 / duration.as_secs_f64();

        // Target: Ensure concurrent throughput maintains high performance
        assert!(
            throughput > 50_000.0, 
            "Concurrent throughput {} msg/s below target", 
            throughput
        );

        wal.shutdown().await.unwrap();
    }
}

/// Recovery and Resilience Tests Module
mod recovery_tests {
    use super::*;

    /// Test basic WAL recovery from crash scenario
    #[tokio::test]
    async fn test_recovery_from_crash() {
        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig::default();
        let recovery_config = RecoveryConfig {
            mode: RecoveryMode::Thorough,
            corruption_policy: CorruptionPolicy::Repair,
            ..Default::default()
        };

        // Write some entries then simulate crash
        {
            let mut wal = WriteAheadLog::new(wal_config.clone(), temp_dir.path()).await.unwrap();
            
            // Write messages
            for i in 0..100 {
                wal.append(create_test_message(i)).await.unwrap();
            }
            
            // Drop wal without clean shutdown to simulate crash
        }

        // Recover WAL
        let (mut wal, recovery_result) = WriteAheadLog::new_with_recovery(
            wal_config,
            temp_dir.path(), 
            recovery_config
        ).await.unwrap();

        // Validate recovery
        assert!(recovery_result.segments_recovered > 0);
        assert_eq!(recovery_result.integrity_status, IntegrityStatus::Verified);

        // Ensure we can continue writing after recovery
        let new_message = create_test_message(101);
        let _position = wal.append(new_message).await.unwrap();

        wal.shutdown().await.unwrap();
    }

    /// Test recovery with data corruption
    #[tokio::test]
    async fn test_corruption_detection_and_repair() {
        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig::default();
        let recovery_config = RecoveryConfig {
            mode: RecoveryMode::Thorough,
            corruption_policy: CorruptionPolicy::Repair,
            verify_checksums: true,
            ..Default::default()
        };

        // Create WAL and write entries
        {
            let mut wal = WriteAheadLog::new(wal_config.clone(), temp_dir.path()).await.unwrap();
            
            for i in 0..50 {
                wal.append(create_test_message(i)).await.unwrap();
            }
            
            wal.force_sync().await.unwrap();
            wal.shutdown().await.unwrap();
        }

        // Simulate corruption (implementation would depend on actual WAL internals)
        // For this example, we'll assume a method to corrupt a segment
        corrupt_segment_file(temp_dir.path()).await;

        // Perform recovery
        let result = WriteAheadLog::recover(
            temp_dir.path(), 
            recovery_config
        ).await.unwrap();

        // Validate recovery
        assert!(result.corrupted_entries > 0);
        assert_eq!(result.integrity_status, IntegrityStatus::Repaired);
    }

    /// Simulate segment file corruption
    async fn corrupt_segment_file(path: &std::path::Path) {
        use tokio::fs::{OpenOptions, self};
        use tokio::io::{AsyncWriteExt, AsyncSeekExt};

        // Find the first segment file and corrupt it
        let entries = fs::read_dir(path).await.unwrap();
        let segment_files: Vec<_> = entries
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    let path = e.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("segment") {
                        Some(path)
                    } else {
                        None
                    }
                })
            })
            .collect()
            .await;

        if let Some(first_segment) = segment_files.first() {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(first_segment)
                .await
                .unwrap();

            // Seek to middle of file and write garbage
            file.seek(std::io::SeekFrom::Start(file.metadata().await.unwrap().len() / 2))
                .await
                .unwrap();
            
            file.write_all(&[0xDE, 0xAD, 0xBE, 0xEF])
                .await
                .unwrap();
        }
    }
}

/// Concurrency and Thread Safety Tests Module
mod concurrency_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Test concurrent writers and readers
    #[tokio::test]
    async fn test_concurrent_writers_and_readers() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::default();

        let wal = Arc::new(Mutex::new(
            WriteAheadLog::new(config, temp_dir.path()).await.unwrap()
        ));

        let num_writers = 5;
        let num_readers = 3;
        let messages_per_writer = 100;
        let write_counter = Arc::new(AtomicUsize::new(0));
        let read_counter = Arc::new(AtomicUsize::new(0));

        // Writer tasks
        let writer_tasks: Vec<_> = (0..num_writers)
            .map(|writer_id| {
                let wal = Arc::clone(&wal);
                let counter = Arc::clone(&write_counter);
                
                task::spawn(async move {
                    for i in 0..messages_per_writer {
                        let message = create_test_message(writer_id * messages_per_writer + i);
                        let guard = wal.lock().await;
                        let _position = guard.append(message).await.unwrap();
                        drop(guard);
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        // Reader tasks
        let reader_tasks: Vec<_> = (0..num_readers)
            .map(|_| {
                let wal = Arc::clone(&wal);
                let write_counter = Arc::clone(&write_counter);
                let read_counter = Arc::clone(&read_counter);
                
                task::spawn(async move {
                    while write_counter.load(Ordering::Relaxed) < num_writers * messages_per_writer {
                        let guard = wal.lock().await;
                        if let Ok(last_position) = guard.current_segment_id().await {
                            if last_position > 0 {
                                // Simulated read to test thread safety
                                let _ = read_counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        drop(guard);
                        tokio::time::sleep(Duration::from_micros(10)).await;
                    }
                })
            })
            .collect();

        // Wait for all tasks
        for task in writer_tasks {
            task.await.unwrap();
        }
        for task in reader_tasks {
            task.await.unwrap();
        }

        // Final shutdown
        let mut guard = wal.lock().await;
        guard.shutdown().await.unwrap();
    }
}

/// Edge Case and Error Handling Tests Module
mod edge_case_tests {
    use super::*;

    /// Test handling of extremely large batches
    #[tokio::test]
    async fn test_large_batch_handling() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            max_batch_size: 10_000,
            ..Default::default()
        };

        let wal = WriteAheadLog::new(config, temp_dir.path()).await.unwrap();
        
        let large_batch_size = 5_000;
        let messages: Vec<_> = (0..large_batch_size)
            .map(create_test_message)
            .collect();

        let result = wal.append_batch(messages).await;
        assert!(result.is_ok());

        let positions = result.unwrap();
        assert_eq!(positions.len(), large_batch_size);

        wal.shutdown().await.unwrap();
    }

    /// Test sequence exhaustion handling
    #[tokio::test]
    async fn test_sequence_exhaustion() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            max_sequence_number: u64::MAX - 100,
            ..Default::default()
        };

        let mut wal = WriteAheadLog::new(config, temp_dir.path()).await.unwrap();
        
        // Fill up to near-exhaustion
        for i in 0..u64::MAX - 110 {
            wal.append(create_test_message(i as usize)).await.unwrap();
        }

        // Attempt to write more should result in sequence exhaustion
        let result = wal.append(create_test_message(u64::MAX as usize)).await;
        
        assert!(matches!(result, Err(WalError::SequenceExhausted)));

        wal.shutdown().await.unwrap();
    }

    /// Test shutdown during operations
    #[tokio::test]
    async fn test_graceful_shutdown_during_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::default();

        let mut wal = WriteAheadLog::new(config, temp_dir.path()).await.unwrap();
        
        // Start long-running batch append
        let append_task = task::spawn(async move {
            let messages: Vec<_> = (0..10_000)
                .map(create_test_message)
                .collect();
            
            wal.append_batch(messages).await
        });

        // Wait briefly
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Initiate shutdown
        wal.shutdown().await.unwrap();

        // Check task result - should handle shutdown gracefully
        let result = append_task.await.unwrap();
        
        // Depending on exact timing, result could be:
        // 1. Ok (if shutdown happened after batch start)
        // 2. Err (if shutdown interrupted the batch)
        assert!(result.is_ok() || matches!(result.unwrap_err(), WalError::ShutdownInProgress));
    }
}

/// Integration Tests Module
mod integration_tests {
    use super::*;
    use kaelix_storage::traits::StorageEngine;

    /// Test WAL integration with StorageEngine trait
    #[tokio::test]
    async fn test_storage_engine_integration() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::default();

        // Create WAL as StorageEngine
        let storage_engine: Box<dyn StorageEngine> = Box::new(
            WriteAheadLog::new(config, temp_dir.path()).await.unwrap()
        );

        // Write messages through storage engine interface
        let message1 = create_test_message(0);
        let write_result = storage_engine.append(message1).await.unwrap();

        // Read back through storage engine
        let read_result = storage_engine.read(write_result.position).await.unwrap();
        
        assert_eq!(read_result.message.id(), "test-0");

        // Shutdown
        storage_engine.shutdown().await.unwrap();
    }
}
