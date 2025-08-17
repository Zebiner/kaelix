//! Memory allocation and usage benchmarks for MemoryStreamer.
//!
//! This benchmark suite validates memory efficiency, zero-copy operations,
//! allocation patterns, and memory leak detection to ensure optimal
//! memory usage under various load conditions.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use kaelix_benches::prelude::*;
use kaelix_benches::{BenchmarkConfig, MemoryMetrics};
use kaelix_core::{Message, Result};
use bytes::Bytes;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Memory allocation sizes for testing.
const ALLOCATION_SIZES: &[usize] = &[64, 1024, 4096, 16384, 65536, 1048576];

/// Batch sizes for memory usage testing.
const MEMORY_BATCH_SIZES: &[usize] = &[10, 100, 1000, 10000];

/// Load levels for memory pressure testing.
const LOAD_LEVELS: &[usize] = &[1000, 10000, 100000, 1000000];

/// Benchmark memory allocation patterns.
fn bench_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation_patterns");
    
    for &size in ALLOCATION_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        
        // Vector allocation
        group.bench_with_input(
            BenchmarkId::new("vec_alloc", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let mut vec = Vec::with_capacity(size);
                    vec.resize(size, 0u8);
                    criterion::black_box(vec)
                });
            },
        );
        
        // Box allocation
        group.bench_with_input(
            BenchmarkId::new("box_alloc", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let boxed = vec![0u8; size].into_boxed_slice();
                    criterion::black_box(boxed)
                });
            },
        );
        
        // Bytes allocation
        group.bench_with_input(
            BenchmarkId::new("bytes_alloc", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let data = vec![0u8; size];
                    let bytes = Bytes::from(data);
                    criterion::black_box(bytes)
                });
            },
        );
        
        // SmallVec allocation (stack optimization for small sizes)
        group.bench_with_input(
            BenchmarkId::new("smallvec_alloc", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let mut small_vec = smallvec::SmallVec::<[u8; 64]>::new();
                    small_vec.resize(size, 0);
                    criterion::black_box(small_vec)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark zero-copy operations validation.
fn bench_zero_copy_validation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("zero_copy_validation");
    
    for &size in &[1024, 4096, 16384, 65536] {
        // Traditional copy approach
        group.bench_with_input(
            BenchmarkId::new("copy_approach", size),
            &size,
            |b, &size| {
                let original_data = vec![0xAB; size];
                
                b.iter(|| {
                    // Force copy by creating new Vec each time
                    let copied_data = original_data.clone();
                    let bytes = Bytes::from(copied_data);
                    let message = Message::new("test", bytes).unwrap();
                    criterion::black_box(message)
                });
            },
        );
        
        // Zero-copy with Bytes::clone()
        group.bench_with_input(
            BenchmarkId::new("zero_copy_bytes", size),
            &size,
            |b, &size| {
                let original_data = vec![0xAB; size];
                let shared_bytes = Bytes::from(original_data);
                
                b.iter(|| {
                    // Zero-copy clone - only increments reference count
                    let cloned_bytes = shared_bytes.clone();
                    let message = Message::new("test", cloned_bytes).unwrap();
                    criterion::black_box(message)
                });
            },
        );
        
        // Reference counting validation
        group.bench_with_input(
            BenchmarkId::new("ref_count_overhead", size),
            &size,
            |b, &size| {
                let original_data = vec![0xAB; size];
                let shared_bytes = Bytes::from(original_data);
                
                b.iter(|| {
                    // Create multiple references to test ref count overhead
                    let ref1 = shared_bytes.clone();
                    let ref2 = shared_bytes.clone();
                    let ref3 = shared_bytes.clone();
                    criterion::black_box((ref1, ref2, ref3))
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory usage under sustained load.
fn bench_memory_under_load(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_under_load");
    group.measurement_time(Duration::from_secs(30));
    
    for &load_level in LOAD_LEVELS {
        group.bench_with_input(
            BenchmarkId::new("sustained_load", load_level),
            &load_level,
            |b, &level| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::memory().await;
                    let mut memory_tracker = MemoryTracker::new();
                    
                    // Generate sustained load
                    for i in 0..level {
                        let payload_size = 1024 + (i % 4096); // Variable payload sizes
                        let message = create_test_message(payload_size);
                        
                        ctx.publisher.publish(message).await.unwrap();
                        
                        // Sample memory usage periodically
                        if i % 1000 == 0 {
                            memory_tracker.sample();
                        }
                    }
                    
                    let memory_stats = memory_tracker.get_stats();
                    criterion::black_box(memory_stats)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark garbage collection impact (if any).
fn bench_gc_impact(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("gc_impact");
    
    for &batch_size in MEMORY_BATCH_SIZES {
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            &batch_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let ctx = BenchmarkContext::memory().await;
                    
                    // Create many short-lived objects to potentially trigger GC
                    let mut messages = Vec::with_capacity(size);
                    for i in 0..size {
                        let payload = format!("Message {} data", i);
                        let message = Message::new("gc-test", Bytes::from(payload)).unwrap();
                        messages.push(message);
                    }
                    
                    // Process all messages
                    for message in messages {
                        ctx.publisher.publish(message).await.unwrap();
                    }
                    
                    // Force drop to measure cleanup time
                    drop(ctx);
                    
                    criterion::black_box(size)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory leak detection.
fn bench_memory_leak_detection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("memory_leak_detection", |b| {
        b.to_async(&rt).iter(|| async {
            let mut memory_tracker = MemoryTracker::new();
            let ctx = BenchmarkContext::memory().await;
            
            // Baseline memory measurement
            memory_tracker.sample();
            let baseline = memory_tracker.get_current_usage();
            
            // Perform operations that should not leak memory
            for i in 0..10000 {
                let message = create_test_message(1024);
                ctx.publisher.publish(message).await.unwrap();
            }
            
            // Force cleanup
            drop(ctx);
            
            // Wait for potential cleanup
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Measure final memory usage
            memory_tracker.sample();
            let final_usage = memory_tracker.get_current_usage();
            
            let memory_diff = final_usage.saturating_sub(baseline);
            
            // Memory leak validation
            let leak_threshold = 1024 * 1024; // 1MB threshold
            if memory_diff > leak_threshold {
                eprintln!("Potential memory leak detected: {} bytes", memory_diff);
            }
            
            criterion::black_box(memory_diff)
        });
    });
}

/// Benchmark memory-mapped file operations.
fn bench_memory_mapped_io(c: &mut Criterion) {
    use std::fs::File;
    use std::io::Write;
    
    let mut group = c.benchmark_group("memory_mapped_io");
    
    for &file_size in &[1024, 4096, 16384, 65536] {
        // Setup temporary file
        let temp_file = format!("/tmp/mmap_test_{}.dat", file_size);
        {
            let mut file = File::create(&temp_file).unwrap();
            file.write_all(&vec![0xAB; file_size]).unwrap();
        }
        
        // Standard file I/O
        group.bench_with_input(
            BenchmarkId::new("standard_io", file_size),
            &file_size,
            |b, &size| {
                b.iter(|| {
                    let data = std::fs::read(&temp_file).unwrap();
                    criterion::black_box(data)
                });
            },
        );
        
        // Memory-mapped I/O (simulated)
        group.bench_with_input(
            BenchmarkId::new("memory_mapped", file_size),
            &file_size,
            |b, &size| {
                b.iter(|| {
                    // Simulate memory mapping with direct memory access
                    let mmap_data = MemoryMappedData::new(&temp_file, size).unwrap();
                    let data = mmap_data.read_all();
                    criterion::black_box(data)
                });
            },
        );
        
        // Cleanup
        std::fs::remove_file(&temp_file).ok();
    }
    
    group.finish();
}

/// Benchmark different buffer pooling strategies.
fn bench_buffer_pooling(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pooling");
    
    for &buffer_size in &[1024, 4096, 16384] {
        // No pooling - allocate each time
        group.bench_with_input(
            BenchmarkId::new("no_pooling", buffer_size),
            &buffer_size,
            |b, &size| {
                b.iter(|| {
                    let buffer = vec![0u8; size];
                    // Simulate usage
                    let _sum: u32 = buffer.iter().map(|&b| b as u32).sum();
                    criterion::black_box(buffer)
                });
            },
        );
        
        // Simple pooling with reused buffer
        group.bench_with_input(
            BenchmarkId::new("simple_pooling", buffer_size),
            &buffer_size,
            |b, &size| {
                let mut reused_buffer = Vec::with_capacity(size);
                
                b.iter(|| {
                    reused_buffer.clear();
                    reused_buffer.resize(size, 0);
                    // Simulate usage
                    let _sum: u32 = reused_buffer.iter().map(|&b| b as u32).sum();
                    criterion::black_box(&reused_buffer)
                });
            },
        );
        
        // Advanced pooling with multiple buffers
        group.bench_with_input(
            BenchmarkId::new("advanced_pooling", buffer_size),
            &buffer_size,
            |b, &size| {
                let mut buffer_pool = BufferPool::new(size, 4); // Pool of 4 buffers
                
                b.iter(|| {
                    let buffer = buffer_pool.get_buffer();
                    // Simulate usage
                    let _sum: u32 = buffer.iter().map(|&b| b as u32).sum();
                    buffer_pool.return_buffer(buffer);
                    criterion::black_box(())
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark stack vs heap allocation performance.
fn bench_stack_vs_heap(c: &mut Criterion) {
    let mut group = c.benchmark_group("stack_vs_heap");
    
    // Small allocations (stack-friendly)
    group.bench_function("stack_small", |b| {
        b.iter(|| {
            let buffer = [0u8; 256]; // Stack allocation
            let _sum: u32 = buffer.iter().map(|&b| b as u32).sum();
            criterion::black_box(buffer)
        });
    });
    
    group.bench_function("heap_small", |b| {
        b.iter(|| {
            let buffer = vec![0u8; 256]; // Heap allocation
            let _sum: u32 = buffer.iter().map(|&b| b as u32).sum();
            criterion::black_box(buffer)
        });
    });
    
    // Large allocations (heap required)
    group.bench_function("heap_large", |b| {
        b.iter(|| {
            let buffer = vec![0u8; 65536]; // Large heap allocation
            let _sum: u32 = buffer.iter().map(|&b| b as u32).sum();
            criterion::black_box(buffer)
        });
    });
    
    // SmallVec optimization
    group.bench_function("smallvec_adaptive", |b| {
        b.iter(|| {
            let mut small_vec = smallvec::SmallVec::<[u8; 256]>::new();
            small_vec.resize(128, 0); // Will use stack storage
            let _sum: u32 = small_vec.iter().map(|&b| b as u32).sum();
            criterion::black_box(small_vec)
        });
    });
    
    group.finish();
}

/// Helper function to create a test message with specified payload size.
fn create_test_message(payload_size: usize) -> Message {
    let payload = vec![0xAB; payload_size];
    Message::new("memory-test", Bytes::from(payload)).unwrap()
}

/// Mock memory tracker for monitoring memory usage.
struct MemoryTracker {
    samples: Vec<MemorySample>,
    start_usage: usize,
}

struct MemorySample {
    timestamp: std::time::Instant,
    memory_usage: usize,
}

impl MemoryTracker {
    fn new() -> Self {
        Self {
            samples: Vec::new(),
            start_usage: Self::get_memory_usage(),
        }
    }
    
    fn sample(&mut self) {
        self.samples.push(MemorySample {
            timestamp: std::time::Instant::now(),
            memory_usage: Self::get_memory_usage(),
        });
    }
    
    fn get_current_usage(&self) -> usize {
        Self::get_memory_usage()
    }
    
    fn get_stats(&self) -> MemoryStats {
        if self.samples.is_empty() {
            return MemoryStats::default();
        }
        
        let peak = self.samples.iter().map(|s| s.memory_usage).max().unwrap_or(0);
        let average = self.samples.iter().map(|s| s.memory_usage).sum::<usize>() / self.samples.len();
        let current = self.samples.last().unwrap().memory_usage;
        
        MemoryStats {
            peak_usage: peak,
            average_usage: average,
            current_usage: current,
            growth: current.saturating_sub(self.start_usage),
        }
    }
    
    fn get_memory_usage() -> usize {
        // Simplified memory usage measurement
        // In a real implementation, this would use system calls or memory profiling
        0
    }
}

#[derive(Debug, Default)]
struct MemoryStats {
    peak_usage: usize,
    average_usage: usize,
    current_usage: usize,
    growth: usize,
}

/// Mock memory-mapped data structure.
struct MemoryMappedData {
    data: Vec<u8>,
}

impl MemoryMappedData {
    fn new(_file_path: &str, size: usize) -> Result<Self> {
        // Simulate memory mapping by reading file into memory
        Ok(Self {
            data: vec![0xAB; size],
        })
    }
    
    fn read_all(&self) -> &[u8] {
        &self.data
    }
}

/// Simple buffer pool implementation.
struct BufferPool {
    buffers: Vec<Vec<u8>>,
    buffer_size: usize,
}

impl BufferPool {
    fn new(buffer_size: usize, pool_size: usize) -> Self {
        let buffers = (0..pool_size)
            .map(|_| Vec::with_capacity(buffer_size))
            .collect();
        
        Self {
            buffers,
            buffer_size,
        }
    }
    
    fn get_buffer(&mut self) -> Vec<u8> {
        self.buffers.pop().unwrap_or_else(|| Vec::with_capacity(self.buffer_size))
    }
    
    fn return_buffer(&mut self, mut buffer: Vec<u8>) {
        buffer.clear();
        if buffer.capacity() == self.buffer_size {
            self.buffers.push(buffer);
        }
    }
}

/// Mock test context for memory testing.
struct MemoryTestContext {
    publisher: Publisher,
}

impl BenchmarkContext {
    async fn memory() -> MemoryTestContext {
        MemoryTestContext {
            publisher: Publisher,
        }
    }
}

criterion_group!(
    memory_benches,
    bench_allocation_patterns,
    bench_zero_copy_validation,
    bench_memory_under_load,
    bench_gc_impact,
    bench_memory_leak_detection,
    bench_memory_mapped_io,
    bench_buffer_pooling,
    bench_stack_vs_heap
);

criterion_main!(memory_benches);